//! Where an SDK instance keeps the state it must not lose.

/// The persistent home of one SDK instance.
///
/// A `Storage` value names a place to persist everything an
/// [`Sdk`](crate::Sdk) owns: the BIP-39 seed it derives every federation
/// secret from, the configuration and client state of each joined
/// federation, the state machines of in-flight operations, and the local
/// activity history. Exactly one `Storage` backs one [`Sdk`](crate::Sdk);
/// federations are namespaced within it rather than each getting their own
/// location.
///
/// # The backend is not API
///
/// Which storage engine sits behind this type is selected per target — a
/// filesystem-backed embedded key/value store on native targets, an
/// OPFS-backed store in the browser — and is deliberately not visible here.
/// Nothing about the on-disk format, the engine, or its tuning is part of
/// the crate's stability contract, so it can be replaced without a breaking
/// API change. Applications choose *where* to persist, never *how*.
///
/// # Choosing a constructor
///
/// - [`Storage::at`] — persistent, native targets. Takes a directory path.
/// - [`Storage::in_browser`] — persistent, wasm targets. Takes a store name
///   within the page's origin.
/// - [`Storage::in_memory`] — ephemeral, every target. Takes nothing.
///
/// The two persistent constructors are separate rather than one call that
/// interprets its argument per target, and each exists only on the target it
/// serves. That is not ceremony; it is the only honest shape. A browser has
/// no filesystem and no path namespace, so a path handed to a browser build
/// could only be mangled into something else — and a native process has no
/// origin, so an origin-scoped store name means nothing to it. Overloading
/// one constructor would produce a signature that silently means a different
/// thing on each target and is wrong on whichever one the caller was not
/// thinking about. Two constructors, each present only where it works, means
/// a wasm binding cannot reach for a path-based API that could never have
/// functioned, and a mistake is a compile error instead of a runtime
/// surprise.
///
/// Everything downstream of construction — the seed rules, the single-opener
/// rule, the durability guarantees — is identical on both, and is stated
/// once below rather than per constructor.
///
/// # Why `&str` and not `&Path`
///
/// The persistent constructors take their location as a `&str` rather than a
/// `&std::path::Path`. This is deliberate: `Path` is an OS-string type with
/// platform-specific encoding rules and has no natural representation in
/// Swift, Kotlin, or JavaScript, so every binding would have to invent a
/// conversion. A string is what those hosts already hand out (an
/// application-support directory, a documents directory, a name chosen for a
/// browser store), and keeping the SDK's own signature a string means one
/// validating parse on the Rust side instead of one per language.
///
/// # Seed and storage lifecycle
///
/// The rules below are enforced by [`SdkBuilder::build`](crate::SdkBuilder::build),
/// which is where a `Storage` is actually opened; they are stated here
/// because they are properties of the storage, not of the builder call. That
/// method documents the exact order the checks run in.
///
/// - **A seed is established only over a backend proven to hold nothing
///   else.** "No seed found" is *not* the condition for writing one. The
///   condition is that the backend holds no state of this SDK's at all — no
///   seed, no federation record, no client state, no operation log, no
///   activity history. Only then is a supplied or freshly generated mnemonic
///   written, and it is written durably *before* any federation-derived
///   state exists, so there is no window in which federation state has been
///   written but the seed that produced it has not. Generating a seed is
///   fallible — it needs the platform's secure random source — and a failure
///   to draw entropy fails the open with
///   [`ErrorCode::Entropy`](crate::ErrorCode::Entropy), leaving the storage
///   untouched.
/// - **Storage that holds state but no usable seed is orphaned, and is
///   refused.** If a federation record or client state is present while the
///   seed entry is missing, truncated, corrupt, or written in a format this
///   build cannot read, the open fails with
///   [`ErrorCode::Storage`](crate::ErrorCode::Storage) and *nothing is
///   written*. Establishing a fresh seed there would bind existing state to
///   a derivation root it did not come from — the wallet would open, appear
///   empty, and the real funds would be unreachable — while overwriting the
///   only local trace of which seed that state belonged to. A refusal is
///   recoverable by pointing at the right location or restoring the right
///   phrase; a wrong write is not recoverable at all.
/// - **A different seed is a refusal, not a migration.** Opening storage
///   that already holds a usable seed while supplying a different mnemonic
///   fails with
///   [`ErrorCode::SeedMismatch`](crate::ErrorCode::SeedMismatch), and it
///   fails *before* any mutation: the existing storage is left exactly as it
///   was found.
/// - **A federation that cannot be opened is reported, not hidden — and does
///   not sink the instance.** Reopening an instance reopens every federation
///   it had joined; one that fails to open is quarantined and surfaced with
///   a reason through
///   [`Sdk::stored_federations`](crate::Sdk::stored_federations) and
///   [`Sdk::federation_status`](crate::Sdk::federation_status), rather than
///   being quietly omitted from
///   [`Sdk::federations`](crate::Sdk::federations) or taken as grounds to
///   fail the whole open. An application can therefore never conclude from a
///   short list that a federation was left, and a single broken federation
///   never denies access to the healthy ones or to
///   [`Sdk::export_mnemonic`](crate::Sdk::export_mnemonic).
///
/// # One opener at a time
///
/// Opening a location that is already open — by another [`Sdk`](crate::Sdk)
/// in this process, by another process, by another browser tab or a worker —
/// fails with
/// [`ErrorCode::StorageInUse`](crate::ErrorCode::StorageInUse). Two writers
/// over one set of client state would corrupt state machines and could
/// double-spend notes, so the lock is not advisory and there is no override.
/// It is taken when [`SdkBuilder::build`](crate::SdkBuilder::build) opens the
/// storage, not when a `Storage` value is constructed, and it is released by
/// [`Sdk::shutdown`](crate::Sdk::shutdown) or when the last handle to the
/// instance is dropped.
///
/// **A lock left behind by a process that died is reclaimed, not fatal.**
/// Neither a killed mobile app nor a closed browser tab reliably gets to
/// release anything, so a lock that outlives its owner must be recoverable:
/// the next opener on that device takes it over. `StorageInUse` therefore
/// means genuinely concurrent use, never a stale marker. A design where one
/// crash could make a wallet permanently unopenable would turn an
/// inconvenience into fund loss.
///
/// The corollary is that this rule protects against *concurrency*, not
/// against a second copy of the data. Pointing two SDK instances at one
/// location is refused; copying a location's contents elsewhere and opening
/// both is outside what the SDK can detect, and is the same mistake as
/// restoring one wallet's backup onto two devices.
///
/// # Durability
///
/// Everything a caller can observe is durably committed before it becomes
/// observable, so an abrupt process death loses nothing that was
/// acknowledged, and [`Sdk::shutdown`](crate::Sdk::shutdown) is an
/// optimisation rather than a correctness requirement. Both halves of that —
/// what survives a kill, and what a clean shutdown adds — are stated on
/// [`Sdk`](crate::Sdk) and [`Sdk::shutdown`](crate::Sdk::shutdown), because
/// they are about what SDK calls promise rather than about where the bytes
/// go.
///
/// What belongs here is the caveat the backends do not share: durable means
/// durable *as far as the platform allows*. A native location lives until
/// something deletes it. A browser store does not — see
/// [`Storage::in_browser`].
///
/// # Recognised future additions behind this type
///
/// Two capabilities are consciously out of the 0.1 contract and recorded
/// here because both would land behind this same type, additively, without
/// changing the shape of the API above:
///
/// - **Cross-process lock delegation.** Today a second opener of the same
///   native location is simply refused with
///   [`ErrorCode::StorageInUse`](crate::ErrorCode::StorageInUse). A future
///   version could instead let the second opener delegate its reads and
///   writes to the process already holding the lock (the pattern a mobile
///   app with a notification-service extension needs, and the same pattern a
///   browser page wanting to share a store with a worker needs). That is a
///   new constructor or option on `Storage`, not a change to the ones here.
/// - **Encryption at rest and host secure storage.** The persisted seed is
///   stored as the backend stores anything else. Encrypting it, or handing
///   custody of it to a platform keychain or keystore and holding only a
///   reference here, is a design point for a later release and would also
///   attach to this type. Note the complementary rule documented on
///   [`Mnemonic`](crate::Mnemonic): protecting a copy the application has
///   already *exported* is the application's responsibility, whereas the
///   at-rest copy inside `Storage` is the SDK's.
#[derive(Debug)]
pub struct Storage {
    inner: StorageInner,
}

