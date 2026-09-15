//! The two lightning drivers, the backfiller, and the stream helpers they share.

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
use crate::operation::{Backfilled, Backfiller, Driver};
use crate::sdk::SdkInner;
use crate::{Error, ErrorCode, LnReceiveState, LnSendState, Result};

pub(super) use crate::operation::{first_state, settled, until_final};

/// What one upstream send state means for this SDK's own send lifecycle.
///
/// Both generations map most of their states straight across. The second arm is why this is a
/// type rather than an `LnSendState`: upstream reports a rejected funding transaction as an
/// ending, and here it is not one. The value that transaction removed is recovered afterwards,
/// by a separate transaction that can itself fail, so the send stays non-final until that
/// settles and the ending is chosen from what it established. See [`crate::inputs`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SendStep {
    /// Hand this state out as it is.
    State(LnSendState),
    /// The funding transaction was rejected. Report the send as still running, settle the
    /// inputs it removed, then end on what that proves: [`LnSendState::Refunded`] when the
    /// value is spendable again, and [`LnSendState::Failed`] when a clean return cannot be
    /// established. One step, two states out; see [`through_settle`] for why the first of them
    /// cannot wait on the second.
    FundingRejected,
}

/// Turns a stream of steps into one of states, settling a rejected funding before ending.
///
/// A rejection becomes **two** items: the non-final state the operation is actually in, and
/// then, once the settle finishes, the ending it established. Yielding the first before
/// waiting is load-bearing rather than cosmetic. Upstream hands back a single cached outcome
/// for an operation whose ending it has already recorded (`ClientContext::outcome_or_updates`),
/// so a send reattached after a restart can have the rejection as the *only* step there is;
/// and `settled`, which every subscription and every `current` goes through, awaits its first
/// item with no timeout. A stream that waited for the settle before yielding anything would
/// hang `Operation::state()` and a new subscriber's first update for as long as the recovery
/// ran, which on a stalled recovery is for ever.
///
/// `Created` is the state to report there: the funding was rejected, so the payment never
/// reached `Funded`, and it is still running while its inputs are recovered. On a live stream
/// that repeats the `Created` the payment already reported, which the engine's `same_state`
/// dedup drops.
///
/// `sdk` and `federation_id` are carried rather than a federation or a client, because the
/// stream outlives the call that built it.
//
// `unfold` rather than `flat_map` over boxed sub-streams: boxing a stream needs `Send`, which
// this crate cannot require (`MaybeSend`, `wasm32`), and the state machine here is small
// enough that spelling it out costs less than working around that.
pub(super) fn through_settle(
    stream: impl futures::Stream<Item = Result<SendStep>> + MaybeSend + 'static,
    sdk: Weak<SdkInner>,
    federation_id: FederationId,
    id: OperationId,
) -> BoxStream<'static, Result<LnSendState>> {
    Box::pin(futures::stream::unfold(
        (Box::pin(stream), sdk, false),
        move |(mut steps, sdk, settle_next)| async move {
            if settle_next {
                let ending = match crate::inputs::settle(sdk.clone(), federation_id, id).await {
                    Ok(Restoration::Restored) => Ok(LnSendState::Refunded),
                    Ok(Restoration::Unproven(reason)) => Ok(LnSendState::Failed { reason }),
                    Err(err) => Err(err),
                };
                return Some((ending, (steps, sdk, false)));
            }
            let item = match steps.next().await? {
                Err(err) => Err(err),
                Ok(SendStep::State(state)) => Ok(state),
                // The pending state now, the ending on the next pull.
                Ok(SendStep::FundingRejected) => {
                    return Some((Ok(LnSendState::Created), (steps, sdk, true)));
                }
            };
            Some((item, (steps, sdk, false)))
        },
    ))
}

/// Observes an outgoing lightning payment of either generation, chosen by the record's module.
pub(crate) struct LnSendDriver;

impl Driver<LnSendState> for LnSendDriver {
    fn current<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<LnSendState>> {
        Box::pin(async move {
            if let Some(encoded) = &record.final_state {
                return wire::decode_send_state(encoded);
            }
            first_state(self.subscribe(federation, id, record).await?).await
        })
    }

    fn subscribe<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<LnSendState>>>> {
        Box::pin(async move {
            let details = wire::decode_send_details(&record.details)?;
            let stream = match record.module.as_str() {
                "ln" => v1::subscribe_send(federation, id, &details).await,
                "lnv2" => {
                    v2::subscribe_send(federation, id, record.phase.unwrap_or(0), &details).await
                }
                other => Err(unknown_module(other)),
            }?;
            Ok(settled(stream))
        })
    }

    fn same_state(&self, previous: &LnSendState, next: &LnSendState) -> bool {
        previous == next
    }

    fn encode_state(&self, state: &LnSendState) -> Result<String> {
        wire::encode_send_state(state)
    }

    fn decode_state(&self, encoded: &str) -> Result<LnSendState> {
        wire::decode_send_state(encoded)
    }

    fn decode_details(&self, json: &str) -> Result<Box<dyn Any + Send + Sync>> {
        Ok(Box::new(wire::decode_send_details(json)?))
    }
}

