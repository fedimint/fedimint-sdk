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
/// storage, and the same background work. On native targets it is `Send`
/// and `Sync`, so clones can be moved between threads and tasks freely; the
/// wasm build is the same type compiled for a single-threaded host, where
/// those bounds are trivially satisfied.
///
/// # One seed, many federations
///
/// An instance holds exactly one mnemonic. Each federation's client secret
/// is derived from it, domain-separated by federation id, using the scheme
/// documented on [`Mnemonic`] — so joining a second federation never
/// reuses the first federation's secret, and the same seed restored in
/// another fedimint client reproduces the same per-federation secrets.
/// Storage is likewise shared and namespaced per federation internally;
/// applications do not manage per-federation locations.
///
/// # Federation lifecycle
///
/// Every federation this instance's storage remembers is in exactly one of
/// the five states below, and every lifecycle call on this type is a
/// transition between them. They are worth reading once as a whole, because
/// the rest of this type's contract is stated in terms of them rather than
/// re-derived per method. ([`Forgotten`](FederationStatus::Forgotten) is the
/// one variant of [`FederationStatus`] that is not among them: it is a
/// notification that a federation is gone, not a state one is stored in.)
///
/// - **[`Running`](FederationStatus::Running)** — open, workers turning,
///   operations progressing, nothing the federation offers withheld.
/// - **[`Recovering`](FederationStatus::Recovering)** — open, with a live
///   handle, but its wallet has not finished being reconstructed from the
///   seed: its balance and activity are incomplete and every spend and
///   receive is refused with
///   [`Recovering`](crate::ErrorCode::Recovering) until the reconstruction
///   completes.
///
///   Those first two are the **open** states, and together they are exactly
///   what [`Sdk::federations`] lists and what [`Sdk::federation`] hands back
///   a live handle for. Completing the reconstruction turns `Recovering`
///   into `Running`, and nothing in this SDK completes or cancels one on
///   demand, so an unfinished reconstruction — and the refusals that come
///   with it — survives closing, reopening and restarting; the destructive
///   erase [`Sdk::forget_federation`] performs is the only way to be rid of
///   one without finishing it. Leaving the *state* is a weaker thing than
///   that: closing or quarantining a recovering federation moves it out of
///   `Recovering` while preserving the unfinished reconstruction, so
///   reopening lands it back here rather than in `Running`.
/// - **[`Quarantined`](FederationStatus::Quarantined)** — stored and
///   intact, but not running, because the SDK could not or would not
///   operate on it: its configuration is one this SDK refuses, its local
///   state could not be read, or no guardian answered when it was opened.
///   Nothing has been deleted, and the state carries the
///   [`ErrorCode`](crate::ErrorCode) and message that explain why.
/// - **[`Closed`](FederationStatus::Closed)** — stored and intact, not
///   running, because the application asked for exactly that with
///   [`Sdk::close_federation`].
/// - **[`Forgetting`](FederationStatus::Forgetting)** — an erase has been
///   committed and is being carried out, or is waiting to be finished by a
///   retry or by a later [`SdkBuilder::build`]; see
///   [`Sdk::forget_federation`]. This one is a dead end by design. The
///   federation is never opened again, never handed a handle and never
///   resurrected — [`Sdk::reopen_federation`] refuses it — and as far as
///   this API is concerned its balance, its history and its local state are
///   already gone. The only way out of this state is out of the storage
///   entirely.
///
/// Three rules hold across all of them, and they are what keep this from
/// being a pile of special cases:
///
/// **A stored federation is never silently absent.** [`Sdk::federations`]
/// lists the federations this instance has open — the two states above, the
/// ones with a live handle — which is what an application needs in order to
/// *act*. Everything the storage holds, whatever its state, is listed by
/// [`Sdk::stored_federations`], and [`Sdk::federation_status`] answers for a
/// single id. A wallet list should be rendered from the second, so that a
/// federation which is closed, quarantined, or being erased appears as
/// itself, with a reason, instead of as a wallet that quietly vanished
/// between two runs.
///
/// **No single federation takes the instance down with it.**
/// [`SdkBuilder::build`] fails only when the *root* storage or the seed is
/// unsound; a federation that cannot be opened is quarantined and reported,
/// because refusing to build would deny the user access to every healthy
/// federation and to [`Sdk::export_mnemonic`] — the one call that gets their
/// money out of a broken installation. A federation that will not finish
/// *being erased* is treated by the same rule: `build` attempts every
/// committed erase, and one that cannot be completed leaves that federation
/// in [`Forgetting`](FederationStatus::Forgetting), reported as such and
/// retried by a later build, rather than failing the instance. Anything
/// scoped to one federation is that federation's status; a top-level `Err`
/// is reserved for what makes the whole instance unsound.
///
/// **Getting a stored federation open again takes one call and no invite
/// code.** [`Sdk::reopen_federation`] moves a federation out of
/// [`Closed`](FederationStatus::Closed) or
/// [`Quarantined`](FederationStatus::Quarantined) using the configuration
/// the SDK already holds, into whichever open state its persisted work leaves
/// it in. An application must never have to have retained an invite code in
/// order to reach a wallet it still has.
///
/// Status changes are also observable as they happen, through
/// [`Sdk::federation_status_updates`], so an application can react to a
/// federation being quarantined underneath it instead of finding out by
/// provoking a failure.
///
/// # Durability: correctness never depends on a clean shutdown
///
/// Mobile operating systems terminate backgrounded applications without
/// warning, without unwinding, and without awaiting anything. A browser tab
/// disappears the same way. This design therefore takes the strongest
/// position it can and states it once, here, because several methods below
/// rely on it:
///
/// **Every transition an application can observe is durably committed before
/// it becomes observable.** A joined federation is persisted before
/// [`Sdk::join`] returns its handle. An operation is persisted before the
/// facade call that started it hands back an [`Operation`](crate::Operation).
/// A state is persisted before [`OperationUpdates::next`](crate::OperationUpdates::next)
/// yields it. An erase is committed before it begins. There is no window in
/// which the SDK has told the caller that value moved, or that a
/// fund-affecting transition happened, and could still forget it.
///
/// The consequence is that [`Sdk::shutdown`] is an optimisation, not a
/// correctness requirement, and that an abrupt kill loses nothing that was
/// acknowledged. What a caller may rely on after the process dies without
/// warning is spelled out on [`Sdk::shutdown`].
///
/// # Closed handles
///
/// Any [`Federation`] handle for a federation that is no longer running —
/// closed, quarantined, being erased — and every handle at all after
/// [`Sdk::shutdown`], fails its **fallible** calls with
/// [`ErrorCode::FederationClosed`](crate::ErrorCode::FederationClosed)
/// rather than panicking or silently doing nothing. Its infallible
/// accessors keep answering; see [`Federation`] for exactly what each of
/// them reports. The *reason* the federation stopped running is not encoded
/// in that error — one code covers all of them deliberately, so that
/// applications have exactly one "this handle is stale" branch — and is read
/// from [`Sdk::federation_status`] instead.
///
/// A [`Recovering`](FederationStatus::Recovering) federation is emphatically
/// *not* one of these. It is open, its handle is live, and the sends and
/// receives it will not perform yet fail with
/// [`Recovering`](crate::ErrorCode::Recovering) instead — a different code
/// because it is a different situation, and the distinction is the reason
/// both exist. `FederationClosed` says "this handle is stale, and no call on
/// it will ever work again"; `Recovering` says "this federation is working,
/// and will accept this call once its wallet has been reconstructed".
/// Retrying the first with the same handle is pointless; retrying the second
/// is the whole plan.
#[derive(Debug, Clone)]
pub struct Sdk {
    inner: Arc<SdkInner>,
}

