//! The SDK root: one storage, one seed, many federations.

use std::sync::Arc;

use crate::{
    Diagnostic, Federation, FederationId, FederationPreview, InviteCode, Mnemonic, Network, Result,
    Storage,
};

/// A running SDK instance: one [`Storage`], one BIP-39 seed, and every
/// federation joined against them.
///
/// `Sdk` is the root object an application builds once at startup (see
/// [`Sdk::builder`]) and keeps for the lifetime of the process. It is a
/// cheap handle over shared internal state: cloning it costs an atomic
/// refcount bump and every clone observes the same federations, the same
/// storage, and the same background work. It is `Send` and `Sync` on every
/// target, including wasm, so clones can be moved between threads and tasks
/// freely.
///
/// # One seed, many federations
///
/// An instance holds exactly one mnemonic. Each federation's client secret
/// is derived from it, domain-separated by federation id, using the scheme
/// documented on [`Mnemonic`], so joining a second federation never reuses
/// the first federation's secret, and the same seed restored elsewhere
/// reproduces the same per-federation secrets. Storage is likewise shared
/// and namespaced per federation internally; applications do not manage
/// per-federation locations.
///
/// # Federation lifecycle
///
/// Every federation this instance's storage remembers is in exactly one of
/// five states, and every lifecycle call on this type is a transition
/// between them. ([`Forgotten`](FederationStatus::Forgotten) is the one
/// [`FederationStatus`] variant that is not among them: it is a
/// notification that a federation is gone, not a state one is stored in.)
///
/// - **[`Running`](FederationStatus::Running)**: open, everything the
///   federation offers is available.
/// - **[`Recovering`](FederationStatus::Recovering)**: open, with a live
///   handle, but the wallet is still being reconstructed from the seed, so
///   its balance and activity are incomplete and every spend and receive is
///   refused with [`Recovering`](crate::ErrorCode::Recovering) until the
///   reconstruction completes.
///
///   These two are the **open** states: exactly what [`Sdk::federations`]
///   lists and what [`Sdk::federation`] hands back a live handle for.
///   Nothing in this SDK completes or cancels a reconstruction on demand,
///   so an unfinished one survives closing, reopening and restarting; the
///   destructive [`Sdk::forget_federation`] is the only way to be rid of
///   one without finishing it.
/// - **[`Quarantined`](FederationStatus::Quarantined)**: stored and intact,
///   but not running, because the SDK could not or would not operate on it
///   (a refused configuration, unreadable local state, or no guardian
///   answering). Nothing is deleted, and the state carries the
///   [`ErrorCode`](crate::ErrorCode) and message that explain why.
/// - **[`Closed`](FederationStatus::Closed)**: stored and intact, not
///   running, because the application asked for that with
///   [`Sdk::close_federation`].
/// - **[`Forgetting`](FederationStatus::Forgetting)**: an erase has been
///   committed and is being carried out, or is waiting to be finished by a
///   retry or a later [`SdkBuilder::build`]. The federation is never opened
///   again, never handed a handle, and its balance, history and local state
///   are already gone as far as this API is concerned. The only way out is
///   out of the storage entirely.
///
/// **A stored federation is never silently absent.** [`Sdk::federations`]
/// lists what this instance has open; [`Sdk::stored_federations`] lists
/// everything the storage holds, in whatever state, and
/// [`Sdk::federation_status`] answers for a single id. Build a wallet list
/// from the second, so a closed, quarantined, or being-erased federation
/// appears as itself, with a reason, instead of disappearing between two
/// runs.
///
/// **No single federation takes the instance down with it.**
/// [`SdkBuilder::build`] fails only when the root storage or the seed is
/// unsound. A federation that cannot be opened is quarantined and reported
/// instead, and a federation whose erase cannot be completed is left
/// [`Forgetting`](FederationStatus::Forgetting) and retried later, rather
/// than failing the whole instance.
///
/// **Getting a stored federation open again takes one call and no invite
/// code.** [`Sdk::reopen_federation`] moves a federation out of
/// [`Closed`](FederationStatus::Closed) or
/// [`Quarantined`](FederationStatus::Quarantined) using the configuration
/// the SDK already holds.
///
/// Status changes are observable as they happen, through
/// [`Sdk::federation_status_updates`].
///
/// # Durability
///
/// **Every transition an application can observe is durably committed
/// before it becomes observable.** A joined federation is persisted before
/// [`Sdk::join`] returns its handle, an operation before the call that
/// started it hands back an [`Operation`](crate::Operation), a state before
/// [`OperationUpdates::next`](crate::OperationUpdates::next) yields it, and
/// an erase before it begins. There is no window in which the SDK has told
/// the caller that value moved and could still forget it. Consequently
/// [`Sdk::shutdown`] is an optimisation, not a correctness requirement: an
/// abrupt kill loses nothing that was acknowledged. See [`Sdk::shutdown`]
/// for exactly what a caller may rely on after the process dies without
/// warning.
///
/// # Closed handles
///
/// Any [`Federation`] handle for a federation that is no longer running,
/// and every handle after [`Sdk::shutdown`], fails its **fallible** calls
/// with [`ErrorCode::FederationClosed`](crate::ErrorCode::FederationClosed)
/// rather than panicking or silently doing nothing. Its infallible
/// accessors keep answering; see [`Federation`] for what each of them
/// reports. The reason the federation stopped running is not encoded in
/// that error, one code covers all of them, and is read from
/// [`Sdk::federation_status`] instead.
///
/// A [`Recovering`](FederationStatus::Recovering) federation is not one of
/// these: its handle is live, and the sends and receives it will not
/// perform yet fail with [`Recovering`](crate::ErrorCode::Recovering)
/// instead. `FederationClosed` means this handle is stale and no call on it
/// will ever work again; `Recovering` means the federation is working and
/// will accept this call once its wallet has been reconstructed. Retrying
/// the first with the same handle is pointless; retrying the second is the
/// whole plan.
#[derive(Debug, Clone)]
pub struct Sdk {
    inner: Arc<SdkInner>,
}

impl Sdk {
    /// Starts building an instance.
    ///
    /// The returned builder holds no storage and no mnemonic yet; see
    /// [`SdkBuilder`] for what each setting means and
    /// [`SdkBuilder::build`] for the rules applied when the instance is
    /// actually opened.
    pub fn builder() -> SdkBuilder {
        SdkBuilder {
            storage: None,
            mnemonic: None,
        }
    }