/// Observes an incoming lightning payment of either generation.
pub(crate) struct LnReceiveDriver;

impl Driver<LnReceiveState> for LnReceiveDriver {
    fn current<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<LnReceiveState>> {
        Box::pin(async move {
            if let Some(encoded) = &record.final_state {
                return wire::decode_receive_state(encoded);
            }
            first_state(self.subscribe(federation, id, record).await?).await
        })
    }

    fn subscribe<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<LnReceiveState>>>> {
        Box::pin(async move {
            let phase = record.phase.unwrap_or(0);
            let stream = match record.module.as_str() {
                "ln" => v1::subscribe_receive(federation, id, phase, &record.details).await,
                "lnv2" => v2::subscribe_receive(federation, id, phase).await,
                other => Err(unknown_module(other)),
            }?;
            Ok(settled(stream))
        })
    }

    fn same_state(&self, previous: &LnReceiveState, next: &LnReceiveState) -> bool {
        previous == next
    }

    fn encode_state(&self, state: &LnReceiveState) -> Result<String> {
        wire::encode_receive_state(state)
    }

    fn decode_state(&self, encoded: &str) -> Result<LnReceiveState> {
        wire::decode_receive_state(encoded)
    }

    fn decode_details(&self, json: &str) -> Result<Box<dyn Any + Send + Sync>> {
        Ok(Box::new(wire::decode_receive_details(json)?))
    }
}

/// Rebuilds a lightning record from the module's own log entry, for either generation.
pub(crate) struct LnBackfiller;

impl Backfiller for LnBackfiller {
    fn backfill(
        &self,
        _id: OperationId,
        module_kind: &str,
        meta: &serde_json::Value,
        created_at: u64,
    ) -> Option<Backfilled> {
        match module_kind {
            "ln" => v1::backfill(meta, created_at),
            "lnv2" => v2::backfill(meta, created_at),
            _ => None,
        }
    }
}

