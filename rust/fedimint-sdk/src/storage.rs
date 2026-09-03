//! Where an SDK instance keeps the state it must not lose.

/// The persistent home of one SDK instance.
///
/// A `Storage` value names a place to persist everything an [`Sdk`](crate::Sdk) owns: the
/// seed phrase every federation secret is derived from, each joined federation's
/// configuration and client state, in-flight operations, and local activity history. Exactly
/// one `Storage` backs one [`Sdk`](crate::Sdk); federations are namespaced within it rather
/// than each getting their own location.
///
/// The concrete storage engine differs per target and is not part of the API: nothing about
/// the on-disk format is guaranteed, and it can change between releases without being a
/// breaking API change. Applications choose *where* to persist, never *how*.
///
/// # Choosing a constructor
///
/// - [`Storage::at`], persistent, native targets: takes a filesystem path.
/// - [`Storage::in_browser`], persistent, wasm targets: takes an origin-scoped namespace, not
///   a path.
/// - [`Storage::in_memory`], ephemeral, every target: takes nothing.
///
/// Each constructor exists only on the target it serves, so a wasm binding cannot reach for a
/// path-based API that could never work there, and a native build cannot reach for a
/// browser-only namespace.
///
/// # Construction describes a place; `build` opens it
///
/// Both persistent constructors only name a location and validate that name locally: no
/// directory or origin-private store is created, nothing is read or written, and no lock is
/// taken. [`SdkBuilder::build`](crate::SdkBuilder::build) is what actually opens the location,
/// creates it if needed, reads or establishes the seed, and reopens the instance's
/// federations; see that method for the exact order and the errors each step can produce.
///
/// # Seed and storage lifecycle
///
/// - A seed is written only when the backend holds no state of this SDK's at all: no seed, no
///   federation record, no client state, no operation log, no activity history. It is written
///   durably before any federation-derived state exists. A failure to generate one fails the
///   open with [`ErrorCode::Entropy`](crate::ErrorCode::Entropy), leaving the storage
///   untouched.
/// - Storage that holds other state but no readable seed is refused rather than silently
///   given a fresh one:
///   [`ErrorCode::StorageOrphaned`](crate::ErrorCode::StorageOrphaned), with
///   [`ErrorDetails::StorageOrphaned`](crate::ErrorDetails::StorageOrphaned) naming the
///   location, and nothing is written. Writing a fresh seed there would bind existing state to
///   a derivation root it did not come from: the wallet would open, appear empty, and the
///   real funds would be unreachable.
/// - Opening storage that already holds a usable seed with a different mnemonic is refused
///   with [`ErrorCode::SeedMismatch`](crate::ErrorCode::SeedMismatch), before any mutation.
/// - A federation that fails to reopen is quarantined and reported through
///   [`Sdk::stored_federations`](crate::Sdk::stored_federations) and
///   [`Sdk::federation_status`](crate::Sdk::federation_status) rather than hidden or treated
///   as fatal to the whole open: a short list from
///   [`Sdk::federations`](crate::Sdk::federations) never means a federation was silently
///   dropped, and one broken federation never blocks access to the healthy ones or to
///   [`Sdk::export_mnemonic`](crate::Sdk::export_mnemonic).
///
/// # One opener at a time
///
/// A location can be open in only one place at a time. Opening a location that is already
/// open, by another [`Sdk`](crate::Sdk) in this process, by another process, or by another
/// browser tab or worker, fails with
/// [`ErrorCode::StorageInUse`](crate::ErrorCode::StorageInUse), with no override: two writers
/// over one wallet's state could corrupt it and double-spend notes. The lock
/// is taken when [`SdkBuilder::build`](crate::SdkBuilder::build) opens the storage and
/// released by [`Sdk::shutdown`](crate::Sdk::shutdown) or when the last handle to the instance
/// is dropped.
///
/// A lock left behind by a process that died is reclaimed by the next opener rather than left
/// stuck: `StorageInUse` always means genuinely concurrent use, never a stale marker. This
/// protects against concurrent use of one location, not against a second copy of the data:
/// copying a location's contents elsewhere and opening both is the same mistake as restoring
/// one wallet's backup onto two devices, and the SDK cannot detect it.
///
/// # Durability
///
/// Everything a caller can observe is durably committed before it becomes observable, so an
/// abrupt process death loses nothing that was acknowledged; see [`Sdk`](crate::Sdk) and
/// [`Sdk::shutdown`](crate::Sdk::shutdown) for what that promises and what a clean shutdown
/// adds. "Durable" means durable as far as the platform allows: a native location lives until
/// something deletes it, while a browser store can be discarded by the user or by the browser
/// under storage pressure, see [`Storage::in_browser`].
///
/// # Current limitations
///
/// The persisted seed is not encrypted at rest; it is stored the way the backend stores
/// everything else. Protecting a copy the application has already exported is the
/// application's own responsibility, see [`Mnemonic`](crate::Mnemonic).
// Implementation notes (delete once implemented):
// - Backend: an embedded key-value store on native targets, OPFS-backed on wasm. Nothing
//   about the format is exposed.
// - `build` order: check for an existing seed record, detect orphaned state (other records
//   present, seed missing/corrupt) before writing anything, compare a supplied mnemonic
//   against a found one, then reopen each stored federation and quarantine failures
//   individually rather than failing the whole open.
// - The single-opener lock is filesystem/OPFS-level, not advisory, and must be reclaimable
//   from a process that died holding it (killed app, closed tab) without corrupting state.
// - Future, additive extensions that would land behind this same type without a signature
//   change: cross-process lock delegation (a second opener forwards reads/writes to the
//   process holding the lock, for a notification-service extension or a shared worker), and
//   encrypting the at-rest seed or handing its custody to a platform keychain or keystore.
#[derive(Debug)]
pub struct Storage {
    inner: StorageInner,
}

