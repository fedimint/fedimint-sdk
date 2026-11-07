//! Where an SDK instance keeps the state it must not lose.

use fedimint_core::db::Database;
use fedimint_core::db::mem_impl::MemDatabase;
#[cfg(not(target_family = "wasm"))]
use fedimint_core::db::{
    DatabaseError, DatabaseResult, IDatabaseTransactionOps, IDatabaseTransactionOpsCore,
    IRawDatabase, IRawDatabaseTransaction, PrefixStream,
};
use fedimint_core::module::registry::ModuleDecoderRegistry;
use fedimint_core::util::FmtCompact as _;
#[cfg(not(target_family = "wasm"))]
use fedimint_core::{apply, async_trait_maybe_send};

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
/// over one wallet's state could corrupt it and double-spend notes. An open never waits for the
/// opener that is in the way: on native targets it retries for a fraction of a second, for the
/// reason below, and then refuses.
///
/// The claim lasts as long as the open store, not as long as one [`Sdk`](crate::Sdk) handle.
/// [`SdkBuilder::build`](crate::SdkBuilder::build) takes it, and it is given back when the store
/// closes. [`Sdk::shutdown`](crate::Sdk::shutdown) closes the store, so once it returns the
/// location can be opened again, in this process or another, even while handles built on the
/// first instance are still alive: they fail closed and never reach the location again. Without
/// a shutdown, the store closes when the last handle over the instance is dropped: every
/// [`Sdk`](crate::Sdk), every [`Federation`](crate::Federation), and every operation handle and
/// subscriber taken from a federation, and possibly a while after that, until the instance's
/// background work has stopped. An attempt to open the location before then is refused rather
/// than left waiting.
///
/// A claim left behind by a process that died is reclaimed by the next opener rather than left
/// stuck: `StorageInUse` means genuinely concurrent use, never a stale marker.
///
/// On native targets a child process shares the claim with the process that starts it, from the
/// moment it is forked until it executes its program. An instance that closes its store while a
/// child is being started therefore leaves the location held for that short window, and an open
/// retries briefly to see it through rather than refusing. A child that forks without ever
/// executing a program keeps the location held for as long as it runs, and opens are refused
/// until it exits.
///
/// This protects against concurrent use of one location, not against a second copy of the data:
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
/// application's own responsibility, see [`Mnemonic`](crate::Mnemonic). So is the
/// directory-level protection the platform offers, which the SDK does not set itself; on Apple
/// platforms see below.
///
/// # On Apple platforms
///
/// Pass [`Storage::at`] a directory under Application Support rather than Documents: the store
/// is the application's own state, and anything in Documents can surface in the Files app.
/// Before the first open, the application should also set two properties on that directory.
/// The SDK sets neither itself.
///
/// - **Data Protection class `NSFileProtectionCompleteUntilFirstUserAuthentication`.** It is
///   deliberately *not* `NSFileProtectionComplete`, the obvious-looking stricter choice.
///   `Complete` makes the files unreadable whenever the device is locked. An instance that
///   keeps working in the background, or is woken for a notification, then fails its next read
///   with [`ErrorCode::Storage`](crate::ErrorCode::Storage), partway through whatever it was
///   doing. Until-first-unlock keeps the store encrypted from boot until the user first unlocks
///   the device, which covers a device taken while powered off. Files the store creates later
///   inherit the class from the directory.
/// - **`isExcludedFromBackup`.** Without it, the store, and with it the unencrypted seed, goes
///   into iCloud and into computer backups, which may not be encrypted. The seed phrase the
///   user writes down is the backup. A restored device rejoins its federations through
///   [`Sdk::recover`](crate::Sdk::recover) rather than from a copied store, which the
///   single-opener rule above cannot tell apart from a second device anyway.
///
/// The iOS demo's `DemoModel.dataDirectory()` does both.
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
    /// created, is not readable and writable, or cannot be locked, as
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
    /// this `Database` goes, or earlier, when the returned [`StoreCloser`] closes it.
    pub(crate) async fn open(self) -> Result<(Database, StoreCloser)> {
        match self.inner {
            #[cfg(not(target_family = "wasm"))]
            StorageInner::Directory { location } => open_directory(location).await,
            #[cfg(target_family = "wasm")]
            StorageInner::Browser { location } => {
                Ok((open_browser(location).await?, StoreCloser::default()))
            }
            StorageInner::Memory { db } => Ok((db, StoreCloser::default())),
        }
    }
}

