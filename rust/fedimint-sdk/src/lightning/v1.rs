//! The v1 lightning module (`ln`): mappings, subscriptions, and the facade operations.

use std::sync::Weak;

use fedimint_client::Client;
use fedimint_client_module::ClientModuleInstance;
use fedimint_core::core::OperationId;
use fedimint_core::db::Database;
use fedimint_core::util::BoxStream;
use fedimint_ln_client::LnReceiveState as UpstreamReceiveState;
use fedimint_ln_client::receive::LightningReceiveError;
use fedimint_ln_client::{InternalPayState, LightningClientModule, LnPayState};
use futures::{StreamExt, stream};

use super::driver::until_final;
use super::subscribe_error;
use super::wire::{self, PHASE_FUNDED};
use crate::federation::FederationInner;
use crate::operation::{record_phase_in, write_details_in};
use crate::sdk::SdkInner;
use crate::{
    Amount, Error, ErrorCode, LightningRoute, LnReceiveState, LnSendDetails, LnSendState,
    OperationState, Preimage, Result,
};

// Upstream `LnPayState` onto `LnSendState`. The fee and the route come from the executed quote:
// the v1 progress stream carries neither.
//
// | upstream                          | here                                   |
// | --------------------------------- | -------------------------------------- |
// | `Created`                         | `Created`                              |
// | `Funded`, `AwaitingChange`        | `Funded`                               |
// | `WaitingForRefund`                | `Funded` (a refund is still in flight) |
// | `Success { preimage }`            | `Success`                              |
// | `Canceled` (never funded)         | `Refunded`                             |
// | `Refunded`                        | `Refunded`                             |
// | `UnexpectedError`                 | `Failed`                               |
pub(super) fn map_ln_pay(
    state: &LnPayState,
    fee: Amount,
    route: &LightningRoute,
) -> Result<LnSendState> {
    Ok(match state {
        LnPayState::Created => LnSendState::Created,
        LnPayState::Funded { .. }
        | LnPayState::AwaitingChange
        | LnPayState::WaitingForRefund { .. } => LnSendState::Funded,
        LnPayState::Success { preimage } => LnSendState::Success {
            // v1 reports the preimage as the hex text the gateway answered with, unvalidated.
            preimage: preimage.parse::<Preimage>().map_err(|err| {
                Error::new(
                    ErrorCode::Internal,
                    format!("the gateway reported an unusable preimage: {err}"),
                )
            })?,
            fee,
            route: route.clone(),
        },
        LnPayState::Canceled | LnPayState::Refunded { .. } => LnSendState::Refunded,
        LnPayState::UnexpectedError { error_message } => LnSendState::Failed {
            reason: error_message.clone(),
        },
    })
}

// Upstream `InternalPayState` onto `LnSendState`, for a payment settled inside the federation.
//
// | upstream                       | here        |
// | ------------------------------ | ----------- |
// | `Funding`                      | `Created`   |
// | `Preimage`                     | `Success`   |
// | `RefundSuccess`                | `Refunded`  |
// | `FundingFailed` (never debited)| `Refunded`  |
// | `RefundError`                  | `Failed`    |
// | `UnexpectedError`              | `Failed`    |
pub(super) fn map_internal_pay(
    state: &InternalPayState,
    fee: Amount,
    route: &LightningRoute,
) -> LnSendState {
    match state {
        InternalPayState::Funding => LnSendState::Created,
        InternalPayState::Preimage(preimage) => LnSendState::Success {
            preimage: Preimage::from_bytes(preimage.0),
            fee,
            route: route.clone(),
        },
        InternalPayState::RefundSuccess { .. } | InternalPayState::FundingFailed { .. } => {
            LnSendState::Refunded
        }
        InternalPayState::RefundError { error_message, .. } => LnSendState::Failed {
            reason: error_message.clone(),
        },
        InternalPayState::UnexpectedError(message) => LnSendState::Failed {
            reason: message.clone(),
        },
    }
}

