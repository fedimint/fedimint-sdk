//! The two on-chain drivers and the backfiller, chosen by the record's module.

use std::any::Any;
use std::sync::Weak;

use fedimint_core::config::FederationId;
use fedimint_core::core::OperationId;
use fedimint_core::task::MaybeSend;
use fedimint_core::util::{BoxFuture, BoxStream};
use futures::StreamExt as _;

use super::{v1, v2, wire};
use crate::db::OperationRecord;
use crate::federation::FederationInner;
use crate::inputs::Restoration;
use crate::operation::{Backfilled, Backfiller, Driver, first_state, settled};
use crate::sdk::SdkInner;
use crate::{Error, ErrorCode, OnchainReceiveState, OnchainSendState, Result};

/// What one upstream withdrawal state means for this SDK's own send lifecycle.
///
/// The counterpart of `lightning::driver::SendStep`, for the same reason: both wallet
/// generations report a rejected funding transaction as an ending, and here it is not one. The
/// value that transaction removed is recovered afterwards, by a separate transaction that can
/// itself fail, so the withdrawal stays non-final until that settles and the ending is chosen
/// from what it established. See [`crate::inputs`].
///
/// Unlike the lightning step this carries a reason, because a wallet module names its rejection
/// and [`OnchainSendState::Refunded`] has somewhere to put it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SendStep {
    /// Hand this state out as it is.
    State(OnchainSendState),
    /// The funding transaction was rejected, for this reason. Report the withdrawal as still
    /// running, settle the inputs it removed, then end on what that proves.
    FundingRejected { reason: String },
}

/// The ending a settled recovery establishes for a withdrawal whose funding was rejected.
///
/// The two reasons are about different things, and which one survives says which ending this
/// is. [`OnchainSendState::Refunded`] is the ordinary "rejected, try again" outcome, so it
/// carries `rejection`, the federation's own reason for refusing the funding — that is what the
/// withdrawal is reporting. [`OnchainSendState::Failed`] is the outcome that cannot say where
/// the funds are, so it carries the recovery's reason instead: what the withdrawal is reporting
/// there is the failure to establish a clean return, not the original refusal.
pub(super) fn ending_of(rejection: String, restoration: Restoration) -> OnchainSendState {
    match restoration {
        Restoration::Restored => OnchainSendState::Refunded { reason: rejection },
        Restoration::Unproven(reason) => OnchainSendState::Failed { reason },
    }
}

/// Turns a stream of withdrawal steps into one of states, settling a rejected funding first.
///
/// The mechanism, and the reasoning for the two items a rejection becomes, is
/// [`crate::inputs::through_settle`]; this names the two ends of it for an on-chain withdrawal.
///
/// [`OnchainSendState::Created`] is the pending state to report: the funding was rejected, so
/// the federation never went on to assemble a transaction, and the withdrawal is still running
/// while its inputs are recovered.
pub(super) fn through_settle(
    steps: impl futures::Stream<Item = Result<SendStep>> + MaybeSend + 'static,
    sdk: Weak<SdkInner>,
    federation_id: FederationId,
    id: OperationId,
) -> BoxStream<'static, Result<OnchainSendState>> {
    crate::inputs::through_settle(
        steps.map(|step| {
            step.map(|step| match step {
                SendStep::State(state) => crate::inputs::Step::State(state),
                SendStep::FundingRejected { reason } => {
                    crate::inputs::Step::FundingRejected(reason)
                }
            })
        }),
        sdk,
        federation_id,
        id,
        OnchainSendState::Created,
        ending_of,
    )
}

/// Observes an on-chain withdrawal of either wallet module generation, chosen by the record's
/// module.
pub(crate) struct OnchainSendDriver;

impl Driver<OnchainSendState> for OnchainSendDriver {
    fn current<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<OnchainSendState>> {
        Box::pin(async move {
            if let Some(encoded) = &record.final_state {
                return wire::decode_send_state(encoded);
            }
            match record.module.as_str() {
                "wallet" => first_state(self.subscribe(federation, id, record).await?).await,
                // walletv2 has no incremental subscription to settle: `current_send` is the
                // module's own bounded final-state await, already capped to the 500 ms rule.
                "walletv2" => v2::current_send(federation, id).await,
                other => Err(unknown_module(other)),
            }
        })
    }

    fn subscribe<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<OnchainSendState>>>> {
        Box::pin(async move {
            let stream = match record.module.as_str() {
                "wallet" => v1::subscribe_withdraw(federation, id).await,
                "walletv2" => v2::subscribe_send(federation, id).await,
                other => Err(unknown_module(other)),
            }?;
            Ok(settled(stream))
        })
    }

    fn same_state(&self, previous: &OnchainSendState, next: &OnchainSendState) -> bool {
        previous == next
    }

    fn encode_state(&self, state: &OnchainSendState) -> Result<String> {
        wire::encode_send_state(state)
    }