/// Closes an opened location's store and gives its claim back, while handles over it may still
/// be alive.
///
/// Only a native location has anything to close. A browser store is handed back when its worker
/// ends, and an in-memory one has no claim, so for those this does nothing.
#[derive(Debug, Default)]
pub(crate) struct StoreCloser {
    /// Weak, so that holding a closer never keeps a store open that every handle has let go of.
    #[cfg(not(target_family = "wasm"))]
    opened: Option<std::sync::Weak<OpenedSlot>>,
}

impl StoreCloser {
    /// Waits for the transactions in flight to end, then closes the store and gives the claim
    /// back, in that order.
    ///
    /// From the moment this starts, no transaction can begin: one that tries parks for good
    /// rather than failing, see [`ClaimedStore`]. Only the transactions already running are
    /// waited for, so this ends as soon as the last of them does.
    pub(crate) async fn close(self) -> Result<()> {
        #[cfg(not(target_family = "wasm"))]
        if let Some(slot) = self.opened.and_then(|opened| opened.upgrade()) {
            // The write lock is granted only once no read guard is left, and every transaction
            // holds one for its whole life, so no transaction is running past this point. A
            // queued writer also refuses `try_read`, so none starts while this waits.
            let opened = slot.write().await.take();
            drop(slot);
            // One blocking task closes the store and then gives the claim back, through the
            // field order of `Opened`. It runs to completion even if this future is dropped, so
            // the order holds whatever the caller does, and RocksDB's blocking close stays off
            // the async threads.
            tokio::task::spawn_blocking(move || drop(opened))
                .await
                .map_err(|err| {
                    Error::new(
                        ErrorCode::Storage,
                        format!("could not close storage: {err}"),
                    )
                })?;
        }
        Ok(())
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

/// The embedded store as `fedimint-rocksdb` opens it, behind its own `db.db.lock`.
#[cfg(not(target_family = "wasm"))]
type Store = fedimint_db_locked::Locked<fedimint_rocksdb::RocksDb>;

/// An open store with the claim on its location inside it.
///
/// The claim is a field of the store rather than a value held next to it so that the two cannot
/// come apart. Fields are dropped in declaration order, so the store is closed before the claim
/// is given up, and there is never a moment when a location looks free while the store on it is
/// still open. That is the moment a second opener would take the free claim and then block inside
/// the store's own file lock, which has no timeout and no error.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug)]
struct Opened {
    /// The store, closed when this value is dropped.
    store: Store,
    /// Given back after the store above has been closed, and never before it.
    claim: Claim,
}

/// Where an [`Opened`] store lives: `None` once a [`StoreCloser`] has closed it.
#[cfg(not(target_family = "wasm"))]
type OpenedSlot = tokio::sync::RwLock<Option<Opened>>;

/// The raw database every `Database` clone over a native location shares.
///
/// The store sits in a slot that [`StoreCloser::close`] can empty while clones of the `Database`
/// are still alive, which is what lets `Sdk::shutdown` give a location back without waiting for
/// every foreign handle to be collected. Each transaction holds a read guard on the slot for its
/// whole life, so the store is never closed under a transaction that is still using it, and
/// closing waits for exactly those transactions.
///
/// A transaction begun once the slot is empty, or while a close is waiting for it, never
/// starts: its `begin_transaction` stays pending for good. It does not fail, because Fedimint's
/// typed database calls panic on any error from the store, and a panic aborts the host
/// application in a release build. Whatever asks for a transaction after a close is work the
/// shutdown has already stopped, and it sleeps until it is dropped, holding neither the store
/// nor the claim.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug)]
struct ClaimedStore {
    slot: std::sync::Arc<OpenedSlot>,
}

#[cfg(not(target_family = "wasm"))]
#[apply(async_trait_maybe_send!)]
impl IRawDatabase for ClaimedStore {
    type Transaction<'a> = ClaimedTransaction;

    async fn begin_transaction<'a>(&'a self) -> ClaimedTransaction {
        // `try_read` never waits: it is refused while a close holds the slot or is queued for
        // it, and a transaction that would start then parks below instead.
        if let Ok(guard) = self.slot.clone().try_read_owned() {
            let begun = ClaimedTransaction::try_new(guard, async |guard| match &**guard {
                Some(opened) => Ok(Some(opened.store.begin_transaction().await)),
                None => Err(()),
            })
            .await;
            if let Ok(transaction) = begun {
                return transaction;
            }
        }
        std::future::pending().await
    }

    fn checkpoint(&self, backup_path: &std::path::Path) -> DatabaseResult<()> {
        match self.slot.try_read().as_deref() {
            Ok(Some(opened)) => opened.store.checkpoint(backup_path),
            _ => Err(DatabaseError::DatabaseBackend(
                "the storage is closed".into(),
            )),
        }
    }
}

/// A transaction of the embedded store, `None` once it has been committed.
#[cfg(not(target_family = "wasm"))]
type StoreTransaction<'a> = Option<<Store as IRawDatabase>::Transaction<'a>>;

#[cfg(not(target_family = "wasm"))]
self_cell::self_cell!(
    /// A transaction of a [`ClaimedStore`], together with the read guard that keeps its store
    /// open until the transaction is gone.
    struct ClaimedTransaction {
        owner: tokio::sync::OwnedRwLockReadGuard<Option<Opened>>,

        #[not_covariant, async_builder]
        dependent: StoreTransaction,
    }

    impl {Debug}
);

/// Runs one operation on the transaction inside a [`ClaimedTransaction`].
///
/// The transaction is only missing once committed, and committing consumes the wrapper, so the
/// `TransactionConsumed` arm cannot be reached; it is an error rather than a panic all the same.
#[cfg(not(target_family = "wasm"))]
macro_rules! with_transaction {
    ($this:expr, |$transaction:ident| $call:expr) => {
        $this
            .with_dependent_mut(|_, transaction| match transaction {
                Some($transaction) => $call,
                None => Box::pin(async { Err(DatabaseError::TransactionConsumed) }),
            })
            .await
    };
}

#[cfg(not(target_family = "wasm"))]
#[apply(async_trait_maybe_send!)]
impl IDatabaseTransactionOpsCore for ClaimedTransaction {
    async fn raw_insert_bytes(
        &mut self,
        key: &[u8],
        value: &[u8],
    ) -> DatabaseResult<Option<Vec<u8>>> {
        with_transaction!(self, |transaction| transaction.raw_insert_bytes(key, value))
    }

