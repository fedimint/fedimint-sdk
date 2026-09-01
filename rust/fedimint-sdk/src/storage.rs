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
/// # Why `&str` and not `&Path`
///
/// [`Storage::at`] takes the location as a `&str` rather than a
/// `&std::path::Path`. This is deliberate: `Path` is an OS-string type with
/// platform-specific encoding rules and has no natural representation in
/// Swift, Kotlin, or JavaScript, so every binding would have to invent a
/// conversion. A string path is what those hosts already hand out (an
/// application-support directory, a documents directory, a browser origin
/// name), and keeping the SDK's own signature a string means one validating
/// parse on the Rust side instead of one per language.
///
/// # Seed and storage lifecycle
///
/// The rules below are enforced by [`SdkBuilder::build`](crate::SdkBuilder::build),
/// which is where a `Storage` is actually opened; they are stated here
/// because they are properties of the storage, not of the builder call:
///
/// - **A seed is written once, before anything derived from it.** When an
///   SDK instance is built against empty storage and no mnemonic was
///   supplied, one is generated and durably persisted *before* any
///   federation-derived state exists. There is no window in which
///   federation state has been written but the seed that produced it has
///   not. Generating a seed is fallible — it needs the platform's secure
///   random source — and a failure to draw entropy fails the open with
///   [`ErrorCode::Entropy`](crate::ErrorCode::Entropy), leaving the storage
///   untouched.
/// - **A different seed is a refusal, not a migration.** Opening storage
///   that already holds a seed while supplying a different mnemonic fails
///   with [`ErrorCode::SeedMismatch`](crate::ErrorCode::SeedMismatch), and
///   it fails *before* any mutation: the existing storage is left exactly
///   as it was found.
/// - **One opener at a time.** Opening a location that is already open —
///   by another [`Sdk`](crate::Sdk) in this process or by another process
///   entirely — fails with
///   [`ErrorCode::StorageInUse`](crate::ErrorCode::StorageInUse). Storage is
///   released again by [`Sdk::shutdown`](crate::Sdk::shutdown), or when the
///   last handle to the instance is dropped.
/// - **A federation that cannot be opened is reported.** Reopening an
///   instance reopens every federation it had joined; one that fails to
///   open is surfaced to the caller rather than quietly omitted from
///   [`Sdk::federations`](crate::Sdk::federations), so an application can
///   never conclude from a short list that a federation was left.
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
///   app with a notification-service extension needs). That is a new
///   constructor or option on `Storage`, not a change to the ones here.
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
    /// Persistent storage rooted at `path`, for native targets.
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
    pub fn at(path: &str) -> crate::Result<Storage> {
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
    /// always starts from an empty seed slot: a supplied mnemonic is
    /// accepted as-is and an omitted one is generated fresh, and
    /// [`ErrorCode::SeedMismatch`](crate::ErrorCode::SeedMismatch) cannot
    /// occur.
    pub fn in_memory() -> Storage {
        unimplemented!()
    }
}

/// Placeholder for the target-selected backend handle. Replaced by the real
/// backend when the implementation lands; kept private so the choice of
/// backend never leaks into the public API.
#[derive(Debug)]
struct StorageInner;