impl Storage {
    /// Names persistent storage rooted at the filesystem path `path`. Native targets only,
    /// use [`Storage::in_browser`] on wasm.
    ///
    /// `path` names a directory the SDK owns outright: it is created if it does not already
    /// exist, and everything inside it belongs to the SDK. Do not point two SDK instances at
    /// the same directory, and do not put other application files inside it.
    ///
    /// This only validates `path` as a location string and records it; nothing is created,
    /// read, written, or locked until [`SdkBuilder::build`](crate::SdkBuilder::build) opens
    /// the storage.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a `path` that is empty
    /// or cannot be expressed as a path on this target.
    ///
    /// Everything that depends on the file system itself is reported by
    /// [`SdkBuilder::build`](crate::SdkBuilder::build) instead: a directory that cannot be
    /// created or is not readable and writable, as
    /// [`ErrorCode::Storage`](crate::ErrorCode::Storage), and a location already open, as
    /// [`ErrorCode::StorageInUse`](crate::ErrorCode::StorageInUse).
    // `doc` keeps both persistent constructors visible in one rendering of the
    // docs, so the whole surface is readable without building the crate twice.
    #[cfg(any(not(target_family = "wasm"), doc))]
    pub fn at(path: &str) -> crate::Result<Storage> {
        // Implementation notes (delete once implemented):
        // - `&str` rather than `std::path::Path`: `Path` has no natural representation in
        //   Swift, Kotlin or JavaScript, so every binding would need its own conversion.
        unimplemented!()
    }

    /// Names persistent browser storage in the origin-scoped namespace `name`. Wasm targets
    /// only, use [`Storage::at`] on native.
    ///
    /// `name` is a namespace, not a path: it has no hierarchy, no parent, and nothing is
    /// resolved relative to it. It selects a subtree of the browser's origin-private storage
    /// that the SDK owns outright, created on first use. `name` must be non-empty, short, and
    /// made only of letters, digits, `-`, `_` and `.`, with no path separators or `..`;
    /// anything else is rejected.
    ///
    /// Storage is scoped to the page's origin: the same origin plus the same `name` is the
    /// same storage, which is how an application finds its wallet again after a reload, and
    /// two different origins never share a store even with identical names. Use more than one
    /// `name` only to keep more than one independent wallet in the same origin.
    ///
    /// # One opener, in a browser
    ///
    /// The single-opener rule described on [`Storage`] applies unchanged, and covers every
    /// context the origin can run in: tabs, iframes, dedicated and shared workers, service
    /// workers. A second opener, a duplicated tab, a second deep link, a worker built
    /// alongside the page, gets
    /// [`ErrorCode::StorageInUse`](crate::ErrorCode::StorageInUse) rather than a second store
    /// or read-only access. Building the SDK in exactly one place per origin, most naturally a
    /// shared worker, and having other contexts talk to it avoids this; an application that
    /// will not do that should treat `StorageInUse` as a state to show the user rather than
    /// retry in a loop.
    ///
    /// # Durability, as far as a browser offers it
    ///
    /// Writes survive reload, navigation and a killed tab, but a browser store is not as
    /// durable as a native directory: clearing site data removes it, and storage pressure can
    /// evict it unless the origin has been granted persistence. Surface this to users: on this
    /// platform, a written-down seed phrase is the backup against a routine "clear browsing
    /// data", not just against losing a device. See
    /// [`Sdk::export_mnemonic`](crate::Sdk::export_mnemonic).
    ///
    /// This only validates `name` and records it; nothing in the browser is touched until
    /// [`SdkBuilder::build`](crate::SdkBuilder::build) opens the storage.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a `name` that is empty,
    /// too long, or contains a character outside the set above.
    ///
    /// Everything that depends on the browser environment is reported by
    /// [`SdkBuilder::build`](crate::SdkBuilder::build) instead: no usable origin-private file
    /// system, or storage access denied, as
    /// [`ErrorCode::Storage`](crate::ErrorCode::Storage), and this origin and `name` already
    /// open elsewhere, as [`ErrorCode::StorageInUse`](crate::ErrorCode::StorageInUse).
    #[cfg(any(target_family = "wasm", doc))]
    pub fn in_browser(name: &str) -> crate::Result<Storage> {
        // Implementation notes (delete once implemented):
        // - Backed by the origin-private file system (OPFS); the async availability and
        //   permission checks belong in `SdkBuilder::build`, not here, since this constructor
        //   is synchronous.
        unimplemented!()
    }

    /// Ephemeral storage held entirely in memory.
    ///
    /// Everything written to it is discarded when the last handle to the SDK instance built
    /// on it is dropped, which makes it the right choice for tests and for throwaway
    /// instances used only to [preview](crate::Sdk::preview) a federation before deciding
    /// whether to join it. Each value names a store of its own, so in-memory instances never
    /// contend for the single-opener lock with each other.
    ///
    /// Infallible: there is no location to validate. Because the backend always starts empty,
    /// an instance built on it accepts a supplied mnemonic as-is, or generates one, and never
    /// produces [`ErrorCode::SeedMismatch`](crate::ErrorCode::SeedMismatch) or the
    /// orphaned-storage refusal described on [`Storage`].
    pub fn in_memory() -> Storage {
        unimplemented!()
    }
}

/// Placeholder for the target-selected backend handle. Replaced by the real backend when the
/// implementation lands; kept private so the choice of backend never leaks into the public
/// API.
#[derive(Debug)]
struct StorageInner;
