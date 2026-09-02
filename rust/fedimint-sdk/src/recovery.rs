//! Seed-based wallet recovery.
//!
//! Recovery is the one operation whose failure mode is silent fund loss: a
//! rescan that stops early, or that double-applies a checkpoint after a
//! crash, produces a wallet that looks fine and is not. The contract in this
//! module is written against that: a recovery either completes, and the
//! wallet is restored, or it has not completed, and the wallet is not
//! spendable. An implementation owes crash-at-every-checkpoint idempotency
//! tests for it — kill the process at each persisted checkpoint of a
//! recovery, restart, and assert the recovered wallet is identical to one
//! recovered without interruption — and nothing here is relaxed to make
//! those easier to pass.
//!
//! # The recovery lock
//!
//! A federation whose recovery is **incomplete** is locked. Every ecash,
//! lightning and on-chain send and receive against it fails with
//! [`Recovering`](crate::ErrorCode::Recovering), as does
//! [`Federation::backup`](crate::Federation::backup). Wherever else this
//! crate says an action is refused "while a recovery is in progress", this
//! lock is what is meant, and this section is the authoritative definition
//! of when it is held.
//!
//! **Exactly one thing releases it: a recovery for that federation reaching
//! [`RecoveryState::Done`].** Everything else leaves it in place:
//!
//! - A recovery that **stopped** — [`RecoveryState::Failed`] — does not
//!   release it. That state is final for the *attempt*, not for the wallet:
//!   the rescan is no longer running, and the wallet is still incomplete.
//!   A stopped recovery is a recovery in progress as far as the lock is
//!   concerned.
//! - Restarting the process does not release it. The lock lives in the
//!   federation's persisted state, not in this instance's memory, so it
//!   survives a crash, a [`Sdk::shutdown`](crate::Sdk::shutdown), and a
//!   [`Sdk::close_federation`](crate::Sdk::close_federation) followed by
//!   [`Sdk::reopen_federation`](crate::Sdk::reopen_federation), which is the
//!   way back for a federation this instance still holds — joining it again
//!   would be refused, since it was never forgotten.
//! - No call in this module releases it. There is deliberately no "spend
//!   anyway", no "mark recovered", and no way to acknowledge a failure into
//!   a usable wallet, because the whole point of the lock is that a payment
//!   funded from a note set that was never fully discovered can double-spend
//!   a note the rescan never reached. An incompletely restored wallet is
//!   never spendable.
//!
//! [`RecoveryState::is_complete`] is the predicate to gate fund-touching UI
//! on. [`OperationState::is_final`](crate::OperationState::is_final) is
//! **not**: it answers a question about the operation ("will this state
//! change again?"), and both `Done` and `Failed` answer it yes. Using it to
//! decide whether the wallet is usable is exactly the mistake this section
//! exists to prevent.
//!
//! If the lock is unacceptable to keep waiting on, the way out is the erase
//! path below — not a release.
//!
//! # Getting back to a recovery
//!
//! [`Sdk::recover`] is the entry point for a federation this instance has
//! not joined yet: it joins *and* starts the recovery, and like
//! [`Sdk::join`](crate::Sdk::join) it refuses an already-joined federation
//! with [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined). That makes it
//! usable exactly once per federation — the join it performs is what puts
//! the federation out of its own reach — so it cannot be the way back to a
//! recovery that stopped, and it is not asked to be.
//!
//! Two entry points cover everything after that first call, and neither
//! needs the invite code again, because the federation's configuration is
//! already in storage:
//!
//! - [`Sdk::recovery_status`] reads where a federation's recovery stands. It
//!   starts nothing, contacts no guardian, and reports "this federation was
//!   never recovered" as an ordinary `None` rather than as an error — a
//!   caller can ask about any federation it holds without provoking a
//!   failure.
//! - [`Sdk::resume_recovery`] returns a live [`Recovery`] for a federation
//!   this instance already holds, resuming the running attempt or starting a
//!   fresh one if the last attempt stopped. This is the retry
//!   [`RecoveryState::Failed`] tells the caller to make.
//!
//! Three ways to reattach after a restart, in order of what the application
//! kept:
//!
//! 1. **It kept the [`OperationId`](crate::OperationId).**
//!    [`Federation::operation`](crate::Federation::operation) then
//!    [`AnyOperation::as_recovery`] gives back the typed handle — to *that
//!    attempt*, which is not always the current one: if it reads
//!    [`RecoveryState::Failed`], a reopen may already have started a newer
//!    attempt under a different id (see below), so check
//!    [`Sdk::recovery_status`] as in case 2 before concluding the recovery
//!    is stuck.
//! 2. **It kept only the [`FederationId`].** [`Sdk::recovery_status`] to
//!    read the state, [`Sdk::resume_recovery`] to get a handle back.
//! 3. **It kept nothing.**
//!    [`Sdk::stored_federations`](crate::Sdk::stored_federations) is the
//!    list to start from — not
//!    [`Sdk::federations`](crate::Sdk::federations), which lists only the
//!    federations this instance currently has *open*. A still-recovering
//!    federation is open and appears in both, and case 2 applies to it
//!    directly. Two kinds do not: one closed with
//!    [`Sdk::close_federation`](crate::Sdk::close_federation), and one
//!    quarantined — which is where a [`Sdk::recover`] whose join committed
//!    but whose open then failed leaves its federation (see that method).
//!    Both appear only in the stored list, labelled, and
//!    [`Sdk::reopen_federation`](crate::Sdk::reopen_federation) brings
//!    either back — still recovery-locked — before case 2 applies to it.
//!
//! # Reopening restarts a stopped attempt by itself
//!
//! The underlying client deliberately does not persist a failed recovery as
//! failed: a stopped attempt's progress rests at its last durable
//! checkpoint, and opening the federation again resumes the rescan from
//! there *automatically* — whether the open came from
//! [`Sdk::reopen_federation`](crate::Sdk::reopen_federation) or from
//! [`SdkBuilder::build`](crate::SdkBuilder::build) bringing up the open
//! federations at startup. The SDK cannot veto that restart, so it makes it
//! observable instead: an open that finds the federation's last recorded
//! attempt at [`RecoveryState::Failed`] mints the new attempt itself —
//! a new [`OperationId`](crate::OperationId), exactly as
//! [`Sdk::resume_recovery`] would — records it durably *before the
//! underlying open is asked to do anything*, and points
//! [`Sdk::recovery_status`] at it. Before invoking, not merely before
//! returning: the underlying open spawns the recovery tasks while it runs,
//! so a rescan can be advancing — or finished — while the open call is
//! still in flight, and a crash in that window must not leave an unlogged
//! attempt running with the SDK's own record still naming the old one, or
//! the next boot would mint a third. The stopped attempt
//! stays in the operation log untouched. `Failed` is therefore a statement
//! about one attempt, never a promise that nothing is running now: after
//! any reopen, the state to trust is [`Sdk::recovery_status`]'s, and
//! [`Sdk::resume_recovery`] then reattaches to the attempt the reopen
//! already started rather than starting another.
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
//!         // Running or stopped — either way the federation is locked, and
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
//! A retry that keeps stopping needs an exit, and because the lock is never
//! released on an incomplete wallet, the only exit is to throw the
//! incomplete wallet away: erase the federation's local state with
//! [`Sdk::forget_federation`](crate::Sdk::forget_federation) — the
//! destructive half of leaving a federation — and then join it again from
//! the invite code with [`Sdk::recover`], which starts the recovery over
//! from nothing.
//!
//! What that costs, stated plainly:
//!
//! - **The invite code is needed again.** Erasing the federation erases its
//!   configuration too, so an application that offers this path must have
//!   kept the invite code (or be able to ask the user for it).
//! - **Local history does not come back.** Activity history is local-only
//!   and a restore reconstructs spendable value, not a narrative — see
//!   [`ActivityItem`](crate::ActivityItem). Erasing it is permanent.
//! - **It is not a shortcut to a usable wallet.** The fresh recovery locks
//!   the federation again until it reaches `Done`. This path replaces one
//!   incomplete attempt with a clean one; it does not lift the invariant.
//! - **Value the local state alone could have reclaimed is forfeited.**
//!   Out-of-band notes this instance handed out and could still have
//!   reclaimed are recorded only locally, so erasing that record gives up
//!   the reclaim.
//!
//! For this exit to exist at all, the erase path must be willing to run on a
//! recovery-locked federation, whether its last attempt is still running or
//! has stopped, and to abort a running rescan as part of the same
//! transactional erase. In particular a guard that refuses to erase a
//! federation which still reports a balance must not count the provisional
//! balance of a locked one: that balance is unspendable *because* of the
//! lock, so counting it would close the last door — the recovery cannot be
//! finished, the balance cannot be spent down, and the federation cannot be
//! erased. There is intentionally no call here to stop or cancel a running
//! recovery, so the erase is the only mechanism that can end one.

