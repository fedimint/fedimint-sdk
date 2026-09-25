//! What the SDK does around the underlying client's own recovery: the records it keeps, the
//! watcher that learns when a rescan ends and forwards its progress, the client swap that makes
//! a recovered wallet usable, and the wait that settles an attempt once what the rescan found has
//! actually landed in it.

use std::collections::BTreeMap;
use std::sync::Arc;

use fedimint_client::ClientHandleArc;
use fedimint_client::error::RecoveryError;
use fedimint_client_module::module::recovery::RecoveryProgress as UpstreamRecoveryProgress;
use fedimint_core::core::{ModuleInstanceId, OperationId as UpstreamOperationId};
use fedimint_core::db::IDatabaseTransactionOpsCoreTyped;
use futures::{FutureExt as _, StreamExt as _};

use super::wire;
use crate::db::{OperationRecord, OperationRecordKey, RecoveryRecord};
use crate::federation::FederationInner;
use crate::operation::OperationInner;
use crate::sdk::SdkInner;
use crate::{FederationStatus, RecoveryProgress, RecoveryState, Result};

/// What is known about a federation's attempt before its client is opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttemptOnFile {
    /// The root record names an attempt with no operation record: the crash window between the
    /// underlying client's own commit and this SDK's write of it.
    None,
    /// The operation record exists and has not recorded a final state.
    Running,
    /// The operation record's final state decodes as done.
    Done,
    /// The operation record's final state decodes as failed, or does not decode at all: an
    /// ending this build cannot interpret is treated the same as a failed one.
    Failed {
        /// Why, for the case this build could decode; the decode error's own text otherwise.
        reason: String,
    },
}

/// What to do about the attempt before the underlying open is invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreOpen {
    /// The attempt is already durable and current: nothing to do before the open.
    Keep,
    /// The root record names an attempt whose operation record is missing; write it, under the
    /// same id, before the open runs.
    RecordMissing,
    /// The last attempt stopped; mint a fresh one before the open runs.
    Mint,
}

/// The pre-open rule from the plan's fixed decisions, as a pure function.
pub(crate) fn plan_pre_open(on_file: &AttemptOnFile) -> PreOpen {
    match on_file {
        AttemptOnFile::None => PreOpen::RecordMissing,
        AttemptOnFile::Running | AttemptOnFile::Done => PreOpen::Keep,
        AttemptOnFile::Failed { .. } => PreOpen::Mint,
    }
}

/// The attempt's state on file, from the root record and the attempt's operation record.
pub(crate) async fn attempt_on_file(
    sdk: &SdkInner,
    federation: &FederationInner,
) -> Result<Option<(RecoveryRecord, AttemptOnFile)>> {
    let Some(record) = crate::db::read_recovery(&sdk.db, &federation.id).await? else {
        return Ok(None);
    };
    let on_file = match read_attempt_record(federation, record.attempt).await {
        None => AttemptOnFile::None,
        Some(op_record) => match op_record.final_state {
            None => AttemptOnFile::Running,
            Some(encoded) => match wire::decode_state(&encoded) {
                Ok(RecoveryState::Running { .. }) => AttemptOnFile::Running,
                Ok(RecoveryState::Done) => AttemptOnFile::Done,
                Ok(RecoveryState::Failed { reason }) => AttemptOnFile::Failed { reason },
                // A fresh attempt is the safe reading of a record this build cannot interpret:
                // minting one beats trusting an ending nothing here can understand.
                Err(err) => AttemptOnFile::Failed {
                    reason: err.message,
                },
            },
        },
    };
    Ok(Some((record, on_file)))
}

