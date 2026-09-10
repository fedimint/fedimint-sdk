//! The two lightning drivers, the backfiller, and the stream helpers they share.

use core::time::Duration;
use std::any::Any;

use fedimint_core::core::OperationId;
use fedimint_core::task::MaybeSend;
use fedimint_core::util::{BoxFuture, BoxStream};
use futures::{Stream, StreamExt, future};

use super::{v1, v2, wire};
use crate::db::OperationRecord;
use crate::federation::FederationInner;
use crate::operation::{Backfilled, Backfiller, Driver};
use crate::{Error, ErrorCode, LnReceiveState, LnSendState, OperationState, Result};

/// How long `current` waits for the next replayed state before calling the last one current.
///
/// Neither upstream module offers a "current state" read: v1's `subscribe_*` are generators that
/// re-run from the first state and resolve each already-passed stage immediately, and lnv2's
/// notifier replays the stored states before streaming new ones. Draining with a short wait per
/// item is how a point-in-time answer is produced from that.
const CURRENT_STATE_SETTLE: Duration = Duration::from_millis(500);

/// Ends a stream after its first final state, and on the first error.
pub(super) fn until_final<S>(
    stream: impl Stream<Item = Result<S>> + MaybeSend + 'static,
) -> BoxStream<'static, Result<S>>
where
    S: OperationState,
{
    Box::pin(stream.scan(false, |done, item| {
        if *done {
            return future::ready(None);
        }
        *done = match &item {
            Ok(state) => state.is_final(),
            Err(_) => true,
        };
        future::ready(Some(item))
    }))
}

/// A subscription that yields the current state first: the replayed history is drained until it
/// settles, the last state it produced is yielded, and every state after that is forwarded as it
/// comes.
///
/// If the inner stream ends before producing anything, this ends too, without yielding: that is
/// not an "empty" current state, it is the absence of one, and `OperationUpdates::next`'s
/// `current` fallback is what turns it into an answer. If the yielded state is final (or an
/// error), nothing more is drained for it; the underlying stream is expected to end right after,
/// per `Driver::subscribe`'s contract, so the next pull simply observes that.
pub(super) fn settled<S>(stream: BoxStream<'static, Result<S>>) -> BoxStream<'static, Result<S>>
where
    S: OperationState,
{
    Box::pin(futures::stream::unfold(
        (stream, false),
        |(mut stream, started)| async move {
            if started {
                return stream.next().await.map(|item| (item, (stream, true)));
            }
            // The first item is awaited without a timeout: both generations yield it promptly,
            // and the engine already races every wait in this call against the federation's
            // `closed` watch. Every item after that is drained with the same per-item timeout,
            // stopping at the first final state, the first error, or the first timeout.
            let mut last = stream.next().await?;
            while matches!(&last, Ok(state) if !state.is_final()) {
                match fedimint_core::runtime::timeout(CURRENT_STATE_SETTLE, stream.next()).await {
                    Ok(Some(item)) => last = item,
                    Ok(None) | Err(_) => break,
                }
            }
            Some((last, (stream, true)))
        },
    ))
}

/// The first state a stream yields, mapping an ended stream to this operation's "no state" error.
///
/// `Driver::subscribe` already returns a stream wrapped in [`settled`], so this drains it once
/// rather than draining it a second time the way calling [`settled`] again over it would.
pub(super) async fn first_state<S>(mut stream: BoxStream<'static, Result<S>>) -> Result<S>
where
    S: OperationState,
{
    match stream.next().await {
        Some(item) => item,
        None => Err(Error::new(
            ErrorCode::Internal,
            "this operation's subscription yielded no state",
        )),
    }
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

    fn decode_details(&self, json: &str) -> Result<Box<dyn Any + Send + Sync>> {
        Ok(Box::new(wire::decode_receive_details(json)?))
    }
}

/// Rebuilds a lightning record from the module's own log entry, for either generation.
pub(crate) struct LnBackfiller;

impl Backfiller for LnBackfiller {
    fn backfill(
        &self,
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
    use futures::stream;
    use tokio::sync::oneshot;

    use super::*;

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
}
