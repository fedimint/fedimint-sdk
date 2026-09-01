//! The crate's single error type and its stable failure taxonomy.

use crate::{Amount, Network, Timestamp};

/// The one error type returned from every fallible call in this crate.
///
/// It has three fields, and they carry three different contracts. Being
/// precise about which is which is the whole of the error contract:
///
/// - [`code`](Error::code) is **the stable thing to branch on**. It is the
///   machine-readable failure category, and it alone is always enough to
///   decide what to do about a failure.
/// - [`details`](Error::details) is **the stable thing to read numbers
///   from**. Where a failure has structured detail — the balance that was
///   short, the networks that disagreed, the modules whose generations
///   conflicted, the total a quote moved to — that detail arrives here as a
///   typed [`ErrorDetails`] case. No caller ever has to parse `message` to
///   get at it.
/// - [`message`](Error::message) is **for humans, and only for humans**
///   (logs, error banners, bug reports). It is deliberately *not* part of
///   the stability contract, so it must never be parsed or matched on; its
///   wording can change in any release without that being a breaking
///   change.
///
/// The full underlying failure (the source chain from `fedimint-client`,
/// storage, or the network) is captured for diagnostics but stays internal to
/// the crate: it surfaces through logging and through [`Error`]'s `Debug`
/// output once an implementation exists behind this skeleton, never through
/// a public accessor. This keeps the public error surface small and stable
/// even as the internals it wraps change.
///
/// # The details envelope
///
/// Structured detail grows over time, and the shape it grows in is fixed
/// now, before the surface freezes: it arrives as a **new case on
/// [`ErrorDetails`]**, carried in the `details` field that exists from day
/// one. It does *not* arrive as a new field on `Error`, and it does not
/// arrive as a new field on an existing `ErrorDetails` case.
///
/// That is a deliberate correction of an earlier plan to defer detail into
/// later fields on `Error`. `Error` is `#[non_exhaustive]`, which makes
/// adding a field non-breaking *for Rust callers* — but this crate is also
/// the single surface the Swift, Kotlin and TypeScript SDKs are generated
/// from, and there a public struct is a generated record. Growing a record
/// is not safely additive across all three targets at once: a pre-generated
/// binding pinned to an older SDK decodes a record it was generated against,
/// and a producer that added a field to it is a producer it can no longer
/// read. Adding a *case* to a data enum is additive in a way growing a
/// record is not, because every target already has to handle a case it does
/// not know (see rule 2 below). So the envelope is reserved up front, empty
/// where there is nothing to say, and later versions fill it in without any
/// record in any language changing shape.
///
/// Three rules govern it, and all three are part of the stability contract:
///
/// 1. **`code` is authoritative; `details` only sharpens it.** `details` is
///    always `Option`, and `None` never means the error is less real — it
///    means this failure had no numbers worth reporting, or the layer that
///    raised it had none to hand. `details` therefore never holds the only
///    copy of something a caller must act on: a caller that ignores
///    `details` entirely still branches correctly on `code`. What `details`
///    buys is what a caller can *show* — "you need 1,500 msat and have
///    1,200" instead of "insufficient balance".
/// 2. **An unrecognized detail is a value, not a failure.** A binding built
///    against an older SDK that meets a detail case it has no vocabulary for
///    maps it onto [`ErrorDetails::Unrecognized`]. It is neither dropped
///    (the caller can still see that a detail was attached, and log the
///    fact) nor fatal (nothing crashes, nothing is misread as a different
///    case), and `code` and `message` are unaffected — they still describe
///    the failure completely and correctly.
/// 3. **A case's meaning never drifts.** Each case's meaning is fixed in the
///    envelope version that introduced it and is never redefined;
///    [`ErrorDetails::version`] reports that version, and
///    [`ErrorDetails::CURRENT_VERSION`] is the newest version this build
///    speaks. Reinterpreting a situation means adding a new case at a new
///    version, leaving the old case meaning exactly what it always meant.
///
/// Detail never arrives as a payload on [`ErrorCode`] either: that enum is a
/// fieldless `Copy` enum so it maps onto a plain Swift, Kotlin, or TypeScript
/// enum, and giving a variant a payload would break both `Copy` and every
/// unit-pattern match. And no addition ever removes or repurposes `code`,
/// `details`, or `message`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Error {
    /// Stable, machine-readable failure category. Safe to match on, and the
    /// only field a caller *has* to look at.
    pub code: ErrorCode,
    /// Human-readable context for logs and diagnostics. Not part of the
    /// stability contract: never match on this field's contents.
    pub message: String,
    /// Structured, machine-readable detail for this failure, where it has
    /// any: the numbers a caller would otherwise have had to scrape out of
    /// `message`.
    ///
    /// `None` means no detail was attached — see rule 1 on the type. Match
    /// it with a wildcard arm: [`ErrorDetails`] is `#[non_exhaustive]` and
    /// the case attached to a given [`ErrorCode`] may become more specific
    /// in a later release.
    pub details: Option<ErrorDetails>,
}