/// Applies the pre-open rule, durably, before the underlying open is invoked. Returns the
/// attempt id the open runs under, or `None` for a federation with no recovery record.
pub(crate) async fn prepare_open(
    sdk: &SdkInner,
    federation: &FederationInner,
) -> Result<Option<UpstreamOperationId>> {
    let Some((record, on_file)) = attempt_on_file(sdk, federation).await? else {
        return Ok(None);
    };
    match plan_pre_open(&on_file) {
        PreOpen::Keep => Ok(Some(record.attempt)),
        PreOpen::RecordMissing => {
            federation.record_recovery_attempt(record.attempt).await?;
            Ok(Some(record.attempt))
        }
        PreOpen::Mint => new_attempt(sdk, federation).await.map(Some),
    }
}

/// Reconciles the attempt with what the freshly opened client says, publishes the status that
/// follows, and starts the watcher when the rescan is still running. `attempt` is
/// `prepare_open`'s answer.
///
/// `client` is the caller's last handle to the client, taken by value so that a swap made here
/// finds nothing else holding it. The status is set here, and set before the watcher spawns,
/// because the watcher's own close-watch reads it: a federation still marked closed from its
/// construction would end the watch before the rescan had started.
///
/// Like [`finish`], this runs under the lifecycle mutex, or inside the build before any handle
/// exists to contend for it.
pub(crate) async fn after_open(
    sdk: &Arc<SdkInner>,
    federation: &Arc<FederationInner>,
    client: ClientHandleArc,
    attempt: Option<UpstreamOperationId>,
) {
    let Some(attempt) = attempt else {
        federation.set_status(FederationStatus::Running);
        return;
    };
    if client.has_pending_recoveries() {
        federation.set_status(FederationStatus::Recovering);
        watch(sdk.clone(), federation.clone(), client, attempt);
        return;
    }
    // A rescan that ended between the client's construction and here reports nothing pending,
    // but a module it held back for the duration (a v1 mint) is still out of the registry
    // until the next build: the same swap the watcher would have made is made now.
    if !client.all_modules_usable() {
        drop(client);
        finish(sdk, federation, attempt).await;
        return;
    }
    // The client's own durable state already corroborates that the rescan is over, but not that
    // what it found has landed: `Done` still waits for that. If the attempt's record has not
    // caught up yet (the crash window between the client's own commit and this SDK's write), it
    // is settled here exactly as the watcher's own completion is, since nothing else starts a
    // watcher for an attempt whose client already reports nothing pending.
    if read_attempt_record(federation, attempt)
        .await
        .is_some_and(|record| record.final_state.is_none())
    {
        federation.set_status(FederationStatus::Recovering);
        start_settle(sdk.clone(), federation.clone(), client, attempt);
        return;
    }
    drop(client);
    federation.set_status(FederationStatus::Running);
}

