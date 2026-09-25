//! Where an SDK instance keeps the state it must not lose.

use fedimint_core::db::Database;
use fedimint_core::db::mem_impl::MemDatabase;
use fedimint_core::module::registry::ModuleDecoderRegistry;
#[cfg(not(target_family = "wasm"))]
use fedimint_core::{apply, async_trait_maybe_send, db::IRawDatabase};

use crate::{Error, ErrorCode, ErrorDetails, Result};

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
/// over one wallet's state could corrupt it and double-spend notes. That refusal is immediate;
/// an open never waits for the opener that is in the way.
///
/// The claim lasts as long as the open store, not as long as one [`Sdk`](crate::Sdk) handle.
/// [`SdkBuilder::build`](crate::SdkBuilder::build) takes it, and it is given back when the store
/// closes, which is when the last handle over that instance has been dropped: every
/// [`Sdk`](crate::Sdk), every [`Federation`](crate::Federation), and every operation handle and
/// subscriber taken from a federation. [`Sdk::shutdown`](crate::Sdk::shutdown) ends the instance's
/// work but does not by itself close the store, so reopening a location in the same process
/// means letting go of everything built on it first, and an attempt before that point is
/// refused rather than left waiting.
///
/// A claim left behind by a process that died is reclaimed by the next opener rather than left
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
        if path.is_empty() {
            return Err(crate::Error::new(
                crate::ErrorCode::InvalidInput,
                "the storage path is empty",
            ));
        }
        // A path is a byte string to the operating system, but a NUL byte terminates it, so a
        // string carrying one cannot be expressed as a path at all. Everything else is left to
        // the file system, which reports its own refusals when `build` opens the location.
        if path.contains('\0') {
            return Err(crate::Error::new(
                crate::ErrorCode::InvalidInput,
                "the storage path contains a NUL byte",
            ));
        }
        Ok(Storage {
            inner: StorageInner::Directory {
                location: path.to_owned(),
            },
        })
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
        // The documented rule, with "short" pinned to 64 characters: the name becomes a file name
        // in origin-private storage, and every engine's limit is far above that, so this is a
        // bound the SDK can promise rather than one the browser might move.
        const MAX_NAME: usize = 64;
        let usable = !name.is_empty()
            && name.len() <= MAX_NAME
            && name != "."
            && name != ".."
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !usable {
            return Err(crate::Error::new(
                crate::ErrorCode::InvalidInput,
                "a storage name must be 1 to 64 characters of letters, digits, '-', '_' or '.'",
            ));
        }
        Ok(Storage {
            inner: StorageInner::Browser {
                location: name.to_owned(),
            },
        })
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
        // The store is created here rather than in `build`, which is what makes each value name a
        // store of its own: the descriptor is not `Clone`, and `SdkBuilder::storage` consumes it,
        // so exactly one instance can ever be built on it.
        Storage {
            inner: StorageInner::Memory {
                db: Database::new(MemDatabase::new(), ModuleDecoderRegistry::default()),
            },
        }
    }
}

impl Storage {
    /// The location string exactly as the caller gave it, for the error details that name it.
    ///
    /// An in-memory store has no location a person could act on, so it reports itself as one.
    pub(crate) fn location(&self) -> String {
        match &self.inner {
            #[cfg(not(target_family = "wasm"))]
            StorageInner::Directory { location } => location.clone(),
            #[cfg(target_family = "wasm")]
            StorageInner::Browser { location } => location.clone(),
            StorageInner::Memory { .. } => "<in memory>".to_owned(),
        }
    }

    /// Opens the location, creating it if needed, and takes the single-opener claim.
    ///
    /// This is step 1 of `SdkBuilder::build`: everything environmental about a location is
    /// reported here, and nothing has been written when it fails.
    ///
    /// The claim on the location travels inside the returned store, so the caller has nothing
    /// to hold on to and nothing to release: the location is given back when the last clone of
    /// this `Database` goes, and not before.
    pub(crate) async fn open(self) -> Result<Database> {
        match self.inner {
            #[cfg(not(target_family = "wasm"))]
            StorageInner::Directory { location } => open_directory(location).await,
            #[cfg(target_family = "wasm")]
            StorageInner::Browser { location } => open_browser(location).await,
            StorageInner::Memory { db } => Ok(db),
        }
    }
}