    /// Fetches a federation's configuration and renders it as a
    /// [`FederationPreview`], without joining it or writing anything to
    /// storage.
    ///
    /// This is the call behind a "join this federation?" screen: it
    /// contacts the guardians named in the invite code, validates what they
    /// return, and hands back the name, network, guardian count, module
    /// list, and configuration metadata needed to show the user what they
    /// are about to commit to.
    ///
    /// Validation here is the same validation [`Sdk::join`] performs,
    /// including the module-generation rule described on that method: a
    /// federation this SDK could not operate on is rejected at preview
    /// rather than previewed and then refused at join.
    ///
    /// # Errors
    ///
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable)
    /// when no guardian answers,
    /// [`Timeout`](crate::ErrorCode::Timeout) when they answer too slowly,
    /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation)
    /// when the configuration mixes module generations or is otherwise one
    /// this SDK refuses, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) if the
    /// instance has been shut down.
    pub async fn preview(&self, invite: &InviteCode) -> Result<FederationPreview> {
        unimplemented!()
    }

    /// Joins the federation named by `invite`, persists it, and returns a
    /// handle to it.
    ///
    /// Joining derives this federation's client secret from the instance
    /// seed, writes its configuration and client state to storage, and
    /// starts its background workers, all before this call returns: a
    /// process killed immediately afterwards comes back with the
    /// federation joined. It is [`Running`](FederationStatus::Running) from
    /// then on, and is reopened automatically by every subsequent
    /// [`SdkBuilder::build`] against the same storage until it is closed or
    /// forgotten.
    ///
    /// # The federation-wide module-generation rule
    ///
    /// Every module of a federation must be of the same generation. A mixed
    /// federation is rejected with
    /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation),
    /// carrying
    /// [`ErrorDetails::MixedModuleGenerations`](crate::ErrorDetails::MixedModuleGenerations)
    /// so the modules that conflict and the generations they declare are
    /// readable as structured data. The rule is checked at [`Sdk::preview`],
    /// here at join, when an existing federation is reopened, and again
    /// whenever its configuration changes while the instance is running,
    /// and it covers every module the federation runs, not only the ones
    /// this SDK exposes as facades.
    ///
    /// ## When a running federation stops satisfying it
    ///
    /// A federation's configuration can change under a running instance. If
    /// the new configuration becomes mixed, or otherwise one this SDK
    /// refuses:
    ///
    /// 1. **The refused configuration is never adopted.** The last
    ///    configuration that validated stays in force, which is what
    ///    [`Federation::name`](crate::Federation::name),
    ///    [`Federation::network`](crate::Federation::network) and
    ///    [`Federation::capabilities`](crate::Federation::capabilities) keep
    ///    reporting.
    /// 2. **The federation is quarantined, not closed and not erased.** Its
    ///    workers stop, it leaves [`Sdk::federations`], and its status
    ///    becomes [`Quarantined`](FederationStatus::Quarantined) carrying
    ///    [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation)
    ///    and a message naming the conflict. Nothing local is deleted, so a
    ///    federation that becomes supported again comes back with
    ///    [`Sdk::reopen_federation`] and loses nothing.
    /// 3. **Pending work terminates observably, and is not thrown away.**
    ///    In-flight operation state is flushed durably, and then every
    ///    outstanding
    ///    [`OperationUpdates::next`](crate::OperationUpdates::next) and
    ///    [`BalanceUpdates::next`](crate::BalanceUpdates::next) against the
    ///    federation resolves with
    ///    [`FederationClosed`](crate::ErrorCode::FederationClosed), as does
    ///    every subsequent fallible call on its handles. The operations
    ///    themselves are neither cancelled nor marked failed: their
    ///    persisted state is preserved and they resume where they left off
    ///    if the federation is reopened.
    ///
    /// The line between quarantine and ordinary trouble is drawn at
    /// refusal, not at reachability: a running federation whose guardians
    /// are unreachable is not quarantined, it is transient and the SDK
    /// keeps retrying in the background.
    ///
    /// # Joining a federation whose erase is committed
    ///
    /// An id in [`Forgetting`](FederationStatus::Forgetting) is on its way
    /// out of the storage, not "already joined", so joining it again is
    /// allowed and produces a *new* federation rather than reviving the old
    /// one. The committed erase is finished first, and the join then
    /// proceeds as a first-time join: an invite code is required, the
    /// configuration and client state are written fresh, and no balance,
    /// operation log or activity history carries over from before.
    ///
    /// If that erase cannot be finished, the join fails with
    /// [`Storage`](crate::ErrorCode::Storage) and writes nothing: the
    /// federation stays [`Forgetting`](FederationStatus::Forgetting), to be
    /// retried by a later [`SdkBuilder::build`] or
    /// [`Sdk::forget_federation`].
    ///
    /// # A stale recovery intent does not survive a plain join
    ///
    /// A federation joined with this call can never be misclassified as
    /// recovery-locked by a write an abandoned [`Sdk::recover`] attempt left
    /// behind.
    ///
    /// # Errors
    ///
    /// [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined) when this
    /// instance already holds the federation, including closed or
    /// quarantined, where [`Sdk::reopen_federation`] rather than a second
    /// join is the call that wants making, and excluding an id in
    /// [`Forgetting`](FederationStatus::Forgetting), which is handled as
    /// above, plus every error [`Sdk::preview`] can produce, and
    /// [`Storage`](crate::ErrorCode::Storage) if the join cannot be
    /// persisted or a committed erase for the same id cannot be finished
    /// first.
    pub async fn join(&self, invite: &InviteCode) -> Result<Federation> {
        // Implementation notes (delete once implemented):
        // - `Sdk::recover` persists its intent to recover, and the operation id of the first
        //   attempt, before asking the client to join, so a failure between those writes can
        //   leave a recovery intent for a federation that never actually joined. This call
        //   must discard such a leftover intent in the same transaction that records the plain
        //   join, unless the client's own durable state corroborates that a recovery was
        //   committed. This matters because the erase path bypasses the balance guard for
        //   recovery-locked federations.
        unimplemented!()
    }

    /// Every federation this instance currently has open.
    ///
    /// "Open" here means [`Running`](FederationStatus::Running) or
    /// [`Recovering`](FederationStatus::Recovering): the SDK holds a live
    /// client for it, its facades work, and it answers calls rather than
    /// reporting itself stale. Federations that are closed, quarantined, or
    /// [`Forgetting`](FederationStatus::Forgetting) are not listed, and
    /// forgotten ones are gone entirely. The order is unspecified.
    ///
    /// A [`Recovering`](FederationStatus::Recovering) entry is a real,
    /// usable handle. Its descriptive accessors answer as they would for
    /// any other federation, and its balance and activity are reported and
    /// kept up to date, provisionally, as the wallet is reconstructed. What
    /// it refuses is the work that needs a complete wallet: every send and
    /// receive, and taking a fresh backup, fail with
    /// [`Recovering`](crate::ErrorCode::Recovering) until the
    /// reconstruction completes.
    ///
    /// This is *not* the list to render a wallet screen from. Use
    /// [`Sdk::stored_federations`] for that: it is a superset of this list
    /// and gives everything the storage holds a [`FederationStatus`], so a
    /// federation that is not currently usable is shown as such instead of
    /// disappearing.
    pub fn federations(&self) -> Vec<Federation> {
        unimplemented!()
    }

    /// The open federation with this id, or `None` if this instance has no
    /// such federation open.
    ///
    /// "Open" means precisely what it means in [`Sdk::federations`]: this
    /// returns `Some` for exactly the federations that list contains, with
    /// the same caveat for a recovering one, that it refuses calls needing
    /// a complete wallet with [`Recovering`](crate::ErrorCode::Recovering).
    ///
    /// `None` covers every reason there is no live handle: never joined,
    /// forgotten, closed, quarantined, or being erased; they are not
    /// distinguished here. When the distinction matters,
    /// [`Sdk::federation_status`] answers it for this same id, and returns
    /// `None` only when the storage genuinely has no such federation.
    pub fn federation(&self, id: &FederationId) -> Option<Federation> {
        unimplemented!()
    }

    /// Every federation this instance's storage holds, running or not, each
    /// with its current [`FederationStatus`].
    ///
    /// This is the list a wallet screen should be built from.
    /// [`Sdk::federations`] answers "what can I act on"; this answers "what
    /// does this user have". A federation closed with
    /// [`Sdk::close_federation`], one quarantined because it could not be
    /// opened, and one whose erase is still finishing all appear here,
    /// labelled, rather than being absent.
    ///
    /// It is a superset of [`Sdk::federations`]: every open federation
    /// appears here too, carrying [`Running`](FederationStatus::Running) or
    /// [`Recovering`](FederationStatus::Recovering). Only a federation that
    /// has been fully forgotten appears in neither list.
    ///
    /// A [`Forgetting`](FederationStatus::Forgetting) row says "this id is
    /// on its way out": the erase is committed, the federation will never
    /// open again, [`Sdk::reopen_federation`] refuses it, and its balance
    /// and history are gone as far as this API is concerned. Render it as
    /// "removing…" and never as a wallet with a reconnect button. It stays
    /// listed until the erase completes, announced once as
    /// [`Forgotten`](FederationStatus::Forgotten) to
    /// [`Sdk::federation_status_updates`] subscribers.
    ///
    /// This is also the way back to a federation an application cannot
    /// reach by any other route: with [`Sdk::reopen_federation`], it closes
    /// the gap that would otherwise require the original invite code again.
    ///
    /// Each entry is a small owned record rather than a handle; see
    /// [`FederationInfo`]. The order is unspecified, as in
    /// [`Sdk::federations`].
    ///
    /// Infallible and synchronous: the statuses are instance state, not a
    /// storage read, and this keeps answering after [`Sdk::shutdown`],
    /// reporting the last statuses the instance knew.
    pub fn stored_federations(&self) -> Vec<FederationInfo> {
        unimplemented!()
    }

    /// What this instance's storage currently knows about one federation.
    ///
    /// `None` means this storage holds no federation with that id, because
    /// it was never joined or was successfully forgotten. Every other case,
    /// running, recovering, closed, quarantined, mid-erase, is a `Some`
    /// carrying the state, so "there is nothing here" and "there is
    /// something here that is not currently usable" are never confused.
    ///
    /// A federation whose erase is committed answers
    /// `Some(`[`Forgetting`](FederationStatus::Forgetting)`)` for as long as
    /// the erase is unfinished, across a stalled attempt, across a restart,
    /// and flips to `None` only once the state is actually gone.
    ///
    /// This is the observable side of quarantine: the
    /// [`ErrorCode`](crate::ErrorCode) and message inside
    /// [`Quarantined`](FederationStatus::Quarantined) say why the
    /// federation is not running, without any call having to fail first.
    ///
    /// Infallible, synchronous, and still answering after
    /// [`Sdk::shutdown`], for the same reasons as
    /// [`Sdk::stored_federations`].
    pub fn federation_status(&self, id: &FederationId) -> Option<FederationStatus> {
        unimplemented!()
    }

    /// Opens a new, independent subscription to every federation's status.
    ///
    /// A status can change without the application having asked for
    /// anything: guardians publish a configuration this SDK refuses and the
    /// federation is quarantined, a recovery finishes, another clone of
    /// this [`Sdk`] closes a federation, an erase completes.
    ///
    /// Each call returns its own cursor, exactly like
    /// [`Federation::balance_updates`](crate::Federation::balance_updates):
    /// two subscribers both see every change and neither consumes the
    /// other's updates. This is instance-wide rather than per-federation,
    /// so it yields a whole [`FederationInfo`], the same record
    /// [`Sdk::stored_federations`] returns, and a list screen updates by
    /// replacing the row whose [`id`](FederationInfo::id) matches.
    ///
    /// This cannot fail, so it hands out a subscriber even after
    /// [`Sdk::shutdown`]; that subscriber's first
    /// [`next`](FederationStatusUpdates::next) yields
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub fn federation_status_updates(&self) -> FederationStatusUpdates {
        unimplemented!()
    }

    /// Starts a stored federation running again, without an invite code.
    ///
    /// This is the way back from [`Closed`](FederationStatus::Closed) and
    /// from [`Quarantined`](FederationStatus::Quarantined). The SDK already
    /// holds the federation's configuration and client state, so no invite
    /// code is required or accepted.
    ///
    /// It runs the same open sequence [`SdkBuilder::build`] runs for a
    /// remembered federation: revalidate the configuration against the
    /// module-generation rule described on [`Sdk::join`], start the
    /// background workers, and resume unfinished operations from where
    /// they were persisted. On success the federation is open and appears
    /// in [`Sdk::federations`] again, and it is reopened automatically by
    /// later builds. Which open state it lands in is the federation's
    /// choice, not this call's: [`Running`](FederationStatus::Running), or
    /// [`Recovering`](FederationStatus::Recovering) if the reconstruction
    /// of its wallet from the seed was still unfinished when it stopped.
    ///
    /// Handles obtained before the federation stopped running are *not*
    /// revived: they stay closed, and their fallible calls keep failing
    /// with [`FederationClosed`](crate::ErrorCode::FederationClosed); the
    /// handle this call returns is the live one.
    ///
    /// Reopening a federation that is already open is not an error: it
    /// returns the live handle. It does not, and cannot, hurry a
    /// reconstruction along.
    ///
    /// A failed reopen leaves the federation
    /// [`Quarantined`](FederationStatus::Quarantined) with the same
    /// [`ErrorCode`](crate::ErrorCode) this call returns, so later builds
    /// retry it too. [`Sdk::close_federation`] is how to give up on it
    /// instead.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) when this storage
    /// holds no federation with that id, or holds one in
    /// [`Forgetting`](FederationStatus::Forgetting): a committed erase is
    /// never resurrected, however unfinished it is. Both cases mean the
    /// same thing to a caller: the id names nothing openable. Then
    /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation)
    /// for a configuration this SDK refuses,
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable)
    /// and [`Timeout`](crate::ErrorCode::Timeout) when the guardians cannot
    /// be reached in time, [`Storage`](crate::ErrorCode::Storage) if the
    /// federation's local state cannot be read, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) if the
    /// whole instance has been shut down.
    pub async fn reopen_federation(&self, id: &FederationId) -> Result<Federation> {
        unimplemented!()
    }

    /// Stops running this federation while keeping all of its data.
    ///
    /// The federation's background workers stop, it is dropped from
    /// [`Sdk::federations`], its status becomes
    /// [`Closed`](FederationStatus::Closed), and it is no longer reopened
    /// automatically by later builds. Nothing is deleted: it stays listed
    /// by [`Sdk::stored_federations`], and [`Sdk::reopen_federation`]
    /// restores access to all of it without an invite code. This is the
    /// non-destructive half of leaving a federation;
    /// [`Sdk::forget_federation`] is the destructive half.
    ///
    /// Any [`Federation`] handle an application still holds keeps existing
    /// but stops doing work: its fallible calls fail with
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed), as do
    /// pending [`BalanceUpdates::next`](crate::BalanceUpdates::next) and
    /// [`OperationUpdates::next`](crate::OperationUpdates::next) calls
    /// against it. Its infallible accessors keep answering.
    ///
    /// Closing is idempotent: an id that names no open federation is not an
    /// error. That includes a quarantined federation, which closing turns
    /// into a deliberate [`Closed`](FederationStatus::Closed) so later
    /// builds stop retrying it.
    ///
    /// An id in [`Forgetting`](FederationStatus::Forgetting) is accepted
    /// and changes nothing: it is already not running, so this returns
    /// `Ok(())`, but its status stays `Forgetting` and the committed erase
    /// still proceeds.
    ///
    /// A [`Recovering`](FederationStatus::Recovering) federation closes
    /// like any other, and closing it is not a way out of the
    /// reconstruction: its status becomes [`Closed`](FederationStatus::Closed),
    /// the unfinished reconstruction is preserved, and
    /// [`Sdk::reopen_federation`] brings it back
    /// [`Recovering`](FederationStatus::Recovering) with that work resuming
    /// where it stopped.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage) if the federation's state
    /// cannot be flushed before it stops, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) if the
    /// whole instance has been shut down.
    pub async fn close_federation(&self, id: &FederationId) -> Result<()> {
        unimplemented!()
    }

    /// Permanently deletes this federation's local state.
    ///
    /// This is destructive and unrecoverable from within the SDK: the
    /// configuration, client state, operation log, and activity history for
    /// the federation are erased. Only the seed survives, so re-joining the
    /// federation later recovers whatever the federation itself can
    /// reconstruct, not what was only ever recorded locally, and re-joining
    /// needs an invite code again. [`Sdk::close_federation`] and
    /// [`Sdk::reopen_federation`] exist so that merely wanting a federation
    /// to stop running never requires this.
    ///
    /// The call runs in three phases whose order is part of the contract.
    ///
    /// # 1. Quiesce, before anything is checked
    ///
    /// The federation is retired first, atomically: it leaves
    /// [`Sdk::federations`], every outstanding [`Federation`] handle,
    /// facade, and subscriber for it is closed, and its background workers,
    /// including a running seed rescan, are stopped after flushing their
    /// state durably. Only then is eligibility evaluated, so nothing can
    /// start new work on a federation that is already retired.
    ///
    /// # 2. Refuse unless it is safe, leaving the data intact
    ///
    /// The call then refuses unless all of the following hold:
    ///
    /// - **Zero spendable balance.** Any remaining spendable ecash fails
    ///   the call with
    ///   [`BalanceNotEmpty`](crate::ErrorCode::BalanceNotEmpty).
    /// - **No non-final operations** and **no reclaimable outgoing value**
    ///   (out-of-band ecash a receiver has not redeemed and this instance
    ///   could still reclaim). Either fails the call with
    ///   [`PendingOperations`](crate::ErrorCode::PendingOperations).
    ///
    /// One class of non-final operation is exempt: an on-chain receive that
    /// has not yet seen a transaction. It holds no value, so it never
    /// blocks the erase; once a transaction has been seen it is an
    /// ordinary pending operation and does block until claimed or failed.
    /// See [`Onchain::receive`](crate::Onchain::receive).
    ///
    /// ## Recovery is never a reason to refuse
    ///
    /// A federation whose recovery has not completed is recovery-locked:
    /// every spend and receive against it is refused with
    /// [`Recovering`](crate::ErrorCode::Recovering), and no call in this
    /// SDK stops or cancels a recovery. This erase is therefore the sole
    /// exit from a recovery that cannot be finished, and **none of the
    /// guards above may block it**:
    ///
    /// - A recovery-locked federation's balance does not count towards the
    ///   zero-balance guard: what it reports is provisional and none of it
    ///   is spendable.
    /// - A rescan, running or stopped short of completing, does not block
    ///   as a pending operation; phase 1 aborts a running rescan as part of
    ///   the erase.
    /// - Reclaimable outgoing value on a locked federation does not block
    ///   it either, and is forfeited by the erase.
    ///
    /// This call never returns [`Recovering`](crate::ErrorCode::Recovering).
    /// What that exit costs should be put in front of the user before they
    /// take it: the recovered-so-far state is thrown away, the local
    /// activity history is gone for good, locally-recorded reclaimable
    /// value is forfeited, and starting over needs the invite code again
    /// and restarts the recovery, and the lock, from the beginning.
    ///
    /// A refusal in this phase deletes nothing, but leaves the federation
    /// stopped, since it was quiesced in phase 1: its status afterwards is
    /// [`Closed`](FederationStatus::Closed), and
    /// [`Sdk::reopen_federation`] is how the application gets it running
    /// again.
    ///
    /// # 3. Commit the erase before performing it
    ///
    /// A durable tombstone is written in a single step before any state is
    /// removed, so the erase is atomic with respect to crashes and
    /// failures: from the moment the tombstone lands, the federation is
    /// gone as far as this API is concerned and the deletion is owed,
    /// whether this call finishes it, a later [`Sdk::forget_federation`]
    /// with the same id resumes it, or the next [`SdkBuilder::build`]
    /// attempts it. A tombstoned federation is never opened, never handed a
    /// handle, and never resurrected, no matter how many attempts fail:
    /// [`Sdk::reopen_federation`] refuses it with
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput), and a fresh
    /// [`Sdk::join`] of the same id finishes the erase first and then joins
    /// from nothing.
    ///
    /// No deadline is promised: an erase whose completion keeps failing
    /// stays committed and unfinished, and the federation sits in
    /// [`Forgetting`](FederationStatus::Forgetting), visible in
    /// [`Sdk::stored_federations`] and answering
    /// [`Sdk::federation_status`], until an attempt succeeds. One
    /// undeletable federation must not lock a user out of the others, of
    /// their history, or of [`Sdk::export_mnemonic`].
    ///
    /// A federation is therefore always in exactly one of three states when
    /// this call returns:
    ///
    /// - **Erased.** `Ok(())`, the state is gone,
    ///   [`Sdk::federation_status`] returns `None`, and the id no longer
    ///   appears in [`Sdk::stored_federations`].
    /// - **Fully intact and closed.** The call refused in phase 2, or could
    ///   not even write the tombstone, and nothing was deleted.
    /// - **Committed but unfinished.** The status is
    ///   [`Forgetting`](FederationStatus::Forgetting): listed by
    ///   [`Sdk::stored_federations`] and answered by
    ///   [`Sdk::federation_status`], absent from [`Sdk::federations`] and
    ///   from [`Sdk::federation`], refused by [`Sdk::reopen_federation`],
    ///   and safely retryable with the same id.
    ///
    /// Forgetting is idempotent: an id with no local state is not an error,
    /// and an id that is already tombstoned finishes the erase and returns
    /// `Ok(())` or, if it still cannot be finished, returns
    /// [`Storage`](crate::ErrorCode::Storage) and leaves it
    /// [`Forgetting`](FederationStatus::Forgetting) again.
    ///
    /// # Errors
    ///
    /// [`BalanceNotEmpty`](crate::ErrorCode::BalanceNotEmpty) and
    /// [`PendingOperations`](crate::ErrorCode::PendingOperations) for a
    /// phase-2 refusal, which deletes nothing;
    /// [`Storage`](crate::ErrorCode::Storage) if the backend fails,
    /// retryable either way; and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) after
    /// shutdown. Never [`Recovering`](crate::ErrorCode::Recovering).
    pub async fn forget_federation(&self, id: &FederationId) -> Result<()> {
        unimplemented!()
    }

    /// Returns this instance's seed phrase, for the user to write down.
    ///
    /// The name says *export* on purpose: this is the one call that takes a
    /// secret out of the SDK's custody, and it should be obvious at the
    /// call site that this is what is happening.
    ///
    /// What the caller receives is a [`Mnemonic`], which neither prints nor
    /// formats itself; extracting the words from it is a separate,
    /// deliberate step ([`Mnemonic::words`]). Everything downstream of that
    /// step is the application's responsibility, as documented on that
    /// type.
    ///
    /// Infallible and synchronous: the seed is loaded once when the
    /// instance is built and held in memory for its lifetime, so this does
    /// not read storage and remains available after [`Sdk::shutdown`]. It
    /// is also why [`SdkBuilder::build`] never fails over a federation: an
    /// instance whose every federation is quarantined still exports its
    /// seed, the user's route to their money by any other client.
    pub fn export_mnemonic(&self) -> Mnemonic {
        unimplemented!()
    }

    /// Best-effort: flushes everything to storage, stops all background
    /// work, and releases the storage lock.
    ///
    /// After this returns, every fallible call on every [`Sdk`] and
    /// [`Federation`] handle, and every subscriber obtained from one, fails
    /// with [`FederationClosed`](crate::ErrorCode::FederationClosed), with
    /// [`Sdk::export_mnemonic`] the deliberate exception, alongside the
    /// infallible status accessors ([`Sdk::stored_federations`],
    /// [`Sdk::federation_status`]) and the infallible accessors on
    /// [`Federation`]. Another instance may then open the same storage.
    /// Shutdown is idempotent.
    ///
    /// # It is an optimisation, not a requirement
    ///
    /// **Correctness does not depend on this call**, because mobile
    /// operating systems terminate backgrounded applications without
    /// warning and a browser tab can vanish the same way. Everything a
    /// caller can observe is already durable at the moment it becomes
    /// observable, see the durability section on [`Sdk`], and what this
    /// call adds is a flush of buffered non-critical state such as caches,
    /// an orderly release of the storage lock, and a defined point after
    /// which no background work is running.
    ///
    /// Call it from the platform's "entering background" or "about to
    /// terminate" callback if there is one, and await it if you are allowed
    /// to. Do not build anything on being able to.
    ///
    /// # What survives an abrupt kill
    ///
    /// If the process dies without this call, then on the next
    /// [`SdkBuilder::build`] over the same storage:
    ///
    /// - **Everything acknowledged is there.** Any value a completed call
    ///   returned or a subscriber yielded is present.
    /// - **In-flight operations resume by themselves**, from their last
    ///   persisted checkpoint, idempotently: neither dropped nor performed
    ///   twice.
    /// - **A call that never returned may or may not have happened.** The
    ///   persisted state after reopening is authoritative, which is why
    ///   operations are addressed by a stable
    ///   [`OperationId`](crate::OperationId): retry by looking up the id,
    ///   not by repeating the request.
    /// - **The seed is never at risk.** It is written before anything
    ///   derived from it exists.
    /// - **A lock left behind is reclaimed, not fatal.**
    ///   [`StorageInUse`](crate::ErrorCode::StorageInUse) means genuinely
    ///   concurrent use, never a stale lock.
    /// - **A committed erase stays committed.** The next build finishes the
    ///   deletion off, and if that attempt fails the federation stays
    ///   [`Forgetting`](FederationStatus::Forgetting) and is retried later;
    ///   it does not come back as a wallet and does not fail the build. See
    ///   [`Sdk::forget_federation`].
    ///
    /// Shutting down does not cancel operations in the sense of undoing
    /// them: an operation that was running resumes when the storage is
    /// opened again, exactly as it would after a crash.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage) if the final flush fails. The
    /// instance is closed either way.
    pub async fn shutdown(&self) -> Result<()> {
        unimplemented!()
    }
}