/// One task per recovering federation: blocks until the client's rescan ends, then records
/// the outcome and, on success, swaps in a usable client. While it waits, it also forwards the
/// client's own per-module progress into [`FederationInner::publish_recovery_progress`].
pub(crate) fn watch(
    sdk: Arc<SdkInner>,
    federation: Arc<FederationInner>,
    client: ClientHandleArc,
    attempt: UpstreamOperationId,
) {
    fedimint_core::task::spawn("sdk-recovery-watch", async move {
        // A watcher starting is the one place progress is reset: whatever an earlier attempt on
        // this federation last published no longer describes anything running.
        federation.reset_recovery_progress();

        // The wait ends either with the rescan's outcome or with the federation closing,
        // whichever comes first. The second matters because the client's own status channel
        // need not close when the federation does (a coordinator whose module gave up parks
        // for ever), and a close has to get this task's clone of the client back promptly:
        // `shutdown_client` needs the last reference.
        let mut closed = federation.closed();
        let outcome = {
            let mut wait = std::pin::pin!(client.wait_for_all_recoveries());
            let mut close = std::pin::pin!(closed.wait_for(|closed| *closed));
            let mut modules: BTreeMap<ModuleInstanceId, UpstreamRecoveryProgress> = BTreeMap::new();
            let mut progress = std::pin::pin!(client.subscribe_to_recovery_progress());
            let mut progress_ended = false;
            loop {
                tokio::select! {
                    outcome = &mut wait => break outcome,
                    _ = &mut close => break Err(RecoveryError::ClientStopped),
                    next = progress.next(), if !progress_ended => {
                        // The stream ends only when the client's status channel closes, which
                        // ends `wait` too. It is not polled again, since an ended stream
                        // answers at once and this loop would spin on it until `wait` is picked.
                        let Some((module, module_progress)) = next else {
                            progress_ended = true;
                            continue;
                        };
                        modules.insert(module, module_progress);
                        // Each change arrives as one item per recovering module, all ready at
                        // once. They are taken together, so no snapshot is published half read.
                        loop {
                            match progress.next().now_or_never() {
                                Some(Some((module, module_progress))) => {
                                    modules.insert(module, module_progress);
                                }
                                Some(None) => {
                                    progress_ended = true;
                                    break;
                                }
                                None => break,
                            }
                        }
                        match aggregate_progress(modules.values().copied()) {
                            Some(aggregate) => {
                                federation.publish_recovery_progress(attempt, aggregate);
                            }
                            None => federation.reset_recovery_progress(),
                        }
                    }
                }
            }
        };
        drop(client);
        match outcome {
            Ok(()) => complete(&sdk, &federation, attempt).await,
            Err(RecoveryError::Failed {
                module_instance_id,
                error,
            }) => {
                record_attempt_failed(
                    &federation,
                    attempt,
                    format!("module {module_instance_id}: {error}"),
                )
                .await;
            }
            Err(RecoveryError::ClientStopped) => {}
            // `RecoveryError` is `#[non_exhaustive]`: a variant this build does not recognise
            // yet is still recorded as a failure, using its own `Display` for the reason,
            // rather than leaving the attempt `Running` forever.
            Err(err) => record_attempt_failed(&federation, attempt, err.to_string()).await,
        }
    });
}

/// The watcher's completion: [`finish`] under the lifecycle mutex, unless the attempt has
/// already been concluded by the time the mutex is held.
///
/// Taken because the swap and the status that follows it are a lifecycle transition, and a
/// `close_federation` or `forget_federation` interleaved between the two would have its
/// `Closed` overwritten with `Recovering` on a federation whose client had just been taken away.
/// The mutex is taken before anything else, as every lifecycle call takes it.
///
/// The attempt is re-read once the mutex is held: a `resume_recovery` that held it while the
/// rescan ended may already have swapped the client and started settling, or even have finished
/// settling outright. Either way `finish` itself, once it holds the mutex too, recognises that
/// and does not touch a client that does not need replacing a second time.
pub(crate) async fn complete(
    sdk: &Arc<SdkInner>,
    federation: &Arc<FederationInner>,
    attempt: UpstreamOperationId,
) {
    let _lifecycle = sdk.lifecycle.lock().await;
    if read_attempt_record(federation, attempt)
        .await
        .is_some_and(|record| record.final_state.is_some())
    {
        return;
    }
    finish(sdk, federation, attempt).await;
}

