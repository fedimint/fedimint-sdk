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

/// The last state a fresh subscription yields promptly: the current one.
pub(super) async fn settle<S>(mut stream: BoxStream<'static, Result<S>>) -> Result<S>
where
    S: OperationState,
{
    let mut last = None;
    loop {
        match fedimint_core::runtime::timeout(CURRENT_STATE_SETTLE, stream.next()).await {
            Ok(Some(Ok(state))) => {
                let done = state.is_final();
                last = Some(state);
                if done {
                    break;
                }
            }
            Ok(Some(Err(err))) => return Err(err),
            Ok(None) | Err(_) => break,
        }
    }
    last.ok_or_else(|| {
        Error::new(
            ErrorCode::Internal,
            "this operation's subscription yielded no state",
        )
    })
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
            settle(self.subscribe(federation, id, record).await?).await
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
            match record.module.as_str() {
                "ln" => v1::subscribe_send(federation, id, &details).await,
                "lnv2" => {
                    v2::subscribe_send(federation, id, record.phase.unwrap_or(0), &details).await
                }
                other => Err(unknown_module(other)),
            }
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
            settle(self.subscribe(federation, id, record).await?).await
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
            match record.module.as_str() {
                "ln" => v1::subscribe_receive(federation, id, phase, &record.details).await,
                "lnv2" => v2::subscribe_receive(federation, id, phase).await,
                other => Err(unknown_module(other)),
            }
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
