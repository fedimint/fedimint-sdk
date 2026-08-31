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
/// Some variants will grow additional structured fields over time (for
/// example, attaching the conflicting module names to
/// [`ErrorCode::UnsupportedFederation`] as a typed field rather than only in
/// `message`). Such additions are additive and won't require callers to
/// parse `message` to get at that detail; new fields land on `Error` or on
/// per-variant payloads, never by removing or repurposing `code` or
/// `message`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Error {
    /// Stable, machine-readable failure category. Safe to match on.
    pub code: ErrorCode,
    /// Human-readable context for logs and diagnostics. Not part of the
    /// stability contract: never match on this field's contents.
    pub message: String,
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
    /// The quote passed to `send` is no longer valid because it expired.
    /// Obtain a fresh quote and retry.
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
    fn error_implements_std_error() {
        fn assert_std_error<E: core::error::Error>(_e: &E) {}
        let err = Error {
            code: ErrorCode::Internal,
            message: "boom".to_string(),
        };
        assert_std_error(&err);
    }
}