/// The v1 module on a live client, or `NotSupported` when the federation dropped it.
pub(super) fn module_of(
    client: &Client,
) -> Result<ClientModuleInstance<'_, LightningClientModule>> {
    client
        .get_first_module::<LightningClientModule>()
        .map_err(|_| {
            Error::new(
                ErrorCode::NotSupported,
                "this federation no longer has a v1 lightning module",
            )
        })
}

/// A fresh stream over a v1 send, chosen by the route the record says the payment took.
///
/// The client guard is held only for the bounded upstream read that produces the stream; the
/// stream itself registers with the module's notifier when first polled, as the driver contract
/// requires.
pub(super) async fn subscribe_send(
    federation: &FederationInner,
    id: OperationId,
    details: &LnSendDetails,
) -> Result<BoxStream<'static, Result<LnSendState>>> {
    let client = federation.client(false).await?;
    let module = module_of(&client)?;
    let fee = details.fee;
    let route = details.route.clone();
    let stream: BoxStream<'static, Result<LnSendState>> = match &route {
        LightningRoute::Internal => {
            let upstream = module
                .subscribe_internal_pay(id)
                .await
                .map_err(subscribe_error)?
                .into_stream();
            Box::pin(upstream.map(move |state| Ok(map_internal_pay(&state, fee, &route))))
        }
        LightningRoute::Gateway { .. } => {
            let upstream = module
                .subscribe_ln_pay(id)
                .await
                .map_err(subscribe_error)?
                .into_stream();
            Box::pin(upstream.map(move |state| map_ln_pay(&state, fee, &route)))
        }
    };
    Ok(until_final(stream))
}

/// What the original receive's next upstream state means: a state to hand out, or the signal to
/// retry the claim.
pub(super) enum ReceiveStep {
    State(LnReceiveState),
    Reclaim,
}

// Upstream v1 `LnReceiveState` onto `LnReceiveState`, keyed on whether the receive was ever seen
// funded. `Rejected` means two things: the invoice-registration transaction refused (before any
// funding, so `Canceled`), or the claim's primary outputs failing after a confirmed payment (at
// or after funding, so `Failed`). `ClaimRejected` and `InvalidPreimage` presuppose a funded
// contract and are not phase-keyed.
//
// | upstream                       | phase reached        | here                   |
// | ------------------------------ | -------------------- | ---------------------- |
// | `Created`                      | any                  | `Created`              |
// | `WaitingForPayment`            | any                  | `WaitingForPayment`    |
// | `Funded`, `AwaitingFunds`      | any                  | `Funded`               |
// | `Claimed`                      | any                  | `Claimed`              |
// | `Canceled { Timeout }`         | any                  | `Expired`              |
// | `Canceled { ClaimRejected }`   | any                  | `Funded`, then reclaim |
// | `Canceled { InvalidPreimage }` | any                  | `Failed`               |
// | `Canceled { Rejected }`        | before `Funded`      | `Canceled`             |
// | `Canceled { Rejected }`        | at or after `Funded` | `Failed`               |
pub(super) fn map_receive(state: &UpstreamReceiveState, funded_before: bool) -> ReceiveStep {
    ReceiveStep::State(match state {
        UpstreamReceiveState::Created => LnReceiveState::Created,
        UpstreamReceiveState::WaitingForPayment { .. } => LnReceiveState::WaitingForPayment,
        UpstreamReceiveState::Funded | UpstreamReceiveState::AwaitingFunds => {
            LnReceiveState::Funded
        }
        UpstreamReceiveState::Claimed => LnReceiveState::Claimed,
        UpstreamReceiveState::Canceled { reason } => match reason {
            LightningReceiveError::Timeout => LnReceiveState::Expired,
            LightningReceiveError::ClaimRejected => return ReceiveStep::Reclaim,
            LightningReceiveError::InvalidPreimage => LnReceiveState::Failed,
            LightningReceiveError::Rejected if funded_before => LnReceiveState::Failed,
            LightningReceiveError::Rejected => LnReceiveState::Canceled {
                reason: reason.to_string(),
            },
        },
    })
}