impl Sdk {
    /// Starts building an instance.
    ///
    /// The returned builder holds no storage and no mnemonic yet; see
    /// [`SdkBuilder`] for what each setting means and
    /// [`SdkBuilder::build`] for the rules that apply when the instance is
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
    /// including the federation-wide module-generation rule described on
    /// that method: a federation this SDK could not operate on is rejected
    /// at preview rather than previewed and then refused at join. That rule
    /// is also re-checked for the lifetime of a joined federation, and
    /// [`Sdk::join`] documents what happens when a configuration that used
    /// to satisfy it stops doing so.
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
    /// starts its background workers. All of that is durably committed
    /// before this call returns, per the durability rule on [`Sdk`]: a
    /// process killed immediately afterwards comes back with the federation
    /// joined. It is [`Running`](FederationStatus::Running) from then on, and
    /// is reopened automatically by every subsequent [`SdkBuilder::build`]
    /// against the same storage until it is closed or forgotten.
    ///
    /// # The federation-wide module-generation rule
    ///
    /// Every module of a federation must be of the same generation — all
    /// v1, or all v2. There is no per-module override and no way for a
    /// caller to opt out: a mixed federation is rejected with
    /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation),
    /// carrying
    /// [`ErrorDetails::MixedModuleGenerations`](crate::ErrorDetails::MixedModuleGenerations)
    /// so the modules that conflict and the generations they declare are
    /// readable as structured data rather than only as prose in the message.
    /// The rule is checked at [`Sdk::preview`],
    /// here at join, when an existing federation is reopened, and again
    /// whenever its configuration changes while the instance is running. It
    /// covers *every* module the federation runs, not only the ones this SDK
    /// exposes as facades: a module the SDK never touches still participates
    /// in the check, because a federation running a mixed set is not a
    /// configuration this SDK is willing to hold funds in.
    ///
    /// ## When a running federation stops satisfying it
    ///
    /// A federation's configuration can change under a running instance —
    /// guardians upgrade, modules are added, a module generation moves. If
    /// the new configuration is mixed, or is otherwise one this SDK refuses,
    /// the outcome is defined rather than left to whatever the
    /// implementation happens to do:
    ///
    /// 1. **The refused configuration is never adopted.** The last
    ///    configuration that validated stays in force as the federation's
    ///    known configuration, which is what
    ///    [`Federation::name`](crate::Federation::name),
    ///    [`Federation::network`](crate::Federation::network) and
    ///    [`Federation::capabilities`](crate::Federation::capabilities) keep
    ///    reporting. The SDK never operates on a half-understood
    ///    configuration, and never silently drops a module out from under a
    ///    facade an application is holding.
    /// 2. **The federation is quarantined, not closed and not erased.** Its
    ///    workers stop, it leaves [`Sdk::federations`], and its status
    ///    becomes
    ///    [`Quarantined`](FederationStatus::Quarantined) carrying
    ///    [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation)
    ///    and a message naming the conflict. Nothing local is deleted: the
    ///    client state, the operation log, and the activity history are all
    ///    still there, so a federation that becomes supported again — the
    ///    guardians finish an upgrade, the application ships an SDK that
    ///    understands the new generation — comes back with
    ///    [`Sdk::reopen_federation`] and loses nothing.
    /// 3. **Pending work terminates observably, and is not thrown away.**
    ///    In-flight operation state is flushed durably first, and then every
    ///    outstanding
    ///    [`OperationUpdates::next`](crate::OperationUpdates::next) and
    ///    [`BalanceUpdates::next`](crate::BalanceUpdates::next) against the
    ///    federation resolves with
    ///    [`FederationClosed`](crate::ErrorCode::FederationClosed), as does
    ///    every subsequent fallible call on its handles. No subscriber is
    ///    left hanging — a promise that never settles is worse than a
    ///    failure, because a failure can be shown and retried. The
    ///    operations themselves are neither cancelled nor marked failed:
    ///    their persisted state is preserved verbatim and they resume where
    ///    they left off if the federation is reopened.
    ///
    /// Quarantine is deliberately loud, because step 3 means the SDK has
    /// stopped driving those state machines locally. Value already committed
    /// to a protocol will resolve however that protocol resolves it, with no
    /// local help until the federation runs again, which is precisely why
    /// this state is reported through
    /// [`Sdk::federation_status`] and
    /// [`Sdk::federation_status_updates`] rather than being inferred from an
    /// error some later call happens to return.
    ///
    /// The line between quarantine and ordinary trouble is drawn at
    /// *refusal*, not at reachability: a running federation whose guardians
    /// are unreachable is not quarantined, because that is transient and the
    /// SDK keeps retrying in the background. Quarantine is for a federation
    /// the SDK will not operate on until something changes.
    ///
    /// # Joining a federation whose erase is committed
    ///
    /// An id in [`Forgetting`](FederationStatus::Forgetting) is not "already
    /// joined" — it is on its way out of the storage — so joining it again is
    /// allowed, and it produces a *new* federation rather than reviving the
    /// old one. The committed erase is finished first, and only then does the
    /// join proceed exactly as a first-time join: the invite code is
    /// required, the configuration and client state are written fresh, and
    /// there is no balance, no operation log and no activity history carried
    /// over from before. Nothing of the erased federation survives its
    /// tombstone; the id being the same is a coincidence of the federation's
    /// identity, not continuity of local state.
    ///
    /// If that erase cannot be finished, the join fails with
    /// [`Storage`](crate::ErrorCode::Storage) and writes nothing: the
    /// federation stays [`Forgetting`](FederationStatus::Forgetting), to be
    /// retried by a later [`SdkBuilder::build`] or
    /// [`Sdk::forget_federation`]. Layering new state over state that is
    /// still being deleted is the one outcome that must not happen, since a
    /// resumed erase would then delete part of the new federation too.
    ///
    /// # A stale recovery intent does not survive a plain join
    ///
    /// The experimental seed-recovery entry point persists its intent to
    /// recover — and the operation id of the first attempt — *before*
    /// asking the underlying client to join, so a failure between those two
    /// writes can leave an intent for a federation that never actually
    /// joined. This call treats such an intent as the leftover it is:
    /// unless the underlying client's own durable state corroborates that a
    /// recovery was committed, the intent is discarded in the same
    /// transaction that records the plain join. A federation joined here
    /// can therefore never be misclassified as recovery-locked by a write
    /// an abandoned recovery attempt left behind — a distinction with
    /// teeth, because the erase path deliberately bypasses the balance
    /// guard for recovery-locked federations.
    ///
    /// # Errors
    ///
    /// [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined) when this
    /// instance already holds the federation — including when it holds it
    /// closed or quarantined, where [`Sdk::reopen_federation`] rather than a
    /// second join is the call that wants making, and excluding an id in
    /// [`Forgetting`](FederationStatus::Forgetting), which is handled as
    /// above — plus every error [`Sdk::preview`] can produce, and
    /// [`Storage`](crate::ErrorCode::Storage) if the join cannot be
    /// persisted or a committed erase for the same id cannot be finished
    /// first.
    pub async fn join(&self, invite: &InviteCode) -> Result<Federation> {
        unimplemented!()
    }

    /// Every federation this instance currently has open.
    ///
    /// This is the "what can I act on right now" list, and *open* is its
    /// exact meaning: a [`Federation`] here is
    /// [`Running`](FederationStatus::Running) or
    /// [`Recovering`](FederationStatus::Recovering) — the two open states of
    /// the lifecycle above — so the SDK is holding a live client for it, its
    /// facades work, and it answers calls rather than reporting itself stale.
    /// Federations that are closed, quarantined, or
    /// [`Forgetting`](FederationStatus::Forgetting) are not listed, because
    /// they have no live handle to hand out, and forgotten ones are gone
    /// entirely. A `Forgetting` federation is absent for as long as it stays
    /// in that state — including one whose erase stalled and is waiting for a
    /// later [`SdkBuilder::build`] to retry it, which never reappears here
    /// however long that takes, because a committed erase is never
    /// resurrected. The order is unspecified.
    ///
    /// A [`Recovering`](FederationStatus::Recovering) entry is a real, usable
    /// handle and not a placeholder. Its descriptive accessors —
    /// [`id`](crate::Federation::id), [`name`](crate::Federation::name),
    /// [`network`](crate::Federation::network),
    /// [`capabilities`](crate::Federation::capabilities) — answer as they
    /// would for any other federation, its facades and metadata are there,
    /// operations can be looked up, and its balance and activity are reported
    /// and kept up to date, provisionally, as the wallet is reconstructed.
    /// What it refuses is the work that needs a complete wallet: every send
    /// and receive, and taking a fresh backup, fails with
    /// [`Recovering`](crate::ErrorCode::Recovering) until the reconstruction
    /// completes. That is a documented refusal by a working
    /// federation, carrying its own code so an application can explain *why*
    /// and offer to wait — not the
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) of a handle
    /// with nothing behind it. So "every federation in this list can be acted
    /// on" holds unweakened; what a recovering one does with one class of
    /// call is part of acting on it, not an exception to it.
    ///
    /// Listing recovering federations is also what keeps a reconstruction
    /// discoverable across a restart. The calls that read and resume one take
    /// a [`FederationId`], and an application that persisted neither an
    /// operation id nor a federation id finds an *open* reconstruction here
    /// — and a closed or quarantined one only in [`Sdk::stored_federations`],
    /// which is why that list, not this one, is where such an application
    /// starts. Hiding recovering federations here would leave it holding a
    /// federation it can neither spend from nor find through the open list —
    /// a documented recovery path that does not exist.
    ///
    /// This is therefore *not* the list to render a wallet screen from. Use
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
    /// "Open" means precisely what it means in [`Sdk::federations`], and the
    /// two can never disagree: this returns `Some` for exactly the
    /// federations that list contains — [`Running`](FederationStatus::Running)
    /// and [`Recovering`](FederationStatus::Recovering) — with the same
    /// caveat attached to a recovering one, that it answers every call except
    /// the ones needing a complete wallet, which it refuses with
    /// [`Recovering`](crate::ErrorCode::Recovering) until its own is
    /// reconstructed. Looking an id up here is a lookup into that list, not a
    /// second and narrower notion of availability.
    ///
    /// `None` covers every reason there is no live handle: never joined,
    /// forgotten, closed, quarantined, or being erased. They are not
    /// distinguished here, because in all of those cases the answer to "may
    /// I act on this federation" is the same no.
    ///
    /// When the distinction matters — and it does for anything that renders
    /// a list, offers a "reconnect" affordance, or explains to a user why a
    /// wallet is not available — [`Sdk::federation_status`] answers it for
    /// this same id, and returns `None` only when the storage genuinely has
    /// no such federation.
    pub fn federation(&self, id: &FederationId) -> Option<Federation> {
        unimplemented!()
    }

    /// Every federation this instance's storage holds, running or not, each
    /// with its current [`FederationStatus`].
    ///
    /// This is the list a wallet screen should be built from.
    /// [`Sdk::federations`] answers "what can I act on"; this answers "what
    /// does this user have", which is the question a list of wallets is
    /// actually asking. A federation that was closed with
    /// [`Sdk::close_federation`], one that was quarantined because it could
    /// not be opened, and one whose erase is still finishing all appear
    /// here, labelled, rather than being absent — an application cannot
    /// distinguish a missing row from a wallet the user left, and would
    /// otherwise present a balance that has quietly lost a federation.
    ///
    /// It is a superset of [`Sdk::federations`], never a different set: every
    /// open federation appears here too, carrying
    /// [`Running`](FederationStatus::Running) or
    /// [`Recovering`](FederationStatus::Recovering), so the shorter list is
    /// this one filtered to the states that have a live handle. Every state
    /// [`FederationStatus`] names as stored appears here, and only a
    /// federation that has been fully forgotten appears in neither.
    ///
    /// A [`Forgetting`](FederationStatus::Forgetting) row is the one entry
    /// here that is not an offer of anything. It says "this id is on its way
    /// out": the erase is committed, the federation will never open again,
    /// [`Sdk::reopen_federation`] refuses it, and its balance and history are
    /// gone as far as this API is concerned — so render it as "removing…" and
    /// never as a wallet with a reconnect button. It stays listed, rather
    /// than vanishing the moment the tombstone lands, so that a stalled erase
    /// is visible instead of silent, and it disappears from this list exactly
    /// when the erase completes (announced once as
    /// [`Forgotten`](FederationStatus::Forgotten) to
    /// [`Sdk::federation_status_updates`] subscribers).
    ///
    /// It also closes a gap that would otherwise be unrecoverable. Closing a
    /// federation keeps all of its data but takes away its handle; without a
    /// listing like this one, and without [`Sdk::reopen_federation`], the
    /// only route back would be joining again with the original invite code
    /// — which an application may never have retained, and which the user
    /// may have no way to obtain a second time. A wallet the SDK is still
    /// holding must never become undiscoverable.
    ///
    /// Each entry is a small owned record rather than a handle, because most
    /// of what it describes has no handle to give: see [`FederationInfo`].
    /// The order is unspecified, as in [`Sdk::federations`]; sort by
    /// whatever the screen shows.
    ///
    /// Infallible and synchronous: the statuses are instance state, not a
    /// storage read. Like the descriptive accessors on [`Federation`], this
    /// keeps answering after [`Sdk::shutdown`], reporting the last statuses
    /// the instance knew.
    pub fn stored_federations(&self) -> Vec<FederationInfo> {
        unimplemented!()
    }

    /// What this instance's storage currently knows about one federation.
    ///
    /// `None` means precisely one thing: this storage holds no federation
    /// with that id, because it was never joined or because it was
    /// successfully forgotten. Every other case — running, recovering,
    /// closed, quarantined, mid-erase — is a `Some` carrying the state, so
    /// that "there is nothing here" and "there is something here that is not
    /// currently usable" are never confused. That distinction is the whole
    /// point of this accessor existing alongside [`Sdk::federation`], which
    /// deliberately collapses them.
    ///
    /// The boundary between those two answers is the erase, not the
    /// tombstone. A federation whose erase is committed answers
    /// `Some(`[`Forgetting`](FederationStatus::Forgetting)`)` for as long as
    /// the erase is unfinished — across a stalled attempt, across a restart,
    /// across as many builds as it takes — and flips to `None` only once the
    /// state is actually gone. So `None` here is always safe to read as "this
    /// storage has nothing of that federation left", and `Some(Forgetting)`
    /// always as "the bytes are still going away", never as a wallet.
    ///
    /// This is the observable side of quarantine, and the reason quarantine
    /// is not something an application has to discover by provoking an
    /// error: the [`ErrorCode`](crate::ErrorCode) and message inside
    /// [`Quarantined`](FederationStatus::Quarantined) say why the federation
    /// is not running, without any call having to fail first. It follows the
    /// same principle as [`Federation::capabilities`](crate::Federation::capabilities)
    /// — what the SDK can and cannot do is a value to read and branch on,
    /// not an exception to catch.
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
    /// federation is quarantined, a recovery finishes, another clone of this
    /// [`Sdk`] closes a federation, an erase completes. Polling
    /// [`Sdk::stored_federations`] would work but would make a UI choose
    /// between a stale list and a timer, so the change is pushed instead.
    ///
    /// Each call returns its own cursor, exactly like
    /// [`Federation::balance_updates`](crate::Federation::balance_updates):
    /// two subscribers both see every change and neither consumes the
    /// other's updates. This is instance-wide rather than per-federation,
    /// which is why it yields a whole [`FederationInfo`] — the same record
    /// [`Sdk::stored_federations`] returns, so a list screen updates by
    /// replacing the row whose
    /// [`id`](FederationInfo::id) matches and needs no second shape to
    /// interpret.
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
    /// code is required and none is accepted: requiring one would make
    /// reaching a wallet the SDK is holding depend on the application having
    /// kept a bearer credential it was never told to keep.
    ///
    /// It runs the same open sequence [`SdkBuilder::build`] runs for a
    /// remembered federation: revalidate the configuration against the
    /// module-generation rule described on [`Sdk::join`], start the
    /// background workers, and resume unfinished operations from where they
    /// were persisted. On success the federation is open and appears in
    /// [`Sdk::federations`] again, and it is reopened automatically by later
    /// builds — reopening clears the opt-out that
    /// [`Sdk::close_federation`] set. Which open state it lands in is not
    /// this call's choice but the federation's: it is
    /// [`Running`](FederationStatus::Running), or
    /// [`Recovering`](FederationStatus::Recovering) if the reconstruction of
    /// its wallet from the seed was still unfinished when it stopped, since
    /// that is persisted with the rest of its state and resumes here like any
    /// other unfinished work. Either way the handle this returns is live and
    /// listed; a recovering one refuses sends and receives with
    /// [`Recovering`](crate::ErrorCode::Recovering) and answers everything
    /// else.
    ///
    /// Handles obtained before the federation stopped running are *not*
    /// revived. They stay closed, and their fallible calls keep failing with
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed); the handle
    /// this call returns is the live one. Reviving old handles would mean a
    /// stale reference silently becoming live again, which is a harder
    /// property to reason about than "a closed handle is closed forever".
    ///
    /// Reopening a federation that is already open — running or recovering —
    /// is not an error: it returns the live handle, mirroring the idempotence
    /// of [`Sdk::close_federation`], because the postcondition the caller
    /// asked for already holds. It does not, and cannot, hurry a
    /// reconstruction along: a recovering federation reopened this way is
    /// handed back still recovering.
    ///
    /// A failed reopen leaves the federation
    /// [`Quarantined`](FederationStatus::Quarantined) with the same
    /// [`ErrorCode`](crate::ErrorCode) this call returns, so the failure is
    /// both reported to the caller and recorded for anything reading statuses
    /// later. That also means later builds will retry it: the application
    /// asked for this federation to be running, and quarantine records "meant
    /// to run, currently cannot" rather than "deliberately stopped".
    /// [`Sdk::close_federation`] is how to give up on it.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) when this storage
    /// holds no federation with that id, or holds one in
    /// [`Forgetting`](FederationStatus::Forgetting) — a committed erase is
    /// never resurrected, however unfinished it is and however long it stays
    /// that way, because resurrecting one would defeat the tombstone that
    /// makes the erase crash-safe in the first place and would hand back a
    /// client whose state has holes in it. Both cases mean the same thing to
    /// a caller: the id names nothing openable. (This mirrors
    /// [`Federation::activity`](crate::Federation::activity), which reports
    /// a cursor it did not issue the same way.) Then
    /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation)
    /// for a configuration this SDK refuses,
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable)
    /// and [`Timeout`](crate::ErrorCode::Timeout) when the guardians cannot
    /// be reached in time, [`Storage`](crate::ErrorCode::Storage) if the
    /// federation's local state cannot be read, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) if the whole
    /// instance has been shut down.
    pub async fn reopen_federation(&self, id: &FederationId) -> Result<Federation> {
        unimplemented!()
    }

    /// Stops running this federation while keeping all of its data.
    ///
    /// The federation's background workers stop, it is dropped from
    /// [`Sdk::federations`], its status becomes
    /// [`Closed`](FederationStatus::Closed), and it is no longer reopened
    /// automatically by later builds against this storage. Nothing is
    /// deleted: the client state, the operation log, and the activity
    /// history all remain, it stays listed by [`Sdk::stored_federations`],
    /// and [`Sdk::reopen_federation`] restores access to all of it without
    /// needing an invite code. This is the non-destructive half of leaving a
    /// federation; [`Sdk::forget_federation`] is the destructive half.
    ///
    /// Any [`Federation`] handle an application still holds for this
    /// federation keeps existing but stops doing work: its fallible calls
    /// fail with
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed), as do
    /// pending [`BalanceUpdates::next`](crate::BalanceUpdates::next) and
    /// [`OperationUpdates::next`](crate::OperationUpdates::next) calls
    /// against it. Its infallible accessors cannot fail and do not pretend
    /// to — [`Federation`] documents what each returns once closed.
    ///
    /// Closing is idempotent: an id that names no open federation is not an
    /// error, because the postcondition — the federation is not running and
    /// its data is intact — already holds. That includes an id that names a
    /// quarantined federation, which closing turns into a deliberate
    /// [`Closed`](FederationStatus::Closed) so that later builds stop
    /// retrying it.
    ///
    /// One id is accepted and deliberately changes nothing: one in
    /// [`Forgetting`](FederationStatus::Forgetting). It is already not
    /// running, so this returns `Ok(())`, but its status stays `Forgetting`
    /// and the committed erase still proceeds. A committed erase is never
    /// turned back into a stored, closed federation — that would resurrect
    /// state the tombstone has already given away — so an application that
    /// wants to know what it now has reads
    /// [`Sdk::federation_status`] rather than assuming this call produced a
    /// [`Closed`](FederationStatus::Closed).
    ///
    /// A [`Recovering`](FederationStatus::Recovering) federation closes like
    /// any other, and closing it is not a way out of the reconstruction it is
    /// in the middle of. Its status becomes
    /// [`Closed`](FederationStatus::Closed), the unfinished reconstruction is
    /// preserved along with the rest of its persisted state, and
    /// [`Sdk::reopen_federation`] brings the federation back as
    /// [`Recovering`](FederationStatus::Recovering) with that work resuming
    /// where it stopped. Finishing the reconstruction, or the destructive
    /// [`Sdk::forget_federation`], are the only things that end one; this
    /// call merely stops working on it for now.
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
    /// reconstruct, not what was only ever recorded locally — and because
    /// the configuration goes too, re-joining needs an invite code again.
    /// This is the one lifecycle call that requires the application to have
    /// kept one; [`Sdk::close_federation`] and [`Sdk::reopen_federation`]
    /// exist so that merely wanting a federation to stop running never does.
    ///
    /// Because of that, the call is guarded rather than forceful, and it
    /// runs in three phases whose order is part of the contract.
    ///
    /// # 1. Quiesce, atomically, before anything is checked
    ///
    /// The federation is retired first: in one atomic step it leaves
    /// [`Sdk::federations`], every outstanding [`Federation`] handle,
    /// facade, and subscriber for it is closed, and its background workers
    /// are stopped after flushing their state durably. Only then is
    /// eligibility evaluated.
    ///
    /// "Its background workers" includes a running seed rescan. Stopping one
    /// here is the *only* way anything in this SDK can end a rescan — there
    /// is deliberately no cancel-recovery call — and it is why this call has
    /// to remain reachable on a recovering federation; see below.
    ///
    /// This ordering is required, not incidental. Checking eligibility
    /// against a federation that is still running would leave a window
    /// between the check and the erase in which a cloned facade — and every
    /// handle in this crate is cheaply cloneable, so an application may hold
    /// many — could start a payment, mint notes, or spawn a state machine.
    /// The erase would then delete the local record of value that had just
    /// moved. There is no way to close that window from the caller's side,
    /// so the SDK closes it: nothing can start work on a federation that is
    /// already retired.
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
    /// One class of non-final operation is deliberately exempt: an on-chain
    /// receive that has not yet seen a transaction. It holds no value — a
    /// deposit address with nothing sent to it protects nothing — and the
    /// only thing that could ever settle it is a stranger deciding to send
    /// money, so counting it would let a single unused address block the
    /// erase indefinitely. Once a transaction *has* been seen it is an
    /// ordinary pending operation and does block, until it is claimed or
    /// fails. See [`Onchain::receive`](crate::Onchain::receive).
    ///
    /// The reclaimable-value condition is the non-obvious one: notes handed
    /// out but not yet redeemed are still worth money to the sender until
    /// their reclaim window closes, and the record needed to reclaim them
    /// lives in exactly the state this call would delete. Reclaimable
    /// *incoming* value is guarded too, through the non-final rule rather
    /// than a condition of its own: a lightning receive whose payment
    /// arrived but whose claim was rejected sits in the non-final
    /// [`ClaimRetrying`](crate::LnReceiveState::ClaimRetrying), and the
    /// local receive keys its retry depends on are likewise state this call
    /// would delete.
    ///
    /// Every guard here is protecting value the caller **could still move**
    /// if they did something else first: spend the balance down, let an
    /// operation settle, reclaim the notes. That framing is what decides the
    /// next question.
    ///
    /// ## Recovery is never a reason to refuse
    ///
    /// A federation whose recovery has not completed is recovery-locked:
    /// every spend and receive against it is refused with
    /// [`Recovering`](crate::ErrorCode::Recovering), only a completed
    /// recovery releases the lock, and no call in this SDK stops or cancels
    /// a recovery. This erase is therefore the sole exit from a recovery that
    /// cannot be finished, and **none of the guards above may block it.**
    /// Concretely:
    ///
    /// - **A recovery-locked federation's balance does not count.** What
    ///   such a federation reports is a *provisional* figure that is still
    ///   moving as the rescan proceeds, and none of it is spendable —
    ///   spendable is what the guard is about, and the lock is precisely why
    ///   nothing is. Counting it would close the last door: the recovery
    ///   cannot be finished, the balance cannot be spent down, and so the
    ///   federation could never be erased either.
    /// - **The rescan is not a "pending operation" for this purpose.** A
    ///   rescan that is still going is a non-final operation, and one that
    ///   stopped short of completing holds the lock just as firmly. Neither
    ///   blocks this call, and phase 1 aborts a running rescan as part of the
    ///   erase.
    /// - **Reclaimable outgoing value on a locked federation does not block
    ///   it either**, because reclaiming is itself a spend and the lock
    ///   refuses it. Such value is forfeited by the erase, which is one of
    ///   the costs of this exit rather than a reason to deny it.
    ///
    /// So "no pending operations" must not be read as "not recovering", and
    /// this call never returns
    /// [`Recovering`](crate::ErrorCode::Recovering) under any circumstances.
    ///
    /// What that exit costs is real and should be put in front of the user
    /// before they take it: the recovered-so-far state is thrown away, the
    /// local activity history is gone for good, locally-recorded reclaimable
    /// value is forfeited, and starting over needs the invite code again. The
    /// federation still holds the funds, so a fresh join-and-recover can find
    /// them again — but it starts the recovery, and the lock, from the
    /// beginning.
    ///
    /// A refusal deletes nothing whatsoever. It does, however, leave the
    /// federation stopped — it was quiesced in phase 1, and handles are not
    /// revived — so its status afterwards is
    /// [`Closed`](FederationStatus::Closed) and
    /// [`Sdk::reopen_federation`] is how the application gets it running
    /// again. That is a deliberate trade: the alternative is to check first
    /// and quiesce afterwards, which reintroduces exactly the race phase 1
    /// exists to prevent. Losing a handle is recoverable in one call;
    /// deleting the record of in-flight value is not.
    ///
    /// # 3. Commit the erase before performing it
    ///
    /// "The deletion failed partway, and nothing records that it was meant
    /// to happen" is the outcome that must not exist. Such a federation is
    /// neither reopenable — doing so would resurrect a client whose state has
    /// holes in it — nor safely retryable, since nothing says what the
    /// half-deleted state was on its way to. Note that this is a statement
    /// about the missing *record*, not about the partial deletion: a
    /// half-finished erase whose decision is recorded is a perfectly
    /// well-defined state, and it is the third of the three below. So the
    /// erase is made atomic with respect to crashes and failures alike by
    /// committing the *decision* first. A durable tombstone is written in a
    /// single step, and only then is any state removed. From the moment the
    /// tombstone lands, the federation is gone as far as this API is
    /// concerned and the deletion is owed: this call finishes it, or a later
    /// [`Sdk::forget_federation`] with the same id resumes it, or the next
    /// [`SdkBuilder::build`] attempts it before the federation could be
    /// opened. A tombstoned federation is never opened, never handed a
    /// handle, and never resurrected, no matter how many of those attempts
    /// fail — [`Sdk::reopen_federation`] refuses it with
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput), and a fresh
    /// [`Sdk::join`] of the same id finishes the erase first and then joins
    /// from nothing.
    ///
    /// What is *not* promised is a deadline. An erase whose completion keeps
    /// failing — a backend that will not delete, a file the platform is
    /// holding — stays committed and unfinished, and the federation sits in
    /// [`Forgetting`](FederationStatus::Forgetting), visible in
    /// [`Sdk::stored_federations`] and answering
    /// [`Sdk::federation_status`], until an attempt succeeds. That is the
    /// deliberate choice: a stalled erase is reported per federation rather
    /// than escalated into a failure of the whole instance, exactly as a
    /// federation that will not open is quarantined rather than sinking
    /// [`SdkBuilder::build`]. One undeletable federation must not lock a
    /// user out of the others, of their history, or of
    /// [`Sdk::export_mnemonic`].
    ///
    /// (A tombstone rather than a backend transaction because this crate has
    /// to make the same promise on two very different backends — see
    /// [`Storage`] — and a multi-key atomic delete cannot be assumed on
    /// both. A single atomic write can.)
    ///
    /// A federation is therefore always in exactly one of three states when
    /// this call returns, and there is no fourth:
    ///
    /// - **Erased.** `Ok(())`, the state is gone,
    ///   [`Sdk::federation_status`] returns `None`, and the id no longer
    ///   appears in [`Sdk::stored_federations`].
    /// - **Fully intact and closed.** The call refused in phase 2, or could
    ///   not even write the tombstone, and nothing was deleted.
    /// - **Committed but unfinished.** The tombstone landed and the erase
    ///   did not complete. The status is
    ///   [`Forgetting`](FederationStatus::Forgetting), so an application can
    ///   show "removing…" instead of a phantom wallet: the id is listed by
    ///   [`Sdk::stored_federations`] and answered by
    ///   [`Sdk::federation_status`], absent from [`Sdk::federations`] and
    ///   from [`Sdk::federation`], and refused by
    ///   [`Sdk::reopen_federation`]. The call is safely retryable with the
    ///   same id, and a later [`SdkBuilder::build`] retries it too.
    ///
    /// Forgetting is idempotent: an id with no local state is not an error,
    /// and an id that is already tombstoned finishes the erase and returns
    /// `Ok(())` — or, if it still cannot be finished, returns
    /// [`Storage`](crate::ErrorCode::Storage) and leaves it
    /// [`Forgetting`](FederationStatus::Forgetting) again.
    ///
    /// The status does not carry *why* an erase stalled. That is not an
    /// oversight but the limit of a state that must stay a bare variant: the
    /// reason belongs in logs and in the [`Error`](crate::Error) this call
    /// returns, and if it turns out applications need it as a value, the
    /// additive route is a new, more specific status variant rather than a
    /// field grown on this one — see the note on [`FederationStatus`].
    ///
    /// # Errors
    ///
    /// [`BalanceNotEmpty`](crate::ErrorCode::BalanceNotEmpty) and
    /// [`PendingOperations`](crate::ErrorCode::PendingOperations) for a
    /// phase-2 refusal, which deletes nothing;
    /// [`Storage`](crate::ErrorCode::Storage) if the backend fails, which
    /// means either that nothing was deleted or that the erase is committed
    /// and unfinished — never that the federation is in an unknown state —
    /// and which is retryable either way; and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) after
    /// shutdown. Never
    /// [`Recovering`](crate::ErrorCode::Recovering): a recovery in any state
    /// is not a reason to refuse, for the reasons given above.
    pub async fn forget_federation(&self, id: &FederationId) -> Result<()> {
        unimplemented!()
    }

    /// Returns this instance's seed phrase, for the user to write down.
    ///
    /// The name says *export* on purpose. This is the one call that takes a
    /// secret out of the SDK's custody, and it should be obvious at the
    /// call site — in a code review, in a grep of the application, in a
    /// binding's generated API — that this is what is happening. A shorter
    /// name like `mnemonic()` would read as an ordinary accessor and hide
    /// that.
    ///
    /// What the caller receives is a [`Mnemonic`], which neither prints
    /// itself nor formats itself; extracting the words from it is a second
    /// deliberate step ([`Mnemonic::words`]). Everything downstream of that
    /// step — a string in Swift, Kotlin, or JavaScript, a clipboard, a
    /// screenshot, a crash report — is the application's responsibility,
    /// as documented on that type.
    ///
    /// This is infallible and synchronous because the seed is loaded once
    /// when the instance is built and held in memory for its lifetime; it
    /// does not read storage and remains available after
    /// [`Sdk::shutdown`].
    ///
    /// It is also the reason [`SdkBuilder::build`] refuses to fail over a
    /// federation. An instance whose every federation is quarantined still
    /// exports its seed, which is the user's route to their money by any
    /// other client. A design where one unreachable federation could deny
    /// this call would be a design in which a guardian outage can look
    /// indistinguishable from lost funds.
    pub fn export_mnemonic(&self) -> Mnemonic {
        unimplemented!()
    }

    /// Best-effort: flushes everything to storage, stops all background
    /// work, and releases the storage lock.
    ///
    /// After this returns, the instance is finished: every fallible call on
    /// every [`Sdk`] and [`Federation`] handle, and every subscriber
    /// obtained from one, fails with
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) — with
    /// [`Sdk::export_mnemonic`] the deliberate exception, as noted there,
    /// alongside the infallible status accessors
    /// ([`Sdk::stored_federations`], [`Sdk::federation_status`]) and the
    /// infallible accessors on [`Federation`]. Another instance may then
    /// open the same storage. Shutdown is idempotent — calling it twice is
    /// not an error.
    ///
    /// # It is an optimisation, not a requirement
    ///
    /// **Correctness does not depend on this call.** iOS and Android
    /// terminate backgrounded applications without warning, without
    /// unwinding, and without awaiting anything an application would like to
    /// await; a browser tab can vanish the same way. Any design in which a
    /// missed shutdown loses money is therefore a design that loses money in
    /// production, so this crate does not have one. Everything a caller can
    /// observe is already durable at the moment it becomes observable — see
    /// the durability rule on [`Sdk`] — and what this call adds is:
    ///
    /// - a flush of buffered non-critical state, such as caches and the
    ///   in-memory tail of activity, so the next open has less to redo;
    /// - an orderly release of the storage lock, so a subsequent opener
    ///   does not have to reclaim it;
    /// - a defined point after which no background work is running, which
    ///   is what a test harness or a CLI wants before it exits.
    ///
    /// Call it from the platform's "entering background" or "about to
    /// terminate" callback if there is one, and await it if you are allowed
    /// to. Do not build anything on being able to.
    ///
    /// # What survives an abrupt kill
    ///
    /// If the process dies without this call — killed by the OS, crashed,
    /// tab closed — then on the next [`SdkBuilder::build`] over the same
    /// storage:
    ///
    /// - **Everything acknowledged is there.** Any value a completed call
    ///   returned or a subscriber yielded — a joined federation, an
    ///   [`OperationId`](crate::OperationId), an operation state, a
    ///   committed erase — is present. Nothing acknowledged is lost.
    /// - **In-flight operations resume by themselves.** They were persisted
    ///   as they ran, they keep their ids, they are found by
    ///   [`Federation::operation`](crate::Federation::operation), and they
    ///   continue from their last persisted checkpoint. Resumption from a
    ///   checkpoint is idempotent, so an operation is neither dropped nor
    ///   performed twice, and it terminates in a state exactly as it would
    ///   have without the interruption.
    /// - **A call that never returned may or may not have happened.** This
    ///   is the one thing a caller does not get to know, and no design can
    ///   give it: the process died between the write and the return. The
    ///   persisted state after reopening is authoritative, which is why
    ///   operations are addressed by a stable
    ///   [`OperationId`](crate::OperationId) — retry by looking up the id,
    ///   not by repeating the request.
    /// - **The seed is never at risk.** It is written before anything
    ///   derived from it exists, so there is no crash window that leaves
    ///   federation state derived from a seed that was never saved. See
    ///   [`SdkBuilder::build`].
    /// - **A lock left behind is reclaimed, not fatal.** A storage lock held
    ///   by a process that was killed is recovered by the next opener on
    ///   that device; [`StorageInUse`](crate::ErrorCode::StorageInUse) means
    ///   genuinely concurrent use, never a stale lock. A single crash must
    ///   not be able to make a wallet permanently unopenable.
    /// - **A committed erase stays committed.** A federation whose tombstone
    ///   landed before the kill is never reopened half-erased: the next
    ///   build finishes the deletion off, and if that attempt fails the
    ///   federation stays [`Forgetting`](FederationStatus::Forgetting) and
    ///   is retried later — it does not come back as a wallet, and it does
    ///   not fail the build. See [`Sdk::forget_federation`].
    ///
    /// Shutting down does not cancel operations in the sense of undoing
    /// them: an operation that was running is persisted mid-flight and
    /// resumes when the storage is opened again, exactly as it would after
    /// a crash. That equivalence is the point — the clean path and the
    /// crash path lead to the same place.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage) if the final flush fails. The
    /// instance is closed either way, and because nothing observable was
    /// waiting on that flush, the failure is a diagnostic rather than a loss
    /// of data.
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
/// `Debug` is hand-written rather than derived, and redacts the mnemonic:
/// the whole point of [`Mnemonic`] not implementing `Debug` would be lost
/// if a builder holding one printed it. (A derive would not compile here
/// for that same reason.)
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
    /// here still touches nothing: the location is opened, and every failure
    /// that needs the environment reported, by [`SdkBuilder::build`].
    pub fn storage(mut self, storage: Storage) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Sets the BIP-39 seed the instance derives every federation secret
    /// from.
    ///
    /// Supply one to restore an existing wallet from a written-down phrase.
    /// Omit it and the instance uses the seed already in storage, or — if
    /// the storage is proven empty — generates a fresh one and persists it
    /// before deriving anything from it. Generating a seed can fail, because
    /// drawing secure entropy can (see [`Mnemonic::generate`]); when it
    /// does, [`SdkBuilder::build`] reports
    /// [`Entropy`](crate::ErrorCode::Entropy) rather than panicking or
    /// settling for a weaker source.
    ///
    /// "Proven empty" is load-bearing and is spelled out on
    /// [`SdkBuilder::build`]: a seed is established only over a backend that
    /// holds nothing else, never merely because no seed was found.
    ///
    /// Supplying a mnemonic that differs from the one the storage already
    /// holds is a mistake the SDK will not paper over: [`SdkBuilder::build`]
    /// fails with
    /// [`SeedMismatch`](crate::ErrorCode::SeedMismatch) and changes
    /// nothing. Restoring a different wallet means pointing at a different
    /// [`Storage`].
    pub fn mnemonic(mut self, mnemonic: Mnemonic) -> Self {
        self.mnemonic = Some(mnemonic);
        self
    }

    /// Opens the storage, loads or establishes the seed, attempts to finish
    /// any erase that was left committed, reopens every federation the
    /// storage remembers, and resumes their pending operations.
    ///
    /// The order of those steps is part of the contract, because it is what
    /// makes the failure modes safe. Two of them are *attempts* whose failure
    /// is scoped to one federation rather than to this call — finishing an
    /// erase, and opening a federation — and both say so below.
    ///
    /// 1. **Open the location and take its lock.** The [`Storage`] given to
    ///    [`SdkBuilder::storage`] is only a descriptor — a validated name for
    ///    a place, per that type — so this is the first moment the place
    ///    itself is touched, and it is where every failure that needs the
    ///    environment is reported. Two things happen, in this order:
    ///
    ///    *The location is created or found.* A native directory that cannot
    ///    be created, or is not readable and writable, fails with
    ///    [`Storage`](crate::ErrorCode::Storage). So does a browser origin
    ///    with no usable origin-private file system — a context that does not
    ///    provide one, or one where storage access is denied or refused by
    ///    the user. None of those can be known synchronously in a browser,
    ///    which is why they are reported by this `async` call rather than by
    ///    [`Storage::in_browser`].
    ///
    ///    *Then the single-opener lock is taken.* If the location is already
    ///    open — in this process or another, or in another tab, iframe or
    ///    worker of the same origin — the call fails with
    ///    [`StorageInUse`](crate::ErrorCode::StorageInUse) and nothing has
    ///    been touched. A lock left behind by a process that died without
    ///    [`Sdk::shutdown`] is reclaimed rather than treated as contention;
    ///    see [`Storage`] for the native and browser cases. (Sharing one
    ///    location between live processes is not part of the 0.1 contract;
    ///    see [`Storage`] for the future shape of that.)
    /// 2. **Reconcile the seed, before any mutation.** This is the step
    ///    where a wrong answer silently corrupts a wallet, so it is stated
    ///    exhaustively. There are exactly four cases:
    ///    - *The storage holds a usable seed.* It is used. If a different
    ///      mnemonic was supplied the call fails with
    ///      [`SeedMismatch`](crate::ErrorCode::SeedMismatch).
    ///    - *The storage is proven empty* — no seed and no state of any kind
    ///      belonging to this SDK: no federation record, no client state, no
    ///      operation log, no activity history. The supplied or freshly
    ///      generated mnemonic is written durably *now*, before any
    ///      federation-derived state can exist, so there is no crash window
    ///      that could leave state derived from a seed that was never saved.
    ///      Generating that mnemonic is itself fallible (see
    ///      [`Mnemonic::generate`]): if the platform's secure random source
    ///      fails, the call fails with
    ///      [`Entropy`](crate::ErrorCode::Entropy) and nothing has been
    ///      written.
    ///    - *There is no usable seed but there is other state.* The storage
    ///      is **orphaned**, and the call fails with
    ///      [`StorageOrphaned`](crate::ErrorCode::StorageOrphaned) without
    ///      writing anything, carrying
    ///      [`ErrorDetails::StorageOrphaned`](crate::ErrorDetails::StorageOrphaned)
    ///      with the location and `seed_present: false`.
    ///    - *The seed entry was read in full but is unusable* — truncated,
    ///      corrupt, or written in a format this build does not understand.
    ///      Also a refusal, with the same code and detail case but
    ///      `seed_present: true`, and again without writing anything. This
    ///      case requires the backend to have **returned** the bytes: a read
    ///      the backend failed to perform decides nothing about the seed and
    ///      fails with [`Storage`](crate::ErrorCode::Storage) instead, which
    ///      is retryable — a transient outage must not be reported under a
    ///      permanent code.
    ///
    ///    The last two cases are why "no seed" must never be read as "fresh
    ///    storage". Writing a new seed over storage that already holds
    ///    federation or client state would associate that state with the
    ///    wrong derivation root: every per-federation secret would be
    ///    derived from a seed that has nothing to do with the notes,
    ///    operations, and backups already sitting there. The wallet would
    ///    open, look empty or nearly so, and the real funds would be
    ///    unreachable without the original phrase — while the only local
    ///    trace of which seed the state belonged to had just been
    ///    overwritten. Refusing is recoverable; a wrong write is not. The
    ///    same applies to an unusable seed entry: it may be a newer on-disk
    ///    format an updated build could read, and overwriting it turns a
    ///    solvable problem into permanent fund loss.
    ///
    ///    **Ordering guarantee.** The emptiness proof and the seed
    ///    reconciliation happen under the lock taken in step 1 and strictly
    ///    before any write this call makes. If step 2 fails for any reason,
    ///    the backend is byte-identical to how it was found.
    /// 3. **Attempt every committed erase, and keep a failure to the one
    ///    federation.** A federation whose tombstone was written but whose
    ///    deletion did not complete (see [`Sdk::forget_federation`]) is
    ///    erased now, before it could be opened. A committed erase is never
    ///    resurrected, whatever the outcome of this step: step 4 does not
    ///    open a tombstoned federation and no handle is ever handed out for
    ///    one.
    ///
    ///    A deletion that fails here **does not fail this call.** That
    ///    federation stays [`Forgetting`](FederationStatus::Forgetting): it
    ///    is absent from [`Sdk::federations`] and [`Sdk::federation`],
    ///    present and labelled in [`Sdk::stored_federations`], answered as
    ///    `Some(Forgetting)` by [`Sdk::federation_status`], refused by
    ///    [`Sdk::reopen_federation`], and attempted again by the next build
    ///    or by another [`Sdk::forget_federation`] with the same id. The
    ///    reasoning is step 4's reasoning, applied to the same class of
    ///    problem: a single federation whose bytes will not go away must not
    ///    deny the user every other federation, all of their history, and
    ///    [`Sdk::export_mnemonic`]. So this call *attempts* to finish a
    ///    committed erase and reports the one it could not finish as that
    ///    federation's status — it does not promise that no `Forgetting`
    ///    federation exists once it returns.
    /// 4. **Reopen the federations, and quarantine the ones that will not
    ///    open.** Each federation the storage remembers, and that was not
    ///    closed with [`Sdk::close_federation`], is revalidated (including
    ///    the module-generation rule described on [`Sdk::join`]) and
    ///    started, and its unfinished operations resume from where they were
    ///    persisted. Before its handle is handed out, the SDK reconciles any
    ///    issuance failure the underlying client's executor produced while
    ///    the SDK was not running — the partial credit such a failure leaves
    ///    is reconstructed, not observed, per [the *Measuring*
    ///    section][measuring] of
    ///    [`EcashReceiveDetails`](crate::EcashReceiveDetails) — so no record
    ///    is ever readable with a credit still being counted.
    ///
    ///    A federation whose wallet was still being reconstructed from the
    ///    seed comes back [`Recovering`](FederationStatus::Recovering) — that
    ///    reconstruction is one of the unfinished operations that resume here
    ///    — and is listed by [`Sdk::federations`] alongside the
    ///    [`Running`](FederationStatus::Running) ones, since both are open.
    ///    That is how an application which persisted nothing across the
    ///    restart finds such a federation again, and why the list is defined
    ///    by having a live handle rather than by being fully usable.
    ///
    ///    A federation that cannot be opened **does not fail this call**. It
    ///    is put into [`Quarantined`](FederationStatus::Quarantined) with
    ///    the [`ErrorCode`](crate::ErrorCode) and message that explain why,
    ///    it is absent from [`Sdk::federations`], and it is present and
    ///    labelled in [`Sdk::stored_federations`]. Later builds retry it.
    ///
    ///    This is a deliberate reversal of the obvious design. Failing the
    ///    build is worse than it looks: one federation whose guardians are
    ///    down, or that has upgraded to a configuration this build refuses,
    ///    would deny the user every *healthy* federation, all of their
    ///    history, and [`Sdk::export_mnemonic`] — the call that gets their
    ///    seed out so they can reach their money with something else. A
    ///    single unreachable guardian set must not be indistinguishable from
    ///    a broken wallet. The original concern that motivated failing —
    ///    that an application would read a short [`Sdk::federations`] list
    ///    as "the user left that federation" — is answered by making the
    ///    federation *visible with a reason* instead of by refusing to
    ///    start.
    ///
    /// Top-level `Err` is therefore reserved for the root storage and the
    /// seed: things that make the whole instance unsound — steps 1 and 2.
    /// Anything scoped to one federation, whether it is a federation that
    /// will not open or an erase that will not finish, is reported as that
    /// federation's status by a call that returns `Ok`.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) if no storage was
    /// set, [`StorageInUse`](crate::ErrorCode::StorageInUse),
    /// [`SeedMismatch`](crate::ErrorCode::SeedMismatch),
    /// [`Entropy`](crate::ErrorCode::Entropy) if a fresh seed had to be
    /// generated and the platform's secure random source failed,
    /// [`Storage`](crate::ErrorCode::Storage) for a root-storage failure —
    /// which covers the location that could not be created, opened, or read
    /// and written in step 1, including a browser origin that provides no
    /// usable file system or denies access to it — and
    /// [`StorageOrphaned`](crate::ErrorCode::StorageOrphaned), with
    /// [`ErrorDetails::StorageOrphaned`](crate::ErrorDetails::StorageOrphaned),
    /// for the orphaned and unreadable-seed cases in step 2. Those two are
    /// deliberately separate codes: a root-storage failure is a backend fault
    /// worth retrying, while an orphaned location is permanent and needs a
    /// person to act.
    ///
    /// Notably **not** here:
    /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation)
    /// and [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable).
    /// Those are per-federation conditions and arrive as
    /// [`Quarantined`](FederationStatus::Quarantined) statuses on a
    /// successfully built instance. Nor does a per-federation storage
    /// failure appear here: a federation whose local state cannot be read is
    /// a [`Quarantined`](FederationStatus::Quarantined) carrying
    /// [`Storage`](crate::ErrorCode::Storage), and a committed erase that
    /// could not be completed is a
    /// [`Forgetting`](FederationStatus::Forgetting) — neither is a reason for
    /// this call to fail.
    ///
    /// [measuring]: crate::EcashReceiveDetails#measuring-what-a-failed-issuance-left-behind
    pub async fn build(self) -> Result<Sdk> {
        unimplemented!()
    }
}