/// Phase 1 of completing a recovery: swaps in a client whose modules the rescan unlocked, then
/// starts the wait that settles the attempt once what it found has actually landed.
///
/// The caller holds the lifecycle mutex, or is the build, which runs before any handle exists to
/// take it, so nothing can close, erase or reopen the federation while this runs. This phase does
/// not mark the attempt done: a fresh client having its modules back is not the same as the notes
/// the rescan found being spendable in it yet, and only [`conclude_settle`] writes
/// [`RecoveryState::Done`], once phase 2 has waited for that. The federation's status stays
/// [`Recovering`](FederationStatus::Recovering) throughout this phase.
///
/// A no-op when a settle is already claimed for the client installed right now: `finish` can be
/// reached twice for one rescan ending (the watcher's own [`complete`], and `resume_recovery`
/// finding nothing pending), and settling can take long enough for both to run before either
/// concludes, so the second call must not swap away a client its predecessor is already watching.
pub(crate) async fn finish(
    sdk: &Arc<SdkInner>,
    federation: &Arc<FederationInner>,
    attempt: UpstreamOperationId,
) {
    if read_attempt_record(federation, attempt)
        .await
        .is_some_and(|record| record.final_state.is_some())
    {
        return;
    }
    if federation.is_settling() {
        return;
    }
    let id = federation.id;
    // The swap is what makes the recovered wallet usable: a v1 mint recovers as
    // `RecoveryMode::Unusable` and only enters the module registry on the client's next build
    // (`fedimint-mint-client/src/lib.rs:839-841`, `fedimint-client/src/client/builder.rs:951`),
    // so every generation is swapped even though only some of them need it.
    match federation
        .replace_client(|| async { sdk.open_client(&id).await })
        .await
    {
        Ok(()) => {}
        // The federation was closed, quarantined or erased while the rescan was finishing; the
        // swap was refused, and whatever did that owns the attempt's fate now. The next open
        // settles it, through `after_open`'s own catch-up branch.
        Err(err) if err.code == crate::ErrorCode::FederationClosed => return,
        Err(err) => {
            federation.set_status(FederationStatus::Quarantined {
                diagnostic: err.into(),
            });
            sdk.announce(federation);
            return;
        }
    }
    // The mutex this function's caller holds rules out anything else touching the client
    // between the swap above and this read, so the fresh client is always found here.
    let client = match federation.client(false).await {
        Ok(guard) => guard.handle(),
        Err(err) => {
            tracing::warn!(
                target: "fedimint_sdk",
                federation = %federation.id,
                attempt = %attempt.fmt_full(),
                error = %err,
                "could not read back the client just swapped in for settling",
            );
            return;
        }
    };
    start_settle(sdk.clone(), federation.clone(), client, attempt);
}

/// Starts phase 2 of completing a recovery (see [`finish`]), which waits for `client` to have
/// nothing left running before marking the attempt done. No-op if a settle is already claimed
/// for the client installed right now; see [`FederationInner::try_start_settle`].
fn start_settle(
    sdk: Arc<SdkInner>,
    federation: Arc<FederationInner>,
    client: ClientHandleArc,
    attempt: UpstreamOperationId,
) {
    let Some(identity) = federation.try_start_settle() else {
        return;
    };
    // Held weakly for the wait, which is unbounded: this task must never be what keeps the
    // instance alive.
    let sdk = Arc::downgrade(&sdk);
    fedimint_core::task::spawn("sdk-recovery-settle", async move {
        let stop = client.task_group().make_handle().make_shutdown_rx();
        // The recovery lock keeps every other facade off this federation for as long as the
        // attempt is not `Done`, so nothing else can have started a state machine on it while
        // this waits: every one the fresh client has right now is the recovery's own issuance
        // of what the rescan found.
        let settled = crate::federation::wait_holding_client(client, stop, |client| async move {
            client.wait_for_all_active_state_machines().await;
            Ok(())
        })
        .await
        .is_ok();
        // Otherwise the client stopped before settling finished: whoever stopped it owns the
        // attempt's fate now, exactly as a swap refused by a closed federation does.
        if settled && let Some(sdk) = sdk.upgrade() {
            conclude_settle(&sdk, &federation, identity, attempt).await;
        }
        // Released only after the conclusion, so a `finish` that runs in between still finds
        // the claim and leaves the client alone, and one that runs after it finds the attempt
        // done.
        federation.finish_settle(identity);
    });
}

