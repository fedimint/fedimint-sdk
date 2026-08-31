//! The SDK root: one storage, one seed, many federations.

use std::sync::Arc;

use crate::{Federation, FederationId, FederationPreview, InviteCode, Mnemonic, Result, Storage};

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
/// # Lifecycle
///
/// An instance is opened by [`SdkBuilder::build`] and closed by
/// [`Sdk::shutdown`]. Between those points, federations may be joined
/// ([`Sdk::join`]), stopped while keeping their data
/// ([`Sdk::close_federation`]), or erased ([`Sdk::forget_federation`]).
/// Any [`Federation`] handle for a federation that has been closed — and
/// every handle at all after [`Sdk::shutdown`] — fails its calls with
/// [`ErrorCode::FederationClosed`](crate::ErrorCode::FederationClosed)
/// rather than panicking or silently doing nothing.
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
    /// at preview rather than previewed and then refused at join.
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
    /// starts its background workers. The federation is reopened
    /// automatically by every subsequent [`SdkBuilder::build`] against the
    /// same storage until it is closed or forgotten.
    ///
    /// # The federation-wide module-generation rule
    ///
    /// Every module of a federation must be of the same generation — all
    /// v1, or all v2. There is no per-module override and no way for a
    /// caller to opt out: a mixed federation is rejected with
    /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation),
    /// with diagnostics in the error message naming the modules that
    /// conflict and the generations they declare. The rule is checked at
    /// [`Sdk::preview`], here at join, when an existing federation is
    /// reopened, and again whenever its configuration changes while the
    /// instance is running. It covers *every* module the federation runs,
    /// not only the ones this SDK exposes as facades: a module the SDK
    /// never touches still participates in the check, because a federation
    /// running a mixed set is not a configuration this SDK is willing to
    /// hold funds in.
    ///
    /// # Errors
    ///
    /// [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined) when this
    /// instance already holds the federation, plus every error
    /// [`Sdk::preview`] can produce, and
    /// [`Storage`](crate::ErrorCode::Storage) if the join cannot be
    /// persisted.
    pub async fn join(&self, invite: &InviteCode) -> Result<Federation> {
        unimplemented!()
    }

    /// Every federation this instance currently has open.
    ///
    /// Federations that were closed with [`Sdk::close_federation`] are not
    /// listed even though their data is retained, and forgotten ones are
    /// gone entirely. The order is unspecified.
    pub fn federations(&self) -> Vec<Federation> {
        unimplemented!()
    }

    /// The open federation with this id, or `None` if this instance has no
    /// such federation open.
    ///
    /// `None` covers both "never joined" and "joined but currently
    /// closed"; the two are not distinguished here, because in both cases
    /// there is nothing an application can do with the federation until it
    /// joins it again.
    pub fn federation(&self, id: &FederationId) -> Option<Federation> {
        unimplemented!()
    }

    /// Stops running this federation while keeping all of its data.
    ///
    /// The federation's background workers stop, it is dropped from
    /// [`Sdk::federations`], and it is no longer reopened automatically by
    /// later builds against this storage. Nothing is deleted: the client
    /// state, the operation log, and the activity history all remain, and
    /// joining the federation again restores access to them rather than
    /// starting over. This is the non-destructive half of leaving a
    /// federation; [`Sdk::forget_federation`] is the destructive half.
    ///
    /// Any [`Federation`] handle an application still holds for this
    /// federation keeps existing but stops working: its calls fail with
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed), as do
    /// pending [`BalanceUpdates::next`](crate::BalanceUpdates::next) and
    /// [`OperationUpdates::next`](crate::OperationUpdates::next) calls
    /// against it.
    ///
    /// Closing is idempotent: an id that names no open federation is not an
    /// error, because the postcondition — the federation is not running and
    /// its data is intact — already holds.
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
    /// client state, operation log, and activity history for the federation
    /// are erased. Only the seed survives, so re-joining the federation
    /// later recovers whatever the federation itself can reconstruct, not
    /// what was only ever recorded locally.
    ///
    /// Because of that, the call is guarded rather than forceful. It
    /// refuses unless all of the following hold, and each refusal leaves
    /// the federation exactly as it was:
    ///
    /// - **Zero spendable balance.** Any remaining spendable ecash fails
    ///   the call with
    ///   [`BalanceNotEmpty`](crate::ErrorCode::BalanceNotEmpty).
    /// - **No non-final operations**, **no reclaimable outgoing value**
    ///   (out-of-band ecash a receiver has not redeemed and this instance
    ///   could still reclaim), and **no recovery in progress**. Any of
    ///   these fails the call with
    ///   [`PendingOperations`](crate::ErrorCode::PendingOperations).
    ///
    /// The reclaimable-value condition is the non-obvious one: notes handed
    /// out but not yet redeemed are still worth money to the sender until
    /// their reclaim window closes, and the record needed to reclaim them
    /// lives in exactly the state this call would delete.
    ///
    /// Outstanding handles behave as they do after
    /// [`Sdk::close_federation`]: they fail with
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed). Forgetting
    /// is idempotent — an id with no local state is not an error.
    ///
    /// # Errors
    ///
    /// [`BalanceNotEmpty`](crate::ErrorCode::BalanceNotEmpty),
    /// [`PendingOperations`](crate::ErrorCode::PendingOperations),
    /// [`Storage`](crate::ErrorCode::Storage) if the deletion fails partway,
    /// and [`FederationClosed`](crate::ErrorCode::FederationClosed) after
    /// shutdown.
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
    pub fn export_mnemonic(&self) -> Mnemonic {
        unimplemented!()
    }

    /// Flushes everything to storage, stops all background work, and
    /// releases the storage lock.
    ///
    /// After this returns, the instance is finished: every [`Sdk`] and
    /// [`Federation`] handle, and every subscriber obtained from one,
    /// fails with
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed). Another
    /// instance may then open the same storage. Shutdown is idempotent —
    /// calling it twice is not an error.
    ///
    /// **On mobile this call is required before the process can die.**
    /// Both iOS and Android terminate backgrounded applications without
    /// warning and without unwinding; an instance that has not been shut
    /// down may have durable writes still buffered, and — on native
    /// storage — leaves its lock to be recovered on the next open. Call
    /// this from the platform's "entering background" or "about to
    /// terminate" callback and await it before returning.
    ///
    /// Shutting down does not cancel operations in the sense of undoing
    /// them: an operation that was running is persisted mid-flight and
    /// resumes when the storage is opened again, exactly as it would after
    /// a crash.
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
    /// guessing a location. Use [`Storage::at`] for a real location or
    /// [`Storage::in_memory`] for a throwaway one.
    pub fn storage(mut self, storage: Storage) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Sets the BIP-39 seed the instance derives every federation secret
    /// from.
    ///
    /// Supply one to restore an existing wallet from a written-down phrase.
    /// Omit it and the instance uses the seed already in storage, or — if
    /// the storage is empty — generates a fresh one and persists it before
    /// deriving anything from it.
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

    /// Opens the storage, loads or establishes the seed, reopens every
    /// federation the storage remembers, and resumes their pending
    /// operations.
    ///
    /// The order of those steps is part of the contract, because it is what
    /// makes the failure modes safe:
    ///
    /// 1. **Take the storage lock.** If the location is already open, in
    ///    this process or another, the call fails with
    ///    [`StorageInUse`](crate::ErrorCode::StorageInUse) and nothing has
    ///    been touched. (Sharing one location between processes is not part
    ///    of the 0.1 contract; see [`Storage`] for the future shape of
    ///    that.)
    /// 2. **Reconcile the seed, before any mutation.** If the storage holds
    ///    a seed and a different mnemonic was supplied, the call fails with
    ///    [`SeedMismatch`](crate::ErrorCode::SeedMismatch) and the storage
    ///    is left byte-for-byte as it was found. If the storage holds no
    ///    seed, the supplied or freshly generated mnemonic is written
    ///    durably *now* — before any federation-derived state exists — so
    ///    there is no crash window that could leave state derived from a
    ///    seed that was never saved.
    /// 3. **Reopen the federations.** Each federation the storage remembers
    ///    is revalidated (including the module-generation rule described on
    ///    [`Sdk::join`]) and started, and its unfinished operations resume
    ///    from where they were persisted. A federation that cannot be
    ///    opened is *reported*: the call fails, naming the federation and
    ///    the reason. It is never silently omitted from
    ///    [`Sdk::federations`], because an application cannot distinguish a
    ///    missing federation from one the user left, and would show a
    ///    balance that has quietly lost a wallet. (The exact reporting
    ///    shape — failing the build, as here, versus surfacing per-
    ///    federation diagnostics on a successfully built instance — is the
    ///    one part of this contract still expected to be refined.)
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) if no storage was
    /// set, [`StorageInUse`](crate::ErrorCode::StorageInUse),
    /// [`SeedMismatch`](crate::ErrorCode::SeedMismatch),
    /// [`Storage`](crate::ErrorCode::Storage) for a backend failure,
    /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation)
    /// for a remembered federation that no longer validates, and
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable)
    /// for one that cannot be contacted at all.
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

/// Placeholder for the shared instance state. Handles hold this behind an
/// `Arc` so cloning an [`Sdk`] shares one set of federations, one storage,
/// and one pool of background work.
#[derive(Debug)]
struct SdkInner;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_debug_redacts_the_mnemonic() {
        // The builder must be printable without the phrase escaping into a
        // log line; only whether one is present may show.
        let builder = Sdk::builder();
        let rendered = format!("{builder:?}");
        assert!(rendered.contains("mnemonic"));
        assert!(rendered.contains("None"));
    }
}