// A retried claim runs as its own upstream operation, started in the confirmed-invoice state:
// the money is in the contract, so everything short of the end is `Funded`, and any cancellation
// of the retry is the end of the road.
pub(super) fn map_reclaim(state: &UpstreamReceiveState) -> LnReceiveState {
    match state {
        UpstreamReceiveState::Claimed => LnReceiveState::Claimed,
        UpstreamReceiveState::Canceled { .. } => LnReceiveState::Failed,
        UpstreamReceiveState::Created
        | UpstreamReceiveState::WaitingForPayment { .. }
        | UpstreamReceiveState::Funded
        | UpstreamReceiveState::AwaitingFunds => LnReceiveState::Funded,
    }
}

/// What a receive subscription needs to retry a rejected claim from inside its own stream.
///
/// A stream outlives the borrow it was made from, so it cannot hold a client guard; it holds the
/// instance and the federation id instead and takes a guard again when the retry happens.
struct ReclaimContext {
    sdk: Weak<SdkInner>,
    federation_id: fedimint_core::config::FederationId,
    db: Database,
    id: OperationId,
}

/// How starting a retry ended: a stream to follow, or the module's word that no retry is
/// possible.
enum ReclaimStart {
    Started(BoxStream<'static, UpstreamReceiveState>),
    Impossible(String),
}

impl ReclaimContext {
    /// Starts the retry upstream, records which operation it runs under, and returns its
    /// stream — or the module's word that no further claim is possible.
    async fn start(&self) -> Result<ReclaimStart> {
        use fedimint_core::db::IDatabaseTransactionOpsCoreTyped;

        let closed = || {
            Error::new(
                ErrorCode::FederationClosed,
                "this federation stopped running",
            )
        };
        let sdk = self.sdk.upgrade().ok_or_else(closed)?;
        let federation = sdk
            .federation_inner(&self.federation_id)
            .ok_or_else(closed)?;
        let client = federation.client(false).await?;
        let module = module_of(&client)?;

        // Re-read the record rather than trust details carried from where this context was
        // built: two subscribers can observe the same `Canceled { ClaimRejected }` and both
        // reach here. Reading again right before deciding lets whichever loses that race see the
        // winner's `reclaim_operation_id` and follow it instead of starting a second retry.
        let mut dbtx = self.db.begin_transaction_nc().await;
        let record = dbtx
            .get_value(&crate::db::OperationRecordKey(self.id))
            .await
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Internal,
                    format!("no record for operation {}", self.id.fmt_full()),
                )
            })?;
        let mut details = wire::decode_receive_wire(&record.details)?;
        if let Some(reclaim) = details.reclaim_operation_id.as_deref() {
            let reclaim_id = reclaim
                .parse::<crate::OperationId>()
                .map_err(|err| {
                    Error::new(
                        ErrorCode::Internal,
                        format!("a stored operation id does not parse: {err}"),
                    )
                })?
                .upstream();
            return Ok(ReclaimStart::Started(
                module
                    .subscribe_ln_receive(reclaim_id)
                    .await
                    .map_err(subscribe_error)?
                    .into_stream(),
            ));
        }

        let reclaim_id = match module.reclaim_ln_receive(self.id).await {
            Ok(id) => id,
            Err(err) => {
                return classify_reclaim_refusal(&err.to_string()).map(ReclaimStart::Impossible);
            }
        };
        // Persisted before it is followed: after a restart the record is the only thing that
        // says which upstream operation to follow.
        details.reclaim_operation_id =
            Some(crate::OperationId::from_upstream(reclaim_id).to_string());
        write_details_in(&self.db, self.id, wire::encode_receive_wire(&details)?).await?;
        Ok(ReclaimStart::Started(
            module
                .subscribe_ln_receive(reclaim_id)
                .await
                .map_err(subscribe_error)?
                .into_stream(),
        ))
    }
}

