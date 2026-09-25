//! The recovery driver: reads one attempt's persisted record and the progress the recovery
//! watcher publishes, and never asks the client itself.

use std::any::Any;

use fedimint_core::core::OperationId;
use fedimint_core::db::{Database, IDatabaseTransactionOpsCoreTyped};
use fedimint_core::util::{BoxFuture, BoxStream};

use super::wire;
use crate::db::{OperationRecord, OperationRecordKey};
use crate::federation::FederationInner;
use crate::operation::Driver;
use crate::{Error, ErrorCode, OperationState, RecoveryProgress, RecoveryState, Result};

/// Observes a seed recovery attempt from its operation record and its published progress.
///
/// Recovery is not a client operation upstream (see the module documentation), so unlike every
/// other driver in this crate, this one has no module call to make and no upstream state machine
/// to fold onto: the attempt's operation record, written by
/// [`FederationInner::record_recovery_attempt`] and completed by the watcher task the recovery
/// flow starts, is the entire source of truth for whether the attempt is running, done or
/// failed. While it runs, [`FederationInner::recovery_progress_of`] is the source of truth for
/// how far it has got, published by that same watcher rather than persisted.
pub(crate) struct RecoveryDriver;

impl Driver<RecoveryState> for RecoveryDriver {
    fn current<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<RecoveryState>> {
        Box::pin(async move { current_state(record, federation.recovery_progress_of(id)) })
    }

