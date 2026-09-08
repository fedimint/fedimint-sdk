//! The lnv2 lightning module: mappings, subscriptions, and the facade operations.

use fedimint_client::Client;
use fedimint_client_module::ClientModuleInstance;
use fedimint_core::core::OperationId;
use fedimint_core::util::BoxStream;
use fedimint_lnv2_client::{LightningClientModule, ReceiveOperationState, SendOperationState};
use futures::StreamExt;

use super::driver::until_final;
use super::subscribe_error;
use super::wire::PHASE_FUNDED;
use crate::federation::FederationInner;
use crate::operation::record_phase_in;
use crate::{
    Amount, Error, ErrorCode, LightningRoute, LnReceiveState, LnSendDetails, LnSendState, Preimage,
    Result,
};

// lnv2 `SendOperationState` onto `LnSendState`. `Failure` means two things: the funding
// transaction was refused (before `Funded`, nothing debited, so `Refunded`) or the refund of a
// funded contract failed (after it, so `Failed`). After a restart the persisted phase is what
// tells them apart. `Refunding` is in progress, not final.
//
// | upstream        | phase reached   | here       |
// | --------------- | --------------- | ---------- |
// | `Funding`       | any             | `Created`  |
// | `Funded`        | any             | `Funded`   |
// | `Refunding`     | any             | `Funded`   |
// | `Success`       | any             | `Success`  |
// | `Refunded`      | any             | `Refunded` |
// | `Failure`       | before `Funded` | `Refunded` |
// | `Failure`       | at/after        | `Failed`   |
pub(super) fn map_send(
    state: &SendOperationState,
    funded_before: bool,
    fee: Amount,
    route: &LightningRoute,
) -> LnSendState {
    match state {
        SendOperationState::Funding => LnSendState::Created,
        SendOperationState::Funded | SendOperationState::Refunding => LnSendState::Funded,
        SendOperationState::Success(preimage) => LnSendState::Success {
            preimage: Preimage::from_bytes(*preimage),
            fee,
            route: route.clone(),
        },
        SendOperationState::Refunded => LnSendState::Refunded,
        SendOperationState::Failure if funded_before => LnSendState::Failed {
            reason: "the payment's contract could not be refunded".to_owned(),
        },
        SendOperationState::Failure => LnSendState::Refunded,
    }
}

/// The phase an upstream state proves was reached.
pub(super) fn send_phase(state: &SendOperationState) -> u32 {
    match state {
        SendOperationState::Funded
        | SendOperationState::Refunding
        | SendOperationState::Success(_)
        | SendOperationState::Refunded => PHASE_FUNDED,
        SendOperationState::Funding | SendOperationState::Failure => 0,
    }
}

/// The lnv2 module on a live client, or `NotSupported` when the federation dropped it.
pub(super) fn module_of(
    client: &Client,
) -> Result<ClientModuleInstance<'_, LightningClientModule>> {
    client
        .get_first_module::<LightningClientModule>()
        .map_err(|_| {
            Error::new(
                ErrorCode::NotSupported,
                "this federation no longer has an lnv2 lightning module",
            )
        })
}

/// A fresh stream over an lnv2 send, persisting the funded phase the moment it is seen.
pub(super) async fn subscribe_send(
    federation: &FederationInner,
    id: OperationId,
    phase: u32,
    details: &LnSendDetails,
) -> Result<BoxStream<'static, Result<LnSendState>>> {
    let client = federation.client(false).await?;
    let module = module_of(&client)?;
    let upstream = module
        .subscribe_send_operation_state_updates(id)
        .await
        .map_err(subscribe_error)?
        .into_stream();
    let db = federation.db();
    let fee = details.fee;
    let route = details.route.clone();
    let stream = upstream.scan((phase, db), move |(phase, db), state| {
        let reached = send_phase(&state);
        let advance = reached > *phase;
        if advance {
            *phase = reached;
        }
        let mapped = map_send(&state, *phase >= PHASE_FUNDED, fee, &route);
        let db = db.clone();
        async move {
            if advance && let Err(err) = record_phase_in(&db, id, reached).await {
                return Some(Err(err));
            }
            Some(Ok(mapped))
        }
    });
    Ok(until_final(stream))
}