/// Phase 2's conclusion: marks the attempt done once what the rescan found has landed, unless
/// something else already decided the attempt's fate or moved the federation on to a different
/// client while this was waiting.
///
/// Takes the lifecycle mutex, for the same reason [`complete`] does: nothing may overwrite a
/// `close_federation` or `forget_federation` interleaved between the wait ending and this.
async fn conclude_settle(
    sdk: &Arc<SdkInner>,
    federation: &Arc<FederationInner>,
    identity: u64,
    attempt: UpstreamOperationId,
) {
    let _lifecycle = sdk.lifecycle.lock().await;
    let Some(record) = read_attempt_record(federation, attempt).await else {
        return;
    };
    if record.final_state.is_some() {
        return;
    }
    // Not open, or open on a client this settle was not watching: whatever replaced or closed
    // it since owns the attempt's fate now.
    if !federation.is_open() || !federation.is_current_client(identity) {
        return;
    }
    if let Err(err) = write_final_state(federation, attempt, record, RecoveryState::Done).await {
        tracing::warn!(
            target: "fedimint_sdk",
            federation = %federation.id,
            attempt = %attempt.fmt_full(),
            error = %err,
            "could not record a finished recovery attempt as done",
        );
    }
    federation.bump_recovery();
    federation.set_status(FederationStatus::Running);
    sdk.announce(federation);
}

/// Mints a brand new attempt: the root record naming it, then its operation record, in that
/// order, so a crash between the two reads back as `PreOpen::RecordMissing` next time rather
/// than as an attempt the root record does not name.
pub(crate) async fn new_attempt(
    sdk: &SdkInner,
    federation: &FederationInner,
) -> Result<UpstreamOperationId> {
    let attempt = UpstreamOperationId::new_random();
    crate::db::write_recovery(&sdk.db, &federation.id, &RecoveryRecord { attempt }).await?;
    federation.record_recovery_attempt(attempt).await?;
    Ok(attempt)
}

/// Records that an attempt stopped, for the reason the watcher observed. Status is untouched:
/// the federation stays `Recovering`, and a later open or `resume_recovery` decides what to do
/// about the attempt from here.
async fn record_attempt_failed(
    federation: &Arc<FederationInner>,
    attempt: UpstreamOperationId,
    reason: String,
) {
    let Some(record) = read_attempt_record(federation, attempt).await else {
        return;
    };
    match write_final_state(
        federation,
        attempt,
        record,
        RecoveryState::Failed { reason },
    )
    .await
    {
        Ok(()) => federation.bump_recovery(),
        Err(err) => tracing::warn!(
            target: "fedimint_sdk",
            federation = %federation.id,
            attempt = %attempt.fmt_full(),
            error = %err,
            "could not record a failed recovery attempt",
        ),
    }
}

/// The attempt's own operation record, straight from the federation's namespace.
async fn read_attempt_record(
    federation: &FederationInner,
    attempt: UpstreamOperationId,
) -> Option<OperationRecord> {
    let db = federation.db();
    let mut dbtx = db.begin_transaction_nc().await;
    dbtx.get_value(&OperationRecordKey(attempt)).await
}

/// Encodes `state` and records it as the attempt's final state, through an [`OperationInner`]
/// built from `record`.
async fn write_final_state(
    federation: &Arc<FederationInner>,
    attempt: UpstreamOperationId,
    record: OperationRecord,
    state: RecoveryState,
) -> Result<()> {
    let encoded = wire::encode_state(&state)?;
    OperationInner {
        federation: federation.clone(),
        id: attempt,
        record,
    }
    .record_final_state(encoded)
    .await
}

/// The rescan's aggregate progress: `complete` and `total` summed, saturating, across every
/// recovering module, or `None` while any of them has not reported how much work it has.
///
/// Every recovering module is listed from the start, those that have not reported yet with an
/// empty progress, so waiting for all of them is what keeps `total` from growing, and the ratio
/// from dipping, when a module reports late. Each module counts its own units of work, history
/// items or sessions depending on what the federation supports, so the sum weighs modules by how
/// much work each has. It is a measure of how far the rescan has got, not a count of anything.
fn aggregate_progress(
    per_module: impl IntoIterator<Item = UpstreamRecoveryProgress>,
) -> Option<RecoveryProgress> {
    let mut aggregate: Option<RecoveryProgress> = None;
    for module in per_module {
        if module.is_none() {
            return None;
        }
        let so_far = aggregate.unwrap_or(RecoveryProgress {
            complete: 0,
            total: 0,
        });
        aggregate = Some(RecoveryProgress {
            complete: so_far.complete.saturating_add(module.complete),
            total: so_far.total.saturating_add(module.total),
        });
    }
    aggregate
}