    fn subscribe<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<RecoveryState>>>> {
        Box::pin(async move {
            // Subscribed to both watches before the record and the progress are read, and
            // neither taken from the caller: a receiver only ever wakes for a change made after
            // it was created, so an ending, or a progress update, landing between the caller's
            // read of `record` and this subscription would otherwise never be seen, and the
            // stream would wait for a second one that never comes.
            let changed = federation.recovery_changed();
            let progress_changed = federation.recovery_progress_changed();
            let db = federation.db();
            let current = match reload_final_state(&db, id).await? {
                Some(state) => state,
                None => current_state(record, progress_of(&progress_changed, id))?,
            };
            // Every other driver's `subscribe` replays an upstream state machine and has to
            // reason about whether resubscribing after a lagged notifier could skip a
            // transition it already produced. This one cannot lose anything that way, because
            // there is no history to replay: the only thing a subscription ever reports is the
            // attempt's record, and its published progress, as they read right now, and every
            // stream this method returns, the first one or a fresh one opened after an earlier
            // stream ended without a final state, starts by reading those same two things and
            // yielding the current reading first. A resubscribe is therefore never a second
            // chance to catch something already missed, it is simply asking the same question
            // again.
            Ok(Box::pin(futures::stream::unfold(
                (changed, progress_changed, db, id, Next::Current(current)),
                |(mut changed, mut progress_changed, db, id, next)| async move {
                    match next {
                        Next::Current(state) => {
                            let after = if state.is_final() {
                                Next::Ended
                            } else {
                                Next::Watching
                            };
                            Some((Ok(state), (changed, progress_changed, db, id, after)))
                        }
                        Next::Watching => loop {
                            tokio::select! {
                                result = changed.changed() => {
                                    if result.is_err() {
                                        // The sender lives on `FederationInner`; it going away
                                        // means the federation itself did, and there is nothing
                                        // left to watch.
                                        return None;
                                    }
                                    match reload_final_state(&db, id).await {
                                        Ok(Some(state)) => {
                                            return Some((
                                                Ok(state),
                                                (changed, progress_changed, db, id, Next::Ended),
                                            ));
                                        }
                                        // Recorded change was not this attempt reaching an
                                        // ending; keep waiting for the one that is.
                                        Ok(None) => continue,
                                        Err(err) => {
                                            return Some((
                                                Err(err),
                                                (changed, progress_changed, db, id, Next::Ended),
                                            ));
                                        }
                                    }
                                }
                                result = progress_changed.changed() => {
                                    if result.is_err() {
                                        return None;
                                    }
                                    match reload_final_state(&db, id).await {
                                        // The attempt ended before this progress update was
                                        // read, so the ending is what belongs here.
                                        Ok(Some(state)) => {
                                            return Some((
                                                Ok(state),
                                                (changed, progress_changed, db, id, Next::Ended),
                                            ));
                                        }
                                        Ok(None) => {
                                            let progress = progress_of(&progress_changed, id);
                                            return Some((
                                                Ok(RecoveryState::Running { progress }),
                                                (
                                                    changed,
                                                    progress_changed,
                                                    db,
                                                    id,
                                                    Next::Watching,
                                                ),
                                            ));
                                        }
                                        Err(err) => {
                                            return Some((
                                                Err(err),
                                                (changed, progress_changed, db, id, Next::Ended),
                                            ));
                                        }
                                    }
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
    /// The current state was not final: wait for the attempt's record, or its published
    /// progress, to change.
    Watching,
    /// A final state has already been handed out; nothing more will ever come.
    Ended,
}

/// The state an attempt's record reads as right now, given `progress` as its published
/// progress.
///
/// A missing final state is `Running`, never an error: an attempt with no ending recorded yet is
/// simply still going.
fn current_state(
    record: &OperationRecord,
    progress: Option<RecoveryProgress>,
) -> Result<RecoveryState> {
    match &record.final_state {
        Some(encoded) => wire::decode_state(encoded),
        None => Ok(RecoveryState::Running { progress }),
    }
}

/// `id`'s published progress, read off a receiver already subscribed to it.
fn progress_of(
    receiver: &tokio::sync::watch::Receiver<Option<(OperationId, RecoveryProgress)>>,
    id: OperationId,
) -> Option<RecoveryProgress> {
    (*receiver.borrow()).and_then(|(published, progress)| (published == id).then_some(progress))
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
        assert_eq!(state, RecoveryState::Running { progress: None });
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn current_reads_the_federations_published_progress_for_this_attempt_only() {
        let federation = detached_federation();
        let id = OperationId([10u8; 32]);
        let other_id = OperationId([11u8; 32]);
        federation
            .record_recovery_attempt(id)
            .await
            .expect("record the attempt");
        let record = record_of(&federation, id).await;

        // Nothing published yet: the same `Running { progress: None }` a fresh attempt reads.
        assert_eq!(
            RecoveryDriver
                .current(&federation, id, &record)
                .await
                .expect("current"),
            RecoveryState::Running { progress: None }
        );

        let progress = RecoveryProgress {
            complete: 4,
            total: 9,
        };
        federation.publish_recovery_progress(id, progress);
        assert_eq!(
            RecoveryDriver
                .current(&federation, id, &record)
                .await
                .expect("current"),
            RecoveryState::Running {
                progress: Some(progress)
            }
        );

        // Another attempt's own `current` never reads this attempt's published progress.
        assert_eq!(
            RecoveryDriver
                .current(&federation, other_id, &record)
                .await
                .expect("current"),
            RecoveryState::Running { progress: None }
        );
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
            RecoveryState::Running { progress: None }
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
            RecoveryState::Running { progress: None }
        );

        federation.bump_recovery();
        let pending = tokio::time::timeout(Duration::from_millis(50), stream.next()).await;
        assert!(pending.is_err(), "the stream must still be waiting");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_published_progress_wakes_the_subscription_and_it_still_ends_on_a_final_state() {
        let federation = detached_federation();
        let id = OperationId([12u8; 32]);
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
            RecoveryState::Running { progress: None }
        );

        let progress = RecoveryProgress {
            complete: 1,
            total: 4,
        };
        federation.publish_recovery_progress(id, progress);
        assert_eq!(
            stream.next().await.expect("second item").expect("ok"),
            RecoveryState::Running {
                progress: Some(progress)
            }
        );

        let inner = Arc::new(OperationInner {
            federation: federation.clone(),
            id,
            record: record_of(&federation, id).await,
        });
        let encoded = RecoveryDriver
            .encode_state(&RecoveryState::Done)
            .expect("encode");
        inner
            .record_final_state(encoded)
            .await
            .expect("record final state");
        federation.bump_recovery();

        assert_eq!(
            stream.next().await.expect("third item").expect("ok"),
            RecoveryState::Done
        );
        assert!(stream.next().await.is_none());
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
        assert_eq!(
            typed.state().await.expect("state"),
            RecoveryState::Running { progress: None }
        );
    }
}
