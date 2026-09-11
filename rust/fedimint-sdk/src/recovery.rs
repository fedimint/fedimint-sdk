//! Seed-based wallet recovery.
//!
//! Recovery restores a wallet from the seed alone: a federation's backup plus a rescan of the
//! history it references. Its failure mode is silent fund loss if handled wrong, so this
//! module's contract is written to rule that out: a recovery either completes and the wallet
//! is restored, or it has not completed and the wallet is not spendable. There is no state in
//! between that is safe to spend from.
//!
//! # The recovery lock
//!
//! A federation whose recovery is **incomplete** is locked. Every ecash, lightning and
//! on-chain send and receive against it fails with
//! [`Recovering`](crate::ErrorCode::Recovering), as does
//! [`Federation::backup`](crate::Federation::backup). Wherever else this crate says an action
//! is refused "while a recovery is in progress", this is the lock meant, and this section is
//! its authoritative definition.
//!
//! **Exactly one thing releases it: a recovery for that federation reaching
//! [`RecoveryState::Done`].** In particular:
//!
//! - A recovery that **stopped** ([`RecoveryState::Failed`]) does not release it. That state
//!   is final for the attempt, not for the wallet: the rescan is no longer running and the
//!   wallet is still incomplete, so it counts as a recovery in progress.
//! - Restarting the process does not release it. The lock lives in the federation's persisted
//!   state, so it survives a crash, an [`Sdk::shutdown`](crate::Sdk::shutdown), and a
//!   [`Sdk::close_federation`](crate::Sdk::close_federation) followed by
//!   [`Sdk::reopen_federation`](crate::Sdk::reopen_federation).
//! - There is no call that releases it directly: no "spend anyway", no "mark recovered". A
//!   payment funded from a note set that was never fully discovered could double-spend a note
//!   the rescan never reached, so an incompletely restored wallet is never spendable.
//!
//! [`RecoveryState::is_complete`] is the predicate to gate fund-touching UI on.
//! [`OperationState::is_final`](crate::OperationState::is_final) is **not**: both `Done` and
//! `Failed` answer "will this state change again?" with yes, and using that to decide whether
//! the wallet is usable is exactly the mistake this section exists to prevent.
//!
//! If the lock is unacceptable to keep waiting on, the way out is the erase path below, not a
//! release.
//!
//! # Getting back to a recovery
//!
//! [`Sdk::recover`] is the entry point for a federation this instance has not joined yet: it
//! joins and starts the recovery in one call, and like [`Sdk::join`](crate::Sdk::join) it
//! refuses an already-joined federation with
//! [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined). It is therefore usable only once per
//! federation and is not the way back to a recovery that stopped.
//!
//! Two entry points cover everything after that first call, and neither needs the invite code
//! again:
//!
//! - [`Sdk::recovery_status`] reads where a federation's recovery stands. It starts nothing
//!   and contacts no guardian, and reports "this federation was never recovered" as an
//!   ordinary `None` rather than an error.
//! - [`Sdk::resume_recovery`] returns a live [`Recovery`] for a federation this instance
//!   already holds, resuming a running attempt or starting a fresh one if the last attempt
//!   stopped. This is the retry [`RecoveryState::Failed`] tells the caller to make.
//!
//! Three ways to reattach after a restart, in order of what the application kept:
//!
//! 1. **It kept the [`OperationId`](crate::OperationId).**
//!    [`Federation::operation`](crate::Federation::operation) then
//!    [`AnyOperation::as_recovery`] gives back the typed handle, to that attempt specifically.
//!    If it reads [`RecoveryState::Failed`], a newer attempt may already be running under a
//!    different id (see the next section), so check [`Sdk::recovery_status`] before
//!    concluding the recovery is stuck.
//! 2. **It kept only the [`FederationId`].** [`Sdk::recovery_status`] to read the state,
//!    [`Sdk::resume_recovery`] to get a handle back.
//! 3. **It kept nothing.** [`Sdk::stored_federations`](crate::Sdk::stored_federations) is the
//!    list to start from, not [`Sdk::federations`](crate::Sdk::federations), which lists only
//!    federations currently open. A still-recovering federation is open and appears in both,
//!    and case 2 applies to it directly. A federation closed with
//!    [`Sdk::close_federation`](crate::Sdk::close_federation), and one left quarantined by a
//!    [`Sdk::recover`] call whose join committed but whose open then failed (see that
//!    method), appear only in the stored list, labelled;
//!    [`Sdk::reopen_federation`](crate::Sdk::reopen_federation) brings either back, still
//!    recovery-locked, before case 2 applies.
//!
//! # Reopening restarts a stopped attempt by itself
//!
//! Opening a federation whose last recorded attempt is [`RecoveryState::Failed`] resumes the
//! rescan automatically, whether the open came from
//! [`Sdk::reopen_federation`](crate::Sdk::reopen_federation) or from
//! [`SdkBuilder::build`](crate::SdkBuilder::build) bringing up open federations at startup.
//! The new attempt gets a new [`OperationId`](crate::OperationId), exactly as
//! [`Sdk::resume_recovery`] would produce, and [`Sdk::recovery_status`] is updated to point at
//! it before the open finishes. The stopped attempt stays in the operation log, untouched.
//!
//! `Failed` therefore describes one attempt, never a promise that nothing is running now:
//! after any reopen, the state to trust is [`Sdk::recovery_status`]'s, and
//! [`Sdk::resume_recovery`] then reattaches to whatever attempt is actually current rather
//! than starting another.
//!
//! ```no_run
//! use fedimint_sdk::{FederationId, RecoveryState, Sdk};
//!
//! /// Makes sure this federation's wallet is restored, retrying a recovery
//! /// that stopped, and reports whether it ended up complete.
//! async fn ensure_restored(sdk: &Sdk, id: &FederationId) -> fedimint_sdk::Result<bool> {
//!     match sdk.recovery_status(id).await? {
//!         // Never recovered here, so never locked: an ordinary federation.
//!         None => Ok(true),
//!         // Done. The lock is released and spends work.
//!         Some(state) if state.is_complete() => Ok(true),
//!         // Running or stopped, either way the federation is locked, and
//!         // either way this hands back a handle to watch, starting a fresh
//!         // attempt if the last one stopped.
//!         Some(_) => {
//!             let recovery = sdk.resume_recovery(id).await?;
//!             let mut updates = recovery.progress.updates();
//!             while let Some(state) = updates.next().await? {
//!                 if let RecoveryState::Failed { reason, .. } = &state {
//!                     // Still locked. Retry, or take the erase path.
//!                     println!("recovery stopped: {reason}");
//!                 }
//!             }
//!             Ok(recovery.progress.state().await?.is_complete())
//!         }
//!     }
//! }
//!
//! // Compiled, never called: running it needs a live federation.
//! fn main() {
//!     let _ = ensure_restored;
//! }
//! ```
//!
//! # When a recovery cannot be finished
//!
//! A retry that keeps stopping needs an exit, and because the lock is never released on an
//! incomplete wallet, the only exit is to throw the incomplete wallet away: erase the
//! federation's local state with
//! [`Sdk::forget_federation`](crate::Sdk::forget_federation), the destructive half of leaving
//! a federation, then join it again from the invite code with [`Sdk::recover`], which starts
//! the recovery over from nothing.
//!
//! What that costs, stated plainly:
//!
//! - **The invite code is needed again.** Erasing the federation erases its configuration
//!   too, so an application that offers this path must have kept the invite code, or be able
//!   to ask the user for it.
//! - **Local history does not come back.** Activity history is local-only and a restore
//!   reconstructs spendable value, not a narrative, see
//!   [`ActivityItem`](crate::ActivityItem). Erasing it is permanent.
//! - **It is not a shortcut to a usable wallet.** The fresh recovery locks the federation
//!   again until it reaches `Done`. This path replaces one incomplete attempt with a clean
//!   one; it does not lift the invariant.
//! - **Value the local state alone could have reclaimed is forfeited.** Out-of-band notes
//!   this instance handed out and could still have reclaimed are recorded only locally, so
//!   erasing that record gives up the reclaim.