impl core::fmt::Debug for SdkBuilder {
    /// Prints the builder with the mnemonic redacted — whether one is set
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
/// and it is deliberately a **value to read** rather than an error to
/// provoke — the same principle as
/// [`Federation::capabilities`](crate::Federation::capabilities). An
/// application lays out a screen from statuses; it does not attempt
/// operations to discover which federations are healthy.
///
/// Read one with [`Sdk::federation_status`], read them all with
/// [`Sdk::stored_federations`], and follow changes with
/// [`Sdk::federation_status_updates`]. The state machine these variants form
/// — and which call moves a federation between them — is documented in one
/// place, on [`Sdk`].
///
/// # Shape
///
/// A flat data enum: no generics, no tuple variants, no borrowed data, and
/// nothing nested beyond the [`Diagnostic`](crate::Diagnostic) record that
/// [`Quarantined`](FederationStatus::Quarantined) carries. One caveat keeps
/// that from being the whole story: a `Diagnostic`'s typed details reach the
/// growing [`ErrorDetails`](crate::ErrorDetails) enum, which is not safely
/// decodable by an older generated binding. So the variants here generate
/// mechanically, but the diagnostic's details field crosses the boundary in
/// the same shape [`Error::details`](crate::Error::details) does — as the
/// raw envelope, projected locally by the reader (see
/// [`DetailEnvelope`](crate::DetailEnvelope)) — rather than as a generated
/// nested enum.
///
/// The enum is `#[non_exhaustive]`; its variants are not. Rust callers write
/// a wildcard arm, and a state that exists never grows a field — a generated
/// record that grows a field is exactly what is not safely additive across
/// three foreign type systems at once. More detail about a situation arrives
/// as a new, more specific variant instead.
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
    /// readable, while balance and activity are incomplete and still moving
    /// and every spend and receive is refused with
    /// [`Recovering`](crate::ErrorCode::Recovering).
    ///
    /// This is the second **open** state, so the federation is listed by
    /// [`Sdk::federations`] and returned by [`Sdk::federation`] exactly as a
    /// [`Running`](FederationStatus::Running) one is. That is deliberate and
    /// load-bearing: this is a federation the SDK is operating, its refusals
    /// are specific answers rather than the
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) of a stale
    /// handle, and a wallet still being reconstructed has to stay reachable
    /// through those two calls or an application that kept nothing across a
    /// restart could not find it at all.
    ///
    /// This state covers a rescan that is *running* and one that has
    /// *stopped without completing*, and it deliberately does not
    /// distinguish them, because the lock does not either: only a completed
    /// recovery releases it, and a stopped attempt holds it exactly as
    /// firmly as a running one. An application that needs the finer
    /// distinction — to offer "retry" rather than "still working" — reads it
    /// from the recovery API. What this status is for is the coarser and
    /// more important fact: this federation cannot move value yet.
    ///
    /// Not to be confused with
    /// [`Quarantined`](FederationStatus::Quarantined). A recovering
    /// federation is one the SDK is happily operating; a quarantined one is
    /// one it refuses to operate. The exits differ accordingly: a quarantine
    /// is cleared by [`Sdk::reopen_federation`] once whatever caused it has
    /// changed, whereas this state gives way to
    /// [`Running`](FederationStatus::Running) when the reconstruction
    /// completes. Closing or quarantining a recovering federation moves it
    /// out of this state without finishing that reconstruction, which is
    /// persisted, so reopening lands it back here; only
    /// [`Sdk::forget_federation`] ends an unfinished one, by erasing it.
    ///
    /// This state only arises when the SDK's recovery API — currently behind
    /// the off-by-default `experimental` feature — has been used; the
    /// variant is unconditional so that the set of statuses a binding must
    /// handle does not change with a cargo feature.
    Recovering,
    /// Stored and intact, but not running, because the SDK could not or
    /// would not open it.
    ///
    /// Nothing has been deleted: the configuration, client state, operation
    /// log, and activity history are all still there, and
    /// [`Sdk::reopen_federation`] retries. Quarantine means "meant to be
    /// running, currently cannot", so later builds retry it too;
    /// [`Sdk::close_federation`] is how an application gives up on it.
    ///
    /// This state also carries the answer to the question a caller would
    /// otherwise have to ask by failing a call.
    Quarantined {
        /// Why the federation is not running: the same stable
        /// [`ErrorCode`](crate::ErrorCode) the equivalent
        /// [`Error`](crate::Error) would carry, a human-readable message, and
        /// the same structured details envelope, so the modules that conflict
        /// in a mixed-generation federation are readable without parsing
        /// text.
        ///
        /// [`code`](crate::Diagnostic::code) is the part to branch on:
        /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation)
        /// for a configuration this SDK refuses (mixed module generations,
        /// most often),
        /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable)
        /// or [`Timeout`](crate::ErrorCode::Timeout) when no guardian
        /// answered in time, and [`Storage`](crate::ErrorCode::Storage) when
        /// the federation's local state could not be read.
        ///
        /// Reusing [`ErrorCode`](crate::ErrorCode) rather than inventing a
        /// parallel reason enum is deliberate: an application already has a
        /// switch over these codes for its error banners, the taxonomy is
        /// already stable and additive-only, and a second enum would drift
        /// from the first.
        ///
        /// [`message`](crate::Diagnostic::message) is human-readable detail —
        /// for a mixed-generation federation, the modules that conflict and
        /// the generations they declare. For humans only: logs, diagnostics,
        /// an expandable "details" row. Exactly like
        /// [`Error::message`](crate::Error::message), it is not part of the
        /// stability contract and must never be parsed or matched on.
        ///
        /// [`details`](crate::Diagnostic::details) is that same information as
        /// structured data, which is what makes a quarantine as
        /// machine-readable as the error it corresponds to: a mixed
        /// federation carries
        /// [`ErrorDetails::MixedModuleGenerations`](crate::ErrorDetails::MixedModuleGenerations),
        /// read with [`Diagnostic::detail`](crate::Diagnostic::detail), so the
        /// conflicting modules are a value rather than something to scrape out
        /// of a sentence.
        diagnostic: Diagnostic,
    },
    /// Stored and intact, and stopped because the application asked for
    /// that with [`Sdk::close_federation`].
    ///
    /// Distinct from [`Quarantined`](FederationStatus::Quarantined) in one
    /// way that matters: later builds do *not* reopen it, because someone
    /// chose this. [`Sdk::reopen_federation`] undoes the choice.
    Closed,
    /// An erase has been committed and is being carried out, or is waiting
    /// to be finished by a retry or by a later
    /// [`SdkBuilder::build`](crate::SdkBuilder::build).
    ///
    /// The federation will not be opened again and cannot be resurrected;
    /// see [`Sdk::forget_federation`]. An application seeing this should
    /// render "removing…" rather than a wallet.
    ///
    /// Concretely, and unchanged by how long the erase takes: the id is
    /// listed by [`Sdk::stored_federations`] and answered here by
    /// [`Sdk::federation_status`], absent from [`Sdk::federations`] and from
    /// [`Sdk::federation`], and refused by [`Sdk::reopen_federation`] with
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput). Its balance,
    /// operation log and activity history are gone as far as this API is
    /// concerned from the moment the tombstone lands, whether or not the
    /// bytes have finished going away. Joining the same federation again is
    /// allowed and produces a new federation with no local history, after the
    /// committed erase is finished; see [`Sdk::join`].
    ///
    /// It is therefore *not* a variant of quarantine, and does not carry a
    /// code or message. [`Quarantined`](FederationStatus::Quarantined) means
    /// "intact, meant to run, currently cannot", and its reason is
    /// actionable; this means "already gone, still being deleted", where the
    /// only action is to wait for a retry. An erase whose completion keeps
    /// failing stays here and is retried by each later build rather than
    /// failing the build — the reason for the stall reaches an application as
    /// the [`Error`](crate::Error) from
    /// [`Sdk::forget_federation`] and in logs, not as part of this state.
    Forgetting,
    /// The erase completed: this federation is gone.
    ///
    /// This is a *notification*, not a stored state. It is delivered once by
    /// [`FederationStatusUpdates::next`] so a list screen can drop the row,
    /// and it is the last thing that subscriber will ever say about this id.
    /// [`Sdk::federation_status`] returns `None` for a forgotten federation,
    /// because there is nothing left for it to describe.
    Forgotten,
}