/// The backend a [`Storage`] names, chosen at construction and opened by `build`.
#[derive(Debug)]
enum StorageInner {
    /// A native directory the SDK owns outright.
    #[cfg(not(target_family = "wasm"))]
    Directory { location: String },
    /// An origin-scoped namespace in the browser's origin-private file system.
    #[cfg(target_family = "wasm")]
    Browser { location: String },
    /// An in-memory store, already created: see [`Storage::in_memory`].
    Memory { db: Database },
}

/// A store with the claim on its location inside it.
///
/// The claim is a field of the store rather than a value held next to it so that the two cannot
/// come apart. Fields are dropped in declaration order, so the store is closed before the claim
/// is given up, and there is never a moment when a location looks free while the store on it is
/// still open. That is the moment a second opener would take the free claim and then block inside
/// the store's own file lock, which has no timeout and no error.
///
/// Generic over the store so that the concrete type `fedimint-rocksdb` hands back, itself a
/// wrapper around a wrapper, never has to be named here.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug)]
struct ClaimedStore<Store> {
    /// The store, closed when this value is dropped.
    store: Store,
    /// Given back after the store above has been closed, and never before it.
    claim: Claim,
}

#[cfg(not(target_family = "wasm"))]
#[apply(async_trait_maybe_send!)]
impl<Store> IRawDatabase for ClaimedStore<Store>
where
    Store: IRawDatabase,
{
    type Transaction<'a> = Store::Transaction<'a>;

    async fn begin_transaction<'a>(&'a self) -> <Self as IRawDatabase>::Transaction<'_> {
        self.store.begin_transaction().await
    }

    fn checkpoint(&self, backup_path: &std::path::Path) -> fedimint_core::db::DatabaseResult<()> {
        self.store.checkpoint(backup_path)
    }
}

/// Proof that this instance is the only opener of its location.
///
/// Dropping it gives the location back, which is what makes a location reopenable once
/// everything built on it is gone, and what makes a claim left by a dead process reclaimable
/// rather than fatal.
#[cfg(not(target_family = "wasm"))]
struct Claim {
    /// The open `LOCK` file whose advisory lock this instance holds. The lock was taken with
    /// `try_write` and its guard forgotten, so it lives exactly as long as this value: `flock`
    /// belongs to the open file description, so closing the file releases it, and so does the
    /// kernel when the process dies.
    _file: fd_lock::RwLock<std::fs::File>,
    /// This location's key in [`OPEN_HERE`], or `None` if it could not be resolved to one.
    here: Option<std::path::PathBuf>,
}

#[cfg(not(target_family = "wasm"))]
impl Drop for Claim {
    fn drop(&mut self) {
        let Some(here) = self.here.take() else {
            return;
        };
        let mut open_here = open_here();
        // Counted rather than simply removed, because one path can name two live claims: an
        // application that deletes a location and opens it again gets a new directory at the
        // same name, and the claim on the old one is still around until its store closes.
        if let Some(claims) = open_here.get_mut(&here) {
            *claims -= 1;
            if *claims == 0 {
                open_here.remove(&here);
            }
        }
    }
}

#[cfg(not(target_family = "wasm"))]
impl core::fmt::Debug for Claim {
    /// Prints the type name and nothing else: a claim has no state worth rendering and the file
    /// handle behind it is not part of any contract.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Claim")
    }
}

/// The locations this process currently holds open.
///
/// This decides nothing. The lock file is the only authority on whether a location can be
/// claimed, and this answers the one question that lock cannot: whether the opener standing in
/// the way is this very process, which the application can do something about, or another one,
/// which it cannot.
#[cfg(not(target_family = "wasm"))]
static OPEN_HERE: std::sync::Mutex<std::collections::BTreeMap<std::path::PathBuf, usize>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// [`OPEN_HERE`], with a poisoned lock treated as merely locked: a panic elsewhere cannot leave
/// a plain map of paths in a state worth refusing to read.
#[cfg(not(target_family = "wasm"))]
fn open_here()
-> std::sync::MutexGuard<'static, std::collections::BTreeMap<std::path::PathBuf, usize>> {
    OPEN_HERE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Opens a native directory: create it, claim it, then open the embedded store inside it.