// Upstream refuses `reclaim_ln_receive` two ways. "Cannot reclaim an active lightning receive"
// (fedimint-ln-client/src/lib.rs, in `reclaim_ln_receive`) is the race between its notifier
// reporting `Canceled { ClaimRejected }` and the state machine itself going inactive: retrying
// later succeeds, so this is an observation failure, not a definitive answer. Any other refusal
// (not a reclaimable receive, the receiving key unrecoverable from history) is definitive: no
// further claim is possible.
fn classify_reclaim_refusal(text: &str) -> Result<String> {
    if text.contains("active") {
        return Err(Error::new(
            ErrorCode::Internal,
            format!("the rejected claim cannot be retried yet: {text}"),
        ));
    }
    Ok(text.to_owned())
}

/// Which upstream operation a receive subscription is following.
enum Following {
    Original,
    Reclaim,
}

/// The per-subscription cursor of a v1 receive.
struct Follow {
    upstream: BoxStream<'static, UpstreamReceiveState>,
    following: Following,
    phase: u32,
    done: bool,
    context: ReclaimContext,
}

/// One step of the receive stream: the next mapped state, having persisted the phase it proves
/// and switched to the retried claim if one was needed.
async fn step(mut follow: Follow) -> Option<(Result<LnReceiveState>, Follow)> {
    if follow.done {
        return None;
    }
    let state = follow.upstream.next().await?;
    let mapped = match follow.following {
        Following::Reclaim => map_reclaim(&state),
        Following::Original => match map_receive(&state, follow.phase >= PHASE_FUNDED) {
            ReceiveStep::State(state) => state,
            // `ClaimRejected` is not final here: the claim is retried under the same SDK
            // operation, and only a retry that cannot be started is the end.
            ReceiveStep::Reclaim => match follow.context.start().await {
                Ok(ReclaimStart::Started(upstream)) => {
                    follow.upstream = upstream;
                    follow.following = Following::Reclaim;
                    LnReceiveState::Funded
                }
                // The module says no further claim is possible: that is the one ending the
                // contract reserves `Failed` for.
                Ok(ReclaimStart::Impossible(_)) => LnReceiveState::Failed,
                // Anything else is an observation failure, not an outcome: the next subscription
                // replays the rejection and tries again.
                Err(err) => {
                    follow.done = true;
                    return Some((Err(err), follow));
                }
            },
        },
    };
    if matches!(mapped, LnReceiveState::Funded) && follow.phase < PHASE_FUNDED {
        follow.phase = PHASE_FUNDED;
        if let Err(err) = record_phase_in(&follow.context.db, follow.context.id, PHASE_FUNDED).await
        {
            follow.done = true;
            return Some((Err(err), follow));
        }
    }
    follow.done = mapped.is_final();
    Some((Ok(mapped), follow))
}

/// A fresh stream over a v1 receive, following the retried claim instead of the original when
/// the record says one was started.
pub(super) async fn subscribe_receive(
    federation: &FederationInner,
    id: OperationId,
    phase: u32,
    details_json: &str,
) -> Result<BoxStream<'static, Result<LnReceiveState>>> {
    let details = wire::decode_receive_wire(details_json)?;
    let client = federation.client(false).await?;
    let module = module_of(&client)?;
    let (upstream, following) = match details.reclaim_operation_id.as_deref() {
        Some(reclaim) => {
            let reclaim_id = reclaim
                .parse::<crate::OperationId>()
                .map_err(|err| {
                    Error::new(
                        ErrorCode::Internal,
                        format!("a stored operation id does not parse: {err}"),
                    )
                })?
                .upstream();
            let upstream = module
                .subscribe_ln_receive(reclaim_id)
                .await
                .map_err(subscribe_error)?
                .into_stream();
            (upstream, Following::Reclaim)
        }
        None => {
            let upstream = module
                .subscribe_ln_receive(id)
                .await
                .map_err(subscribe_error)?
                .into_stream();
            (upstream, Following::Original)
        }
    };
    let follow = Follow {
        upstream,
        following,
        phase,
        done: false,
        context: ReclaimContext {
            sdk: federation.sdk.clone(),
            federation_id: federation.id,
            db: federation.db(),
            id,
        },
    };
    Ok(Box::pin(stream::unfold(follow, step)))
}