use std::sync::Arc;

use fedimint_core::core::OperationId as UpstreamOperationId;
use fedimint_core::db::IDatabaseTransactionOpsCoreTyped;

use crate::db::{FederationRecord, OperationRecordKey, RecoveryRecord, StoredStatus};
use crate::federation::FederationInner;
use crate::operation::OperationInner;
use crate::{
    Federation, FederationId, FederationStatus, InviteCode, Operation, OperationState, Result, Sdk,
};

mod driver;
pub(crate) mod engine;
mod wire;

pub(crate) use driver::RecoveryDriver;

impl Sdk {
    /// Joins a federation and restores this seed's wallet in it from the
    /// federation's backup plus a rescan.
    ///
    /// Use this instead of [`Sdk::join`] when the instance was built from a mnemonic the user
    /// restored and the federation may already hold funds belonging to that seed. A plain
    /// join starts a fresh client and would not look for them.
    ///
    /// The call returns as soon as the recovery has started, with a [`Recovery`] carrying
    /// both the joined [`Federation`] and the [`Operation`] tracking the rescan: recovery can
    /// take a long time, so it is observed like any other background operation rather than
    /// awaited inline. From that moment the federation is recovery-locked; see the module
    /// documentation for what that refuses and what releases it.
    ///
    /// This method is the *first* step only. Calling it again for the same federation is
    /// [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined); resuming or retrying a recovery is
    /// [`Sdk::resume_recovery`] instead, which takes a [`FederationId`], needs no invite code,
    /// and works on precisely the joined federation this one refuses.
    ///
    /// # A failed call may still have joined
    ///
    /// An `Err` from this call does not certify that nothing happened: a
    /// [`Timeout`](crate::ErrorCode::Timeout) or [`Storage`](crate::ErrorCode::Storage) can
    /// arrive after the federation is already joined and already committed to recovering.
    ///
    /// When that happens the federation is not lost, but it is not open either, since the
    /// failure struck before a live handle existed. It surfaces as
    /// [`Quarantined`](crate::FederationStatus::Quarantined) with the error as its
    /// diagnostic, and the way back is:
    /// [`Sdk::federation_status`](crate::Sdk::federation_status) to see it, then
    /// [`Sdk::reopen_federation`](crate::Sdk::reopen_federation), whose open resumes the
    /// committed recovery itself. Only then do [`Sdk::recovery_status`] and
    /// [`Sdk::resume_recovery`] answer; both report
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) while no live handle exists.
    ///
    /// Every call on that route takes a [`FederationId`], and a failed caller has one without
    /// any help from the error: the id is encoded in the invite code itself, and
    /// [`InviteCode::federation_id`](crate::InviteCode::federation_id) reads it locally,
    /// before this call is ever made. A retry of *this* call after such an error reports
    /// [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined), the signpost to that route rather
    /// than a dead end.
    ///
    /// # Errors
    ///
    /// The same errors as [`Sdk::join`]:
    /// [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed), each with the
    /// may-already-have-joined caveat above.
    pub async fn recover(&self, invite: &InviteCode) -> Result<Recovery> {
        let _lifecycle = self.inner().lifecycle.lock().await;
        self.inner().alive()?;
        let id = invite.inner().federation_id();

        // An id already here is `AlreadyJoined`, closed and quarantined included, because
        // `reopen_federation` is the call that wants making. A committed erase is the exception:
        // it is finished first and then this is a first-time recovery of the same federation.
        if let Some(existing) = self.inner().federation_inner(&id) {
            if existing.status() == FederationStatus::Forgetting {
                self.inner().finish_erase(&id).await?;
                self.inner().remove(&id);
            } else {
                return Err(crate::Error::new(
                    crate::ErrorCode::AlreadyJoined,
                    "this instance already holds that federation",
                ));
            }
        }

        let config = self.inner().download_config(invite).await?;
        let preview = crate::modules::preview_of(&self.inner().module_inits, &config)?;
        let record = FederationRecord {
            invite: invite.inner().clone(),
            network: preview.network.into(),
            status: StoredStatus::Joining,
            capabilities: crate::modules::capabilities_of(&preview.modules).into(),
            generation: crate::modules::check_generation(&preview.modules)?,
            name: preview.name.clone(),
        };

        let attempt = UpstreamOperationId::new_random();
        // The intent is durable, together with the row it belongs to, before the client writes
        // a byte: a process killed anywhere after this comes back with the federation
        // recovering, and `SdkInner::start` redoes the recover under this same attempt id.
        crate::db::write_joining_with_recovery(
            &self.inner().db,
            &id,
            &record,
            &RecoveryRecord { attempt },
        )
        .await?;

        let client = match self.inner().recover_client(&id, &record).await {
            Ok(client) => client,
            Err(err) => {
                let namespace = self
                    .inner()
                    .db
                    .with_prefix(crate::db::federation_prefix(&id).to_vec());
                // Whether the underlying client got as far as writing into the federation's
                // namespace is what decides between the two outcomes the docs describe. Nothing
                // written means nothing joined: the row and its recovery record are dropped, as
                // a failed `Sdk::join` drops its own. Anything written means the join may have
                // committed, and the docs promise that such a federation is not lost: it stays
                // `Joining` with its recovery record, is reported `Quarantined` with this error
                // as its diagnostic, and the next open redoes the recovery under the same
                // attempt id.
                // An inconclusive read counts as committed: erasing is the destructive answer, and
                // it is only right when the namespace is known to be empty.
                let committed = crate::db::is_empty(&namespace)
                    .await
                    .map(|empty| !empty)
                    .unwrap_or(true);
                if !committed {
                    let _ = self.inner().finish_erase(&id).await;
                    self.inner().remove(&id);
                    return Err(err);
                }
                let federation = Arc::new(FederationInner::new(
                    id,
                    Arc::downgrade(self.inner()),
                    namespace,
                    record,
                    FederationStatus::Quarantined {
                        diagnostic: err.clone().into(),
                    },
                    None,
                ));
                self.inner().insert(federation.clone());
                self.inner().announce(&federation);
                return Err(err);
            }
        };

        let mut joined = record;
        joined.status = StoredStatus::Open;
        crate::db::write_federation(&self.inner().db, &id, &joined).await?;

        let federation = Arc::new(FederationInner::new(
            id,
            Arc::downgrade(self.inner()),
            self.inner()
                .db
                .with_prefix(crate::db::federation_prefix(&id).to_vec()),
            joined,
            FederationStatus::Running,
            Some(client.clone()),
        ));
        self.inner().insert(federation.clone());
        federation.record_recovery_attempt(attempt).await?;
        let status = engine::after_open(self.inner(), &federation, &client, Some(attempt)).await;
        federation.set_status(status);
        crate::federation::reconcile_on_open(&federation).await;
        self.inner().announce(&federation);
        Ok(Recovery {
            federation: Federation::new(federation.clone()),
            progress: progress_handle(&federation, attempt).await?,
        })
    }

    /// Resumes, or retries, the recovery of a federation this instance
    /// already holds.
    ///
    /// This is the entry point [`Sdk::recover`] cannot be, since by the time a recovery needs
    /// resuming the federation is joined and `recover` refuses it with
    /// [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined). It takes a [`FederationId`] rather
    /// than an [`InviteCode`] because the federation's configuration is already in storage.
    ///
    /// The call is idempotent with respect to its goal, "this federation's wallet is restored
    /// from the seed", so what it does depends on where the federation stands, in the cases
    /// [`Sdk::recovery_status`] reports:
    ///
    /// - **A recovery is running.** Nothing new is started. The returned [`Recovery`]
    ///   observes the attempt that is already running, which is how an application reattaches
    ///   to it after a restart without having kept the operation id. An attempt a reopen
    ///   started on its own lands here too, see the module documentation.
    /// - **The last attempt stopped** ([`RecoveryState::Failed`]). A new attempt starts, with
    ///   a new [`OperationId`](crate::OperationId). The stopped attempt is not rewritten or
    ///   removed: it stays in the operation log and in activity history, so a recovery that
    ///   fails repeatedly leaves a trail to diagnose rather than one row that keeps changing
    ///   its mind. The federation was locked throughout and stays locked.
    ///
    ///   Starting the attempt is **not** cancellable mid-flight by dropping this call's
    ///   future: a caller that times out or is dropped abandons its *observation* of the
    ///   retry, never the retry itself.
    /// - **A recovery completed** ([`RecoveryState::Done`]). Nothing is started and nothing
    ///   is rescanned; the returned `Recovery` carries the completed one, and its
    ///   [`progress`](Recovery::progress) reads `Done`. This is `Ok` rather than an error for
    ///   the reason [`Sdk::close_federation`](crate::Sdk::close_federation) is idempotent:
    ///   the postcondition the call promises already holds.
    /// - **This federation was never recovered.** Refused; see below.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) when this instance holds the
    /// federation but has no recovery for it, because it was joined with [`Sdk::join`] rather
    /// than [`Sdk::recover`]. Resuming a recovery is a different request from starting the
    /// first one, and this call deliberately does not do the second: turning a plainly
    /// joined federation into a recovering one would re-derive its client state from a backup
    /// while local state derived from that same seed already exists, a double-application
    /// hazard no rescan can be trusted to survive. A wallet that should have been recovered
    /// and was joined plainly instead has to take the erase path in the module documentation.
    ///
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) when the id names no open
    /// federation, never joined or closed with
    /// [`Sdk::close_federation`](crate::Sdk::close_federation), or when the whole instance
    /// has been shut down. [`Sdk::reopen_federation`](crate::Sdk::reopen_federation) brings a
    /// closed federation back, recovery record and all, and this call then works on it:
    /// re-joining is neither needed nor accepted.
    ///
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable) and
    /// [`Timeout`](crate::ErrorCode::Timeout) when the guardians cannot be reached to fetch
    /// the backup a new attempt starts from, and [`Storage`](crate::ErrorCode::Storage) if
    /// the attempt cannot be recorded durably. None of these releases the lock or unwinds the
    /// recovery record. An error raised while retrying a *stopped* attempt on an open
    /// federation can leave the federation with no live handle, in which case it transitions
    /// to [`Quarantined`](crate::FederationStatus::Quarantined) carrying the error as its
    /// diagnostic, this call reports
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) until
    /// [`Sdk::reopen_federation`](crate::Sdk::reopen_federation) brings it back, and the new
    /// attempt's id survives as the current attempt for that reopen to resume. An error
    /// raised before that point leaves the running client untouched, and the call can simply
    /// be made again.
    pub async fn resume_recovery(&self, id: &FederationId) -> Result<Recovery> {
        let _lifecycle = self.inner().lifecycle.lock().await;
        self.inner().alive()?;
        let upstream = id.inner();
        let federation = self
            .inner()
            .federation_inner(&upstream)
            .filter(|federation| federation.is_open())
            .ok_or_else(|| {
                crate::Error::new(
                    crate::ErrorCode::FederationClosed,
                    "this instance holds no open federation with that id",
                )
            })?;

        let Some((record, on_file)) = engine::attempt_on_file(self.inner(), &federation).await?
        else {
            return Err(crate::Error::new(
                crate::ErrorCode::InvalidInput,
                "this federation was joined, not recovered; there is no recovery to resume",
            ));
        };

        let attempt = match on_file {
            engine::AttemptOnFile::Done => {
                return Ok(Recovery {
                    federation: Federation::new(federation.clone()),
                    progress: progress_handle(&federation, record.attempt).await?,
                });
            }
            // "Running" is verified against the underlying client rather than taken on the
            // record's word: a current attempt whose rescan is not actually live is completed
            // here, exactly as the watcher would, rather than trusted and handed back to watch.
            engine::AttemptOnFile::None | engine::AttemptOnFile::Running => {
                let client = federation.client(false).await?.handle();
                if client.has_pending_recoveries() {
                    // Idempotent: repairs the crash window where the record is missing, and is
                    // a harmless rewrite of the same record otherwise.
                    federation.record_recovery_attempt(record.attempt).await?;
                } else {
                    engine::complete(self.inner(), &federation, record.attempt).await;
                }
                record.attempt
            }
            // The underlying client only derives and spawns recoveries when it is built, so
            // retrying a stopped attempt means rebuilding the client. The new attempt is made
            // durable before the rebuild, so it survives as the current attempt for a later
            // reopen to resume rather than minting another.
            engine::AttemptOnFile::Failed { .. } => {
                let new_attempt = engine::new_attempt(self.inner(), &federation).await?;
                match federation
                    .replace_client(|| async { self.inner().open_client(&upstream).await })
                    .await
                {
                    // The federation was closed, quarantined or erased while this call was
                    // minting the new attempt: whatever did that owns the status now, and this
                    // call must not quarantine a federation that is no longer this call's to
                    // decide about.
                    Err(err) if err.code == crate::ErrorCode::FederationClosed => {
                        return Err(err);
                    }
                    Err(err) => {
                        federation.set_status(FederationStatus::Quarantined {
                            diagnostic: err.clone().into(),
                        });
                        self.inner().announce(&federation);
                        return Err(err);
                    }
                    Ok(()) => {
                        let client = federation.client(false).await?.handle();
                        let status = engine::after_open(
                            self.inner(),
                            &federation,
                            &client,
                            Some(new_attempt),
                        )
                        .await;
                        federation.set_status(status);
                        self.inner().announce(&federation);
                    }
                }
                new_attempt
            }
        };

        Ok(Recovery {
            federation: Federation::new(federation.clone()),
            progress: progress_handle(&federation, attempt).await?,
        })
    }

    /// Where this federation's recovery stands, or `None` if it never had
    /// one.
    ///
    /// A read, not a request: it starts nothing, resumes nothing, contacts no guardian, and
    /// leaves the federation exactly as it was. It exists so the recovery lock is
    /// *discoverable*: an application can ask whether spending is refused before offering to
    /// spend, instead of attempting a payment and interpreting
    /// [`Recovering`](crate::ErrorCode::Recovering).
    ///
    /// What each answer means for the lock:
    ///
    /// | answer | the federation is |
    /// | --- | --- |
    /// | `None` | not locked; it was never recovered |
    /// | `Some(`[`Running`](RecoveryState::Running)`)` | locked; a rescan is under way |
    /// | `Some(`[`Failed`](RecoveryState::Failed)`)` | locked; the last attempt stopped |
    /// | `Some(`[`Done`](RecoveryState::Done)`)` | not locked; the wallet is restored |
    ///
    /// `None` means this federation was joined with
    /// [`Sdk::join`](crate::Sdk::join) rather than [`Sdk::recover`], so it
    /// never had a recovery and never had the lock. It does **not** mean
    /// "not recovering any more": a federation whose recovery finished
    /// keeps reporting `Some(RecoveryState::Done)` for the rest of its
    /// life, which is what makes the two situations distinguishable. Read
    /// the other way round: the lock is held exactly when this returns
    /// `Some(state)` with
    /// [`state.is_complete()`](RecoveryState::is_complete) false.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage) if the recovery record cannot
    /// be read, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) when the id
    /// names no open federation or the instance has been shut down. A
    /// federation that has no recovery is `Ok(None)`, never an error, that
    /// is the whole point of the `Option`.
    pub async fn recovery_status(&self, id: &FederationId) -> Result<Option<RecoveryState>> {
        self.inner().alive()?;
        let upstream = id.inner();
        let federation = self
            .inner()
            .federation_inner(&upstream)
            .filter(|federation| federation.is_open())
            .ok_or_else(|| {
                crate::Error::new(
                    crate::ErrorCode::FederationClosed,
                    "this instance holds no open federation with that id",
                )
            })?;

        let Some((_, on_file)) = engine::attempt_on_file(self.inner(), &federation).await? else {
            return Ok(None);
        };
        Ok(Some(match on_file {
            engine::AttemptOnFile::None | engine::AttemptOnFile::Running => RecoveryState::Running,
            engine::AttemptOnFile::Done => RecoveryState::Done,
            engine::AttemptOnFile::Failed { reason } => RecoveryState::Failed { reason },
        }))
    }
}