///
/// Claiming and opening happen in one blocking task, which is what keeps the claim and the store
/// inseparable even when the caller gives up waiting. A blocking task runs to completion whether
/// or not anyone is still waiting for it, and what it hands back is the two together, so a
/// caller that walks away leaves a store that is closed before its claim is given up, never a
/// free claim over a store that is still being opened.
#[cfg(not(target_family = "wasm"))]
async fn open_directory(location: String) -> Result<Database> {
    let store = tokio::task::spawn_blocking(move || -> Result<_> {
        let directory = std::path::PathBuf::from(&location);
        let claim = take_claim(&directory, &location)?;
        let store = fedimint_rocksdb::RocksDb::build(directory.join("db"))
            .open_blocking()
            .map_err(|err| {
                Error::new(
                    ErrorCode::Storage,
                    format!("could not open the storage at {location}: {err}"),
                )
            })?;
        Ok(ClaimedStore { store, claim })
    })
    .await
    .map_err(|err| Error::new(ErrorCode::Storage, format!("could not open storage: {err}")))??;

    Ok(Database::new(store, ModuleDecoderRegistry::default()))
}

/// Creates the directory and claims it, or reports why it cannot be claimed.
#[cfg(not(target_family = "wasm"))]
fn take_claim(directory: &std::path::Path, location: &str) -> Result<Claim> {
    std::fs::create_dir_all(directory).map_err(|err| {
        Error::new(
            ErrorCode::Storage,
            format!("could not create the storage directory {location}: {err}"),
        )
    })?;
    // Resolved once the directory exists, so that two spellings of one location are one entry.
    // A location that will not resolve is simply not tracked; only the wording of a refusal
    // depends on it.
    let here = std::fs::canonicalize(directory).ok();

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("LOCK"))
        .map_err(|err| {
            Error::new(
                ErrorCode::Storage,
                format!("could not open the storage at {location}: {err}"),
            )
        })?;

    let mut file = fd_lock::RwLock::new(file);
    let Ok(guard) = file.try_write() else {
        return Err(in_use(location, here.as_deref()));
    };
    // Forgetting the guard keeps the advisory lock without keeping a borrow of the `RwLock` that
    // would make this value self-referential. Nothing leaks: the guard owns only a reference, and
    // the lock is released when the file below is closed.
    core::mem::forget(guard);

    if let Some(here) = &here {
        *open_here().entry(here.clone()).or_default() += 1;
    }
    Ok(Claim { _file: file, here })
}

/// The refusal a second opener gets, worded for whichever opener is in its way.
///
/// Both cases are the same [`ErrorCode::StorageInUse`] carrying the same details; what differs is
/// what the message tells the application to do about it.
#[cfg(not(target_family = "wasm"))]
fn in_use(location: &str, here: Option<&std::path::Path>) -> Error {
    let message = if here.is_some_and(|here| open_here().contains_key(here)) {
        format!(
            "the storage at {location} is still open in this process: shutting an instance down \
             does not close it, every handle over it has to be dropped first"
        )
    } else {
        format!("the storage at {location} is already open")
    };
    Error::with_details(
        ErrorCode::StorageInUse,
        message,
        ErrorDetails::StorageInUse {
            location: location.to_owned(),
        },
    )
}