impl Storage {
    /// Persistent storage rooted at `path`. **Native targets only** — a
    /// wasm build has [`Storage::in_browser`] instead.
    ///
    /// `path` names a directory the SDK owns outright: it creates it if it
    /// does not exist and treats everything inside it as its own. Do not
    /// point two SDK instances at the same directory, and do not put other
    /// application files in it.
    ///
    /// This call validates and prepares the location; it does not yet take
    /// the single-opener lock described on the type, which is acquired when
    /// [`SdkBuilder::build`](crate::SdkBuilder::build) opens the storage.
    /// A location already in use therefore fails at `build` with
    /// [`ErrorCode::StorageInUse`](crate::ErrorCode::StorageInUse), not
    /// here.
    ///
    /// # Errors
    ///
    /// Fails with [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput)
    /// if `path` is not a usable location string (empty, or not a path this
    /// target can express), and with
    /// [`ErrorCode::Storage`](crate::ErrorCode::Storage) if the directory
    /// cannot be created or is not readable and writable.
    // `doc` keeps both persistent constructors visible in one rendering of the
    // docs, so a reviewer or a binding author reads the whole surface without
    // having to build the crate twice.
    #[cfg(any(not(target_family = "wasm"), doc))]
    pub fn at(path: &str) -> crate::Result<Storage> {
        unimplemented!()
    }

