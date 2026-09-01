//! The crate's single error type and its stable failure taxonomy.

/// The one error type returned from every fallible call in this crate.
///
/// Callers are expected to branch on [`Error::code`] — `code` is the stable,
/// machine-readable contract. `message` is for humans (logs, error banners,
/// bug reports): it is deliberately **not** part of the stability contract,
/// so it must never be parsed or matched on; its wording can change in any
/// release without that being a breaking change.
///
/// The full underlying failure (the source chain from `fedimint-client`,
/// storage, or the network) is captured for diagnostics but stays internal to
/// the crate: it surfaces through logging and through [`Error`]'s `Debug`
/// output once an implementation exists behind this skeleton, never through
/// a public accessor. This keeps the public error surface small and stable
/// even as the internals it wraps change.
///
/// Structured detail will grow over time (for example, attaching the
/// conflicting module names to [`ErrorCode::UnsupportedFederation`] as a
/// typed field rather than only in `message`). Such additions are additive
/// and won't require callers to parse `message` to get at that detail: they
/// land as **new fields on `Error`**, which is `#[non_exhaustive]` precisely
/// so that adding one is not a breaking change. They never arrive as
/// payloads on [`ErrorCode`] — that enum is a fieldless `Copy` enum so it
/// maps onto a plain Swift, Kotlin, or TypeScript enum, and giving a variant
/// a payload would break both `Copy` and every unit-pattern match — and they
/// never remove or repurpose `code` or `message`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Error {
    /// Stable, machine-readable failure category. Safe to match on.
    pub code: ErrorCode,
    /// Human-readable context for logs and diagnostics. Not part of the
    /// stability contract: never match on this field's contents.
    pub message: String,
}