/// Opens an origin-private store: find the origin's directory, claim the file, open redb on it.
///
/// Sync access handles are only obtainable inside a worker, and the browser grants one per file at
/// a time, which is exactly the single-opener rule the documentation promises across tabs,
/// iframes and workers.
#[cfg(target_family = "wasm")]
async fn open_browser(location: String) -> Result<Database> {
    use wasm_bindgen::JsCast;

    let denied = |detail: &str| {
        Error::new(
            ErrorCode::Storage,
            format!("could not open the storage named {location}: {detail}"),
        )
    };

    let scope: web_sys::WorkerGlobalScope = js_sys::global()
        .dyn_into()
        .map_err(|_| denied("origin-private storage is only reachable from a worker"))?;
    let directory: web_sys::FileSystemDirectoryHandle =
        wasm_bindgen_futures::JsFuture::from(scope.navigator().storage().get_directory())
            .await
            .map_err(|_| denied("no usable origin-private file system"))?
            .dyn_into()
            .map_err(|_| denied("no usable origin-private file system"))?;

    let options = web_sys::FileSystemGetFileOptions::new();
    options.set_create(true);
    let file: web_sys::FileSystemFileHandle = wasm_bindgen_futures::JsFuture::from(
        directory.get_file_handle_with_options(&format!("{location}.fedimint-sdk"), &options),
    )
    .await
    .map_err(|_| denied("storage access denied"))?
    .dyn_into()
    .map_err(|_| denied("storage access denied"))?;

    let handle: web_sys::FileSystemSyncAccessHandle =
        match wasm_bindgen_futures::JsFuture::from(file.create_sync_access_handle()).await {
            Ok(handle) => handle
                .dyn_into()
                .map_err(|_| denied("storage access denied"))?,
            Err(_) => {
                // A sync access handle is exclusive per file per origin, so the one way this is
                // refused is another tab, worker or iframe already holding it.
                return Err(Error::with_details(
                    ErrorCode::StorageInUse,
                    format!("the storage named {location} is already open"),
                    ErrorDetails::StorageInUse {
                        location: location.clone(),
                    },
                ));
            }
        };

    let raw = fedimint_cursed_redb::MemAndRedb::new(handle).map_err(|err| {
        Error::new(
            ErrorCode::Storage,
            format!("could not open the storage named {location}: {err}"),
        )
    })?;
    Ok(Database::new(raw, ModuleDecoderRegistry::default()))
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use crate::{ErrorCode, ErrorDetails};

    use super::*;

    #[test]
    fn a_path_is_only_validated_never_touched() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let inside = dir.path().join("not-created-yet");
        let path = inside.to_str().expect("a utf-8 path").to_owned();

        let storage = Storage::at(&path).expect("a valid path is accepted");
        assert_eq!(storage.location(), path);
        assert!(!inside.exists(), "the constructor must not create anything");
    }

    #[test]
    fn an_unusable_path_is_rejected_without_touching_anything() {
        for bad in ["", "with\0nul"] {
            let err = Storage::at(bad).expect_err("an unusable path is refused");
            assert_eq!(err.code, ErrorCode::InvalidInput);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn opening_a_location_creates_it() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let inside = dir.path().join("wallet");
        let path = inside.to_str().expect("a utf-8 path").to_owned();

        let opened = Storage::at(&path)
            .expect("a valid path")
            .open()
            .await
            .expect("the location is created and locked");
        assert!(inside.exists());
        drop(opened);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_second_opener_is_refused_and_the_claim_returns_when_the_store_closes() {
        // The whole point of the claim is that two writers can never share one
        // wallet's state, and that a dead holder is not a permanent lockout. The
        // second half is what makes `StorageInUse` mean "genuinely concurrent".
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().to_str().expect("a utf-8 path").to_owned();

        let first = Storage::at(&path)
            .expect("a valid path")
            .open()
            .await
            .expect("the first opener wins");

        // Timed, because what this guards against is not a wrong answer but no answer: the
        // refusal has to come back rather than queue behind the store's own file lock, which
        // waits with no timeout and no error.
        let err = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            Storage::at(&path).expect("a valid path").open(),
        )
        .await
        .expect("the second opener answers rather than waiting on the first")
        .expect_err("the second opener is refused");
        assert_eq!(err.code, ErrorCode::StorageInUse);
        match err.detail() {
            Some(ErrorDetails::StorageInUse { location }) => assert_eq!(location, &path),
            other => panic!("expected the location, got {other:?}"),
        }
        assert!(
            err.message.contains("in this process"),
            "a refusal an application can act on has to name the opener in the way: {}",
            err.message,
        );

        drop(first);

        let third = Storage::at(&path)
            .expect("a valid path")
            .open()
            .await
            .expect("the claim is reclaimed once the store is closed");
        drop(third);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn in_memory_stores_never_contend() {
        // Each value names a store of its own, so two of them are two wallets and
        // neither can report the other as concurrent use.
        let first = Storage::in_memory()
            .open()
            .await
            .expect("an in-memory store opens");
        let second = Storage::in_memory()
            .open()
            .await
            .expect("and so does a second");
        drop((first, second));
    }
}
