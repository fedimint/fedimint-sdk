//! The two lightning drivers, the backfiller, and the stream helpers they share.

use core::time::Duration;

use fedimint_core::task::MaybeSend;
use fedimint_core::util::BoxStream;
use futures::{Stream, StreamExt, future};

use crate::{Error, ErrorCode, OperationState, Result};

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