/// Builder for an [`Sdk`].
///
/// Obtained from [`Sdk::builder`], configured with [`SdkBuilder::storage`]
/// and optionally [`SdkBuilder::mnemonic`], and consumed by
/// [`SdkBuilder::build`].
///
/// `Debug` output redacts the mnemonic: whether one is set is visible, its
/// contents never are.
pub struct SdkBuilder {
    storage: Option<Storage>,
    mnemonic: Option<Mnemonic>,
}

impl SdkBuilder {
    /// Sets where the instance persists its state.
    ///
    /// Required: [`SdkBuilder::build`] fails without it rather than
    /// guessing a location. Use [`Storage::at`] for a native filesystem
    /// path, [`Storage::in_browser`] for an origin-scoped namespace in a
    /// browser, or [`Storage::in_memory`] for a throwaway store.
    ///
    /// A [`Storage`] is a descriptor and not an open handle, so setting one
    /// here still touches nothing: the location is opened, and every
    /// failure that needs the environment reported, by
    /// [`SdkBuilder::build`].
    pub fn storage(mut self, storage: Storage) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Sets the BIP-39 seed the instance derives every federation secret
    /// from.
    ///
    /// Supply one to restore an existing wallet from a written-down
    /// phrase. Omit it and the instance uses the seed already in storage,
    /// or, if the storage is proven empty, generates a fresh one and
    /// persists it before deriving anything from it. Generating a seed can
    /// fail, because drawing secure entropy can (see
    /// [`Mnemonic::generate`]); when it does, [`SdkBuilder::build`] reports
    /// [`Entropy`](crate::ErrorCode::Entropy) rather than panicking or
    /// settling for a weaker source.
    ///
    /// "Proven empty" is load-bearing and is spelled out on
    /// [`SdkBuilder::build`]: a seed is established only over a backend
    /// that holds nothing else, never merely because no seed was found.
    ///
    /// Supplying a mnemonic that differs from the one the storage already
    /// holds is a mistake the SDK will not paper over: [`SdkBuilder::build`]
    /// fails with [`SeedMismatch`](crate::ErrorCode::SeedMismatch) and
    /// changes nothing. Restoring a different wallet means pointing at a
    /// different [`Storage`].
    pub fn mnemonic(mut self, mnemonic: Mnemonic) -> Self {
        self.mnemonic = Some(mnemonic);
        self
    }