    async fn raw_get_bytes(&mut self, key: &[u8]) -> DatabaseResult<Option<Vec<u8>>> {
        with_transaction!(self, |transaction| transaction.raw_get_bytes(key))
    }

    async fn raw_remove_entry(&mut self, key: &[u8]) -> DatabaseResult<Option<Vec<u8>>> {
        with_transaction!(self, |transaction| transaction.raw_remove_entry(key))
    }

    async fn raw_find_by_prefix(&mut self, key_prefix: &[u8]) -> DatabaseResult<PrefixStream<'_>> {
        with_transaction!(self, |transaction| transaction
            .raw_find_by_prefix(key_prefix))
    }

    async fn raw_find_by_prefix_sorted_descending(
        &mut self,
        key_prefix: &[u8],
    ) -> DatabaseResult<PrefixStream<'_>> {
        with_transaction!(self, |transaction| transaction
            .raw_find_by_prefix_sorted_descending(key_prefix))
    }

    async fn raw_find_by_range(
        &mut self,
        range: std::ops::Range<&[u8]>,
    ) -> DatabaseResult<PrefixStream<'_>> {
        with_transaction!(self, |transaction| transaction.raw_find_by_range(range))
    }

    async fn raw_remove_by_prefix(&mut self, key_prefix: &[u8]) -> DatabaseResult<()> {
        with_transaction!(self, |transaction| transaction
            .raw_remove_by_prefix(key_prefix))
    }
}

#[cfg(not(target_family = "wasm"))]
impl IDatabaseTransactionOps for ClaimedTransaction {}

#[cfg(not(target_family = "wasm"))]
#[apply(async_trait_maybe_send!)]
impl IRawDatabaseTransaction for ClaimedTransaction {
    async fn commit_tx(mut self) -> DatabaseResult<()> {
        // The read guard outlives the commit: it is dropped with `self`, after this returns.
        self.with_dependent_mut(|_, transaction| match transaction.take() {
            Some(transaction) => transaction.commit_tx(),
            None => Box::pin(async { Err(DatabaseError::TransactionConsumed) }),
        })
        .await
    }
}