#[cfg(test)]
mod tests {
    use fedimint_ln_client::pay::GatewayPayError;

    use super::*;
    use crate::Amount;

    fn fee() -> Amount {
        Amount::from_msats(1_050)
    }

    fn gateway_route() -> LightningRoute {
        LightningRoute::Gateway {
            gateway_id: "0218845781f631c48f1c9709e23092067d06837f30aa0cd0544ac887fe91ddd166"
                .parse()
                .expect("a gateway id"),
        }
    }

    #[test]
    fn ln_pay_states_fold_onto_the_send_lifecycle() {
        let cases = [
            (LnPayState::Created, LnSendState::Created),
            (LnPayState::Funded { block_height: 7 }, LnSendState::Funded),
            (LnPayState::AwaitingChange, LnSendState::Funded),
            (
                LnPayState::WaitingForRefund {
                    error_reason: "no route".to_owned(),
                },
                LnSendState::Funded,
            ),
            (LnPayState::Canceled, LnSendState::Refunded),
            (
                LnPayState::Refunded {
                    gateway_error: GatewayPayError::OutgoingContractError,
                },
                LnSendState::Refunded,
            ),
            (
                LnPayState::UnexpectedError {
                    error_message: "boom".to_owned(),
                },
                LnSendState::Failed {
                    reason: "boom".to_owned(),
                },
            ),
        ];
        for (upstream, expected) in cases {
            assert_eq!(
                map_ln_pay(&upstream, fee(), &gateway_route()).expect("maps"),
                expected,
                "{upstream:?}"
            );
        }
    }

    #[test]
    fn a_v1_success_parses_the_hex_preimage_and_carries_the_quoted_terms() {
        let mapped = map_ln_pay(
            &LnPayState::Success {
                preimage: "11".repeat(32),
            },
            fee(),
            &gateway_route(),
        )
        .expect("maps");
        assert_eq!(
            mapped,
            LnSendState::Success {
                preimage: Preimage::from_bytes([0x11; 32]),
                fee: fee(),
                route: gateway_route(),
            }
        );
    }

    #[test]
    fn a_v1_success_with_an_unparsable_preimage_is_internal() {
        let err = map_ln_pay(
            &LnPayState::Success {
                preimage: "not hex".to_owned(),
            },
            fee(),
            &gateway_route(),
        )
        .expect_err("refused");
        assert_eq!(err.code, ErrorCode::Internal);
    }

    #[test]
    fn internal_pay_states_fold_onto_the_send_lifecycle() {
        use fedimint_ln_client::incoming::IncomingSmError;

        let error = IncomingSmError::TimeoutFetchingOffer {
            payment_hash: fedimint_core::bitcoin::hashes::Hash::hash(b"x"),
        };
        let cases = [
            (InternalPayState::Funding, LnSendState::Created),
            (
                InternalPayState::Preimage(fedimint_ln_common::contracts::Preimage([0x22; 32])),
                LnSendState::Success {
                    preimage: Preimage::from_bytes([0x22; 32]),
                    fee: fee(),
                    route: LightningRoute::Internal,
                },
            ),
            (
                InternalPayState::RefundSuccess {
                    out_points: Vec::new(),
                    error: error.clone(),
                },
                LnSendState::Refunded,
            ),
            (
                InternalPayState::FundingFailed {
                    error: error.clone(),
                },
                LnSendState::Refunded,
            ),
            (
                InternalPayState::RefundError {
                    error_message: "stuck".to_owned(),
                    error,
                },
                LnSendState::Failed {
                    reason: "stuck".to_owned(),
                },
            ),
            (
                InternalPayState::UnexpectedError("odd".to_owned()),
                LnSendState::Failed {
                    reason: "odd".to_owned(),
                },
            ),
        ];
        for (upstream, expected) in cases {
            assert_eq!(
                map_internal_pay(&upstream, fee(), &LightningRoute::Internal),
                expected,
                "{upstream:?}"
            );
        }
    }