    /// Opens the storage, loads or establishes the seed, attempts to finish
    /// any erase that was left committed, reopens every federation the
    /// storage remembers, and resumes their pending operations.
    ///
    /// The order of those steps is part of the contract, because it is
    /// what makes the failure modes safe: only the root storage and the
    /// seed can fail this call outright (steps 1 and 2 below); anything
    /// scoped to one federation, whether a federation that will not open or
    /// an erase that will not finish, is reported as that federation's
    /// status instead.
    ///
    /// 1. **Open the location and take its lock.** The location is created
    ///    or found first; a native directory that cannot be created, or is
    ///    not readable and writable, fails with
    ///    [`Storage`](crate::ErrorCode::Storage), as does a browser origin
    ///    with no usable origin-private file system or one where storage
    ///    access is denied. Then the single-opener lock is taken: if the
    ///    location is already open elsewhere, the call fails with
    ///    [`StorageInUse`](crate::ErrorCode::StorageInUse) and nothing has
    ///    been touched. A lock left behind by a process that died without
    ///    [`Sdk::shutdown`] is reclaimed rather than treated as contention;
    ///    see [`Storage`] for the native and browser cases.
    /// 2. **Reconcile the seed, before any mutation.** There are exactly
    ///    four cases:
    ///    - *The storage holds a usable seed.* It is used. If a different
    ///      mnemonic was supplied, the call fails with
    ///      [`SeedMismatch`](crate::ErrorCode::SeedMismatch).
    ///    - *The storage is proven empty*, no seed and no state of any kind
    ///      belonging to this SDK. The supplied or freshly generated
    ///      mnemonic is written durably now, before any federation-derived
    ///      state can exist. Generating a fresh mnemonic can itself fail
    ///      (see [`Mnemonic::generate`]): if the platform's secure random
    ///      source fails, the call fails with
    ///      [`Entropy`](crate::ErrorCode::Entropy) and nothing has been
    ///      written.
    ///    - *There is no usable seed but there is other state.* The
    ///      storage is **orphaned**, and the call fails with
    ///      [`StorageOrphaned`](crate::ErrorCode::StorageOrphaned) without
    ///      writing anything, carrying
    ///      [`ErrorDetails::StorageOrphaned`](crate::ErrorDetails::StorageOrphaned)
    ///      with the location and `seed_present: false`.
    ///    - *The seed entry was read in full but is unusable*, truncated,
    ///      corrupt, or in a format this build does not understand. Also a
    ///      refusal with the same code and detail case, but
    ///      `seed_present: true`, and again without writing anything. A
    ///      read the backend failed to perform, rather than one that
    ///      returned unusable bytes, fails with
    ///      [`Storage`](crate::ErrorCode::Storage) instead, which is
    ///      retryable.
    ///
    ///    "No seed" must never be read as "fresh storage": writing a new
    ///    seed over storage that already holds federation or client state
    ///    would associate that state with the wrong derivation root,
    ///    making funds unreachable without the original phrase while
    ///    destroying the only local trace of which seed the state belonged
    ///    to. Refusing is recoverable; a wrong write is not.
    ///
    ///    The emptiness proof and the seed reconciliation happen under the
    ///    lock taken in step 1 and strictly before any write this call
    ///    makes: if step 2 fails for any reason, the backend is
    ///    byte-identical to how it was found.
    /// 3. **Attempt every committed erase, and keep a failure to the one
    ///    federation.** A federation whose tombstone was written but whose
    ///    deletion did not complete (see [`Sdk::forget_federation`]) is
    ///    erased now, before it could be opened, and a committed erase is
    ///    never resurrected regardless of the outcome. A deletion that
    ///    fails here **does not fail this call**: that federation stays
    ///    [`Forgetting`](FederationStatus::Forgetting), reported as such by
    ///    [`Sdk::stored_federations`] and [`Sdk::federation_status`], and
    ///    attempted again by the next build or by another
    ///    [`Sdk::forget_federation`] with the same id.
    /// 4. **Reopen the federations, and quarantine the ones that will not
    ///    open.** Each federation the storage remembers, that was not
    ///    closed with [`Sdk::close_federation`], is revalidated, including
    ///    the module-generation rule described on [`Sdk::join`], started,
    ///    and its unfinished operations resume from where they were
    ///    persisted. One whose wallet was still being reconstructed comes
    ///    back [`Recovering`](FederationStatus::Recovering) and is listed
    ///    alongside the [`Running`](FederationStatus::Running) ones. A
    ///    federation that cannot be opened **does not fail this call**: it
    ///    is put into [`Quarantined`](FederationStatus::Quarantined) with
    ///    the [`ErrorCode`](crate::ErrorCode) and message that explain why,
    ///    and later builds retry it. Failing the whole build over one
    ///    unreachable or unsupported federation would deny the user every
    ///    healthy federation, all of their history, and
    ///    [`Sdk::export_mnemonic`].
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) if no storage was
    /// set, [`StorageInUse`](crate::ErrorCode::StorageInUse),
    /// [`SeedMismatch`](crate::ErrorCode::SeedMismatch),
    /// [`Entropy`](crate::ErrorCode::Entropy) if a fresh seed had to be
    /// generated and the platform's secure random source failed,
    /// [`Storage`](crate::ErrorCode::Storage) for a root-storage failure,
    /// and [`StorageOrphaned`](crate::ErrorCode::StorageOrphaned), with
    /// [`ErrorDetails::StorageOrphaned`](crate::ErrorDetails::StorageOrphaned),
    /// for the orphaned and unreadable-seed cases in step 2.
    ///
    /// Notably **not** returned here:
    /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation)
    /// and [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable).
    /// Those are per-federation conditions and arrive as
    /// [`Quarantined`](FederationStatus::Quarantined) statuses on a
    /// successfully built instance, as does a per-federation storage
    /// failure and an unfinished erase.
    pub async fn build(self) -> Result<Sdk> {
        unimplemented!()
    }
}