/// Proof that this instance is the only opener of its location.
///
/// Dropping it gives the location back, which is what makes a location reopenable once its
/// store has closed, and what makes a claim left by a dead process reclaimable
/// rather than fatal.
#[cfg(not(target_family = "wasm"))]
struct Claim {
    /// The open `LOCK` file whose advisory lock this instance holds. The lock was taken with
    /// `try_write` and its guard forgotten, so it lives exactly as long as this value: `flock`
    /// belongs to the open file description, so closing the file releases it, and so does the
    /// kernel when the process dies. For the same reason a child forked while this is open holds
    /// the lock too, until it executes its program: nothing unlocks it explicitly, unlike the
    /// embedded store's own lock. See [`claim_lock_file`].
    _file: fd_lock::RwLock<std::fs::File>,
}

#[cfg(not(target_family = "wasm"))]
impl core::fmt::Debug for Claim {
    /// Prints the type name and nothing else: a claim has no state worth rendering and the file
    /// handle behind it is not part of any contract.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Claim")
    }
}

/// Opens a native directory: create it, claim it, then open the embedded store inside it.
///
/// Claiming and opening happen in one blocking task, which is what keeps the claim and the store
/// inseparable even when the caller gives up waiting. A blocking task runs to completion whether
/// or not anyone is still waiting for it, and what it hands back is the two together, so a
/// caller that walks away leaves a store that is closed before its claim is given up, never a
/// free claim over a store that is still being opened.
#[cfg(not(target_family = "wasm"))]
async fn open_directory(location: String) -> Result<(Database, StoreCloser)> {
    let opened = tokio::task::spawn_blocking(move || -> Result<_> {
        let directory = std::path::PathBuf::from(&location);
        let claim = take_claim(&directory, &location)?;
        let store = fedimint_rocksdb::RocksDb::build(directory.join("db"))
            .open_blocking()
            .map_err(|err| {
                Error::new(
                    ErrorCode::Storage,
                    format!(
                        "could not open the storage at {location}: {}",
                        err.fmt_compact()
                    ),
                )
            })?;
        Ok(Opened { store, claim })
    })
    .await
    .map_err(|err| Error::new(ErrorCode::Storage, format!("could not open storage: {err}")))??;

    let slot = std::sync::Arc::new(tokio::sync::RwLock::new(Some(opened)));
    let closer = StoreCloser {
        opened: Some(std::sync::Arc::downgrade(&slot)),
    };
    Ok((
        Database::new(ClaimedStore { slot }, ModuleDecoderRegistry::default()),
        closer,
    ))
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
    claim_lock_file(file, location)
}

/// How long a claim is retried before an open is refused.
///
/// Long enough to outlast a child process that is being started at the moment the previous
/// opener closes, and short enough that a genuine refusal still comes back well within any
/// reasonable UI wait. See [`claim_lock_file`].
#[cfg(not(target_family = "wasm"))]
const CLAIM_RETRY_BUDGET: std::time::Duration = std::time::Duration::from_millis(200);

/// Takes the advisory lock on an open `LOCK` file, retrying for at most [`CLAIM_RETRY_BUDGET`].
///
/// The retry exists because `flock` belongs to the open file description, not to the fd. A child
/// process started between `fork` and `exec` holds a copy of every fd its parent has open, and
/// `O_CLOEXEC` only closes that copy at `exec`. An opener that closes inside that window leaves
/// the lock held by the child until it execs, so an immediate reopen would be refused even though
/// nothing has the location open. The window cannot outlive the children already forked when the
/// fd was closed, since no later child can inherit it, so a short bounded retry covers it.
///
/// The embedded store's own `db.db.lock` is an `flock` too, but it plays no part in that window:
/// `fs-lock` unlocks it explicitly when the store is dropped, and an explicit unlock releases the
/// whole open file description, the child's copy included. This claim lingers only because its
/// guard is forgotten and nothing unlocks it.
///
/// A claim that children do not inherit (`fcntl` record locks) would still be worse, in the case
/// where no destructor runs. If the process dies while a child still holds its descriptors,
/// nothing unlocks `db.db.lock`, and the child keeps it held. An `fcntl` claim would have been
/// released by the death, letting the next opener past it into the store's wait for
/// `db.db.lock`, which has no timeout, for as long as the child lives. This claim is held by the
/// same child, so that opener is refused instead. `fcntl` locks also belong to the whole process,
/// so a second opener in the same process would need a registry of its own to be refused.
///
/// Only contention is retried. Any other failure to lock means the location cannot be locked at
/// all, a file system without advisory locks for instance, and waiting will not change that, so
/// it is reported straight away as [`ErrorCode::Storage`] rather than as a location in use.
#[cfg(not(target_family = "wasm"))]
fn claim_lock_file(file: std::fs::File, location: &str) -> Result<Claim> {
    let mut file = fd_lock::RwLock::new(file);
    // Forgetting the guard keeps the advisory lock without keeping a borrow of the `RwLock` that
    // would make this value self-referential. Nothing leaks: the guard owns only a reference, and
    // the lock is released when the file below is closed.
    retry_while_contended(location, || file.try_write().map(core::mem::forget))?;
    Ok(Claim { _file: file })
}