fn unknown_module(module: &str) -> Error {
    Error::new(
        ErrorCode::Internal,
        format!("a lightning record names a module this build cannot observe: {module:?}"),
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::stream;
    use tokio::sync::oneshot;

    use super::*;
    use crate::{Amount, LightningRoute, OperationState as _, Preimage};

    // Only `LnSendState`'s non-final (`Created`, `Funded`) and final (`Refunded`, `Failed`)
    // variants that need no fields are used below; `settled` treats every state the same way
    // regardless of which state enum it is instantiated with.

    #[tokio::test(flavor = "multi_thread")]
    async fn late_subscriber_sees_settled_state_first() {
        let stream: BoxStream<'static, Result<LnSendState>> = Box::pin(
            stream::iter([Ok(LnSendState::Created), Ok(LnSendState::Funded)])
                .chain(stream::pending()),
        );
        let mut settled_stream = settled(stream);

        let first = settled_stream.next().await;
        assert_eq!(
            first.expect("stream ended").expect("stream errored"),
            LnSendState::Funded
        );

        // The replayed history is exhausted and the tail is still pending, so nothing more
        // should arrive within a settle window.
        let second = tokio::time::timeout(Duration::from_millis(50), settled_stream.next()).await;
        assert!(
            second.is_err(),
            "a second item arrived when none should have"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn transitions_after_the_first_item_are_forwarded() {
        let (tx, rx) = oneshot::channel::<()>();
        let gated = stream::once(async move {
            rx.await.expect("gate sender dropped");
            Ok(LnSendState::Funded)
        });
        let stream: BoxStream<'static, Result<LnSendState>> = Box::pin(
            stream::iter([Ok(LnSendState::Created)])
                .chain(gated)
                .chain(stream::iter([Ok(LnSendState::Refunded)])),
        );
        let mut settled_stream = settled(stream);

        // The settle window elapses waiting for the gated state, so the first pull settles on
        // `Created`.
        let first = settled_stream.next().await;
        assert_eq!(
            first.expect("stream ended").expect("stream errored"),
            LnSendState::Created
        );

        tx.send(()).expect("gate receiver dropped");
        let second = settled_stream.next().await;
        assert_eq!(
            second.expect("stream ended").expect("stream errored"),
            LnSendState::Funded
        );
        let third = settled_stream.next().await;
        assert_eq!(
            third.expect("stream ended").expect("stream errored"),
            LnSendState::Refunded
        );
        assert!(settled_stream.next().await.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn replay_ending_in_a_final_state_yields_only_that_state() {
        let stream: BoxStream<'static, Result<LnSendState>> = Box::pin(stream::iter([
            Ok(LnSendState::Created),
            Ok(LnSendState::Funded),
            Ok(LnSendState::Refunded),
        ]));
        let mut settled_stream = settled(stream);

        let first = settled_stream.next().await;
        assert_eq!(
            first.expect("stream ended").expect("stream errored"),
            LnSendState::Refunded
        );
        assert!(settled_stream.next().await.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn empty_stream_yields_nothing() {
        let stream: BoxStream<'static, Result<LnSendState>> = Box::pin(stream::empty());
        let mut settled_stream = settled(stream);
        assert!(settled_stream.next().await.is_none());

        // `first_state` (what `current` calls on `subscribe`'s stream) turns that into its usual
        // "no state" error.
        let stream: BoxStream<'static, Result<LnSendState>> = Box::pin(stream::empty());
        let err = first_state(stream)
            .await
            .expect_err("an empty stream must not settle");
        assert_eq!(err.code, ErrorCode::Internal);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_state_step_passes_straight_through() {
        let steps: BoxStream<'static, Result<SendStep>> = Box::pin(stream::iter([
            Ok(SendStep::State(LnSendState::Created)),
            Ok(SendStep::State(LnSendState::Funded)),
        ]));
        let mut stream = through_settle(steps, Weak::new(), a_federation_id(), an_operation_id());

        assert_eq!(
            stream.next().await.expect("a state").expect("not an error"),
            LnSendState::Created
        );
        assert_eq!(
            stream.next().await.expect("a state").expect("not an error"),
            LnSendState::Funded
        );
        assert!(stream.next().await.is_none());
    }

    /// The regression this whole path exists for: a rejected funding must reach the settle gate
    /// rather than being handed out as an ending. The instance is gone here, so the gate cannot
    /// run and reports the federation closed, which is still proof the step went to the gate.
    /// Mapping `FundingRejected` back onto `LnSendState::Refunded` would yield a state instead.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_rejected_funding_goes_to_the_gate_rather_than_ending_the_send() {
        let steps: BoxStream<'static, Result<SendStep>> = Box::pin(stream::iter([
            Ok(SendStep::State(LnSendState::Created)),
            Ok(SendStep::FundingRejected),
        ]));
        let mut stream = through_settle(steps, Weak::new(), a_federation_id(), an_operation_id());

        assert_eq!(
            stream.next().await.expect("a state").expect("not an error"),
            LnSendState::Created
        );
        // The rejection's own pending state, yielded before the gate is waited on at all.
        assert_eq!(
            stream.next().await.expect("a state").expect("not an error"),
            LnSendState::Created
        );
        let err = stream
            .next()
            .await
            .expect("the rejection produces an ending")
            .expect_err("the gate cannot run without an instance");
        assert_eq!(err.code, ErrorCode::FederationClosed);
    }

    /// Reattaching to an operation whose rejection upstream has already cached: the whole step
    /// stream is the rejection, with no earlier state to fall back on. The first item still has
    /// to be a non-final state, because `settled` awaits its first item without a timeout and
    /// every `Operation::state()` and new subscription goes through that. A stream that waited
    /// for the settle before yielding would hang them for as long as the recovery ran.
    ///
    /// The ordering is the testable half of that. How long the gate itself takes cannot be
    /// exercised here, because with no instance behind the `Weak` it fails at once instead of
    /// waiting; what this pins down is that the pending state comes first regardless.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_cached_rejection_reports_pending_before_waiting() {
        let steps: BoxStream<'static, Result<SendStep>> =
            Box::pin(stream::iter([Ok(SendStep::FundingRejected)]));
        let mut stream = through_settle(steps, Weak::new(), a_federation_id(), an_operation_id());

        let first = stream
            .next()
            .await
            .expect("a cached rejection still reports where the operation is")
            .expect("not an error");
        assert_eq!(first, LnSendState::Created);
        assert!(
            !first.is_final(),
            "a send whose inputs are still being recovered was reported as finished"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_error_step_is_forwarded_unchanged() {
        let steps: BoxStream<'static, Result<SendStep>> = Box::pin(stream::iter([Err(
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

    fn a_federation_id() -> FederationId {
        FederationId::dummy()
    }

    fn an_operation_id() -> OperationId {
        OperationId([0x11; 32])
    }

    #[test]
    fn send_driver_decodes_what_it_encodes() {
        let driver = LnSendDriver;
        let preimage: Preimage = "11".repeat(32).parse().expect("a preimage");
        for state in [
            LnSendState::Created,
            LnSendState::Funded,
            LnSendState::Success {
                preimage,
                fee: Amount::from_msats(1_050),
                route: LightningRoute::Internal,
            },
            LnSendState::Refunded,
            LnSendState::Failed {
                reason: "gone".to_owned(),
            },
        ] {
            let encoded = driver.encode_state(&state).expect("encode");
            assert_eq!(driver.decode_state(&encoded).expect("decode"), state);
        }
    }

    #[test]
    fn receive_driver_decodes_what_it_encodes() {
        let driver = LnReceiveDriver;
        for state in [
            LnReceiveState::Created,
            LnReceiveState::WaitingForPayment,
            LnReceiveState::Funded,
            LnReceiveState::Claimed,
            LnReceiveState::Canceled {
                reason: "withdrawn".to_owned(),
            },
            LnReceiveState::Expired,
            LnReceiveState::Failed,
        ] {
            let encoded = driver.encode_state(&state).expect("encode");
            assert_eq!(driver.decode_state(&encoded).expect("decode"), state);
        }
    }
}