impl core::fmt::Debug for SdkBuilder {
    /// Prints the builder with the mnemonic redacted: whether one is set
    /// is visible, its contents never are.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SdkBuilder")
            .field("storage", &self.storage)
            .field("mnemonic", &self.mnemonic.as_ref().map(|_| Redacted))
            .finish()
    }
}

/// Stands in for a secret in `Debug` output: prints `<redacted>` and
/// nothing else, so `Option<Redacted>` renders as `Some(<redacted>)` or
/// `None` without the secret ever being formatted.
struct Redacted;

impl core::fmt::Debug for Redacted {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("<redacted>")
    }
}

/// What an SDK instance's storage currently knows about one federation.
///
/// This is the crate's answer to "why can I not use this wallet right now",
/// and it is a **value to read** rather than an error to provoke.
///
/// Read one with [`Sdk::federation_status`], read them all with
/// [`Sdk::stored_federations`], and follow changes with
/// [`Sdk::federation_status_updates`]. The lifecycle these variants form
/// is documented in one place, on [`Sdk`].
///
/// The enum is `#[non_exhaustive]`; its variants are not. Rust callers
/// write a wildcard arm, and more detail about a situation arrives as a
/// new, more specific variant rather than a field grown on an existing one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FederationStatus {
    /// Open and fully working: workers are running, operations are
    /// progressing, and nothing the federation offers is withheld.
    ///
    /// One of the two open states, so the federation is listed by
    /// [`Sdk::federations`] and [`Sdk::federation`] hands out a live handle;
    /// [`Recovering`](FederationStatus::Recovering) is the other, and differs
    /// only in refusing fund-touching calls.
    Running,
    /// Open, but the wallet has not finished being reconstructed from the
    /// seed, so the federation is **recovery-locked**.
    ///
    /// A live handle exists and identity, metadata and capabilities are
    /// readable, while balance and activity are incomplete and still
    /// moving and every spend and receive is refused with
    /// [`Recovering`](crate::ErrorCode::Recovering).
    ///
    /// This is the second **open** state, so the federation is listed by
    /// [`Sdk::federations`] and returned by [`Sdk::federation`] exactly as
    /// a [`Running`](FederationStatus::Running) one is.
    ///
    /// This state covers a rescan that is running and one that has stopped
    /// without completing: only a completed recovery releases the lock,
    /// and a stopped attempt holds it just as firmly.
    ///
    /// Not to be confused with [`Quarantined`](FederationStatus::Quarantined):
    /// a recovering federation is one the SDK is happily operating, a
    /// quarantined one is one it refuses to operate. Closing or
    /// quarantining a recovering federation moves it out of this state
    /// without finishing the reconstruction, which is persisted, so
    /// reopening lands it back here; only [`Sdk::forget_federation`] ends
    /// an unfinished one, by erasing it.
    ///
    /// This state only arises for a federation joined with
    /// [`Sdk::recover`]; one joined with [`Sdk::join`] never enters it.
    Recovering,
    /// Stored and intact, but not running, because the SDK could not or
    /// would not open it.
    ///
    /// Nothing has been deleted, and [`Sdk::reopen_federation`] retries.
    /// Quarantine means "meant to be running, currently cannot", so later
    /// builds retry it too; [`Sdk::close_federation`] is how an
    /// application gives up on it.
    ///
    /// This state also carries the answer to the question a caller would
    /// otherwise have to ask by failing a call.
    Quarantined {
        /// Why the federation is not running: the same stable
        /// [`ErrorCode`](crate::ErrorCode) the equivalent
        /// [`Error`](crate::Error) would carry, a human-readable message,
        /// and the same structured details, so the modules that conflict
        /// in a mixed-generation federation are readable without parsing
        /// text.
        ///
        /// [`code`](crate::Diagnostic::code) is the part to branch on:
        /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation)
        /// for a configuration this SDK refuses (mixed module generations,
        /// most often),
        /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable)
        /// or [`Timeout`](crate::ErrorCode::Timeout) when no guardian
        /// answered in time, and [`Storage`](crate::ErrorCode::Storage)
        /// when the federation's local state could not be read.
        ///
        /// [`message`](crate::Diagnostic::message) is human-readable
        /// detail, for humans only: logs, diagnostics, an expandable
        /// "details" row. Not part of the stability contract and must
        /// never be parsed or matched on.
        ///
        /// [`details`](crate::Diagnostic::details) is that same
        /// information as structured data: a mixed federation carries
        /// [`ErrorDetails::MixedModuleGenerations`](crate::ErrorDetails::MixedModuleGenerations),
        /// read with [`Diagnostic::detail`](crate::Diagnostic::detail).
        diagnostic: Diagnostic,
    },
    /// Stored and intact, and stopped because the application asked for
    /// that with [`Sdk::close_federation`].
    ///
    /// Distinct from [`Quarantined`](FederationStatus::Quarantined): later
    /// builds do *not* reopen it, because someone chose this.
    /// [`Sdk::reopen_federation`] undoes the choice.
    Closed,
    /// An erase has been committed and is being carried out, or is waiting
    /// to be finished by a retry or by a later
    /// [`SdkBuilder::build`](crate::SdkBuilder::build).
    ///
    /// The federation will not be opened again and cannot be resurrected;
    /// see [`Sdk::forget_federation`]. An application seeing this should
    /// render "removing…" rather than a wallet.
    ///
    /// The id is listed by [`Sdk::stored_federations`] and answered here
    /// by [`Sdk::federation_status`], absent from [`Sdk::federations`] and
    /// from [`Sdk::federation`], and refused by [`Sdk::reopen_federation`]
    /// with [`InvalidInput`](crate::ErrorCode::InvalidInput). Its balance,
    /// operation log and activity history are gone as far as this API is
    /// concerned from the moment the tombstone lands. Joining the same
    /// federation again is allowed and produces a new federation with no
    /// local history, after the committed erase is finished; see
    /// [`Sdk::join`].
    Forgetting,
    /// The erase completed: this federation is gone.
    ///
    /// This is a *notification*, not a stored state. It is delivered once
    /// by [`FederationStatusUpdates::next`] so a list screen can drop the
    /// row. [`Sdk::federation_status`] returns `None` for a forgotten
    /// federation.
    Forgotten,
}