/// Runs `attempt` until it succeeds, retrying contention for at most [`CLAIM_RETRY_BUDGET`].
///
/// fd-lock reports contention as [`std::io::ErrorKind::WouldBlock`] on every platform: it maps
/// `EWOULDBLOCK` on Unix and `ERROR_LOCK_VIOLATION` on Windows to it. That is the only error that
/// means someone else holds the lock, so it is the only one that can end in `StorageInUse`.
/// `Interrupted` is a signal landing during the call and is retried the same way.
#[cfg(not(target_family = "wasm"))]
fn retry_while_contended(
    location: &str,
    mut attempt: impl FnMut() -> std::io::Result<()>,
) -> Result<()> {
    const FIRST_PAUSE: std::time::Duration = std::time::Duration::from_millis(1);
    const LONGEST_PAUSE: std::time::Duration = std::time::Duration::from_millis(25);

    let deadline = std::time::Instant::now() + CLAIM_RETRY_BUDGET;
    let mut pause = FIRST_PAUSE;
    loop {
        match attempt() {
            Ok(()) => return Ok(()),
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(err) => {
                return Err(Error::new(
                    ErrorCode::Storage,
                    format!("could not lock the storage at {location}: {err}"),
                ));
            }
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            break;
        }
        std::thread::sleep(pause.min(deadline - now));
        pause = (pause * 2).min(LONGEST_PAUSE);
    }
    Err(Error::with_details(
        ErrorCode::StorageInUse,
        format!(
            "the storage at {location} is already open, in this process or another one: an \
             instance on it closes its storage when it is shut down, or once every handle over it \
             has been dropped"
        ),
        ErrorDetails::StorageInUse {
            location: location.to_owned(),
        },
    ))
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
            format!(
                "could not open the storage named {location}: {}",
                err.fmt_compact()
            ),
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

        drop(first);

        let third = Storage::at(&path)
            .expect("a valid path")
            .open()
            .await
            .expect("the claim is reclaimed once the store is closed");
        drop(third);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn closing_waits_for_a_transaction_in_flight_then_frees_the_location() {
        // A transaction that is running when the close starts finishes on an open store: the
        // close waits for it, and only then closes the store and gives the claim back.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().to_str().expect("a utf-8 path").to_owned();

        let (db, closer) = Storage::at(&path)
            .expect("a valid path")
            .open()
            .await
            .expect("the location opens");
        let mut dbtx = db.begin_transaction().await;
        dbtx.raw_insert_bytes(b"key", b"value")
            .await
            .expect("the write lands on the open store");

        let mut closing = tokio::spawn(closer.close());
        tokio::time::timeout(std::time::Duration::from_millis(200), &mut closing)
            .await
            .expect_err("the close waits for the transaction in flight");
        let err = Storage::at(&path)
            .expect("a valid path")
            .open()
            .await
            .expect_err("the location is still held while the close waits");
        assert_eq!(err.code, ErrorCode::StorageInUse);

        dbtx.commit_tx().await;
        tokio::time::timeout(std::time::Duration::from_secs(10), closing)
            .await
            .expect("the close ends once the transaction does")
            .expect("the close task finishes")
            .expect("the store closes");

        let (reopened, _closer) = Storage::at(&path)
            .expect("a valid path")
            .open()
            .await
            .expect("the location is free while the first database is still alive");
        let mut dbtx = reopened.begin_transaction_nc().await;
        assert_eq!(
            dbtx.raw_get_bytes(b"key").await.expect("the read succeeds"),
            Some(b"value".to_vec()),
            "the transaction the close waited for was committed"
        );
        drop(dbtx);
        drop((reopened, db));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_transaction_begun_after_close_parks_rather_than_failing() {
        // Fedimint's typed database calls panic on an error from the store, and a panic aborts
        // the host application in a release build, so a closed store must never answer with
        // one: whatever asks for a transaction after the close waits for good instead.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().to_str().expect("a utf-8 path").to_owned();

        let (db, closer) = Storage::at(&path)
            .expect("a valid path")
            .open()
            .await
            .expect("the location opens");
        closer.close().await.expect("the store closes");

        tokio::time::timeout(
            std::time::Duration::from_millis(200),
            db.begin_transaction_nc(),
        )
        .await
        .expect_err("no transaction starts on a closed store");

        let (reopened, _closer) = Storage::at(&path)
            .expect("a valid path")
            .open()
            .await
            .expect("the location is free while the closed database is still alive");
        drop((reopened, db));
    }

    /// Claims `directory` the way `take_claim` does, and returns the claim together with a second
    /// descriptor on the same open file description.
    ///
    /// `try_clone` is `dup`, and a dup shares the open file description, which is exactly what a
    /// child process holds between `fork` and `exec`. Dropping the claim while keeping the copy is
    /// therefore the situation a process-spawning thread creates, without depending on hitting
    /// that window by timing.
    fn claim_with_inherited_copy(directory: &std::path::Path) -> (Claim, std::fs::File) {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("LOCK"))
            .expect("the lock file opens");
        let inherited = file.try_clone().expect("the descriptor duplicates");
        let claim = claim_lock_file(file, "held").expect("a free location is claimed");
        (claim, inherited)
    }

    #[test]
    fn a_claim_held_by_an_inherited_descriptor_is_reclaimed_once_it_closes() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let (claim, inherited) = claim_with_inherited_copy(dir.path());
        drop(claim);

        // Stands in for the child reaching `exec`, well inside the retry budget.
        let child = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            drop(inherited);
        });

        let reclaimed = take_claim(dir.path(), "reopened")
            .expect("the location comes free once the inherited copy closes");
        drop(reclaimed);
        child.join().expect("the stand-in child finishes");
    }

    #[test]
    fn a_claim_held_past_the_retry_budget_is_still_refused() {
        // The retry covers a child on its way to `exec`; it must not turn into waiting for a
        // holder that stays. A child that forks and never execs is that holder.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().to_str().expect("a utf-8 path").to_owned();
        let (claim, inherited) = claim_with_inherited_copy(dir.path());
        drop(claim);

        let started = std::time::Instant::now();
        let err = take_claim(dir.path(), &path).expect_err("a held location is refused");
        let waited = started.elapsed();

        assert_eq!(err.code, ErrorCode::StorageInUse);
        match err.detail() {
            Some(ErrorDetails::StorageInUse { location }) => assert_eq!(location, &path),
            other => panic!("expected the location, got {other:?}"),
        }
        assert!(
            waited >= CLAIM_RETRY_BUDGET,
            "refused after {waited:?}, before the budget"
        );
        assert!(
            waited < std::time::Duration::from_secs(5),
            "refused after {waited:?}: the retry is not bounded"
        );
        drop(inherited);
    }

    #[test]
    fn a_lock_failure_other_than_contention_is_reported_at_once() {
        // A file system without advisory locks fails every attempt the same way. Retrying it only
        // delays the answer, and calling it `StorageInUse` sends the caller looking for an opener
        // that does not exist.
        let mut attempts = 0;
        let started = std::time::Instant::now();
        let err = retry_while_contended("unlockable", || {
            attempts += 1;
            Err(std::io::ErrorKind::Unsupported.into())
        })
        .expect_err("a location that cannot be locked is refused");

        assert_eq!(err.code, ErrorCode::Storage);
        assert_eq!(
            attempts, 1,
            "a failure that is not contention is not retried"
        );
        assert!(
            started.elapsed() < CLAIM_RETRY_BUDGET,
            "reported after {:?}, not at once",
            started.elapsed()
        );
    }

    #[test]
    fn contention_is_retried_until_the_lock_comes_free() {
        let mut attempts = 0;
        retry_while_contended("contended", || {
            attempts += 1;
            if attempts < 3 {
                Err(std::io::ErrorKind::WouldBlock.into())
            } else {
                Ok(())
            }
        })
        .expect("the lock is taken once the holder lets go");
        assert_eq!(attempts, 3);
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