/// The typed handle over an attempt just written, built from the record as it reads right now.
///
/// Shared by [`Sdk::recover`] and [`Sdk::resume_recovery`], the two calls that hand a fresh or
/// resumed attempt back to the caller.
///
/// # Errors
///
/// [`Internal`](crate::ErrorCode::Internal) if `attempt` has no operation record: every caller
/// writes or verifies one first, so this is a bug in this crate rather than a condition an
/// application can be in.
async fn progress_handle(
    federation: &Arc<FederationInner>,
    attempt: UpstreamOperationId,
) -> Result<Operation<RecoveryState>> {
    let db = federation.db();
    let mut dbtx = db.begin_transaction_nc().await;
    let record = dbtx.get_value(&OperationRecordKey(attempt)).await;
    drop(dbtx);
    let record = record.ok_or_else(|| {
        crate::Error::new(
            crate::ErrorCode::Internal,
            format!("no record for recovery attempt {}", attempt.fmt_full()),
        )
    })?;
    Ok(Operation::attach(
        Arc::new(OperationInner {
            federation: federation.clone(),
            id: attempt,
            record,
        }),
        Arc::new(RecoveryDriver),
    ))
}

/// A federation that is being recovered, plus the operation doing it.
///
/// Returned by [`Sdk::recover`], which joins the federation and starts the
/// first attempt, and by [`Sdk::resume_recovery`], which hands back the
/// running attempt or starts a fresh one for a federation already joined.
///
/// # What is usable while the recovery is incomplete
///
/// The [`Federation`] handle is live immediately: its identity, network,
/// metadata, and capabilities are all readable, and an application can show
/// the federation in its list right away. What is *not* trustworthy yet is
/// anything derived from the wallet's contents:
///
/// - **Spending and receiving are refused.** Every ecash, lightning, and
///   on-chain send or receive against this federation fails with
///   [`ErrorCode::Recovering`](crate::ErrorCode::Recovering) for as long as
///   the recovery is incomplete, which includes after an attempt has
///   stopped, not only while one is running.
/// - **Balance and activity are incomplete and moving.**
///   [`Federation::balance`](crate::Federation::balance) reports what has
///   been recovered *so far* and will generally rise as the rescan
///   proceeds; [`Federation::activity`](crate::Federation::activity) shows
///   only what has been reconstructed so far. Both are safe to display,
///   and worth displaying so the user sees progress, but an application
///   should label them as provisional rather than presenting a partial
///   balance as the final one. A provisional balance on a locked federation
///   is not spendable no matter what it says.
///
/// Observe [`Recovery::progress`] to know when that changes, and gate
/// anything fund-touching on [`RecoveryState::is_complete`] rather than on
/// the operation merely having finished. The operation is an ordinary
/// background operation: it survives restarts, resumes on the next build,
/// and dropping this struct does not stop it, nor does dropping it release
/// the lock.
#[derive(Debug)]
#[non_exhaustive]
pub struct Recovery {
    /// The joined federation. Usable for identity and metadata
    /// immediately; spends and receives fail with
    /// [`Recovering`](crate::ErrorCode::Recovering) until a recovery for it
    /// reaches [`RecoveryState::Done`].
    pub federation: Federation,
    /// The attempt this call started or picked up, observable like any
    /// other operation. Reads [`RecoveryState::Done`] already if the
    /// federation's recovery had completed before
    /// [`Sdk::resume_recovery`] was called.
    pub progress: Operation<RecoveryState>,
}

