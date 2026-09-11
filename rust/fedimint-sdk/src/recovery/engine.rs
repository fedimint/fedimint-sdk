//! What the SDK does around the underlying client's own recovery: the records it keeps, the
//! watcher that learns when a rescan ends, and the client swap that makes a recovered wallet
//! usable.

use std::sync::Arc;

use fedimint_client::ClientHandleArc;
use fedimint_client::error::RecoveryError;
use fedimint_core::core::OperationId as UpstreamOperationId;
use fedimint_core::db::IDatabaseTransactionOpsCoreTyped;

use super::wire;
use crate::db::{OperationRecord, OperationRecordKey, RecoveryRecord};
use crate::federation::FederationInner;
use crate::operation::OperationInner;
use crate::sdk::SdkInner;
use crate::{FederationStatus, RecoveryState, Result};

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
                Ok(RecoveryState::Running) => AttemptOnFile::Running,
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

/// Reconciles the attempt with what the freshly opened client says, sets the status, and starts
/// the watcher when the rescan is still running. `attempt` is `prepare_open`'s answer.
pub(crate) async fn after_open(
    sdk: &Arc<SdkInner>,
    federation: &Arc<FederationInner>,
    client: &ClientHandleArc,
    attempt: Option<UpstreamOperationId>,
) -> FederationStatus {
    let Some(attempt) = attempt else {
        return FederationStatus::Running;
    };
    if client.has_pending_recoveries() {
        watch(sdk.clone(), federation.clone(), client.clone(), attempt);
        return FederationStatus::Recovering;
    }
    // The client's own durable state already corroborates completion. If the attempt's record
    // has not caught up yet (the crash window between the client's own commit and this SDK's
    // write), catch it up now: no watcher starts for an attempt whose client reports nothing
    // pending, so nothing else ever will.
    if let Some(record) = read_attempt_record(federation, attempt).await
        && record.final_state.is_none()
    {
        match write_final_state(federation, attempt, record, RecoveryState::Done).await {
            Ok(()) => federation.bump_recovery(),
            Err(err) => tracing::warn!(
                target: "fedimint_sdk",
                federation = %federation.id,
                attempt = %attempt.fmt_full(),
                error = %err,
                "could not record a finished recovery attempt as done",
            ),
        }
    }
    FederationStatus::Running
}

/// One task per recovering federation: blocks until the client's rescan ends, then records
/// the outcome and, on success, swaps in a usable client.
pub(crate) fn watch(
    sdk: Arc<SdkInner>,
    federation: Arc<FederationInner>,
    client: ClientHandleArc,
    attempt: UpstreamOperationId,
) {
    fedimint_core::task::spawn("sdk-recovery-watch", async move {
        // The wait ends either with the rescan's outcome or with the federation closing,
        // whichever comes first. The second matters because the client's own status channel
        // need not close when the federation does (a coordinator whose module gave up parks
        // for ever), and a close has to get this task's clone of the client back promptly:
        // `shutdown_client` needs the last reference.
        let mut closed = federation.closed();
        let outcome = {
            let wait = std::pin::pin!(client.wait_for_all_recoveries());
            let close = std::pin::pin!(closed.wait_for(|closed| *closed));
            match futures::future::select(wait, close).await {
                futures::future::Either::Left((outcome, _)) => outcome,
                futures::future::Either::Right(_) => Err(RecoveryError::ClientStopped),
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

/// The completion rule from the plan's fixed decisions.
pub(crate) async fn complete(
    sdk: &Arc<SdkInner>,
    federation: &Arc<FederationInner>,
    attempt: UpstreamOperationId,
) {
    let id = federation.id;
    // The swap is what makes the recovered wallet usable: a v1 mint recovers as
    // `RecoveryMode::Unusable` and only enters the module registry on the client's next build
    // (`fedimint-mint-client/src/lib.rs:839-841`, `fedimint-client/src/client/builder.rs:951`),
    // so every generation is swapped even though only some of them need it.
    let opened = federation
        .replace_client(|| async { sdk.open_client(&id).await })
        .await;
    if let Some(record) = read_attempt_record(federation, attempt).await
        && let Err(err) = write_final_state(federation, attempt, record, RecoveryState::Done).await
    {
        tracing::warn!(
            target: "fedimint_sdk",
            federation = %federation.id,
            attempt = %attempt.fmt_full(),
            error = %err,
            "could not record a finished recovery attempt as done",
        );
    }
    federation.bump_recovery();
    match opened {
        Ok(()) => federation.set_status(FederationStatus::Running),
        // The federation was closed, quarantined or erased while the rescan was finishing;
        // the swap was refused, and whatever closed it owns the status now.
        Err(err) if err.code == crate::ErrorCode::FederationClosed => return,
        Err(err) => federation.set_status(FederationStatus::Quarantined {
            diagnostic: err.into(),
        }),
    }
    sdk.announce(federation);
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
}