#[cfg(test)]
mod tests {
    use fedimint_core::db::Database;

    use super::*;
    use crate::db::federation_namespace;
    use crate::operation::kinds;
    use crate::{Sdk, Storage};

    /// A real, minimal instance backed by in-memory storage. `attempt_on_file` and
    /// `prepare_open` read and write the root store through a genuine `SdkInner`, and
    /// `SdkInner`'s own fields are private outside `crate::sdk`, so there is no cheaper way to
    /// get one here than the public builder.
    async fn detached_sdk() -> Sdk {
        Sdk::builder()
            .storage(Storage::in_memory())
            .build()
            .await
            .expect("an instance opens")
    }

    /// A federation with no client, whose own namespace sits inside `root`: the same
    /// relationship an `SdkInner`'s root database and a federation's own database have in a
    /// real instance.
    fn detached_federation(root: &Database) -> Arc<FederationInner> {
        FederationInner::detached(federation_namespace(root, [1u8; 32]), true)
    }

    /// Plants an attempt's operation record directly, without going through
    /// `record_recovery_attempt` or `record_final_state`, so a test can set up any final state
    /// it wants, decodable or not.
    async fn plant_operation_record(
        federation: &FederationInner,
        id: UpstreamOperationId,
        final_state: Option<String>,
    ) {
        let db = federation.db();
        let mut dbtx = db.begin_transaction().await;
        dbtx.insert_entry(
            &OperationRecordKey(id),
            &OperationRecord {
                schema_version: crate::operation::READABLE_STATE_SCHEMA,
                kind: kinds::RECOVERY.to_owned(),
                module: String::new(),
                created_at: 0,
                details: "{}".to_owned(),
                phase: None,
                cancel_requested_at: None,
                final_state,
            },
        )
        .await;
        dbtx.commit_tx().await;
    }