/// How a recovery is going.
///
/// Deliberately coarse: this reports only what can be said truthfully, without a made-up
/// completion percentage.
///
/// # Two different questions
///
/// This enum answers both, and they are not the same question:
///
/// - *Is the attempt over?*
///   [`OperationState::is_final`](crate::OperationState::is_final), true
///   for [`Done`](Self::Done) and for [`Failed`](Self::Failed), because
///   neither transitions again. Retrying starts a *new* operation with a
///   new id rather than reviving this one.
/// - *Is the wallet restored, and is the federation spendable?*
///   [`is_complete`](Self::is_complete), true for [`Done`](Self::Done)
///   only.
///
/// Reading the first as an answer to the second is the trap: it would let a
/// stopped recovery look like a finished one and an incomplete wallet look
/// spendable. The module documentation states the invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecoveryState {
    /// The rescan is running. Spends and receives are refused with
    /// [`Recovering`](crate::ErrorCode::Recovering); balance and activity
    /// are incomplete.
    Running,
    /// Final, and the wallet is recovered: the federation behaves like any
    /// other joined federation and the recovery lock is released.
    ///
    /// This is the only state that releases the lock, and the only one for
    /// which [`is_complete`](Self::is_complete) is true. It says the wallet
    /// is restored: everything the seed owned in this federation that a
    /// backup and a rescan can find has been found.
    Done,
    /// Final for this attempt, and the wallet is **not** recovered.
    ///
    /// The rescan stopped before completing. The federation stays joined
    /// and stays recovery-locked: spends and receives keep failing with
    /// [`Recovering`](crate::ErrorCode::Recovering), because a wallet whose
    /// note set was never fully discovered is not safe to spend from
    /// whether the rescan is still running or has given up. This state
    /// releases nothing, [`is_complete`](Self::is_complete) is false, even
    /// though [`OperationState::is_final`](crate::OperationState::is_final)
    /// is true for it, which is a statement about the operation and not
    /// about the wallet.
    ///
    /// Two things can follow, and nothing else:
    ///
    /// - **Retry.** [`Sdk::resume_recovery`] starts a fresh attempt for
    ///   this federation. It needs only the [`FederationId`], because the
    ///   federation is already joined. A retry can also arrive without
    ///   being asked for: reopening the federation restarts a stopped
    ///   rescan by itself, minting the new attempt exactly as
    ///   `resume_recovery` would, see the module documentation.
    /// - **Erase and start over.** If retrying keeps stopping, the
    ///   documented exit is to erase the federation's local state and
    ///   recover it again from the invite code, with the costs the module
    ///   documentation lists. There is no third option that makes this
    ///   wallet spendable as it is.
    Failed {
        /// Human-readable explanation. Diagnostic only, not a stable
        /// contract, and not something to match on.
        reason: String,
    },
}