    #[test]
    fn receive_states_fold_onto_the_receive_lifecycle() {
        use fedimint_ln_client::receive::LightningReceiveError;

        let waiting = UpstreamReceiveState::WaitingForPayment {
            invoice: String::new(),
            timeout: core::time::Duration::from_secs(1),
        };
        let cases = [
            (
                UpstreamReceiveState::Created,
                false,
                LnReceiveState::Created,
            ),
            (waiting, false, LnReceiveState::WaitingForPayment),
            (UpstreamReceiveState::Funded, false, LnReceiveState::Funded),
            (
                UpstreamReceiveState::AwaitingFunds,
                true,
                LnReceiveState::Funded,
            ),
            (UpstreamReceiveState::Claimed, true, LnReceiveState::Claimed),
            (
                UpstreamReceiveState::Canceled {
                    reason: LightningReceiveError::Timeout,
                },
                true,
                LnReceiveState::Expired,
            ),
            (
                UpstreamReceiveState::Canceled {
                    reason: LightningReceiveError::InvalidPreimage,
                },
                false,
                LnReceiveState::Failed,
            ),
            (
                UpstreamReceiveState::Canceled {
                    reason: LightningReceiveError::Rejected,
                },
                false,
                LnReceiveState::Canceled {
                    reason: LightningReceiveError::Rejected.to_string(),
                },
            ),
            (
                UpstreamReceiveState::Canceled {
                    reason: LightningReceiveError::Rejected,
                },
                true,
                LnReceiveState::Failed,
            ),
        ];
        for (upstream, funded_before, expected) in cases {
            match map_receive(&upstream, funded_before) {
                ReceiveStep::State(state) => assert_eq!(state, expected, "{upstream:?}"),
                ReceiveStep::Reclaim => panic!("{upstream:?} must not ask for a reclaim"),
            }
        }
    }

    #[test]
    fn a_rejected_claim_asks_for_a_retry_whatever_the_phase() {
        use fedimint_ln_client::receive::LightningReceiveError;

        for funded_before in [false, true] {
            let step = map_receive(
                &UpstreamReceiveState::Canceled {
                    reason: LightningReceiveError::ClaimRejected,
                },
                funded_before,
            );
            assert!(matches!(step, ReceiveStep::Reclaim));
        }
    }

    #[test]
    fn a_retried_claim_is_funded_until_it_is_claimed_or_fails() {
        use fedimint_ln_client::receive::LightningReceiveError;

        assert_eq!(
            map_reclaim(&UpstreamReceiveState::Created),
            LnReceiveState::Funded
        );
        assert_eq!(
            map_reclaim(&UpstreamReceiveState::Funded),
            LnReceiveState::Funded
        );
        assert_eq!(
            map_reclaim(&UpstreamReceiveState::AwaitingFunds),
            LnReceiveState::Funded
        );
        assert_eq!(
            map_reclaim(&UpstreamReceiveState::Claimed),
            LnReceiveState::Claimed
        );
        for reason in [
            LightningReceiveError::Rejected,
            LightningReceiveError::Timeout,
            LightningReceiveError::ClaimRejected,
            LightningReceiveError::InvalidPreimage,
        ] {
            assert_eq!(
                map_reclaim(&UpstreamReceiveState::Canceled { reason }),
                LnReceiveState::Failed
            );
        }
    }

    #[test]
    fn the_notifiers_active_receive_race_is_retryable() {
        let err = classify_reclaim_refusal("Cannot reclaim an active lightning receive")
            .expect_err("the race is retryable, not definitive");
        assert_eq!(err.code, ErrorCode::Internal);
    }

    #[test]
    fn any_other_refusal_is_definitive() {
        let text = classify_reclaim_refusal("Operation is not a reclaimable lightning receive")
            .expect("a refusal that is not the race is definitive");
        assert_eq!(text, "Operation is not a reclaimable lightning receive");
    }
}