/// A stored federation, described without a live handle.
///
/// [`Sdk::stored_federations`] returns these and
/// [`FederationStatusUpdates::next`] yields them. A closed federation, a
/// quarantined one, or one whose erase is finishing has no [`Federation`]
/// handle to describe it, so listing them uses this small record instead.
///
/// A [`Recovering`](FederationStatus::Recovering) federation is in this
/// list and can still be acted on through a live [`Federation`] handle; a
/// record here has no client behind it at all.
///
/// `#[non_exhaustive]`: fields may be added, so match it with `..` or by
/// field access.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FederationInfo {
    /// The federation's id: the key for [`Sdk::federation`],
    /// [`Sdk::federation_status`] and [`Sdk::reopen_federation`], and what
    /// identifies the row to replace when this record arrives from
    /// [`FederationStatusUpdates::next`].
    pub id: FederationId,
    /// The federation's human-readable name, when its configuration
    /// declares one.
    ///
    /// From the last configuration that validated, exactly as
    /// [`Federation::name`](crate::Federation::name) reports it, so a
    /// closed or quarantined federation still has a label to show. Not a
    /// verified or unique identifier: identity is [`id`](FederationInfo::id).
    pub name: Option<String>,
    /// The Bitcoin network this federation operates on, from the same
    /// last-good configuration as [`name`](FederationInfo::name).
    pub network: Network,
    /// What the SDK can currently do with it.
    pub status: FederationStatus,
}