impl Error {
    /// Builds an SDK error from a code and a human-readable message, with no
    /// structured detail.
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
    /// Use [`Error::with_details`] where the layer has the numbers to go
    /// with the code; that is strictly better than putting them in
    /// `message`, because `message` is the one thing no caller may read
    /// programmatically.
    ///
    /// `Error` is `#[non_exhaustive]`, so this constructor, rather than a
    /// struct literal, is also the only way to build one from another crate.
    /// Fields added in later releases get sensible defaults here, which is
    /// what keeps such an addition non-breaking.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Error {
        Error {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Builds an SDK error from a code, a human-readable message, and the
    /// structured [`ErrorDetails`] for it.
    ///
    /// The same constructor as [`Error::new`] and for the same audience —
    /// including the out-of-crate binding layers — for the case where the
    /// numbers behind the failure are known. A decoder in a binding layer
    /// also uses this to rebuild an error that crossed a boundary, mapping a
    /// detail case it does not recognize onto
    /// [`ErrorDetails::Unrecognized`] rather than dropping it.
    ///
    /// `details` should describe the same failure as `code`; the pairing
    /// documented on each [`ErrorDetails`] case is the intended one. Nothing
    /// enforces it, because `code` remains authoritative either way: a
    /// caller branches on `code` and reads `details` only to enrich what it
    /// shows.
    pub fn with_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: ErrorDetails,
    ) -> Error {
        Error {
            code,
            message: message.into(),
            details: Some(details),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl core::error::Error for Error {}

/// Structured, machine-readable detail attached to an [`Error`].
///
/// This is the envelope described under [`Error`]'s *details envelope*
/// section: the reserved place where the numbers behind a failure are
/// reported, so that they are never available only by parsing
/// [`Error::message`]. Each case names the failure it accompanies and
/// carries exactly the values a caller needs to render or act on it.
///
/// # Shape, and why it is this shape
///
/// A flat data enum of plain records: no generics, no tuple variants, no
/// borrowed data, no trait objects, and no nesting beyond a list of a small
/// record. That is the intersection of what UniFFI and wasm can both express
/// directly, so this type generates into a Swift or Kotlin sealed
/// enum-with-associated-values and a TypeScript discriminated union
/// mechanically, with no per-target hand-written adapter.
///
/// The enum is `#[non_exhaustive]`; its **variants deliberately are not**.
/// Rust callers must therefore write a wildcard arm, while a binding layer
/// can still *construct* any case it needs (which
/// [`Error::with_details`] exists for). The asymmetry is the point: a case
/// may be added, but a case that exists never grows a field, because a
/// generated record that grows a field is exactly the thing that is not
/// safely additive across Swift, Kotlin and TypeScript at once. More detail
/// about an existing situation therefore arrives as a *new, more specific
/// case* at a new envelope version, not as an extra field here.
///
/// # Versioning
///
/// [`ErrorDetails::CURRENT_VERSION`] is the envelope version this build
/// speaks, and [`ErrorDetails::version`] reports the version a particular
/// detail belongs to. A case's meaning is frozen at the version that
/// introduced it: it is never redefined, never given a wider or narrower
/// reading, and never repurposed. Adding cases bumps
/// `CURRENT_VERSION`; nothing else does.
///
/// Two sides that disagree compare those two numbers. A producer speaking a
/// version the consumer does not know may emit a case the consumer has no
/// vocabulary for, and the consumer maps it onto
/// [`Unrecognized`](ErrorDetails::Unrecognized) — which is why a version
/// mismatch degrades to "there is a detail here I cannot interpret" rather
/// than to a crash, a silent drop, or a case read as the wrong case.
///
/// # What must never go in here
///
/// Details are diagnostics a caller may display, so they carry no secrets:
/// no seed or mnemonic material, no invite-code API secret, no preimage that
/// has not already settled. A value that a caller must not log has no
/// business in a type whose purpose is to be logged and shown.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorDetails {
    /// The spendable balance was short. Accompanies
    /// [`ErrorCode::InsufficientBalance`].
    ///
    /// Both amounts are millisatoshi [`Amount`]s and both are needed: a UI
    /// that only knows "not enough" cannot tell the user how much to top up
    /// by, and computing `required - available` is the caller's to do (with
    /// [`Amount::checked_sub`]) rather than a third field that could
    /// disagree with the first two.
    InsufficientBalance {
        /// What the operation needed in total, including any fee already
        /// quoted for it.
        required: Amount,
        /// What was actually spendable when the check ran.
        available: Amount,
    },
    /// A value's Bitcoin network disagreed with the federation's.
    /// Accompanies [`ErrorCode::NetworkMismatch`].
    NetworkMismatch {
        /// The network the federation operates on — the one a value had to
        /// match.
        expected: Network,
        /// The network of the value that was passed in (typically an
        /// [`Address`](crate::Address)).
        actual: Network,
    },
    /// A federation runs modules of more than one generation, which this SDK
    /// refuses to operate on. Accompanies
    /// [`ErrorCode::UnsupportedFederation`].
    ///
    /// Carries every module that takes part in the conflict together with
    /// the generation each declares, so diagnostics can name them without
    /// parsing [`Error::message`] — the requirement that made this envelope
    /// necessary in the first place. There are always at least two entries,
    /// since a single module cannot conflict with itself, and the list is
    /// not necessarily the federation's full module set: modules that agree
    /// with the majority may be omitted.
    ///
    /// This is one case of [`ErrorCode::UnsupportedFederation`], not the
    /// whole of it. That code also covers configurations the SDK refuses
    /// for other reasons, which may gain cases of their own later, so match
    /// on `details` with a wildcard arm and treat `code` as the category.
    MixedModuleGenerations {
        /// The conflicting modules and the generation each declares.
        modules: Vec<ModuleGeneration>,
    },
    /// A quote can no longer be executed because its life is over — the
    /// validity window lapsed, or it had already been spent on a payment.
    /// Accompanies [`ErrorCode::QuoteExpired`].
    ///
    /// Distinguishing this case from
    /// [`QuoteTermsChanged`](ErrorDetails::QuoteTermsChanged) is what lets a
    /// UI say "that quote timed out, here is a fresh one" rather than "the
    /// price moved" — different sentences for a user, even though the remedy
    /// (re-quote) is the same for the program.
    QuoteExpired {
        /// When the quote's validity window ended, as reported by
        /// [`LnQuote::expires_at`](crate::LnQuote::expires_at) or its
        /// on-chain equivalent.
        expires_at: Timestamp,
        /// `true` when the quote was refused because it had already been
        /// executed, rather than because its window lapsed.
        ///
        /// This is the sub-case a binding layer raises when a quote object
        /// crosses the boundary and is used a second time: Rust's move
        /// semantics prevent it outright, a foreign language has to refuse
        /// it at runtime. Both sub-cases share
        /// [`ErrorCode::QuoteExpired`] because the remedy is identical; the
        /// flag exists so a UI can be honest about which happened, and can
        /// avoid telling someone their payment timed out when in fact it
        /// already went through.
        already_executed: bool,
    },
    /// A quote's terms moved after it was issued, so executing it would not
    /// charge what the user approved. Accompanies
    /// [`ErrorCode::QuoteChanged`].
    ///
    /// Both totals are the number a caller shows as "you will pay" — the
    /// full debit, amount plus fee — so a UI can say exactly what changed
    /// instead of only that something did.
    QuoteTermsChanged {
        /// The total debit the expired plan promised: what the user was
        /// shown and approved.
        quoted_total: Amount,
        /// The total debit the same payment would cost now.
        current_total: Amount,
    },
    /// A federation was asked to be permanently forgotten while spendable
    /// balance remained in it. Accompanies [`ErrorCode::BalanceNotEmpty`].
    BalanceNotEmpty {
        /// The spendable balance still held in the federation. A caller
        /// needs this to tell the user what to move out first, and it is not
        /// otherwise available from the failed call.
        remaining: Amount,
    },
    /// A storage location was already open, in this process or another.
    /// Accompanies [`ErrorCode::StorageInUse`].
    StorageInUse {
        /// The location that could not be locked, as it was given to
        /// [`Storage::at`](crate::Storage::at). Echoed back so that a host
        /// juggling more than one location — a mobile app and its
        /// notification-service extension, say — can report which one is
        /// held. A path, never a credential.
        location: String,
    },
    /// Storage already held a seed and it did not match the mnemonic
    /// supplied to open it. Accompanies [`ErrorCode::SeedMismatch`].
    ///
    /// Carries the location and nothing else. No seed, no mnemonic, no
    /// fingerprint or hash of either: this is an error a host will log, and
    /// nothing derived from key material may be in it. The existing storage
    /// is untouched, so the remedy is to open it with the right mnemonic (or
    /// none) rather than to compare seeds.
    SeedMismatch {
        /// The storage location whose seed disagrees, as it was given to
        /// [`Storage::at`](crate::Storage::at).
        location: String,
    },
    /// A detail this build has no vocabulary for.
    ///
    /// This is the graceful-degradation case, and the reason a details
    /// envelope can be filled in later without breaking anything already
    /// generated. A consumer built against an older SDK that meets a case it
    /// does not know maps it here: [`Error::code`] and [`Error::message`]
    /// still describe the failure completely, and the fact that a detail was
    /// present is still observable — worth logging, and worth reporting in a
    /// bug report, because it says the producer knew more than this build
    /// can express.
    ///
    /// The one thing not to do with it is treat it as an error of its own.
    /// It is an ordinary value, and a caller that has already branched on
    /// `code` has everything it needs.
    ///
    /// A Rust caller compiled against this crate never sees this case for a
    /// detail produced *inside* the crate: the enum it compiles against and
    /// the enum the crate populates are the same one. It appears where a
    /// decoder sits between a producer and a consumer of different vintages
    /// — which is every generated binding.
    Unrecognized {
        /// The envelope version the producing side declared it was
        /// speaking, or `0` where it declared none.
        ///
        /// `0` is reserved for "unstated" and is never a real envelope
        /// version, so a decoder always has something honest to put here.
        /// Compare it against [`ErrorDetails::CURRENT_VERSION`] to see how
        /// far ahead the producer is.
        version: u32,
        /// A short, opaque label for the case that could not be
        /// interpreted, for logs and bug reports.
        ///
        /// Diagnostic only, with exactly the same standing as
        /// [`Error::message`]: not part of the stability contract, never to
        /// be parsed, and never to be matched on to recover the case a
        /// consumer could not decode. A consumer that does not even have a
        /// label for it uses an empty string.
        kind: String,
    },
}

impl ErrorDetails {
    /// The details-envelope version this build speaks.
    ///
    /// Bumped when cases are added, and never for any other reason — a case
    /// that exists neither changes meaning nor grows fields, so nothing else
    /// can change what this number means. `0` is not a version: it is
    /// reserved for "the producer declared none", as carried by
    /// [`Unrecognized`](ErrorDetails::Unrecognized).
    pub const CURRENT_VERSION: u32 = 1;

    /// The envelope version this particular detail belongs to.
    ///
    /// For a case this build knows, that is the version whose vocabulary the
    /// case was introduced in and whose meaning it is frozen at — never
    /// later than [`ErrorDetails::CURRENT_VERSION`]. For
    /// [`Unrecognized`](ErrorDetails::Unrecognized) it is the version the
    /// producing side declared (or `0`), which is what makes "this came from
    /// something newer than me" a thing a consumer can state precisely
    /// rather than guess at.
    ///
    /// A binding layer exposes this as a generated helper rather than a
    /// method, since a foreign enum carries no methods of its own; the
    /// mapping is mechanical either way.
    pub fn version(&self) -> u32 {
        match self {
            // Version 1: the cases the envelope shipped with.
            ErrorDetails::InsufficientBalance { .. }
            | ErrorDetails::NetworkMismatch { .. }
            | ErrorDetails::MixedModuleGenerations { .. }
            | ErrorDetails::QuoteExpired { .. }
            | ErrorDetails::QuoteTermsChanged { .. }
            | ErrorDetails::BalanceNotEmpty { .. }
            | ErrorDetails::StorageInUse { .. }
            | ErrorDetails::SeedMismatch { .. } => 1,
            ErrorDetails::Unrecognized { version, .. } => *version,
        }
    }

    /// Whether this detail is one this build could not interpret, i.e. the
    /// [`Unrecognized`](ErrorDetails::Unrecognized) case.
    ///
    /// Useful for the one thing a consumer should do about such a detail:
    /// log it, and otherwise carry on with [`Error::code`].
    pub fn is_unrecognized(&self) -> bool {
        matches!(self, ErrorDetails::Unrecognized { .. })
    }
}

/// One module of a federation, and the generation it declares.
///
/// Carried in lists by
/// [`ErrorDetails::MixedModuleGenerations`](ErrorDetails::MixedModuleGenerations)
/// so that a mixed-generation federation can be diagnosed by naming the
/// modules that disagree, which is not something a caller should have to
/// recover from an error message.
///
/// `#[non_exhaustive]`, so build one with [`ModuleGeneration::new`] rather
/// than a struct literal. As a generated record it does not grow fields for
/// the reason set out on [`ErrorDetails`]: more to say about a module
/// arrives as a new details case, not as another field here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ModuleGeneration {
    /// The module's kind name, spelled as it is in
    /// [`FederationPreview::modules`](crate::FederationPreview::modules) —
    /// for example `"mint"`, `"ln"`, `"wallet"`. Not restricted to the
    /// modules this SDK exposes a facade for: the generation rule covers
    /// every module a federation runs.
    pub kind: String,
    /// The generation this module declares: `1` for v1, `2` for v2.
    ///
    /// A plain integer rather than an enum, deliberately. The failure being
    /// reported is precisely that an unexpected set of generations turned
    /// up, and a federation declaring a generation this SDK has never heard
    /// of is exactly the case worth reporting faithfully rather than
    /// flattening into an "unknown" variant.
    pub generation: u32,
}

impl ModuleGeneration {
    /// Records a module kind name and the generation it declares.
    pub fn new(kind: impl Into<String>, generation: u32) -> ModuleGeneration {
        ModuleGeneration {
            kind: kind.into(),
            generation,
        }
    }
}

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
    /// to operate on. For the mixed-generation case,
    /// [`ErrorDetails::MixedModuleGenerations`] names the conflicting
    /// modules and the generation each declares.
    UnsupportedFederation,
    /// No guardian could be reached to service the request.
    FederationUnreachable,
    /// The spendable balance is too low to cover the requested amount.
    /// [`ErrorDetails::InsufficientBalance`] carries the required and
    /// available amounts.
    InsufficientBalance,
    /// A request to permanently forget a federation was made while spendable
    /// balance still remains in it. [`ErrorDetails::BalanceNotEmpty`]
    /// carries how much.
    BalanceNotEmpty,
    /// A request to permanently forget a federation was made while
    /// non-final operations or reclaimable outgoing value still exist for
    /// it.
    ///
    /// An incomplete recovery is deliberately *not* one of those reasons:
    /// erasing the federation is the only way out of a recovery that cannot
    /// be finished, so this code is never returned on that account.
    PendingOperations,
    /// No usable lightning gateway is currently available.
    GatewayUnavailable,
    /// The requested action is unavailable because this federation's
    /// recovery is incomplete.
    ///
    /// Incomplete is not the same as still running: a recovery that stopped
    /// short leaves the lock in place, because a wallet restored only partly
    /// must not be spendable. Only a recovery that runs to completion
    /// releases it.
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
    /// supplied to open it. [`ErrorDetails::SeedMismatch`] names the
    /// storage location.
    SeedMismatch,
    /// The storage location is already open, in this process or another.
    /// [`ErrorDetails::StorageInUse`] names the location.
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
    ///
    /// [`ErrorDetails::QuoteExpired`] carries the validity window that
    /// lapsed and says which of the two sub-cases occurred, for a UI that
    /// wants to phrase them differently.
    QuoteExpired,
    /// Conditions material to the quote (fees, routing, federation state)
    /// changed since it was issued. Obtain a fresh quote and retry.
    /// [`ErrorDetails::QuoteTermsChanged`] carries the total debit that was
    /// quoted and the total it moved to.
    QuoteChanged,
    /// The bolt11 invoice specifies no amount, and such an invoice cannot be
    /// paid.
    ///
    /// This is **not** a request for the caller to supply an amount, and no
    /// amount the caller supplies can make the invoice payable. Fedimint
    /// does not support paying amountless bolt11 invoices: that is a
    /// deliberate and permanent upstream limitation, confirmed as one that
    /// cannot be implemented safely, rather than a gap in this SDK that a
    /// later release will fill.
    ///
    /// The only remedy is a different invoice — one that names its own
    /// amount. An application taking an invoice from a QR code or a paste
    /// buffer should say so plainly ("this invoice does not specify an
    /// amount and cannot be paid here") instead of prompting for a number it
    /// cannot use.
    AmountlessInvoice,
    /// A supplied address or invoice is for a different network than the
    /// federation's.
    /// [`ErrorDetails::NetworkMismatch`] carries both networks.
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