    /// Persistent storage in the browser, in the store named `name`.
    /// **Wasm targets only** — a native build has [`Storage::at`] instead.
    ///
    /// This is the browser counterpart of [`Storage::at`], and it is what
    /// makes a durable wasm binding possible at all: a page has no
    /// filesystem path to hand to `at`, so without a constructor of its own
    /// the only storage reachable from JavaScript would be
    /// [`Storage::in_memory`], which loses the seed and every joined
    /// federation on reload.
    ///
    /// # The namespace
    ///
    /// `name` is a namespace, not a path. It selects a subtree of the
    /// browser's origin-private file system that the SDK owns outright,
    /// exactly as `at` owns a directory: the SDK creates it on first use and
    /// treats everything inside it as its own.
    ///
    /// The naming is deliberately narrow. `name` must be non-empty, short,
    /// and made only of characters that cannot be read as structure —
    /// letters, digits, `-`, `_`, `.`, with no path separators, no `..`, and
    /// no leading or trailing separator-like characters. Anything else is
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput). A name is
    /// not a place to encode a hierarchy, and accepting one that looked like
    /// a path would invite exactly the traversal-shaped bugs the browser
    /// sandbox is there to prevent.
    ///
    /// Scoping follows the browser's own boundary, which the SDK cannot
    /// widen or narrow: the store belongs to the page's **origin**. The same
    /// origin plus the same `name` is the same storage — that is how an
    /// application finds its wallet again after a reload — and two different
    /// origins never share a store even with identical names. Pass more than
    /// one name only if an application genuinely wants more than one
    /// independent wallet in one origin, each with its own seed.
    ///
    /// # One opener, in a browser
    ///
    /// The single-opener rule on this type applies here unchanged and is the
    /// part most likely to be met in practice, because a browser makes it so
    /// easy to run a second copy of a page. The lock covers one origin and
    /// one `name`, and it is held across every context that origin can run:
    /// tabs, iframes, dedicated and shared workers, service workers.
    ///
    /// A second opener — the user duplicating the tab, a deep link opening a
    /// new one, a worker built alongside the page — gets
    /// [`ErrorCode::StorageInUse`](crate::ErrorCode::StorageInUse) from
    /// [`SdkBuilder::build`](crate::SdkBuilder::build). It does not get a
    /// second independent store, it does not get read-only access, and it
    /// does not get to write concurrently: the whole point of the rule is
    /// that two writers over one wallet's state machines can corrupt them
    /// and double-spend notes. A tab that was killed without releasing its
    /// lock does not poison the store — a stale lock is reclaimed by the
    /// next opener, as documented on the type.
    ///
    /// The pattern that follows from this is to build the SDK in exactly one
    /// place per origin — most naturally a shared worker — and have other
    /// contexts talk to it, rather than each constructing its own instance
    /// and racing for the lock. An application that will not do that should
    /// treat `StorageInUse` as a first-class state and tell the user which
    /// window is holding the wallet, not retry in a loop.
    ///
    /// # Durability, as far as a browser offers it
    ///
    /// A browser store is durable in the sense that matters here — writes
    /// survive reload, navigation, and a killed tab, and the durability
    /// guarantees on [`Sdk`](crate::Sdk) hold — but it is not durable in the
    /// sense a native directory is. The user or the browser can discard it:
    /// clearing site data removes it, and storage pressure can evict it
    /// unless the origin has been granted persistence (which only the host
    /// page can request, and which the user can refuse). The SDK cannot
    /// change any of that.
    ///
    /// This is worth surfacing to users rather than hiding, and it is the
    /// strongest argument for putting
    /// [`Sdk::export_mnemonic`](crate::Sdk::export_mnemonic) in front of a
    /// web user early: on this platform the written-down seed phrase is not
    /// a precaution against losing a device, it is the backup against a
    /// routine "clear browsing data".
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// `name` that is empty, too long, or not restricted to the characters
    /// above, and
    /// [`ErrorCode::Storage`](crate::ErrorCode::Storage) if the origin has no
    /// usable file system at all — a context that does not provide one, or
    /// one where storage access is denied — since there is nothing to
    /// prepare in that case. As with [`Storage::at`], the single-opener lock
    /// is taken later, so
    /// [`ErrorCode::StorageInUse`](crate::ErrorCode::StorageInUse) comes from
    /// [`SdkBuilder::build`](crate::SdkBuilder::build) rather than from here.
    #[cfg(any(target_family = "wasm", doc))]
    pub fn in_browser(name: &str) -> crate::Result<Storage> {
        unimplemented!()
    }

    /// Ephemeral storage held entirely in memory.
    ///
    /// Everything written to it is discarded when the last handle to the
    /// SDK instance built on it is dropped, which makes it the right choice
    /// for tests and for throwaway instances used only to
    /// [preview](crate::Sdk::preview) a federation before deciding whether
    /// to join it. Each call produces an independent store, so in-memory
    /// instances never contend for the single-opener lock with each other.
    ///
    /// Because nothing survives, an SDK instance built on in-memory storage
    /// always starts from a backend that is trivially empty: a supplied
    /// mnemonic is accepted as-is and an omitted one is generated fresh, and
    /// neither
    /// [`ErrorCode::SeedMismatch`](crate::ErrorCode::SeedMismatch) nor the
    /// orphaned-storage refusal described on this type can occur.
    pub fn in_memory() -> Storage {
        unimplemented!()
    }
}

/// Placeholder for the target-selected backend handle. Replaced by the real
/// backend when the implementation lands; kept private so the choice of
/// backend never leaks into the public API.
#[derive(Debug)]
struct StorageInner;