// lnv2 `ReceiveOperationState` onto `LnReceiveState`. Direct: lnv2 names its stages.
//
// | upstream       | here                |
// | -------------- | ------------------- |
// | `Pending`      | `WaitingForPayment` |
// | `Claiming`     | `Funded`            |
// | `Claimed`      | `Claimed`           |
// | `Expired`      | `Expired`           |
// | `Failure`      | `Failed`            |
// | `Uneconomical` | `Failed`            |
pub(super) fn map_receive(state: &ReceiveOperationState) -> LnReceiveState {
    match state {
        ReceiveOperationState::Pending => LnReceiveState::WaitingForPayment,
        ReceiveOperationState::Claiming => LnReceiveState::Funded,
        ReceiveOperationState::Claimed => LnReceiveState::Claimed,
        ReceiveOperationState::Expired => LnReceiveState::Expired,
        ReceiveOperationState::Failure | ReceiveOperationState::Uneconomical => {
            LnReceiveState::Failed
        }
    }
}

/// A fresh stream over an lnv2 receive. The funded phase is recorded for symmetry with v1, though
/// no lnv2 receive mapping reads it.
pub(super) async fn subscribe_receive(
    federation: &FederationInner,
    id: OperationId,
    phase: u32,
) -> Result<BoxStream<'static, Result<LnReceiveState>>> {
    let client = federation.client(false).await?;
    let module = module_of(&client)?;
    let upstream = module
        .subscribe_receive_operation_state_updates(id)
        .await
        .map_err(subscribe_error)?
        .into_stream();
    let db = federation.db();
    let stream = upstream.scan((phase, db), move |(phase, db), state| {
        let mapped = map_receive(&state);
        let advance = matches!(mapped, LnReceiveState::Funded) && *phase < PHASE_FUNDED;
        if advance {
            *phase = PHASE_FUNDED;
        }
        let db = db.clone();
        async move {
            if advance && let Err(err) = record_phase_in(&db, id, PHASE_FUNDED).await {
                return Some(Err(err));
            }
            Some(Ok(mapped))
        }
    });
    Ok(until_final(stream))
}

/// Rebuilds a record from the module's log entry; filled in with the facade operations.
pub(super) fn backfill(
    _meta: &serde_json::Value,
    _created_at: u64,
) -> Option<crate::operation::Backfilled> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Amount;

    fn fee() -> Amount {
        Amount::from_msats(1_050)
    }

    fn route() -> LightningRoute {
        LightningRoute::Gateway {
            gateway_id: "0218845781f631c48f1c9709e23092067d06837f30aa0cd0544ac887fe91ddd166"
                .parse()
                .expect("a gateway id"),
        }
    }

    #[test]
    fn send_states_fold_onto_the_send_lifecycle() {
        let cases = [
            (SendOperationState::Funding, false, LnSendState::Created),
            (SendOperationState::Funded, true, LnSendState::Funded),
            (SendOperationState::Refunding, true, LnSendState::Funded),
            (SendOperationState::Refunded, true, LnSendState::Refunded),
            (
                SendOperationState::Success([0x33; 32]),
                true,
                LnSendState::Success {
                    preimage: Preimage::from_bytes([0x33; 32]),
                    fee: fee(),
                    route: route(),
                },
            ),
        ];
        for (upstream, funded_before, expected) in cases {
            assert_eq!(
                map_send(&upstream, funded_before, fee(), &route()),
                expected
            );
        }
    }

    #[test]
    fn a_failure_before_funding_is_a_refund_and_after_it_is_a_failure() {
        assert_eq!(
            map_send(&SendOperationState::Failure, false, fee(), &route()),
            LnSendState::Refunded
        );
        assert!(matches!(
            map_send(&SendOperationState::Failure, true, fee(), &route()),
            LnSendState::Failed { .. }
        ));
    }

    #[test]
    fn only_funding_advances_the_phase() {
        assert_eq!(send_phase(&SendOperationState::Funding), 0);
        assert_eq!(send_phase(&SendOperationState::Funded), PHASE_FUNDED);
        assert_eq!(send_phase(&SendOperationState::Refunding), PHASE_FUNDED);
        assert_eq!(
            send_phase(&SendOperationState::Success([0; 32])),
            PHASE_FUNDED
        );
        assert_eq!(send_phase(&SendOperationState::Refunded), PHASE_FUNDED);
        assert_eq!(send_phase(&SendOperationState::Failure), 0);
    }

    #[test]
    fn receive_states_fold_onto_the_receive_lifecycle() {
        let cases = [
            (
                ReceiveOperationState::Pending,
                LnReceiveState::WaitingForPayment,
            ),
            (ReceiveOperationState::Claiming, LnReceiveState::Funded),
            (ReceiveOperationState::Claimed, LnReceiveState::Claimed),
            (ReceiveOperationState::Expired, LnReceiveState::Expired),
            (ReceiveOperationState::Failure, LnReceiveState::Failed),
            (ReceiveOperationState::Uneconomical, LnReceiveState::Failed),
        ];
        for (upstream, expected) in cases {
            assert_eq!(map_receive(&upstream), expected);
        }
    }
}