impl Error {
    /// Builds an SDK error from a code and a human-readable message.
    ///
    /// This is how a **binding or adapter layer outside this crate produces
    /// an SDK error**, so that there is genuinely one error surface. The
    /// UniFFI, wasm and JavaScript layers all have failures of their own to
    /// report — a quote object re-used after it was already executed, a
    /// worker or transport dying with in-flight operations that must each
    /// terminate observably, a value that could not be carried across the
    /// boundary — and every one of those reaches the application as an
    /// [`Error`] with an [`ErrorCode`] to branch on, exactly like a failure
    /// raised inside the SDK. Without a constructor those layers would have
    /// to invent a parallel error type per platform, which is the outcome
    /// this crate exists to prevent.
    ///
    /// Pick the `code` that describes the failure from the caller's point of
    /// view rather than the layer's — [`ErrorCode::QuoteExpired`] for a
    /// re-used quote, [`ErrorCode::Internal`] only where nothing else fits.
    /// `message` is for humans: it is not part of the stability contract and
    /// must never be parsed.
    ///
    /// `Error` is `#[non_exhaustive]`, so this constructor, rather than a
    /// struct literal, is also the only way to build one from another crate.
    /// Fields added in later releases get sensible defaults here, which is
    /// what keeps such an addition non-breaking.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Error {
        Error {
            code,
            message: message.into(),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl core::error::Error for Error {}

/// Stable, machine-readable failure category for [`Error`].
///
/// This enum is **additive-only after 1.0**: new variants may be added in
/// minor releases, but existing variants are never removed or renamed. It is
/// marked `#[non_exhaustive]` for the same reason — Rust callers must write
/// non-exhaustive matches (with a wildcard arm), and foreign-language
/// bindings generated over this crate map any variant they don't yet know
/// about to an explicit "unknown" case in their own enum rather than
/// crashing or silently misinterpreting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    /// The input could not be parsed or was structurally invalid — a
    /// malformed invite code, invoice, ecash notes, address, mnemonic, or
    /// activity cursor.
    InvalidInput,
    /// The federation identified by an invite code has already been joined.
    AlreadyJoined,
    /// The federation cannot be used as configured. This includes mixed
    /// module generations within one federation (all modules must share the
    /// same v1/v2 generation) and configurations the SDK otherwise refuses
    /// to operate on. `message` names the conflicting modules and versions.
    UnsupportedFederation,
    /// No guardian could be reached to service the request.
    FederationUnreachable,
    /// The spendable balance is too low to cover the requested amount.
    InsufficientBalance,
    /// A request to permanently forget a federation was made while spendable
    /// balance still remains in it.
    BalanceNotEmpty,
    /// A request to permanently forget a federation was made while
    /// non-final operations, reclaimable outgoing value, or an in-progress
    /// recovery still exist for it.
    PendingOperations,
    /// No usable lightning gateway is currently available.
    GatewayUnavailable,
    /// The requested action is unavailable because recovery is still in
    /// progress for this federation.
    Recovering,
    /// The federation does not have the module backing this facade. This
    /// occurs when a facade obtained earlier is used after the federation's
    /// configuration changed to drop that module.
    NotSupported,
    /// A persisted operation exists but this SDK version or module set
    /// cannot interpret it. The operation is still observable (its kind and
    /// id are readable) but not actionable.
    UnsupportedOperation,
    /// Storage already holds a seed and it does not match the mnemonic
    /// supplied to open it.
    SeedMismatch,
    /// The storage location is already open, in this process or another.
    StorageInUse,
    /// The quote passed to `send` is no longer valid: either its validity
    /// window has passed, or it has already been executed and a quote funds
    /// exactly one payment. Both are the same situation from a caller's
    /// point of view — this particular quote can never be sent — and both
    /// have the same remedy: obtain a fresh quote and retry.
    ///
    /// The already-executed case is what a binding reports when a quote
    /// object crosses the boundary and is used a second time. Rust's type
    /// system prevents that outright, because `send` takes the quote by
    /// value; a foreign language has no move semantics, so the runtime has
    /// to refuse the second use, and it refuses it with this code rather
    /// than paying twice.
    QuoteExpired,
    /// Conditions material to the quote (fees, routing, federation state)
    /// changed since it was issued. Obtain a fresh quote and retry.
    QuoteChanged,
    /// An amountless bolt11 invoice was passed to a quote call without an
    /// explicit amount override.
    AmountRequired,
    /// The address's network does not match the federation's network.
    NetworkMismatch,
    /// The federation handle is closed, either because it was individually
    /// closed while retaining its data, or because the whole SDK instance
    /// was shut down.
    FederationClosed,
    /// The operation did not complete within an internal time budget.
    Timeout,
    /// The platform's secure random source was unavailable or failed, so no
    /// entropy could be drawn — as when
    /// [`Mnemonic::generate`](crate::Mnemonic::generate) creates a fresh seed,
    /// directly or through [`SdkBuilder::build`](crate::SdkBuilder::build)
    /// establishing one for empty storage.
    ///
    /// This is the one failure a caller can do nothing about: there is no
    /// input to correct, no permission to grant, and no retry that reliably
    /// helps. It is still surfaced rather than papered over, because the
    /// alternatives are worse — panicking would take down a binding layer
    /// that must not panic, and falling back to a weaker source would mint a
    /// guessable seed and lose funds silently. Report it and stop; do not
    /// substitute entropy of your own.
    Entropy,
    /// The local storage backend failed to read or write.
    Storage,
    /// An internal error that does not fit any other category. Its presence
    /// generally indicates a bug; `message` carries what diagnostic detail
    /// is available.
    Internal,
}

/// Crate-wide result alias: every fallible call in this crate returns
/// `Result<T>`, with [`Error`] as the default error type.
pub type Result<T, E = Error> = core::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_formats_as_code_then_message() {
        let err = Error {
            code: ErrorCode::InsufficientBalance,
            message: "need 1000 msat, have 500 msat".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "InsufficientBalance: need 1000 msat, have 500 msat"
        );
    }

    #[test]
    fn new_builds_an_error_from_a_code_and_message() {
        // The constructor a binding layer uses; it must accept anything
        // string-shaped and preserve both fields verbatim.
        let from_str = Error::new(ErrorCode::QuoteExpired, "quote already executed");
        assert_eq!(from_str.code, ErrorCode::QuoteExpired);
        assert_eq!(from_str.message, "quote already executed");

        let from_string = Error::new(ErrorCode::Internal, String::from("worker died"));
        assert_eq!(from_string.code, ErrorCode::Internal);
        assert_eq!(from_string.message, "worker died");
        assert_eq!(from_string.to_string(), "Internal: worker died");
    }

    #[test]
    fn error_implements_std_error() {
        fn assert_std_error<E: core::error::Error>(_e: &E) {}
        let err = Error {
            code: ErrorCode::Internal,
            message: "boom".to_string(),
        };
        assert_std_error(&err);
    }
}