use crate::{Federation, FederationId, InviteCode, Operation, OperationState, Result, Sdk};

impl Sdk {
    /// Joins a federation and restores this seed's wallet in it from the
    /// federation's backup plus a rescan.
    ///
    /// Use this instead of [`Sdk::join`] when the instance was built from a
    /// mnemonic the user restored and the federation may already hold funds
    /// belonging to that seed. A plain join starts a fresh client and would
    /// not look for them.
    ///
    /// The call returns as soon as the recovery has started, with a
    /// [`Recovery`] carrying both the joined [`Federation`] and the
    /// [`Operation`] tracking the rescan — recovery can take a long time,
    /// so it is observed like any other background operation rather than
    /// awaited inline. From that moment the federation is recovery-locked;
    /// see the module documentation for what that refuses and what releases
    /// it.
    ///
    /// This method is the *first* step only. Because it joins, calling it
    /// again for the same federation is
    /// [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined), which is why
    /// resuming or retrying a recovery is [`Sdk::resume_recovery`] instead:
    /// that takes a [`FederationId`], needs no invite code, and works on
    /// precisely the joined federation this one refuses.
    ///
    /// # A failed call may still have joined
    ///
    /// The join is not the last fallible step. The underlying client
    /// commits the join durably — configuration, secret hash, and the
    /// pending-recovery marker — *before* the steps that can still fail
    /// after it, so an `Err` from this call does not certify that nothing
    /// happened: a [`Timeout`](crate::ErrorCode::Timeout) or
    /// [`Storage`](crate::ErrorCode::Storage) can arrive with the
    /// federation already joined and already committed to recovering.
    ///
    /// The SDK makes that window survivable by ordering its own writes
    /// first: the recovery intent, and the operation id this call would
    /// have returned, are persisted durably *before* the upstream join is
    /// asked to mutate anything. One guarantee follows, and it holds for
    /// **every** error this call can return: if the join committed, the
    /// SDK's recovery record exists and the federation is not lost. It is
    /// not *open* either — the failure struck before a live handle existed
    /// — so it surfaces as
    /// [`Quarantined`](crate::FederationStatus::Quarantined) with the error
    /// as its diagnostic, and the way back runs through the lifecycle calls
    /// before the recovery ones:
    /// [`Sdk::federation_status`](crate::Sdk::federation_status) to see it,
    /// then [`Sdk::reopen_federation`](crate::Sdk::reopen_federation),
    /// whose open resumes the committed recovery itself and adopts the
    /// pre-minted attempt (see the module documentation's reopen section).
    /// Only then do [`Sdk::recovery_status`] and [`Sdk::resume_recovery`]
    /// answer — both report
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) while no
    /// live handle exists, so pointing at them *first* would be a
    /// contradiction, not a route.
    ///
    /// Every call on that route takes a [`FederationId`], and the failed
    /// caller has one without any help from the error: the id is encoded in
    /// the invite code itself, and
    /// [`InviteCode::federation_id`](crate::InviteCode::federation_id)
    /// reads it locally, before this call is ever made. That is what makes
    /// the route deterministic — an application recovering several
    /// federations at once does not have to guess which quarantined row is
    /// whose — and it is why the error carries no id of its own. A retry of
    /// *this* call after such an error reports
    /// [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined) — the signpost to
    /// that route, not a dead end. An intent written for a join that never
    /// committed is inert and harmless: only the underlying client's own
    /// durable recovery marker, not the intent alone, makes a federation
    /// count as recovering, so the next `recover` for the same federation
    /// supersedes the leftover and a plain
    /// [`Sdk::join`](crate::Sdk::join) discards it in the same transaction
    /// that joins.
    ///
    /// # Errors
    ///
    /// The same errors as [`Sdk::join`]:
    /// [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) — each with
    /// the may-already-have-joined caveat above.
    pub async fn recover(&self, invite: &InviteCode) -> Result<Recovery> {
        unimplemented!()
    }

    /// Resumes, or retries, the recovery of a federation this instance
    /// already holds.
    ///
    /// This is the entry point [`Sdk::recover`] cannot be. By the time a
    /// recovery needs resuming the federation is joined, and `recover`
    /// refuses a joined federation with
    /// [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined); without this
    /// method the retry that [`RecoveryState::Failed`] tells a caller to
    /// make would be unreachable. It takes a [`FederationId`] rather than
    /// an [`InviteCode`] for the same reason: the federation's
    /// configuration is already in storage, and requiring the invite code
    /// again would mean an application had to keep one around for a
    /// federation it had already joined.
    ///
    /// The call is idempotent with respect to its goal — "this federation's
    /// wallet is restored from the seed" — so what it does depends on where
    /// the federation stands, in the cases [`Sdk::recovery_status`]
    /// reports:
    ///
    /// - **A recovery is running.** Nothing new is started. The returned
    ///   [`Recovery`] observes the attempt that is already running, which
    ///   is how an application reattaches to it after a restart without
    ///   having kept the operation id. An attempt a reopen started on its
    ///   own lands here too — reopening a federation whose last attempt
    ///   stopped restarts the rescan automatically; see the module
    ///   documentation. *Running* means verified against the underlying
    ///   client, not taken from the record's word: a current-attempt record
    ///   whose rescan is not actually live (see the cancellation note
    ///   below) is completed here — the restart is finished, then observed
    ///   — rather than trusted and watched.
    /// - **The last attempt stopped** ([`RecoveryState::Failed`]). A new
    ///   attempt starts, with a new [`OperationId`](crate::OperationId).
    ///   The stopped attempt is not rewritten or removed: it stays in the
    ///   operation log and in activity history, so a recovery that fails
    ///   repeatedly leaves a trail to diagnose rather than one row that
    ///   keeps changing its mind. The federation was locked throughout and
    ///   stays locked.
    ///
    ///   Starting the attempt is **not** cancellable mid-flight by
    ///   dropping this call's future. Once the new attempt's record is
    ///   durable, the client restart it requires runs in a task the SDK
    ///   owns, so a caller that times out or is dropped abandons its
    ///   *observation* of the retry, never the retry itself — which
    ///   otherwise could shut the old client down and vanish before the
    ///   rebuild, leaving a record claiming `Running` above a federation
    ///   with no live handle. Should the whole process die instead, the
    ///   durable record plus the verified-running rule above make the next
    ///   open or resume complete the restart rather than believe the
    ///   record.
    /// - **A recovery completed** ([`RecoveryState::Done`]). Nothing is
    ///   started and nothing is rescanned; the returned `Recovery` carries
    ///   the completed one, and its
    ///   [`progress`](Recovery::progress) reads `Done`. This is `Ok`
    ///   rather than an error for the reason
    ///   [`Sdk::close_federation`](crate::Sdk::close_federation) is
    ///   idempotent: the postcondition the call promises already holds.
    /// - **This federation was never recovered.** Refused; see below.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) when this instance
    /// holds the federation but has no recovery for it, because it was
    /// joined with [`Sdk::join`] rather than [`Sdk::recover`]. That is the
    /// only way the record can be missing: `recover` persists its recovery
    /// intent *before* it lets upstream join (see its docs), so a
    /// federation a failed `recover` call left joined still carries the
    /// record, and this call accepts it. The converse holds too: an intent
    /// whose upstream join never committed is not a record — it is
    /// discarded by the next plain join and superseded by the next
    /// `recover` — so presence of a *corroborated* record is what this call
    /// keys on, and a plainly joined federation cannot ride in on a
    /// leftover. Resuming a
    /// recovery is a different request from starting the first one, and
    /// this call deliberately does not do the second: turning a plainly
    /// joined federation into a recovering one would re-derive its client
    /// state from a backup while local state derived from that same seed
    /// already exists, which is a double-application hazard no rescan can
    /// be trusted to survive. A wallet that should have been
    /// recovered and was joined plainly instead has to take the erase path
    /// in the module documentation. (Reporting a well-formed id that this
    /// call does not apply to as `InvalidInput` matches
    /// [`Federation::activity`](crate::Federation::activity), which uses it
    /// for a cursor the federation did not issue.)
    ///
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) when the id
    /// names no open federation — never joined, or closed with
    /// [`Sdk::close_federation`](crate::Sdk::close_federation) — or when
    /// the whole instance has been shut down.
    /// [`Sdk::reopen_federation`](crate::Sdk::reopen_federation) brings a
    /// closed federation back, recovery record and all, and this call then
    /// works on it — re-joining is neither needed nor accepted, because the
    /// federation was closed rather than forgotten.
    ///
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable)
    /// and [`Timeout`](crate::ErrorCode::Timeout) when the guardians cannot
    /// be reached to fetch the backup a new attempt starts from, and
    /// [`Storage`](crate::ErrorCode::Storage) if the attempt cannot be
    /// recorded durably. None of these releases the lock or unwinds the
    /// recovery record — but they are not all equally clean to retry,
    /// because of how a new attempt has to start. The underlying client
    /// only derives and spawns recoveries when it is built, so retrying a
    /// *stopped* attempt on an open federation means shutting the live
    /// client down and rebuilding it — and the shutdown comes first, before
    /// the step that can still fail. An error from that rebuild therefore
    /// leaves the federation with no live handle: it transitions to
    /// [`Quarantined`](crate::FederationStatus::Quarantined) carrying the
    /// error as its diagnostic, this call reports
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) until
    /// [`Sdk::reopen_federation`](crate::Sdk::reopen_federation) brings the
    /// federation back, and — because the new attempt's id was persisted
    /// durably before the rebuild was attempted — that id survives as the
    /// current attempt, which the reopen then resumes itself rather than
    /// minting another. An error raised *before* the rebuild begins leaves
    /// the running client untouched, and there the call can simply be made
    /// again.
    pub async fn resume_recovery(&self, id: &FederationId) -> Result<Recovery> {
        unimplemented!()
    }

    /// Where this federation's recovery stands, or `None` if it never had
    /// one.
    ///
    /// A read, not a request: it starts nothing, resumes nothing, contacts
    /// no guardian, and leaves the federation exactly as it was. It exists
    /// so that the recovery lock is *discoverable* — an application can ask
    /// whether spending is refused before offering to spend, instead of
    /// attempting a payment and interpreting
    /// [`Recovering`](crate::ErrorCode::Recovering), which is the
    /// error-driven discovery this crate rejects elsewhere and has no
    /// reason to require here.
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
    /// federation that has no recovery is `Ok(None)`, never an error — that
    /// is the whole point of the `Option`.
    pub async fn recovery_status(&self, id: &FederationId) -> Result<Option<RecoveryState>> {
        unimplemented!()
    }
}

