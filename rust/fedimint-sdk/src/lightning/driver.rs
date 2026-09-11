//! The two lightning drivers, the backfiller, and the stream helpers they share.

use std::any::Any;

use fedimint_core::core::OperationId;
use fedimint_core::util::{BoxFuture, BoxStream};

use super::{v1, v2, wire};
use crate::db::OperationRecord;
use crate::federation::FederationInner;
use crate::operation::{Backfilled, Backfiller, Driver};
use crate::{Error, ErrorCode, LnReceiveState, LnSendState, Result};

pub(super) use crate::operation::{first_state, settled, until_final};


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

    use futures::{StreamExt as _, stream};
    use tokio::sync::oneshot;

    use super::*;
    use crate::{Amount, LightningRoute, Preimage};

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