/// One independent subscription to every federation's status.
///
/// Obtained from [`Sdk::federation_status_updates`]. Not `Clone`, for the
/// same reason [`BalanceUpdates`](crate::BalanceUpdates) is not: it is a
/// single cursor, and a second consumer should have a second subscription.
/// Dropping it stops only this subscription and never any work.
#[derive(Debug)]
pub struct FederationStatusUpdates {
    inner: Arc<FederationStatusUpdatesInner>,
}

impl FederationStatusUpdates {
    /// Waits for the next status change, anywhere in this instance.
    ///
    /// The first calls deliver the current state of every federation this
    /// storage holds, one [`FederationInfo`] each, in unspecified order, so
    /// a subscriber can be the only thing a list screen reads. After that,
    /// each call resolves when some federation's status changes.
    ///
    /// There is no final value: an instance's set of federations can
    /// always change again. The one way this stream ends is the instance
    /// shutting down, which surfaces as
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    ///
    /// A federation being forgotten is not the end of the stream. It
    /// arrives as an ordinary update carrying
    /// [`Forgotten`](FederationStatus::Forgotten), the last update for
    /// that id, and the subscription stays open for every other
    /// federation.
    ///
    /// # Errors
    ///
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) once the
    /// SDK has been shut down, the terminal condition for this stream, and
    /// what the very first call yields on a subscriber taken after
    /// shutdown. Other errors are infrastructure failures:
    /// [`Storage`](crate::ErrorCode::Storage) or
    /// [`Internal`](crate::ErrorCode::Internal).
    pub async fn next(&mut self) -> Result<FederationInfo> {
        unimplemented!()
    }
}

