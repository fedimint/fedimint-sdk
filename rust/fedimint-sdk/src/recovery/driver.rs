//! The recovery driver: reads one attempt's persisted state, and never asks the client.

use std::any::Any;

use fedimint_core::core::OperationId;
use fedimint_core::db::{Database, IDatabaseTransactionOpsCoreTyped};
use fedimint_core::util::{BoxFuture, BoxStream};

use super::wire;
use crate::db::{OperationRecord, OperationRecordKey};
use crate::federation::FederationInner;
use crate::operation::Driver;
use crate::{Error, ErrorCode, OperationState, RecoveryState, Result};

/// Observes a seed recovery attempt from its operation record alone.
///
/// Recovery is not a client operation upstream (see the module documentation), so unlike every
/// other driver in this crate, this one has no module call to make and no upstream state machine
/// to fold onto: the attempt's operation record, written by
/// [`FederationInner::record_recovery_attempt`] and completed by the watcher task the recovery
/// flow starts, is the entire source of truth.
pub(crate) struct RecoveryDriver;

impl Driver<RecoveryState> for RecoveryDriver {
    fn current<'a>(
        &'a self,
        _federation: &'a FederationInner,
        _id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<RecoveryState>> {
        Box::pin(async move { current_state(record) })
    }

    fn subscribe<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<RecoveryState>>>> {
        Box::pin(async move {
            // Subscribed to the watch before the record is read, and the record read fresh
            // rather than taken from the caller: a receiver only ever wakes for bumps made after
            // it was created, so an ending recorded and bumped between the caller's read of
            // `record` and this subscription would otherwise never be seen, and the stream
            // would wait for a second bump that never comes.
            let changed = federation.recovery_changed();
            let db = federation.db();
            let current = match reload_final_state(&db, id).await? {
                Some(state) => state,
                None => current_state(record)?,
            };
            // Every other driver's `subscribe` replays an upstream state machine and has to
            // reason about whether resubscribing after a lagged notifier could skip a
            // transition it already produced. This one cannot lose anything that way, because
            // there is no history to replay: the only thing a subscription ever reports is the
            // attempt's record as it reads right now, and every stream this method returns,
            // the first one or a fresh one opened after an earlier stream ended without a final
            // state, starts by reading that same record and yielding its current reading first.
            // A resubscribe is therefore never a second chance to catch something already
            // missed, it is simply asking the same question again.
            Ok(Box::pin(futures::stream::unfold(
                (changed, db, id, Next::Current(current)),
                |(mut changed, db, id, next)| async move {
                    match next {
                        Next::Current(state) => {
                            let after = if state.is_final() {
                                Next::Ended
                            } else {
                                Next::Watching
                            };
                            Some((Ok(state), (changed, db, id, after)))
                        }
                        Next::Watching => loop {
                            if changed.changed().await.is_err() {
                                // The sender lives on `FederationInner`; it going away means the
                                // federation itself did, and there is nothing left to watch.
                                return None;
                            }
                            match reload_final_state(&db, id).await {
                                Ok(Some(state)) => {
                                    return Some((Ok(state), (changed, db, id, Next::Ended)));
                                }
                                // Recorded change was not this attempt reaching an ending; keep
                                // waiting for the one that is.
                                Ok(None) => continue,
                                Err(err) => {
                                    return Some((Err(err), (changed, db, id, Next::Ended)));
                                }
                            }
                        },
                        Next::Ended => None,
                    }
                },
            )) as BoxStream<'static, Result<RecoveryState>>)
        })
    }

    fn same_state(&self, previous: &RecoveryState, next: &RecoveryState) -> bool {
        previous == next
    }

    fn encode_state(&self, state: &RecoveryState) -> Result<String> {
        wire::encode_state(state)
    }

    fn decode_state(&self, encoded: &str) -> Result<RecoveryState> {
        wire::decode_state(encoded)
    }

    fn decode_details(&self, _json: &str) -> Result<Box<dyn Any + Send + Sync>> {
        Err(Error::new(
            ErrorCode::Internal,
            "a recovery has no details record",
        ))
    }
}

/// Where a `subscribe` stream stands, threaded through `futures::stream::unfold`.
enum Next {
    /// The state read from the record when the subscription was opened, not yet handed out.
    Current(RecoveryState),
    /// The current state was not final: wait for the attempt's record to change.
    Watching,
    /// A final state has already been handed out; nothing more will ever come.
    Ended,
}

/// The state an attempt's record reads as right now.
///
/// A missing final state is `Running`, never an error: an attempt with no ending recorded yet is
/// simply still going.
fn current_state(record: &OperationRecord) -> Result<RecoveryState> {
    match &record.final_state {
        Some(encoded) => wire::decode_state(encoded),
        None => Ok(RecoveryState::Running),
    }
}

/// The decoded final state on `id`'s record, or `None` if it has not recorded one yet.
async fn reload_final_state(db: &Database, id: OperationId) -> Result<Option<RecoveryState>> {
    let mut dbtx = db.begin_transaction_nc().await;
    let record = dbtx.get_value(&OperationRecordKey(id)).await;
    drop(dbtx);
    let Some(record) = record else {
        return Err(Error::new(
            ErrorCode::Internal,
            format!("no record for operation {}", id.fmt_full()),
        ));
    };
    match record.final_state {
        Some(encoded) => wire::decode_state(&encoded).map(Some),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use futures::StreamExt;

    use super::*;
    use crate::operation::OperationInner;

    /// A detached federation over a fresh in-memory namespace, the way this driver's tests
    /// exercise it: no client, so the driver's promise to never touch one is load-bearing.
    fn detached_federation() -> Arc<FederationInner> {
        FederationInner::detached(
            crate::db::federation_namespace(&crate::db::in_memory_root(), [1u8; 32]),
            true,
        )
    }

    /// Reads back the record an attempt was written under, the way a driver reloads it.
    async fn record_of(federation: &FederationInner, id: OperationId) -> OperationRecord {
        let db = federation.db();
        let mut dbtx = db.begin_transaction_nc().await;
        dbtx.get_value(&OperationRecordKey(id))
            .await
            .expect("the attempt was recorded")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_fresh_attempt_reads_running() {
        let federation = detached_federation();
        let id = OperationId([1u8; 32]);
        federation
            .record_recovery_attempt(id)
            .await
            .expect("record the attempt");
        let record = record_of(&federation, id).await;
        let state = RecoveryDriver
            .current(&federation, id, &record)
            .await
            .expect("current");
        assert_eq!(state, RecoveryState::Running);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_recorded_ending_reads_back() {
        for ending in [
            RecoveryState::Done,
            RecoveryState::Failed {
                reason: "guardian went away".to_owned(),
            },
        ] {
            let federation = detached_federation();
            let id = OperationId([2u8; 32]);
            federation
                .record_recovery_attempt(id)
                .await
                .expect("record the attempt");
            let record = record_of(&federation, id).await;
            let inner = Arc::new(OperationInner {
                federation: federation.clone(),
                id,
                record,
            });
            let encoded = RecoveryDriver.encode_state(&ending).expect("encode");
            inner
                .record_final_state(encoded)
                .await
                .expect("record final state");
            let record = record_of(&federation, id).await;
            let state = RecoveryDriver
                .current(&federation, id, &record)
                .await
                .expect("current");
            assert_eq!(state, ending);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_subscription_yields_running_then_the_ending_when_it_is_recorded() {
        let federation = detached_federation();
        let id = OperationId([3u8; 32]);
        federation
            .record_recovery_attempt(id)
            .await
            .expect("record the attempt");
        let record = record_of(&federation, id).await;
        let mut stream = RecoveryDriver
            .subscribe(&federation, id, &record)
            .await
            .expect("subscribe");
        assert_eq!(
            stream.next().await.expect("first item").expect("ok"),
            RecoveryState::Running
        );

        let ending = RecoveryState::Failed {
            reason: "guardian went away".to_owned(),
        };
        let inner = Arc::new(OperationInner {
            federation: federation.clone(),
            id,
            record: record_of(&federation, id).await,
        });
        let encoded = RecoveryDriver.encode_state(&ending).expect("encode");
        inner
            .record_final_state(encoded)
            .await
            .expect("record final state");
        federation.bump_recovery();

        assert_eq!(
            stream.next().await.expect("second item").expect("ok"),
            ending
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_ending_recorded_between_the_read_and_the_subscription_is_not_missed() {
        let federation = detached_federation();
        let id = OperationId([9u8; 32]);
        federation
            .record_recovery_attempt(id)
            .await
            .expect("record the attempt");
        // The caller's snapshot says the attempt is running.
        let stale = record_of(&federation, id).await;

        // The ending lands, and is bumped, before the subscription exists.
        let inner = Arc::new(OperationInner {
            federation: federation.clone(),
            id,
            record: stale.clone(),
        });
        let encoded = RecoveryDriver
            .encode_state(&RecoveryState::Done)
            .expect("encode");
        inner
            .record_final_state(encoded)
            .await
            .expect("record final state");
        federation.bump_recovery();

        let mut stream = RecoveryDriver
            .subscribe(&federation, id, &stale)
            .await
            .expect("subscribe");
        assert_eq!(
            stream.next().await.expect("first item").expect("ok"),
            RecoveryState::Done
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_subscription_to_a_finished_attempt_yields_only_the_ending() {
        let federation = detached_federation();
        let id = OperationId([4u8; 32]);
        federation
            .record_recovery_attempt(id)
            .await
            .expect("record the attempt");
        let record = record_of(&federation, id).await;
        let inner = Arc::new(OperationInner {
            federation: federation.clone(),
            id,
            record,
        });
        let encoded = RecoveryDriver
            .encode_state(&RecoveryState::Done)
            .expect("encode");
        inner
            .record_final_state(encoded)
            .await
            .expect("record final state");

        let record = record_of(&federation, id).await;
        let mut stream = RecoveryDriver
            .subscribe(&federation, id, &record)
            .await
            .expect("subscribe");
        assert_eq!(
            stream.next().await.expect("first item").expect("ok"),
            RecoveryState::Done
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_bump_with_nothing_recorded_yields_nothing() {
        let federation = detached_federation();
        let id = OperationId([5u8; 32]);
        federation
            .record_recovery_attempt(id)
            .await
            .expect("record the attempt");
        let record = record_of(&federation, id).await;
        let mut stream = RecoveryDriver
            .subscribe(&federation, id, &record)
            .await
            .expect("subscribe");
        assert_eq!(
            stream.next().await.expect("first item").expect("ok"),
            RecoveryState::Running
        );

        federation.bump_recovery();
        let pending = tokio::time::timeout(Duration::from_millis(50), stream.next()).await;
        assert!(pending.is_err(), "the stream must still be waiting");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn as_recovery_hands_back_a_typed_handle() {
        let federation = detached_federation();
        let id = OperationId([6u8; 32]);
        federation
            .record_recovery_attempt(id)
            .await
            .expect("record the attempt");
        let any = federation
            .operation(id)
            .await
            .expect("lookup")
            .expect("the attempt was recorded");
        let typed = any.as_recovery().expect("this build reads recovery");
        assert_eq!(typed.state().await.expect("state"), RecoveryState::Running);
    }
}