    fn decode_state(&self, encoded: &str) -> Result<OnchainSendState> {
        wire::decode_send_state(encoded)
    }

    fn decode_details(&self, json: &str) -> Result<Box<dyn Any + Send + Sync>> {
        Ok(Box::new(wire::decode_send_details(json)?))
    }
}

/// Observes an on-chain deposit of either wallet module generation.
pub(crate) struct OnchainReceiveDriver;

impl Driver<OnchainReceiveState> for OnchainReceiveDriver {
    fn current<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<OnchainReceiveState>> {
        Box::pin(async move {
            if let Some(encoded) = &record.final_state {
                return wire::decode_receive_state(encoded);
            }
            match record.module.as_str() {
                "wallet" => first_state(self.subscribe(federation, id, record).await?).await,
                "walletv2" => v2::current_receive(federation, id).await,
                other => Err(unknown_module(other)),
            }
        })
    }

    fn subscribe<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<OnchainReceiveState>>>> {
        Box::pin(async move {
            let stream = match record.module.as_str() {
                "wallet" => v1::subscribe_deposit(federation, id).await,
                "walletv2" => v2::subscribe_receive(federation, id).await,
                other => Err(unknown_module(other)),
            }?;
            Ok(settled(stream))
        })
    }

    fn same_state(&self, previous: &OnchainReceiveState, next: &OnchainReceiveState) -> bool {
        previous == next
    }

    fn encode_state(&self, state: &OnchainReceiveState) -> Result<String> {
        wire::encode_receive_state(state)
    }

    fn decode_state(&self, encoded: &str) -> Result<OnchainReceiveState> {
        wire::decode_receive_state(encoded)
    }

    fn decode_details(&self, json: &str) -> Result<Box<dyn Any + Send + Sync>> {
        Ok(Box::new(wire::decode_receive_details(json)?))
    }
}

/// Rebuilds an on-chain record from the wallet module's own log entry, for either generation.
pub(crate) struct OnchainBackfiller;

impl Backfiller for OnchainBackfiller {
    fn backfill(
        &self,
        id: OperationId,
        module_kind: &str,
        meta: &serde_json::Value,
        created_at: u64,
    ) -> Option<Backfilled> {
        match module_kind {
            "wallet" => v1::backfill(id, meta, created_at),
            "walletv2" => v2::backfill(id, meta, created_at),
            _ => None,
        }
    }
}