/// A federation that is being recovered, plus the operation doing it.
///
/// Returned by [`Sdk::recover`], which joins the federation and starts the
/// first attempt, and by [`Sdk::resume_recovery`], which hands back the
/// running attempt or starts a fresh one for a federation already joined.
///
/// # What is usable while the recovery is incomplete
///
/// The [`Federation`] handle is live immediately — its identity, network,
/// metadata, and capabilities are all readable, and an application can show
/// the federation in its list right away. What is *not* trustworthy yet is
/// anything derived from the wallet's contents:
///
/// - **Spending and receiving are refused.** Every ecash, lightning, and
///   on-chain send or receive against this federation fails with
///   [`ErrorCode::Recovering`](crate::ErrorCode::Recovering) for as long as
///   the recovery is incomplete — which includes after an attempt has
///   stopped, not only while one is running. This is not a race the SDK
///   tries to win: a payment funded from a note set that is still being
///   discovered could double-spend a note the rescan has not reached yet,
///   and that is no less true of a rescan that stopped early than of one
///   still going. See the module documentation for what releases the lock.
/// - **Balance and activity are incomplete and moving.**
///   [`Federation::balance`](crate::Federation::balance) reports what has
///   been recovered *so far* and will generally rise as the rescan
///   proceeds; [`Federation::activity`](crate::Federation::activity) shows
///   only what has been reconstructed so far. Both are safe to display —
///   and worth displaying, so the user sees progress — but an application
///   should label them as provisional rather than presenting a partial
///   balance as the final one. A provisional balance on a locked federation
///   is not spendable no matter what it says.
///
/// Observe [`Recovery::progress`] to know when that changes, and gate
/// anything fund-touching on [`RecoveryState::is_complete`] rather than on
/// the operation merely having finished. The operation is an ordinary
/// background operation: it survives restarts, resumes on the next build,
/// and dropping this struct does not stop it — nor does dropping it release
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
/// Deliberately coarse. Upstream recovery does not currently expose a
/// meaningful completion fraction, and a made-up percentage would be worse
/// than none; this reports only what can be said truthfully. Finer-grained
/// progress is an additive change if upstream grows it.
///
/// # Two different questions
///
/// This enum answers both, and they are not the same question:
///
/// - *Is the attempt over?*
///   [`OperationState::is_final`](crate::OperationState::is_final) — true
///   for [`Done`](Self::Done) and for [`Failed`](Self::Failed), because
///   neither transitions again. Retrying starts a *new* operation with a
///   new id rather than reviving this one.
/// - *Is the wallet restored, and is the federation spendable?*
///   [`is_complete`](Self::is_complete) — true for [`Done`](Self::Done)
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
    /// releases nothing —
    /// [`is_complete`](Self::is_complete) is false — even though
    /// [`OperationState::is_final`](crate::OperationState::is_final) is
    /// true for it, which is a statement about the operation and not about
    /// the wallet.
    ///
    /// Two things can follow, and nothing else:
    ///
    /// - **Retry.** [`Sdk::resume_recovery`] starts a fresh attempt for
    ///   this federation. It needs only the [`FederationId`], because the
    ///   federation is already joined. A retry can also arrive without
    ///   being asked for: reopening the federation restarts a stopped
    ///   rescan by itself, minting the new attempt exactly as
    ///   `resume_recovery` would — see the module documentation.
    /// - **Erase and start over.** If retrying keeps stopping, the
    ///   documented exit is to erase the federation's local state and
    ///   recover it again from the invite code, with the costs the module
    ///   documentation lists. There is no third option that makes this
    ///   wallet spendable as it is.
    Failed {
        /// Human-readable explanation. Diagnostic only — not a stable
        /// contract, and not something to match on.
        reason: String,
    },
}

impl RecoveryState {
    /// Whether the wallet is fully restored — and therefore whether the
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
    ///
    /// A plain predicate over the enum, exactly like
    /// [`OperationState::is_final`](crate::OperationState::is_final), so a
    /// binding that cannot carry a method on an enum re-derives it from the
    /// variant instead of needing anything new to cross the boundary.
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
}