/// A stored federation, described without a live handle.
///
/// [`Sdk::stored_federations`] returns these and
/// [`FederationStatusUpdates::next`] yields them. It exists because most of
/// what an application needs in order to *list* federations is needed
/// exactly when there is no [`Federation`] to ask: a closed federation, a
/// quarantined one, one whose erase is finishing. Making
/// [`Sdk::federations`] polymorphic over live and non-live federations would
/// have changed what a `Federation` means — every handle in that list can be
/// acted on — so the listing gets its own small record instead.
///
/// That invariant is about the handle, not about what every call through it
/// will agree to do. A [`Recovering`](FederationStatus::Recovering)
/// federation is in that list and can be acted on: a live client answers, and
/// the sends and receives it declines are declined with
/// [`Recovering`](crate::ErrorCode::Recovering) by that same live client. A
/// record here, by contrast, has no client behind it at all, which is exactly
/// why it is a record.
///
/// Owned, flat, and free of tuples and borrows, so it crosses into Swift,
/// Kotlin and TypeScript as a plain record. `#[non_exhaustive]`: fields may
/// be added, so construct it only through the SDK and match it with `..` or
/// by field access.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FederationInfo {
    /// The federation's id — the key for [`Sdk::federation`],
    /// [`Sdk::federation_status`] and [`Sdk::reopen_federation`], and what
    /// identifies the row to replace when this record arrives from
    /// [`FederationStatusUpdates::next`].
    pub id: FederationId,
    /// The federation's human-readable name, when its configuration
    /// declares one.
    ///
    /// From the last configuration that validated, exactly as
    /// [`Federation::name`](crate::Federation::name) reports it — so a
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
    /// storage holds — one [`FederationInfo`] each, in unspecified order —
    /// so a subscriber can be the only thing a list screen reads, without a
    /// separate priming call to [`Sdk::stored_federations`] that could race
    /// with the first change. After that, each call resolves when some
    /// federation's status changes.
    ///
    /// # Why this is not `Option`-shaped
    ///
    /// Like [`BalanceUpdates::next`](crate::BalanceUpdates::next) and unlike
    /// [`OperationUpdates::next`](crate::OperationUpdates::next), there is
    /// no final value: an instance's set of federations can always change
    /// again, so `Ok(None)` could not arise and would be a permanently-`Some`
    /// wrapper for every caller to unwrap. The one way this stream ends is
    /// the instance shutting down, and that is a condition callers must
    /// notice, so it surfaces as
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    ///
    /// A federation being forgotten is not the end of the stream either. It
    /// arrives as an ordinary update carrying
    /// [`Forgotten`](FederationStatus::Forgotten), which is the last update
    /// for that id and tells a list screen to drop the row; the
    /// subscription itself stays open for every other federation.
    ///
    /// # Errors
    ///
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) once the
    /// SDK has been shut down — the terminal condition for this stream, and
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

    #[test]
    fn builder_debug_redacts_the_mnemonic() {
        // The builder must be printable without the phrase escaping into a
        // log line; only whether one is present may show.
        let builder = Sdk::builder();
        let rendered = format!("{builder:?}");
        assert!(rendered.contains("mnemonic"));
        assert!(rendered.contains("None"));
    }

    #[test]
    fn a_quarantine_carries_the_code_to_branch_on() {
        // The point of the status is that an application learns *why* a
        // federation is unavailable without provoking an error, so the code
        // has to be readable straight off the value — and, since the
        // diagnosis carries the same envelope an `Error` would, so do the
        // modules that conflict.
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
        // whose erase is committed but unfinished, so the listing record has
        // to be able to carry it — and it must stay distinguishable from the
        // "stored and intact" states an application offers a reconnect for,
        // and from the `Forgotten` notification that drops the row.
        let info = FederationInfo {
            id: FederationId::from_raw("fed-id".to_owned()),
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
        // has no handle at all — that is the case it exists for.
        let info = FederationInfo {
            id: FederationId::from_raw("fed-id".to_owned()),
            name: Some("Test Federation".to_owned()),
            network: Network::Regtest,
            status: FederationStatus::Closed,
        };
        assert_eq!(info.status, FederationStatus::Closed);
        assert_eq!(info.clone(), info);
    }
}