fn unknown_module(module: &str) -> Error {
    Error::new(
        ErrorCode::Internal,
        format!("a record names a wallet module this build does not know: {module:?}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::federation::FederationInner;
    use crate::operation::kinds;
    use crate::{Amount, OperationState as _, Sats, Txid};

    fn a_federation_id() -> FederationId {
        FederationId::dummy()
    }

    fn an_operation_id() -> OperationId {
        OperationId([0x11; 32])
    }

    fn a_rejection() -> String {
        "the federation rejected the funding transaction".to_owned()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_state_step_passes_straight_through() {
        let steps: BoxStream<'static, Result<SendStep>> = Box::pin(futures::stream::iter([
            Ok(SendStep::State(OnchainSendState::Created)),
            Ok(SendStep::State(OnchainSendState::Succeeded {
                txid: a_txid(),
            })),
        ]));
        let mut stream = through_settle(steps, Weak::new(), a_federation_id(), an_operation_id());

        assert_eq!(
            stream.next().await.expect("a state").expect("not an error"),
            OnchainSendState::Created
        );
        assert_eq!(
            stream.next().await.expect("a state").expect("not an error"),
            OnchainSendState::Succeeded { txid: a_txid() }
        );
        assert!(stream.next().await.is_none());
    }

    /// The regression this path exists for, on the wallet side: a rejected funding must reach
    /// the settle gate rather than being handed out as an ending. The instance is gone here, so
    /// the gate cannot run and reports the federation closed, which is still proof the step went
    /// to the gate. Mapping the rejection back onto `Refunded` would yield a state instead —
    /// and would persist it, since `Refunded` is final.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_rejected_funding_goes_to_the_gate_rather_than_ending_the_withdrawal() {
        let steps: BoxStream<'static, Result<SendStep>> = Box::pin(futures::stream::iter([
            Ok(SendStep::State(OnchainSendState::Created)),
            Ok(SendStep::FundingRejected {
                reason: a_rejection(),
            }),
        ]));
        let mut stream = through_settle(steps, Weak::new(), a_federation_id(), an_operation_id());

        assert_eq!(
            stream.next().await.expect("a state").expect("not an error"),
            OnchainSendState::Created
        );
        // The rejection's own pending state, yielded before the gate is waited on at all.
        assert_eq!(
            stream.next().await.expect("a state").expect("not an error"),
            OnchainSendState::Created
        );
        let err = stream
            .next()
            .await
            .expect("the rejection produces an ending")
            .expect_err("the gate cannot run without an instance");
        assert_eq!(err.code, ErrorCode::FederationClosed);
    }

    /// Reattaching to a withdrawal whose rejection upstream has already cached: the whole step
    /// stream is the rejection, with no earlier state to fall back on. The first item still has
    /// to be a non-final state, because `settled` awaits its first item without a timeout and
    /// every `Operation::state()` and new subscription goes through that.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_cached_rejection_reports_pending_before_waiting() {
        let steps: BoxStream<'static, Result<SendStep>> =
            Box::pin(futures::stream::iter([Ok(SendStep::FundingRejected {
                reason: a_rejection(),
            })]));
        let mut stream = through_settle(steps, Weak::new(), a_federation_id(), an_operation_id());

        let first = stream
            .next()
            .await
            .expect("a cached rejection still reports where the withdrawal is")
            .expect("not an error");
        assert_eq!(first, OnchainSendState::Created);
        assert!(
            !first.is_final(),
            "a withdrawal whose inputs are still being recovered was reported as finished"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_error_step_is_forwarded_unchanged() {
        let steps: BoxStream<'static, Result<SendStep>> = Box::pin(futures::stream::iter([Err(
            Error::new(ErrorCode::Internal, "upstream went wrong"),
        )]));
        let mut stream = through_settle(steps, Weak::new(), a_federation_id(), an_operation_id());

        let err = stream
            .next()
            .await
            .expect("an item")
            .expect_err("the error is forwarded");
        assert_eq!(err.code, ErrorCode::Internal);
        assert_eq!(err.message, "upstream went wrong");
    }

    /// The two endings a settled recovery can establish, and which reason each carries.
    #[test]
    fn a_settled_recovery_chooses_the_ending_and_the_reason_with_it() {
        // Established: the ordinary "rejected, try again" ending, reporting why the federation
        // refused the funding.
        assert_eq!(
            ending_of(a_rejection(), Restoration::Restored),
            OnchainSendState::Refunded {
                reason: a_rejection(),
            }
        );

        // Not established: the ending that cannot say where the funds are, reporting why the
        // return could not be proven rather than why the funding was refused.
        assert_eq!(
            ending_of(
                a_rejection(),
                Restoration::Unproven("a recovery output never issued its notes".to_owned()),
            ),
            OnchainSendState::Failed {
                reason: "a recovery output never issued its notes".to_owned(),
            }
        );
    }

    fn a_txid() -> Txid {
        "0000000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .expect("a well-formed transaction id")
    }

    #[test]
    fn send_driver_decodes_what_it_encodes() {
        let driver = OnchainSendDriver;
        for state in [
            OnchainSendState::Created,
            OnchainSendState::Succeeded { txid: a_txid() },
            OnchainSendState::Refunded {
                reason: "the federation rejected the funding transaction".to_owned(),
            },
            OnchainSendState::Failed {
                reason: "the funding was accepted but no transaction came of it".to_owned(),
            },
        ] {
            let encoded = driver.encode_state(&state).expect("encode");
            assert_eq!(driver.decode_state(&encoded).expect("decode"), state);
        }
    }

    #[test]
    fn receive_driver_decodes_what_it_encodes() {
        let driver = OnchainReceiveDriver;
        for state in [
            OnchainReceiveState::WaitingForTransaction,
            OnchainReceiveState::WaitingForConfirmation {
                txid: a_txid(),
                gross_deposited: Sats::from_sats(100_000),
            },
            OnchainReceiveState::Confirmed {
                txid: a_txid(),
                gross_deposited: Sats::from_sats(100_000),
            },
            OnchainReceiveState::Claimed {
                txid: a_txid(),
                gross_deposited: Sats::from_sats(100_000),
                net_credit: Amount::from_msats(99_998_500),
            },
            OnchainReceiveState::Failed {
                reason: "the deposit does not exceed the federation's deposit fee and was not \
                          claimed"
                    .to_owned(),
            },
        ] {
            let encoded = driver.encode_state(&state).expect("encode");
            assert_eq!(driver.decode_state(&encoded).expect("decode"), state);
        }
    }

    /// A record naming a wallet module this build does not know is `Internal` from `current`.
    /// The federation here is detached with no client at all, which is what proves the module
    /// check runs before the client is ever taken: were it the other way round, this would
    /// report `FederationClosed` instead.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_driver_on_an_unknown_module_reports_internal_from_current() {
        let db = crate::db::federation_namespace(&crate::db::in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db, false);
        let record = OperationRecord {
            schema_version: crate::operation::READABLE_STATE_SCHEMA,
            kind: kinds::ONCHAIN_RECEIVE.to_owned(),
            module: "made_up_wallet_module".to_owned(),
            created_at: 0,
            details: "{}".to_owned(),
            phase: None,
            cancel_requested_at: None,
            final_state: None,
        };
        let err = OnchainReceiveDriver
            .current(&federation, OperationId::new_random(), &record)
            .await
            .expect_err("an unknown module cannot be observed");
        assert_eq!(err.code, ErrorCode::Internal);
    }
}