impl RecoveryState {
    /// Whether the wallet is fully restored, and therefore whether the
    /// federation's recovery lock has been released.
    ///
    /// True for [`Done`](Self::Done) and nothing else. This is the
    /// predicate to gate anything fund-touching on: a spend button, a
    /// "wallet ready" banner, a background sweep. Its negation is exactly
    /// "this federation still refuses sends and receives with
    /// [`Recovering`](crate::ErrorCode::Recovering)".
    ///
    /// It is deliberately *not*
    /// [`OperationState::is_final`](crate::OperationState::is_final), which
    /// is true for [`Failed`](Self::Failed) too: an attempt that stopped is
    /// finished as an operation and unfinished as a recovery. See the type
    /// documentation.
    pub fn is_complete(&self) -> bool {
        matches!(self, RecoveryState::Done)
    }
}

impl crate::operation::sealed::Sealed for RecoveryState {}

impl OperationState for RecoveryState {
    fn is_final(&self) -> bool {
        match self {
            RecoveryState::Running => false,
            RecoveryState::Done | RecoveryState::Failed { .. } => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{OperationRecord, StoredCapabilities, StoredNetwork};
    use crate::operation::kinds;
    use crate::{ErrorCode, Storage};

    #[test]
    fn recovery_state_running_is_not_final() {
        assert!(!RecoveryState::Running.is_final());
    }

    #[test]
    fn recovery_state_done_is_final() {
        assert!(RecoveryState::Done.is_final());
    }

    #[test]
    fn recovery_state_failed_is_final() {
        assert!(
            RecoveryState::Failed {
                reason: String::new(),
            }
            .is_final()
        );
    }

    #[test]
    fn recovery_state_running_is_not_complete() {
        assert!(!RecoveryState::Running.is_complete());
    }

    #[test]
    fn recovery_state_done_is_complete() {
        assert!(RecoveryState::Done.is_complete());
    }

    /// The whole point of having two predicates: a stopped recovery is
    /// final as an operation and incomplete as a recovery, so the
    /// federation stays locked. Gating a spend on `is_final` would unlock
    /// an incompletely restored wallet.
    #[test]
    fn recovery_state_failed_is_final_but_not_complete() {
        let stopped = RecoveryState::Failed {
            reason: "guardian went away mid-rescan".to_string(),
        };
        assert!(stopped.is_final());
        assert!(!stopped.is_complete());
    }

    /// A real, in-memory instance: `recovery_status` and `resume_recovery` read and write the
    /// root store through a genuine `SdkInner`, and there is no cheaper way to get one here than
    /// the public builder, exactly as `engine`'s own tests do.
    async fn detached_sdk() -> Sdk {
        Sdk::builder()
            .storage(Storage::in_memory())
            .build()
            .await
            .expect("an instance opens")
    }

    /// Plants an open federation with no live client, so the lifecycle calls that read only
    /// records can be tested natively. Mirrors `sdk::tests::building::plant_closed_federation`,
    /// which cannot be reused from here: it is private to that module.
    async fn plant_open_federation(sdk: &Sdk, id: fedimint_core::config::FederationId) {
        let record = FederationRecord {
            invite: fedimint_core::invite_code::InviteCode::new(
                fedimint_core::util::SafeUrl::parse("wss://guardian.example:5000")
                    .expect("a valid url"),
                fedimint_core::PeerId::from(0),
                id,
                None,
            ),
            network: StoredNetwork::Regtest,
            status: StoredStatus::Open,
            capabilities: StoredCapabilities {
                ecash: true,
                lightning: false,
                onchain: false,
            },
            generation: Some(1),
            name: Some("Planted".to_owned()),
        };
        crate::db::write_federation(&sdk.inner().db, &id, &record)
            .await
            .expect("the row is written");
        sdk.inner().insert(Arc::new(FederationInner::new(
            id,
            Arc::downgrade(sdk.inner()),
            sdk.inner()
                .db
                .with_prefix(crate::db::federation_prefix(&id).to_vec()),
            record,
            FederationStatus::Running,
            None,
        )));
    }

    /// Plants an attempt's operation record directly, without going through
    /// `record_recovery_attempt`, so a test can set up any final state it wants.
    async fn plant_attempt_record(
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

    #[tokio::test(flavor = "multi_thread")]
    async fn recovery_status_of_an_unknown_id_is_federation_closed() {
        let sdk = detached_sdk().await;
        let id = FederationId::from_upstream(fedimint_core::config::FederationId::dummy());

        let err = sdk
            .recovery_status(&id)
            .await
            .expect_err("this instance holds no such federation");
        assert_eq!(err.code, ErrorCode::FederationClosed);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn recovery_status_of_a_plainly_joined_federation_is_none() {
        let sdk = detached_sdk().await;
        let upstream = fedimint_core::config::FederationId::dummy();
        plant_open_federation(&sdk, upstream).await;

        let status = sdk
            .recovery_status(&FederationId::from_upstream(upstream))
            .await
            .expect("a plainly joined federation is readable");
        assert_eq!(status, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn recovery_status_reads_the_attempt_on_file() {
        let sdk = detached_sdk().await;
        let upstream = fedimint_core::config::FederationId::dummy();
        plant_open_federation(&sdk, upstream).await;
        let federation = sdk
            .inner()
            .federation_inner(&upstream)
            .expect("just planted");

        let attempt = UpstreamOperationId([9u8; 32]);
        crate::db::write_recovery(&sdk.inner().db, &upstream, &RecoveryRecord { attempt })
            .await
            .expect("write the root record");
        plant_attempt_record(
            &federation,
            attempt,
            Some(
                wire::encode_state(&RecoveryState::Failed {
                    reason: "guardian gone".to_owned(),
                })
                .expect("encode"),
            ),
        )
        .await;

        let status = sdk
            .recovery_status(&FederationId::from_upstream(upstream))
            .await
            .expect("the attempt's record reads back");
        assert_eq!(
            status,
            Some(RecoveryState::Failed {
                reason: "guardian gone".to_owned(),
            })
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resume_recovery_refuses_a_plainly_joined_federation() {
        let sdk = detached_sdk().await;
        let upstream = fedimint_core::config::FederationId::dummy();
        plant_open_federation(&sdk, upstream).await;

        let err = sdk
            .resume_recovery(&FederationId::from_upstream(upstream))
            .await
            .expect_err("this federation was joined, not recovered");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn resume_recovery_of_a_completed_attempt_hands_back_done() {
        let sdk = detached_sdk().await;
        let upstream = fedimint_core::config::FederationId::dummy();
        plant_open_federation(&sdk, upstream).await;
        let federation = sdk
            .inner()
            .federation_inner(&upstream)
            .expect("just planted");

        let attempt = UpstreamOperationId([11u8; 32]);
        crate::db::write_recovery(&sdk.inner().db, &upstream, &RecoveryRecord { attempt })
            .await
            .expect("write the root record");
        plant_attempt_record(
            &federation,
            attempt,
            Some(wire::encode_state(&RecoveryState::Done).expect("encode")),
        )
        .await;

        let recovery = sdk
            .resume_recovery(&FederationId::from_upstream(upstream))
            .await
            .expect("a completed recovery is handed back rather than restarted");
        assert_eq!(
            recovery.progress.state().await.expect("state"),
            RecoveryState::Done
        );
    }
}