/// Placeholder for the shared instance state. Handles hold this behind an
/// `Arc` so cloning an [`Sdk`] shares one set of federations, one storage,
/// and one pool of background work.
#[derive(Debug)]
struct SdkInner;

/// Placeholder for one federation-status subscription's state.
#[derive(Debug)]
struct FederationStatusUpdatesInner;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    /// The upstream dummy federation id: 32 bytes of `0x2a`, printed as hex.
    const TEST_FEDERATION_ID: &str =
        "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a";

    #[test]
    fn builder_debug_redacts_the_mnemonic() {
        // The builder must be printable without the phrase escaping into a
        // log line; only whether one is present may show.
        let builder = Sdk::builder();
        let rendered = format!("{builder:?}");
        assert!(rendered.contains("mnemonic"));
        assert!(rendered.contains("None"));

        // The canonical all-zero-entropy BIP-39 phrase, the same fixture
        // `types::mnemonic`'s own tests use.
        const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon \
                              abandon abandon abandon about";
        let mnemonic = PHRASE.parse::<Mnemonic>().expect("a valid phrase");
        let rendered = format!("{:?}", Sdk::builder().mnemonic(mnemonic));
        assert!(!rendered.contains("abandon"));
        assert_eq!(
            rendered,
            "SdkBuilder { storage: None, mnemonic: Some(<redacted>) }"
        );
    }

    #[test]
    fn a_quarantine_carries_the_code_to_branch_on() {
        // The point of the status is that an application learns *why* a
        // federation is unavailable without provoking an error, so the
        // code has to be readable straight off the value. The diagnosis
        // carries the same envelope an `Error` would, so the modules that
        // conflict are too.
        let status = FederationStatus::Quarantined {
            diagnostic: Diagnostic::with_details(
                ErrorCode::UnsupportedFederation,
                "modules mint=v1, ln=v2",
                crate::ErrorDetails::MixedModuleGenerations {
                    modules: vec![
                        crate::ModuleGeneration::new("mint", 1),
                        crate::ModuleGeneration::new("ln", 2),
                    ],
                },
            ),
        };
        match &status {
            FederationStatus::Quarantined { diagnostic } => {
                assert_eq!(diagnostic.code, ErrorCode::UnsupportedFederation);
                match diagnostic.detail() {
                    Some(crate::ErrorDetails::MixedModuleGenerations { modules }) => {
                        let named: Vec<(&str, u32)> = modules
                            .iter()
                            .map(|module| (module.kind.as_str(), module.generation))
                            .collect();
                        assert_eq!(named, vec![("mint", 1), ("ln", 2)]);
                    }
                    other => panic!("expected the conflicting modules, got {other:?}"),
                }
            }
            other => panic!("expected a quarantine, got {other:?}"),
        }
    }

    #[test]
    fn statuses_distinguish_deliberate_closure_from_quarantine() {
        // These two are both "stored, intact, not running" and differ only
        // in whether a later build retries. Collapsing them would lose the
        // difference between a wallet the user left and one that broke.
        assert_ne!(
            FederationStatus::Closed,
            FederationStatus::Quarantined {
                diagnostic: Diagnostic::new(ErrorCode::FederationUnreachable, ""),
            }
        );
    }

    #[test]
    fn a_committed_erase_is_a_listable_state_of_its_own() {
        // `Forgetting` is what `stored_federations` shows for a federation
        // whose erase is committed but unfinished, so the listing record
        // has to be able to carry it. It must also stay distinguishable
        // from the "stored and intact" states an application offers a
        // reconnect for, and from the `Forgotten` notification that drops
        // the row.
        let info = FederationInfo {
            id: TEST_FEDERATION_ID.parse().expect("a valid federation id"),
            name: Some("Test Federation".to_owned()),
            network: Network::Regtest,
            status: FederationStatus::Forgetting,
        };
        assert_eq!(info.status, FederationStatus::Forgetting);
        assert_ne!(FederationStatus::Forgetting, FederationStatus::Closed);
        assert_ne!(FederationStatus::Forgetting, FederationStatus::Forgotten);
    }

    #[test]
    fn a_stored_federation_is_describable_without_a_live_handle() {
        // The listing record must be constructible for a federation that
        // has no handle at all, that is the case it exists for.
        let info = FederationInfo {
            id: TEST_FEDERATION_ID.parse().expect("a valid federation id"),
            name: Some("Test Federation".to_owned()),
            network: Network::Regtest,
            status: FederationStatus::Closed,
        };
        assert_eq!(info.status, FederationStatus::Closed);
        assert_eq!(info.clone(), info);
    }
}