    #[test]
    fn plan_pre_open_answers_the_three_fixed_cases() {
        assert_eq!(plan_pre_open(&AttemptOnFile::None), PreOpen::RecordMissing);
        assert_eq!(plan_pre_open(&AttemptOnFile::Running), PreOpen::Keep);
        assert_eq!(plan_pre_open(&AttemptOnFile::Done), PreOpen::Keep);
        assert_eq!(
            plan_pre_open(&AttemptOnFile::Failed {
                reason: "guardian gone".to_owned(),
            }),
            PreOpen::Mint
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn attempt_on_file_needs_a_root_record_before_it_reads_anything_else() {
        let sdk = detached_sdk().await;
        let federation = detached_federation(&sdk.inner().db);

        // No recovery record at all: this federation was never recovered.
        assert_eq!(
            attempt_on_file(sdk.inner(), &federation)
                .await
                .expect("read"),
            None
        );

        // A root record naming an attempt that crashed before its own operation record was
        // written.
        let attempt = UpstreamOperationId([1u8; 32]);
        crate::db::write_recovery(&sdk.inner().db, &federation.id, &RecoveryRecord { attempt })
            .await
            .expect("write the root record");
        let (record, on_file) = attempt_on_file(sdk.inner(), &federation)
            .await
            .expect("read")
            .expect("a root record now exists");
        assert_eq!(record.attempt, attempt);
        assert_eq!(on_file, AttemptOnFile::None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn attempt_on_file_reads_the_operation_records_final_state() {
        let sdk = detached_sdk().await;
        let federation = detached_federation(&sdk.inner().db);
        let broken = r#"{"Paused":{}}"#.to_owned();
        let undecodable_reason = wire::decode_state(&broken)
            .expect_err("this build has no Paused variant")
            .message;

        let cases = [
            (1u8, None, AttemptOnFile::Running),
            (
                2u8,
                Some(wire::encode_state(&RecoveryState::Done).expect("encode")),
                AttemptOnFile::Done,
            ),
            (
                3u8,
                Some(
                    wire::encode_state(&RecoveryState::Failed {
                        reason: "guardian gone".to_owned(),
                    })
                    .expect("encode"),
                ),
                AttemptOnFile::Failed {
                    reason: "guardian gone".to_owned(),
                },
            ),
            (
                4u8,
                Some(broken),
                AttemptOnFile::Failed {
                    reason: undecodable_reason,
                },
            ),
        ];
        for (byte, final_state, expected) in cases {
            let attempt = UpstreamOperationId([byte; 32]);
            crate::db::write_recovery(&sdk.inner().db, &federation.id, &RecoveryRecord { attempt })
                .await
                .expect("write the root record");
            plant_operation_record(&federation, attempt, final_state).await;

            let (_, on_file) = attempt_on_file(sdk.inner(), &federation)
                .await
                .expect("read")
                .expect("a root record exists");
            assert_eq!(on_file, expected);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn prepare_open_records_a_missing_attempt_under_the_same_id() {
        let sdk = detached_sdk().await;
        let federation = detached_federation(&sdk.inner().db);
        let attempt = UpstreamOperationId([5u8; 32]);
        crate::db::write_recovery(&sdk.inner().db, &federation.id, &RecoveryRecord { attempt })
            .await
            .expect("write the root record");

        let prepared = prepare_open(sdk.inner(), &federation)
            .await
            .expect("prepare")
            .expect("an attempt to open under");
        assert_eq!(prepared, attempt);

        let (record, on_file) = attempt_on_file(sdk.inner(), &federation)
            .await
            .expect("read")
            .expect("a root record");
        assert_eq!(record.attempt, attempt);
        assert_eq!(on_file, AttemptOnFile::Running);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn prepare_open_keeps_a_running_attempt() {
        let sdk = detached_sdk().await;
        let federation = detached_federation(&sdk.inner().db);
        let attempt = UpstreamOperationId([6u8; 32]);
        crate::db::write_recovery(&sdk.inner().db, &federation.id, &RecoveryRecord { attempt })
            .await
            .expect("write the root record");
        federation
            .record_recovery_attempt(attempt)
            .await
            .expect("record the attempt");

        let prepared = prepare_open(sdk.inner(), &federation)
            .await
            .expect("prepare")
            .expect("an attempt to open under");
        assert_eq!(prepared, attempt);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn completing_a_closed_federation_leaves_the_attempt_for_the_next_open_to_settle() {
        let sdk = detached_sdk().await;
        // Closed, with no client: what the watcher finds when a close won the race against
        // the rescan's end.
        let federation =
            FederationInner::detached(federation_namespace(&sdk.inner().db, [1u8; 32]), false);
        let attempt = UpstreamOperationId([8u8; 32]);
        crate::db::write_recovery(&sdk.inner().db, &federation.id, &RecoveryRecord { attempt })
            .await
            .expect("write the root record");
        plant_operation_record(&federation, attempt, None).await;

        complete(sdk.inner(), &federation, attempt).await;

        // No client to swap in means `finish` cannot settle this attempt either: it is left
        // exactly as it was, without a final state, for the next open's own catch-up branch to
        // settle once a client exists again.
        assert_eq!(federation.status(), FederationStatus::Closed);
        let record = read_attempt_record(&federation, attempt)
            .await
            .expect("the record survives");
        assert!(
            record.final_state.is_none(),
            "a closed federation's attempt is not concluded here"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_concluded_attempt_is_not_completed_a_second_time() {
        let sdk = detached_sdk().await;
        let federation = detached_federation(&sdk.inner().db);
        let attempt = UpstreamOperationId([9u8; 32]);
        crate::db::write_recovery(&sdk.inner().db, &federation.id, &RecoveryRecord { attempt })
            .await
            .expect("write the root record");
        // What the watcher finds when `resume_recovery` held the lifecycle mutex while the
        // rescan ended and completed the attempt first.
        plant_operation_record(
            &federation,
            attempt,
            Some(wire::encode_state(&RecoveryState::Done).expect("encode")),
        )
        .await;
        let observed = federation.recovery_changed();

        complete(sdk.inner(), &federation, attempt).await;

        // A completion writes the ending and wakes the attempt's subscribers; a skipped one
        // does neither.
        assert!(
            !observed.has_changed().expect("the sender is alive"),
            "a second completion must leave the concluded attempt alone"
        );
        assert_eq!(federation.status(), FederationStatus::Running);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn prepare_open_mints_a_new_attempt_after_a_failed_one() {
        let sdk = detached_sdk().await;
        let federation = detached_federation(&sdk.inner().db);
        let old_attempt = UpstreamOperationId([7u8; 32]);
        crate::db::write_recovery(
            &sdk.inner().db,
            &federation.id,
            &RecoveryRecord {
                attempt: old_attempt,
            },
        )
        .await
        .expect("write the root record");
        plant_operation_record(
            &federation,
            old_attempt,
            Some(
                wire::encode_state(&RecoveryState::Failed {
                    reason: "guardian gone".to_owned(),
                })
                .expect("encode"),
            ),
        )
        .await;

        let new_attempt = prepare_open(sdk.inner(), &federation)
            .await
            .expect("prepare")
            .expect("a fresh attempt");
        assert_ne!(new_attempt, old_attempt);

        let root_record = crate::db::read_recovery(&sdk.inner().db, &federation.id)
            .await
            .expect("read")
            .expect("the root record now exists");
        assert_eq!(root_record.attempt, new_attempt);

        let (_, new_on_file) = attempt_on_file(sdk.inner(), &federation)
            .await
            .expect("read")
            .expect("a root record");
        assert_eq!(new_on_file, AttemptOnFile::Running);

        // The old attempt's own record is untouched: it still reads failed.
        let old_record = read_attempt_record(&federation, old_attempt)
            .await
            .expect("the old record survives");
        assert_eq!(
            wire::decode_state(&old_record.final_state.expect("a final state")).expect("decode"),
            RecoveryState::Failed {
                reason: "guardian gone".to_owned(),
            }
        );
    }

    #[test]
    fn aggregate_progress_of_nothing_reporting_is_none() {
        assert_eq!(aggregate_progress([]), None);
        assert_eq!(
            aggregate_progress([UpstreamRecoveryProgress {
                complete: 0,
                total: 0,
            }]),
            None,
            "total 0 is upstream's own encoding of a module that has not reported yet"
        );
    }

    #[test]
    fn aggregate_progress_sums_the_modules_that_have_reported() {
        let sum = aggregate_progress([
            UpstreamRecoveryProgress {
                complete: 3,
                total: 10,
            },
            UpstreamRecoveryProgress {
                complete: 4,
                total: 5,
            },
        ]);
        assert_eq!(
            sum,
            Some(RecoveryProgress {
                complete: 7,
                total: 15
            })
        );
    }

    #[test]
    fn aggregate_progress_waits_for_every_module_to_report() {
        let sum = aggregate_progress([
            UpstreamRecoveryProgress {
                complete: 2,
                total: 8,
            },
            UpstreamRecoveryProgress {
                complete: 0,
                total: 0,
            },
        ]);
        assert_eq!(
            sum, None,
            "a module that has not reported would make the total grow once it does"
        );
    }

    #[test]
    fn aggregate_progress_saturates_rather_than_overflows() {
        let sum = aggregate_progress([
            UpstreamRecoveryProgress {
                complete: u32::MAX,
                total: u32::MAX,
            },
            UpstreamRecoveryProgress {
                complete: 1,
                total: 1,
            },
        ]);
        assert_eq!(
            sum,
            Some(RecoveryProgress {
                complete: u32::MAX,
                total: u32::MAX
            })
        );
    }
}