    /// One sample of every case this build knows, for the checks that must
    /// hold across all of them.
    fn every_known_detail() -> Vec<ErrorDetails> {
        vec![
            ErrorDetails::InsufficientBalance {
                required: Amount::from_msats(1_500),
                available: Amount::from_msats(1_200),
            },
            ErrorDetails::NetworkMismatch {
                expected: Network::Bitcoin,
                actual: Network::Signet,
            },
            ErrorDetails::MixedModuleGenerations {
                modules: vec![
                    ModuleGeneration::new("mint", 1),
                    ModuleGeneration::new("ln", 2),
                ],
            },
            ErrorDetails::QuoteExpired {
                expires_at: Timestamp::from_epoch_millis(1_700_000_000_000),
                already_executed: false,
            },
            ErrorDetails::QuoteTermsChanged {
                quoted_total: Amount::from_msats(101_000),
                current_total: Amount::from_msats(103_500),
            },
            ErrorDetails::BalanceNotEmpty {
                remaining: Amount::from_msats(7_000),
            },
            ErrorDetails::StorageInUse {
                location: "/var/app/wallet".to_owned(),
            },
            ErrorDetails::SeedMismatch {
                location: "/var/app/wallet".to_owned(),
            },
        ]
    }

    #[test]
    fn error_display_formats_as_code_then_message() {
        let err = Error {
            code: ErrorCode::InsufficientBalance,
            message: "need 1000 msat, have 500 msat".to_string(),
            details: None,
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
    fn new_attaches_no_details() {
        // `None` is the no-detail case, and it stays available: adding the
        // envelope did not make every error carry one.
        assert!(Error::new(ErrorCode::Timeout, "gave up").details.is_none());
    }

    #[test]
    fn error_implements_std_error() {
        fn assert_std_error<E: core::error::Error>(_e: &E) {}
        let err = Error {
            code: ErrorCode::Internal,
            message: "boom".to_string(),
            details: None,
        };
        assert_std_error(&err);
    }

    #[test]
    fn with_details_carries_insufficient_balance_amounts() {
        let err = Error::with_details(
            ErrorCode::InsufficientBalance,
            "balance is short",
            ErrorDetails::InsufficientBalance {
                required: Amount::from_msats(1_500),
                available: Amount::from_msats(1_200),
            },
        );
        assert_eq!(err.code, ErrorCode::InsufficientBalance);
        match err.details {
            Some(ErrorDetails::InsufficientBalance {
                required,
                available,
            }) => {
                assert_eq!(required, Amount::from_msats(1_500));
                assert_eq!(available, Amount::from_msats(1_200));
                // The shortfall a UI wants is the caller's subtraction, and
                // it is exact rather than a third field that could drift.
                assert_eq!(
                    required.checked_sub(available),
                    Some(Amount::from_msats(300))
                );
            }
            other => panic!("expected an InsufficientBalance detail, got {other:?}"),
        }
    }

    #[test]
    fn with_details_carries_both_networks() {
        let err = Error::with_details(
            ErrorCode::NetworkMismatch,
            "wrong network",
            ErrorDetails::NetworkMismatch {
                expected: Network::Bitcoin,
                actual: Network::Testnet4,
            },
        );
        match err.details {
            Some(ErrorDetails::NetworkMismatch { expected, actual }) => {
                assert_eq!(expected, Network::Bitcoin);
                assert_eq!(actual, Network::Testnet4);
            }
            other => panic!("expected a NetworkMismatch detail, got {other:?}"),
        }
    }

    #[test]
    fn with_details_names_the_conflicting_modules_and_generations() {
        let err = Error::with_details(
            ErrorCode::UnsupportedFederation,
            "mixed module generations",
            ErrorDetails::MixedModuleGenerations {
                modules: vec![
                    ModuleGeneration::new("mint", 1),
                    ModuleGeneration::new("ln", 2),
                    ModuleGeneration::new(String::from("wallet"), 2),
                ],
            },
        );
        assert_eq!(err.code, ErrorCode::UnsupportedFederation);
        match err.details {
            Some(ErrorDetails::MixedModuleGenerations { modules }) => {
                // The whole point of the case: the modules are readable
                // without touching `message`.
                let named: Vec<(&str, u32)> = modules
                    .iter()
                    .map(|m| (m.kind.as_str(), m.generation))
                    .collect();
                assert_eq!(named, vec![("mint", 1), ("ln", 2), ("wallet", 2)]);
                // A conflict needs at least two participants.
                assert!(modules.len() >= 2);
            }
            other => panic!("expected a MixedModuleGenerations detail, got {other:?}"),
        }
    }

    #[test]
    fn module_generation_new_records_kind_and_generation() {
        let module = ModuleGeneration::new("mint", 1);
        assert_eq!(module.kind, "mint");
        assert_eq!(module.generation, 1);
        // A generation this SDK has never heard of is reported faithfully
        // rather than flattened away.
        assert_eq!(ModuleGeneration::new("ln", 7).generation, 7);
    }

    #[test]
    fn quote_expired_detail_separates_a_lapsed_window_from_a_reused_quote() {
        let lapsed = Error::with_details(
            ErrorCode::QuoteExpired,
            "quote expired",
            ErrorDetails::QuoteExpired {
                expires_at: Timestamp::from_epoch_millis(1_700_000_000_000),
                already_executed: false,
            },
        );
        match lapsed.details {
            Some(ErrorDetails::QuoteExpired {
                expires_at,
                already_executed,
            }) => {
                assert_eq!(expires_at, Timestamp::from_epoch_millis(1_700_000_000_000));
                assert!(!already_executed);
            }
            other => panic!("expected a QuoteExpired detail, got {other:?}"),
        }

        // What a binding layer reports for a quote object used twice.
        let reused = Error::with_details(
            ErrorCode::QuoteExpired,
            "quote already executed",
            ErrorDetails::QuoteExpired {
                expires_at: Timestamp::from_epoch_millis(1_700_000_000_000),
                already_executed: true,
            },
        );
        match reused.details {
            Some(ErrorDetails::QuoteExpired {
                already_executed, ..
            }) => assert!(already_executed),
            other => panic!("expected a QuoteExpired detail, got {other:?}"),
        }
    }

    #[test]
    fn quote_terms_changed_detail_carries_the_old_and_new_total() {
        let err = Error::with_details(
            ErrorCode::QuoteChanged,
            "the fee moved",
            ErrorDetails::QuoteTermsChanged {
                quoted_total: Amount::from_msats(101_000),
                current_total: Amount::from_msats(103_500),
            },
        );
        assert_eq!(err.code, ErrorCode::QuoteChanged);
        match err.details {
            Some(ErrorDetails::QuoteTermsChanged {
                quoted_total,
                current_total,
            }) => {
                assert_eq!(quoted_total, Amount::from_msats(101_000));
                assert_eq!(current_total, Amount::from_msats(103_500));
            }
            other => panic!("expected a QuoteTermsChanged detail, got {other:?}"),
        }
    }

    #[test]
    fn balance_not_empty_detail_carries_what_is_left() {
        let err = Error::with_details(
            ErrorCode::BalanceNotEmpty,
            "still holding funds",
            ErrorDetails::BalanceNotEmpty {
                remaining: Amount::from_msats(7_000),
            },
        );
        match err.details {
            Some(ErrorDetails::BalanceNotEmpty { remaining }) => {
                assert_eq!(remaining, Amount::from_msats(7_000));
            }
            other => panic!("expected a BalanceNotEmpty detail, got {other:?}"),
        }
    }

    #[test]
    fn storage_details_carry_the_location() {
        let in_use = Error::with_details(
            ErrorCode::StorageInUse,
            "already open",
            ErrorDetails::StorageInUse {
                location: "/var/app/wallet".to_owned(),
            },
        );
        match in_use.details {
            Some(ErrorDetails::StorageInUse { location }) => {
                assert_eq!(location, "/var/app/wallet");
            }
            other => panic!("expected a StorageInUse detail, got {other:?}"),
        }

        let mismatch = Error::with_details(
            ErrorCode::SeedMismatch,
            "different seed",
            ErrorDetails::SeedMismatch {
                location: "/var/app/wallet".to_owned(),
            },
        );
        match mismatch.details {
            Some(ErrorDetails::SeedMismatch { location }) => {
                assert_eq!(location, "/var/app/wallet");
            }
            other => panic!("expected a SeedMismatch detail, got {other:?}"),
        }
    }

    #[test]
    fn every_known_detail_belongs_to_a_shipped_envelope_version() {
        for detail in every_known_detail() {
            let version = detail.version();
            assert!(
                version >= 1 && version <= ErrorDetails::CURRENT_VERSION,
                "{detail:?} reports version {version}, outside 1..={}",
                ErrorDetails::CURRENT_VERSION
            );
            assert!(!detail.is_unrecognized(), "{detail:?} is a known case");
        }
    }

    #[test]
    fn unrecognized_detail_leaves_code_and_message_intact() {
        // A binding built against an older SDK meeting a case it has no
        // vocabulary for: the failure is still fully described, and the
        // undecodable detail is observable rather than dropped or fatal.
        let err = Error::with_details(
            ErrorCode::InsufficientBalance,
            "balance is short",
            ErrorDetails::Unrecognized {
                version: 9,
                kind: "SomethingNewerThanThisBuild".to_owned(),
            },
        );

        // The code still branches, and the message still reads.
        assert_eq!(err.code, ErrorCode::InsufficientBalance);
        assert_eq!(err.message, "balance is short");
        assert_eq!(
            err.to_string(),
            "InsufficientBalance: balance is short",
            "an unrecognized detail must not change how an error renders"
        );

        let detail = err.details.expect("the detail is preserved, not dropped");
        assert!(detail.is_unrecognized());
        // The producer's declared version is readable, so "this came from
        // something newer than me" is a statement rather than a guess.
        assert_eq!(detail.version(), 9);
        assert!(detail.version() > ErrorDetails::CURRENT_VERSION);
        match detail {
            ErrorDetails::Unrecognized { version, kind } => {
                assert_eq!(version, 9);
                assert_eq!(kind, "SomethingNewerThanThisBuild");
            }
            other => panic!("expected an Unrecognized detail, got {other:?}"),
        }
    }

    #[test]
    fn unrecognized_detail_accepts_an_undeclared_version_and_no_label() {
        // A decoder that cannot tell what the producer was speaking, and has
        // no label for the case, still has something honest to record: `0`
        // is reserved for "unstated" and is never a real envelope version.
        let detail = ErrorDetails::Unrecognized {
            version: 0,
            kind: String::new(),
        };
        assert!(detail.is_unrecognized());
        assert_eq!(detail.version(), 0);
        assert_ne!(0, ErrorDetails::CURRENT_VERSION);
    }

    #[test]
    fn details_are_matchable_with_a_wildcard_arm() {
        // How a forward-compatible caller reads the envelope: branch on
        // `code`, enrich from `details`, and fall through for anything else.
        fn shortfall(err: &Error) -> Option<Amount> {
            match &err.details {
                Some(ErrorDetails::InsufficientBalance {
                    required,
                    available,
                }) => required.checked_sub(*available),
                _ => None,
            }
        }

        let detailed = Error::with_details(
            ErrorCode::InsufficientBalance,
            "balance is short",
            ErrorDetails::InsufficientBalance {
                required: Amount::from_msats(1_500),
                available: Amount::from_msats(1_200),
            },
        );
        assert_eq!(shortfall(&detailed), Some(Amount::from_msats(300)));

        // The same caller, unchanged, on an error whose detail it cannot
        // interpret and on one with no detail at all.
        let unknown = Error::with_details(
            ErrorCode::InsufficientBalance,
            "balance is short",
            ErrorDetails::Unrecognized {
                version: 9,
                kind: String::new(),
            },
        );
        assert_eq!(shortfall(&unknown), None);
        assert_eq!(
            shortfall(&Error::new(ErrorCode::InsufficientBalance, "short")),
            None
        );
    }
}
