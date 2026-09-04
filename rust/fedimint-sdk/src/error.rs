//! The crate's single error type and its stable failure taxonomy.

use crate::{Amount, Network, Timestamp};

/// The one error type returned from every fallible call in this crate.
///
/// It has three fields, and each has a different job:
///
/// - [`code`](Error::code) is the stable, machine-readable failure category. It is always
///   enough on its own to decide what to do about a failure.
/// - [`details`](Error::details) carries structured detail where a failure has any: the
///   balance that was short, the networks that disagreed, the modules whose generations
///   conflicted, and so on. [`Error::detail`] is the short path to the typed
///   [`ErrorDetails`] case; no caller has to parse `message` to get at these numbers.
/// - [`message`](Error::message) is for humans only (logs, error banners, bug reports). It
///   is not part of the stability contract and must never be parsed or matched on.
///
/// The same three fields, with the same meanings, also travel as [`Diagnostic`]: a value for
/// when a failure is reported as state rather than raised, such as why a federation sits in
/// [`FederationStatus::Quarantined`](crate::FederationStatus::Quarantined).
/// [`Error`] and [`Diagnostic`] convert both ways.
///
/// # The details envelope
///
/// New structured detail always arrives as a new kind inside the `details` envelope, never
/// as a new field on `Error` and never as a new field on an existing [`ErrorDetails`] case.
/// See [`RawErrorDetails`] for the wire form the envelope takes crossing a language boundary.
/// Three rules govern it:
///
/// 1. **`code` is authoritative; `details` only sharpens it.** `details` is always
///    `Option`, and `None` never means the error is less real: it means this failure had no
///    numbers worth reporting. A caller that ignores `details` entirely still branches
///    correctly on `code`. What `details` buys is what a caller can *show*, such as "you
///    need 1,500 msat and have 1,200" instead of "insufficient balance".
/// 2. **An uninterpretable detail is a value, not a failure.** A caller that meets a `kind`
///    it has no vocabulary for keeps the raw envelope as [`DetailEnvelope::Opaque`]. Nothing
///    is dropped, nothing is fatal, and `code` and `message` still describe the failure
///    completely and correctly.
/// 3. **A kind's meaning and payload layout never drift.** [`ErrorDetails::version`] reports
///    the envelope version that introduced a case, and
///    [`RawErrorDetails::CURRENT_VERSION`] is the newest version this build speaks. A
///    reinterpreted situation is a new kind at a new version; the old kind keeps meaning
///    exactly what it always meant.
///
/// [`ErrorCode`] never carries detail either: it is a fieldless `Copy` enum, so it maps onto
/// a plain Swift, Kotlin, or TypeScript enum. No addition ever removes or repurposes `code`,
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
    /// `None` means no detail was attached; see rule 1 on the type. `Some`
    /// carries a [`DetailEnvelope`], which is either
    /// [`Interpreted`](DetailEnvelope::Interpreted) with the typed case or
    /// [`Opaque`](DetailEnvelope::Opaque) with the raw envelope this build
    /// could not project; its kind and version read the same either way. Most
    /// callers want [`Error::detail`], which goes straight to the typed case;
    /// match this field when telling the two states apart matters.
    ///
    /// Match the typed case with a wildcard arm: [`ErrorDetails`] is
    /// `#[non_exhaustive]` and the case attached to a given [`ErrorCode`] may
    /// become more specific in a later release.
    pub details: Option<DetailEnvelope>,
}

impl Error {
    /// Builds an SDK error from a code and a human-readable message, with no
    /// structured detail.
    ///
    /// Available to code outside this crate too, such as a binding or adapter layer, so a
    /// failure that happens there still reaches the application as an [`Error`] with an
    /// [`ErrorCode`] to branch on, exactly like a failure raised inside the SDK, rather than
    /// a parallel error type per platform.
    ///
    /// Pick the `code` that describes the failure from the caller's point of
    /// view; [`ErrorCode::Internal`] only where nothing else fits.
    /// `message` is for humans: it is not part of the stability contract and
    /// must never be parsed.
    ///
    /// Use [`Error::with_details`] where structured detail is available; that
    /// is strictly better than putting it in `message`, because `message` is
    /// the one thing no caller may read programmatically.
    ///
    /// `Error` is `#[non_exhaustive]`, so this constructor, rather than a
    /// struct literal, is also the only way to build one from another crate.
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
    /// `details` should describe the same failure as `code`; the pairing
    /// documented on each [`ErrorDetails`] case is the intended one. Nothing
    /// enforces it, because `code` remains authoritative either way: a
    /// caller branches on `code` and reads `details` only to enrich what it
    /// shows.
    ///
    /// This is the constructor for detail produced locally by this build. A
    /// decoder rebuilding an error that crossed a boundary should use
    /// [`Error::with_projected_details`] when it recognised the kind, or
    /// [`Error::with_raw_details`] when it did not, so the envelope records
    /// the version the far side actually declared.
    pub fn with_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: ErrorDetails,
    ) -> Error {
        Error::with_projected_details(code, message, details, RawErrorDetails::CURRENT_VERSION)
    }

    /// Constructs an error whose detail was projected off a received raw
    /// envelope, preserving the envelope version the producing side
    /// declared.
    ///
    /// Use this once a boundary decoder has read a [`RawErrorDetails`], recognised the kind
    /// and decoded the payload into `details`, passing `raw.version` through as
    /// `producer_version` so that "how far ahead is the other side" stays answerable even
    /// when interpretation succeeded. A locally-originated detail uses [`Error::with_details`]
    /// instead, which is this with [`CURRENT_VERSION`](RawErrorDetails::CURRENT_VERSION).
    pub fn with_projected_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: ErrorDetails,
        producer_version: u32,
    ) -> Error {
        Error {
            code,
            message: message.into(),
            details: Some(DetailEnvelope::Interpreted {
                detail: details,
                producer_version,
            }),
        }
    }

    /// Builds an SDK error from a code, a human-readable message, and a
    /// [`RawErrorDetails`] the caller could not project into a typed case.
    ///
    /// This is how a detail that could not be interpreted, such as a `kind` from a newer
    /// SDK, survives as an observable value with its version and kind intact instead of
    /// being dropped or turned into a failure of its own.
    pub fn with_raw_details(
        code: ErrorCode,
        message: impl Into<String>,
        raw: RawErrorDetails,
    ) -> Error {
        Error {
            code,
            message: message.into(),
            details: Some(DetailEnvelope::Opaque { raw }),
        }
    }

    /// The typed detail attached to this error, where there is one this build
    /// can interpret.
    ///
    /// `None` covers all three of "no detail was attached", "a detail was attached whose
    /// kind this build does not know", and "the payload did not decode". A caller that wants
    /// to tell those apart matches the [`details`](Error::details) field instead, whose
    /// [`Opaque`](DetailEnvelope::Opaque) case is exactly the second and third.
    ///
    /// ```
    /// use fedimint_sdk::{Amount, Error, ErrorCode, ErrorDetails};
    ///
    /// let err = Error::with_details(
    ///     ErrorCode::InsufficientBalance,
    ///     "balance is short",
    ///     ErrorDetails::InsufficientBalance {
    ///         required: Amount::from_msats(1_500),
    ///         available: Amount::from_msats(1_200),
    ///     },
    /// );
    /// let shortfall = match err.detail() {
    ///     Some(ErrorDetails::InsufficientBalance { required, available }) => {
    ///         required.checked_sub(*available)
    ///     }
    ///     _ => None,
    /// };
    /// assert_eq!(shortfall, Some(Amount::from_msats(300)));
    /// ```
    pub fn detail(&self) -> Option<&ErrorDetails> {
        self.details.as_ref()?.typed()
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl core::error::Error for Error {}

/// A failure described as a **value to read** rather than an error to raise: a
/// stable [`ErrorCode`], a human-readable message, and the same optional
/// [`DetailEnvelope`] an [`Error`] carries.
///
/// This is the type a state that means "something went wrong here" holds. The
/// case it exists for today is
/// [`FederationStatus::Quarantined`](crate::FederationStatus::Quarantined):
/// the SDK could not or would not open a stored federation, and rather than
/// failing the whole instance it records why, for an application to read later
/// with [`Sdk::federation_status`](crate::Sdk::federation_status) or
/// [`Sdk::stored_federations`](crate::Sdk::stored_federations). The same shape
/// fits any later status with a reason attached.
///
/// Unlike [`Error`], `Diagnostic` is `PartialEq` and `Eq`: a status is diffed to decide
/// whether anything changed, which is what
/// [`Sdk::federation_status_updates`](crate::Sdk::federation_status_updates) emits on.
/// It also does not implement [`core::error::Error`], since a value sitting in a status
/// field is not an in-flight failure. Convert between the two with `Error::from(diagnostic)`
/// (or `.into()`), and `Diagnostic::from(error)` the other way, which is how the SDK records
/// the error that stopped a federation opening. A diagnosis can outlive the call that
/// produced it: it may have been decided by an earlier build, in an earlier process, and
/// read back long after.
///
/// A plain record of a fieldless [`Copy`] enum, a string, and an optional envelope: it
/// generates mechanically into a Swift or Kotlin record and a TypeScript interface, and a
/// binding decodes its `details` field with the same projection it uses for
/// [`Error::details`]. Like [`Error`], this record does not grow fields: more structured
/// detail about a diagnosed situation arrives as a new kind inside the envelope. It is
/// `#[non_exhaustive]`; build one with [`Diagnostic::new`], [`Diagnostic::with_details`] or
/// [`Diagnostic::with_raw_details`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// Stable, machine-readable failure category: the same taxonomy, with
    /// the same meanings, as [`Error::code`]. Safe to match on, and the only
    /// field a caller *has* to look at.
    pub code: ErrorCode,
    /// Human-readable context for logs and diagnostics. Not part of the
    /// stability contract: never match on this field's contents.
    pub message: String,
    /// Structured, machine-readable detail for this failure, where it has
    /// any, identical in meaning to [`Error::details`].
    ///
    /// Without it, an application that wanted to name the modules whose generations
    /// conflict in a quarantined federation would have to parse `message`, which the
    /// contract forbids; with it, the quarantine carries
    /// [`ErrorDetails::MixedModuleGenerations`] and the modules are readable.
    ///
    /// `None` means no detail was attached, and never that the diagnosis is
    /// less real: `code` is authoritative on its own. An envelope written by a newer SDK
    /// than the one reading it stays a value, not a failure. Most callers want
    /// [`Diagnostic::detail`]; match the typed case with a wildcard arm.
    pub details: Option<DetailEnvelope>,
}

impl Diagnostic {
    /// Records a diagnosis from a code and a human-readable message, with no
    /// structured detail.
    ///
    /// The counterpart of [`Error::new`]: pick the `code` that describes the situation from
    /// the reader's point of view; [`ErrorCode::Internal`] only where nothing else fits.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Records a diagnosis from a code, a human-readable message, and the
    /// structured [`ErrorDetails`] for it.
    ///
    /// The counterpart of [`Error::with_details`]: a federation refused for mixed module
    /// generations, for instance, is diagnosed with [`ErrorCode::UnsupportedFederation`] and
    /// [`ErrorDetails::MixedModuleGenerations`], so an application can name the modules that
    /// disagree instead of scraping them out of a sentence.
    ///
    /// `details` should describe the same situation as `code`; the pairing
    /// documented on each [`ErrorDetails`] case is the intended one. Nothing
    /// enforces it, because `code` stays authoritative either way.
    ///
    /// This is the local form, for detail produced by this build; a decoder projecting a
    /// received raw envelope should use [`Diagnostic::with_projected_details`] instead, to
    /// preserve the version the far side declared.
    pub fn with_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: ErrorDetails,
    ) -> Diagnostic {
        Diagnostic::with_projected_details(code, message, details, RawErrorDetails::CURRENT_VERSION)
    }

    /// Builds a diagnostic whose detail was decoded off a received raw envelope, carrying
    /// `producer_version` through from [`RawErrorDetails::version`], exactly as
    /// [`Error::with_projected_details`] does for an error.
    pub fn with_projected_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: ErrorDetails,
        producer_version: u32,
    ) -> Diagnostic {
        Diagnostic {
            code,
            message: message.into(),
            details: Some(DetailEnvelope::Interpreted {
                detail: details,
                producer_version,
            }),
        }
    }

    /// Records a diagnosis from a code, a human-readable message, and a
    /// [`RawErrorDetails`] this side could not project into a typed case.
    ///
    /// The counterpart of [`Error::with_raw_details`]: the detail survives as an observable
    /// value with a version and a kind, rather than being dropped or turned into a failure
    /// of its own.
    pub fn with_raw_details(
        code: ErrorCode,
        message: impl Into<String>,
        raw: RawErrorDetails,
    ) -> Diagnostic {
        Diagnostic {
            code,
            message: message.into(),
            details: Some(DetailEnvelope::Opaque { raw }),
        }
    }

    /// The typed detail attached to this diagnosis, where there is one this
    /// build can interpret.
    ///
    /// Exactly [`Error::detail`] on the other type: `None` covers "no detail
    /// was attached", "the kind is unknown to this build", and "the payload did
    /// not decode", and the [`details`](Diagnostic::details) field tells those
    /// apart where the difference matters.
    pub fn detail(&self) -> Option<&ErrorDetails> {
        self.details.as_ref()?.typed()
    }
}

impl core::fmt::Display for Diagnostic {
    /// Formats as `Code: message`, the same as [`Error`], so a log line reads
    /// identically whether the failure was raised or read off a status.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl From<Error> for Diagnostic {
    /// Records a failure that already happened as the diagnosis of a state:
    /// the code, message and details envelope carry over unchanged, and the
    /// error's internal source chain, which is not part of the public
    /// surface, is dropped, since a status value is compared and stored
    /// rather than propagated.
    fn from(error: Error) -> Diagnostic {
        Diagnostic {
            code: error.code,
            message: error.message,
            details: error.details,
        }
    }
}

impl From<Diagnostic> for Error {
    /// Raises a recorded diagnosis as an error, unchanged: the code, message
    /// and details envelope carry over, and the result is an ordinary
    /// [`Error`] that a caller can return or a binding can throw. Nothing is
    /// added, in particular no source chain, because there is no live failure
    /// underneath a diagnosis that was read back from a status.
    fn from(diagnostic: Diagnostic) -> Error {
        Error {
            code: diagnostic.code,
            message: diagnostic.message,
            details: diagnostic.details,
        }
    }
}

/// The raw, length-delimited form of an error's structured detail: the only
/// form that ever crosses a language boundary.
///
/// A version, a kind, and an opaque payload. That is the whole record, and it never grows
/// another field: a frozen record whose only variable part is a byte string with its length
/// in front is the one shape a reader of *any* vintage can consume completely, even one that
/// has never heard of a given kind. Everything that varies is inside
/// [`payload`](RawErrorDetails::payload), and everything in it is skippable by its length.
///
/// It is deliberately **not** `#[non_exhaustive]`, unlike every other public
/// struct in this crate: its fields are frozen for good, so a binding layer can destructure
/// the record exhaustively, secure that a later release cannot add a field it would then
/// silently ignore. [`RawErrorDetails::new`] exists for convenience, not
/// because a struct literal is unavailable.
///
/// # The encoding contract
///
/// This crate has no serialization dependency and will not grow one, so the
/// payload is opaque bytes here and the encoding is a *documented contract*
/// that each boundary implements. Framing the record's three fields is the
/// boundary's own business, a native record, a JSON object, anything, subject
/// to one requirement:
///
/// > `version` and the *length* of `kind` and `payload` are readable without
/// > interpreting either, so `payload` can be consumed or skipped by its
/// > length by a reader that has never heard of its kind.
///
/// ## Kinds
///
/// `kind` is a stable ASCII identifier, spelled exactly as the
/// [`ErrorDetails`] variant it projects to: `"InsufficientBalance"`,
/// `"NetworkMismatch"`, and so on; [`ErrorDetails::kind`] is the mapping.
/// Unlike [`Error::message`], these strings **are** part of the stability
/// contract: they are the discriminator, so one is never renamed, never
/// reused for a different meaning, and never case-shifted. A kind a reader
/// does not know is not an error: it skips the payload and keeps the
/// envelope as [`DetailEnvelope::Opaque`].
///
/// ## Primitives inside the payload
///
/// | Form | Encoding |
/// |------|----------|
/// | `u32`, `u64` | big-endian, 4 or 8 bytes, unframed |
/// | `bool` | one byte, `0` or `1`; any other value makes the payload uninterpretable |
/// | `str`, `bytes` | a `u32` big-endian byte length, then that many bytes; `str` is UTF-8 |
/// | `list<T>` | a `u32` big-endian element count, then that many `T` encodings |
/// | record | its fields in the documented order, with no framing of its own |
/// | fieldless enum | as `str`, holding the Rust variant name verbatim (`"Bitcoin"`, `"Testnet4"`) |
///
/// A fieldless enum travels as its *name*, never as an integer tag. A name is
/// legible in a log without a table to look it up in, and a name a reader does
/// not know is skipped by its own length like any other string, the same
/// trick as the payload itself, one level down.
///
/// ## Payload layout per kind
///
/// | `kind` | Since | Payload fields, in order |
/// |--------|-------|--------------------------|
/// | `InsufficientBalance` | 1 | `required: u64`, `available: u64` (millisatoshis) |
/// | `NetworkMismatch` | 1 | `expected: str`, `compatible: list<str>`, `observed_prefix: str` |
/// | `MixedModuleGenerations` | 1 | `modules: list<record { kind: str, generation: u32 }>` |
/// | `QuoteExpired` | 1 | `expires_at: u64` (epoch milliseconds), `already_executed: bool` |
/// | `QuoteTermsChanged` | 1 | `quoted_total: u64`, `current_total: u64` (millisatoshis) |
/// | `BalanceNotEmpty` | 1 | `remaining: u64` (millisatoshis) |
/// | `StorageInUse` | 1 | `location: str` |
/// | `SeedMismatch` | 1 | `location: str` |
/// | `StorageOrphaned` | 1 | `location: str`, `seed_present: bool` |
///
/// ## What a reader must do
///
/// - **Unknown kind:** consume `payload` by its length, do not look inside it,
///   and keep the envelope as [`DetailEnvelope::Opaque`].
/// - **Known kind:** read that kind's fields in order, and require the
///   payload to end exactly where the last field does. A kind's layout never
///   drifts, so a well-formed newer producer never has more to
///   say under an old kind: it says it under a new kind instead. Trailing
///   bytes therefore mean the payload does not match the layout this kind
///   froze, and the envelope is kept as [`DetailEnvelope::Opaque`], exactly
///   like a short one.
/// - **Short or invalid payload:** a payload that ends before the last field,
///   holds invalid UTF-8, or holds a `bool` that is neither `0` nor `1` is
///   *uninterpretable*. Keep it as [`DetailEnvelope::Opaque`] and never guess
///   at a missing value: a fabricated amount in an error about amounts is
///   worse than no amount at all.
/// - **Never gate on `version`.** The kind alone decides whether a projection
///   is possible. A producer at envelope version 7 still emits version-1
///   kinds, so refusing to read a payload because `version` is unfamiliar
///   would throw away details this build understands perfectly well.
///   `version` is for saying how far ahead the producer is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RawErrorDetails {
    /// The envelope version the producing side declared it speaks, normally
    /// that side's own [`CURRENT_VERSION`](RawErrorDetails::CURRENT_VERSION).
    ///
    /// `0` is reserved for "unstated" and is never a real envelope version, so
    /// a reader that received no version has something honest to record.
    /// Compare it against this build's `CURRENT_VERSION` to say precisely how
    /// far ahead the producer is, rather than guessing.
    ///
    /// It is a diagnostic, not a gate: see *What a reader must do* on the
    /// type.
    pub version: u32,
    /// The stable kind discriminator: the [`ErrorDetails`] variant name this
    /// payload projects to, or a name from a newer SDK that this build has no
    /// projection for.
    ///
    /// Part of the stability contract, unlike [`Error::message`]: this string
    /// is what a reader dispatches on. An empty string is permitted for a
    /// reader that genuinely received no kind, and projects to nothing.
    pub kind: String,
    /// The kind's fields, encoded per the contract on this type and opaque at
    /// this layer.
    ///
    /// A raw envelope only exists where a detail crossed a boundary or is
    /// about to, so the payload is normally the encoded truth of the detail.
    /// It may still be empty: a producer that stated a kind and nothing else
    /// is legal, and a reader treats the missing fields as an uninterpretable
    /// payload rather than guessing at them.
    pub payload: Vec<u8>,
}

impl RawErrorDetails {
    /// The details-envelope version this build speaks.
    ///
    /// Bumped when kinds are added, and never for any other reason: an existing kind
    /// neither changes meaning nor changes payload layout. `0` is not a version, it is
    /// reserved for "the producer declared none". See *Versioning* on [`ErrorDetails`].
    pub const CURRENT_VERSION: u32 = 1;

    /// Records a raw envelope, as a decoder read it or as an encoder is about
    /// to write it.
    pub fn new(
        version: u32,
        kind: impl Into<String>,
        payload: impl Into<Vec<u8>>,
    ) -> RawErrorDetails {
        RawErrorDetails {
            version,
            kind: kind.into(),
            payload: payload.into(),
        }
    }
}

/// An error's structured detail, in whichever of its two states this side
/// managed to reach: projected into a typed case, or still opaque.
///
/// This is the type of [`Error::details`]. Either this side knew the kind and decoded the
/// payload, in which case the typed case is the whole truth, or it did not, in which case
/// the raw envelope is all there is. [`kind`](DetailEnvelope::kind) and
/// [`version`](DetailEnvelope::version) answer either way, so a detail is always loggable. A
/// caller after the numbers should go through [`Error::detail`] rather than touching bytes.
///
/// Unlike every other public enum in this crate, this one is deliberately **not**
/// `#[non_exhaustive]`: interpreted and not-interpreted exhausts the possibilities, so a Rust
/// caller gets a total match. This enum must never be the type a binding decodes directly off
/// the wire, though, since [`Interpreted`](DetailEnvelope::Interpreted) carries the growing
/// [`ErrorDetails`]. The boundary shape is always an optional [`RawErrorDetails`], with each
/// side projecting this type locally from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailEnvelope {
    /// This side knew the kind and read the payload, so the detail is
    /// available as a typed [`ErrorDetails`] case.
    Interpreted {
        /// The typed detail. Its [`kind`](ErrorDetails::kind) is what the
        /// envelope reports.
        detail: ErrorDetails,
        /// The envelope version the producing side declared: preserved from
        /// [`RawErrorDetails::version`] when this was projected off a raw
        /// envelope, and this build's
        /// [`CURRENT_VERSION`](RawErrorDetails::CURRENT_VERSION) when it was
        /// constructed locally. Different from the version that *introduced*
        /// this case, [`ErrorDetails::version`].
        producer_version: u32,
    },
    /// This side could not project the detail: an unrecognized kind, or a
    /// payload that did not decode, so the raw envelope is kept as it
    /// arrived.
    ///
    /// This is an ordinary value, not a failure of its own: [`Error::code`] and
    /// [`Error::message`] still describe the failure completely and correctly. Log it,
    /// since it says the producer knew more than this build can express, and carry on
    /// branching on `code`.
    Opaque {
        /// The envelope exactly as it was received, payload included: an
        /// unknown kind is skipped by its length, never parsed, so the bytes
        /// survive intact for a log line or a bug report.
        raw: RawErrorDetails,
    },
}

impl DetailEnvelope {
    /// The stable kind identifier of this detail, projected or not.
    ///
    /// For [`Interpreted`](DetailEnvelope::Interpreted) that is
    /// [`ErrorDetails::kind`]; for [`Opaque`](DetailEnvelope::Opaque) it is
    /// [`RawErrorDetails::kind`], which may be a name from a newer SDK, or
    /// empty, where the producing side stated none.
    pub fn kind(&self) -> &str {
        match self {
            DetailEnvelope::Interpreted { detail, .. } => detail.kind(),
            DetailEnvelope::Opaque { raw } => &raw.kind,
        }
    }

    /// The envelope version the producing side declared it speaks.
    ///
    /// The same answer for both cases: preserved through projection for
    /// [`Interpreted`](DetailEnvelope::Interpreted), read straight off the
    /// raw envelope for [`Opaque`](DetailEnvelope::Opaque), or `0` where
    /// the producer declared none. The version that *introduced* an
    /// interpreted case is a different number, available as
    /// [`ErrorDetails::version`] on [`typed`](DetailEnvelope::typed).
    pub fn version(&self) -> u32 {
        match self {
            DetailEnvelope::Interpreted {
                producer_version, ..
            } => *producer_version,
            DetailEnvelope::Opaque { raw } => raw.version,
        }
    }

    /// The typed detail, where this side could project one.
    pub fn typed(&self) -> Option<&ErrorDetails> {
        match self {
            DetailEnvelope::Interpreted { detail, .. } => Some(detail),
            DetailEnvelope::Opaque { .. } => None,
        }
    }

    /// The raw envelope, where this side could *not* project one.
    ///
    /// `None` for an [`Interpreted`](DetailEnvelope::Interpreted) detail, whose
    /// bytes were spent decoding it and are not kept: a second, encoded copy
    /// alongside the typed case could only drift out of step with it, and the
    /// boundary encoder re-derives the payload from the typed case at the
    /// moment a detail actually crosses.
    pub fn raw(&self) -> Option<&RawErrorDetails> {
        match self {
            DetailEnvelope::Interpreted { .. } => None,
            DetailEnvelope::Opaque { raw } => Some(raw),
        }
    }

    /// Whether the detail was projected into a typed case.
    ///
    /// `false` is the graceful-degradation state, and the one thing to do
    /// about it is log [`kind`](DetailEnvelope::kind) and
    /// [`version`](DetailEnvelope::version) and carry on branching on
    /// [`Error::code`].
    pub fn is_interpreted(&self) -> bool {
        matches!(self, DetailEnvelope::Interpreted { .. })
    }
}

/// Structured, machine-readable detail attached to an [`Error`].
///
/// Each case names the failure it accompanies and carries exactly the values a caller needs
/// to render or act on it. A case here is always a local projection of a
/// [`RawErrorDetails`], which is why this enum has no "unrecognized" case of its own: a kind
/// with no projection is [`DetailEnvelope::Opaque`], with the raw envelope still there to
/// read.
///
/// The enum is `#[non_exhaustive]`; its variants are not. A Rust caller writes a wildcard
/// arm, while a binding layer can still construct any case it needs (which
/// [`Error::with_details`] and [`DetailEnvelope::Interpreted`] exist for). A case that
/// exists never grows a field, because that is not safely additive across Swift, Kotlin and
/// TypeScript at once; more detail about an existing situation arrives as a new, more
/// specific case at a new envelope version instead.
///
/// # Versioning
///
/// [`RawErrorDetails::CURRENT_VERSION`] is the envelope version this build
/// speaks, and [`ErrorDetails::version`] reports the version that introduced
/// a particular case. A case's meaning and payload layout are frozen at
/// that version: never redefined, never given a wider or narrower reading,
/// never repurposed. Adding cases bumps `CURRENT_VERSION`; nothing else does.
///
/// A producer ahead of its consumer may emit a kind the consumer has no
/// projection for, and the consumer keeps the raw envelope as
/// [`DetailEnvelope::Opaque`], which is why a version mismatch degrades to
/// "there is a detail here I cannot interpret", stated with a version and a
/// kind, rather than a crash, a silent drop, or a payload read as the wrong
/// case.
///
/// # What must never go in here
///
/// Details are diagnostics a caller may display, so they carry no secrets:
/// no seed or mnemonic material, no invite-code API secret, no preimage that
/// has not already settled.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorDetails {
    /// The spendable balance was short. Accompanies
    /// [`ErrorCode::InsufficientBalance`].
    ///
    /// Both amounts are millisatoshi [`Amount`]s; compute the shortfall with
    /// [`Amount::checked_sub`] rather than reading a third field.
    InsufficientBalance {
        /// What the operation needed in total, including any fee already
        /// quoted for it.
        required: Amount,
        /// What was actually spendable when the check ran.
        available: Amount,
    },
    /// A value's Bitcoin network disagreed with the federation's.
    /// Accompanies [`ErrorCode::NetworkMismatch`].
    ///
    /// The federation's network is read from its own configuration and is exact. The
    /// rejected value's is not always knowable exactly from its encoding alone (testnet3,
    /// testnet4 and signet share address prefixes, and BOLT11 has one currency, `tb`, for
    /// both public testnets), so what is carried is the set of networks the value could have
    /// been for and the prefix it was actually spelled with.
    NetworkMismatch {
        /// The network the federation operates on, the one a value had to
        /// match. Read from federation configuration, so exact.
        expected: Network,
        /// Every network the rejected value could have been intended for,
        /// given what its encoding actually proves. Unordered and free of
        /// duplicates.
        ///
        /// A single entry means the value's encoding pinned one network; several mean it
        /// did not. **Empty** means the value named a network this crate's [`Network`] enum
        /// cannot represent (a BOLT11 `sb`/simnet invoice, today): a real, meaningful
        /// answer, and it still proves the mismatch. May omit networks a reader could not
        /// name, which only shrinks the set.
        compatible: Vec<Network>,
        /// The network prefix or BOLT11 currency the rejected value was
        /// actually spelled with, verbatim and lowercased: `"bc"`, `"tb"`,
        /// `"tbs"`, `"bcrt"`, `"sb"`, or a base58 address's leading character.
        ///
        /// Show it in a diagnostic: it is the only field that can describe a network this
        /// SDK has no name for. Branch on `compatible` and `expected` instead.
        observed_prefix: String,
    },
    /// A federation runs modules of more than one generation, which this SDK
    /// refuses to operate on. Accompanies
    /// [`ErrorCode::UnsupportedFederation`].
    ///
    /// Carries every module that takes part in the conflict together with
    /// the generation each declares. There are always at least two entries,
    /// and the list is not necessarily the federation's full module set: modules that agree
    /// with the majority may be omitted.
    ///
    /// This is one case of [`ErrorCode::UnsupportedFederation`], not the
    /// whole of it: match on `details` with a wildcard arm and treat `code` as the category.
    MixedModuleGenerations {
        /// The conflicting modules and the generation each declares.
        modules: Vec<ModuleGeneration>,
    },
    /// A quote can no longer be executed because its life is over: the
    /// validity window lapsed, or it had already been spent on a payment.
    /// Accompanies [`ErrorCode::QuoteExpired`].
    QuoteExpired {
        /// When the quote's validity window ended, as reported by
        /// [`LnQuote::expires_at`](crate::LnQuote::expires_at) or its
        /// on-chain equivalent.
        expires_at: Timestamp,
        /// `true` when the quote was refused because it had already been
        /// executed, rather than because its window lapsed.
        ///
        /// This is the sub-case a binding layer raises when a quote object crosses the
        /// boundary and is used a second time. Both sub-cases share
        /// [`ErrorCode::QuoteExpired`] because the remedy is identical; the flag lets a UI
        /// be honest about which happened.
        already_executed: bool,
    },
    /// A quote's terms moved after it was issued, so executing it would not
    /// charge what the user approved. Accompanies
    /// [`ErrorCode::QuoteChanged`].
    ///
    /// Both totals are the full debit, amount plus fee, the number a caller shows as "you
    /// will pay".
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
        /// The spendable balance still held in the federation.
        remaining: Amount,
    },
    /// A storage location was already open, in this process or another.
    /// Accompanies [`ErrorCode::StorageInUse`].
    StorageInUse {
        /// The location that could not be locked, as it was given to
        /// [`Storage::at`](crate::Storage::at) or
        /// [`Storage::in_browser`](crate::Storage::in_browser): a native
        /// filesystem path or an origin-scoped namespace. A path or a namespace, never a
        /// credential.
        location: String,
    },
    /// Storage already held a seed and it did not match the mnemonic
    /// supplied to open it. Accompanies [`ErrorCode::SeedMismatch`].
    ///
    /// Carries the location and nothing else: no seed, mnemonic, fingerprint or hash of
    /// either. The existing storage is untouched, so the remedy is to open it with the
    /// right mnemonic rather than to compare seeds.
    SeedMismatch {
        /// The storage location whose seed disagrees, as it was given to
        /// [`Storage::at`](crate::Storage::at) or
        /// [`Storage::in_browser`](crate::Storage::in_browser).
        location: String,
    },
    /// Storage held state belonging to this SDK but no usable seed, so it was
    /// refused without being opened. Accompanies
    /// [`ErrorCode::StorageOrphaned`].
    ///
    /// No seed, mnemonic, fingerprint or hash of either appears here: there was no usable
    /// seed to describe, and this is an error a host will log.
    StorageOrphaned {
        /// The storage location that was refused, as it was given to
        /// [`Storage::at`](crate::Storage::at) or
        /// [`Storage::in_browser`](crate::Storage::in_browser). A path or a namespace,
        /// never a credential.
        location: String,
        /// Whether a seed entry existed at all beside the state that was
        /// found.
        ///
        /// `false`: there was no seed entry. The state came from a seed that
        /// is not here.
        ///
        /// `true`: an entry was there and this build could not use it,
        /// truncated, corrupt, or written in a format only a newer SDK
        /// understands. This is **not** a licence to overwrite it: the bytes may be a
        /// perfectly good seed that a newer build reads fine, so try a newer build first;
        /// the entry is left exactly as it was found either way.
        seed_present: bool,
    },
}

impl ErrorDetails {
    /// The stable kind identifier for this case: the discriminator that
    /// crosses a boundary in [`RawErrorDetails::kind`].
    ///
    /// Spelled exactly as the variant, and part of the stability contract:
    /// see *Kinds* on [`RawErrorDetails`]. This is the encoder's half of the
    /// projection; a decoder's half is the reverse map, which each boundary
    /// hand-writes because the payload decoding lives there too.
    pub fn kind(&self) -> &'static str {
        match self {
            ErrorDetails::InsufficientBalance { .. } => "InsufficientBalance",
            ErrorDetails::NetworkMismatch { .. } => "NetworkMismatch",
            ErrorDetails::MixedModuleGenerations { .. } => "MixedModuleGenerations",
            ErrorDetails::QuoteExpired { .. } => "QuoteExpired",
            ErrorDetails::QuoteTermsChanged { .. } => "QuoteTermsChanged",
            ErrorDetails::BalanceNotEmpty { .. } => "BalanceNotEmpty",
            ErrorDetails::StorageInUse { .. } => "StorageInUse",
            ErrorDetails::SeedMismatch { .. } => "SeedMismatch",
            ErrorDetails::StorageOrphaned { .. } => "StorageOrphaned",
        }
    }

    /// The envelope version that introduced this case, and at which its
    /// meaning and payload layout are frozen.
    ///
    /// Never later than [`RawErrorDetails::CURRENT_VERSION`]. This is a
    /// property of the *case*, and a different number from
    /// [`RawErrorDetails::version`], which is the version the producing side
    /// declared it speaks: a producer at envelope version 7 emitting a
    /// version-1 case reports 7 there and 1 here, and both are true.
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
            | ErrorDetails::SeedMismatch { .. }
            | ErrorDetails::StorageOrphaned { .. } => 1,
        }
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
    /// [`FederationPreview::modules`](crate::FederationPreview::modules), for
    /// example `"mint"`, `"ln"`, `"wallet"`. Not restricted to the
    /// modules this SDK exposes a facade for: the generation rule covers
    /// every module a federation runs.
    pub kind: String,
    /// The generation this module declares: `1` for v1, `2` for v2.
    ///
    /// A plain integer rather than an enum, so a generation this SDK has never heard of is
    /// reported faithfully rather than flattened into an "unknown" variant.
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
/// Additive-only after 1.0: new variants may be added in minor releases, and
/// existing variants are never removed or renamed. `#[non_exhaustive]`, so a
/// Rust caller must write a wildcard arm; a variant added later is not a
/// breaking change for Rust.
///
/// That guarantee is Rust-only. A generated Swift, Kotlin or TypeScript
/// decoder does not tolerate a tag it has never seen: it fails to decode the
/// error rather than falling back to an "unknown" case. Regenerate the
/// binding against the SDK version it talks to (the default expectation for
/// this crate) to avoid the gap, or, where that is not possible, hand-write
/// and test an adapter that carries the code across as its stable variant
/// name with an explicit unknown fallback.
///
/// [`ErrorDetails`] is the one place forward decodability is built into the
/// wire format itself, because there the payload is length-delimited opaque
/// bytes; see [`RawErrorDetails`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    /// The input could not be parsed or was structurally invalid, a
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
    /// A persisted operation exists but this build cannot interpret it: the
    /// record names a kind this build does not know, or a state schema newer
    /// than it reads. The operation is still observable (its id, and its kind
    /// where known, are readable) but not actionable.
    /// [`OperationSupport`](crate::OperationSupport) names which of the two
    /// it was.
    UnsupportedOperation,
    /// Storage already holds a seed and it does not match the mnemonic
    /// supplied to open it. [`ErrorDetails::SeedMismatch`] names the
    /// storage location.
    SeedMismatch,
    /// The storage location is already open, in this process or another.
    /// [`ErrorDetails::StorageInUse`] names the location.
    StorageInUse,
    /// Storage holds federation or client state but no usable seed, so it
    /// cannot be opened. The seed entry is either **absent** altogether, or
    /// **present and unusable**, truncated, corrupt, or written in a format
    /// this build does not understand. [`ErrorDetails::StorageOrphaned`] names
    /// the location and says which of the two it was.
    ///
    /// **Permanent, not transient**, unlike [`Storage`](ErrorCode::Storage): nothing about a
    /// store whose state has no matching seed changes by asking again. Nothing is mutated
    /// by the refusal either, so the location is exactly as it was found.
    ///
    /// **What a caller can do.** Not retry, and not repair it automatically. Report it and
    /// offer the ways out, in this order:
    ///
    /// 1. Where the entry was present but unusable, **update the SDK or the
    ///    app**: a newer build may read a format this one cannot.
    /// 2. **Point at a different location.** The everyday cause is a path
    ///    that moved, or storage belonging to another app, profile, or user.
    ///    If the seed phrase is known, it also recovers the funds, at a
    ///    *fresh* location, never by writing into this one.
    /// 3. **Abandon the location deliberately**, by deleting its contents,
    ///    which makes it provably empty and therefore openable. This destroys
    ///    any funds whose only backup was that seed, so it is a last resort a
    ///    person chooses explicitly, never a step an application takes on
    ///    their behalf.
    StorageOrphaned,
    /// The quote passed to `send` is no longer valid: either its validity
    /// window has passed, or it has already been executed, and a quote funds
    /// exactly one payment. Both have the same remedy: obtain a fresh quote
    /// and retry.
    ///
    /// The already-executed case is what a binding reports when a quote
    /// object crosses the boundary and is used a second time.
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
    /// does not support paying amountless bolt11 invoices at all: that is a
    /// permanent limitation, not a gap in this SDK that a later release will
    /// fill.
    ///
    /// The only remedy is a different invoice, one that names its own
    /// amount. An application taking an invoice from a QR code or a paste
    /// buffer should say so plainly ("this invoice does not specify an
    /// amount and cannot be paid here") instead of prompting for a number it
    /// cannot use.
    AmountlessInvoice,
    /// A supplied address or invoice is for a different network than the
    /// federation's.
    ///
    /// [`ErrorDetails::NetworkMismatch`] carries the federation's network
    /// exactly, and, for the rejected value, what its encoding actually
    /// proves: the set of networks it could have been for, plus the prefix it
    /// was spelled with. That is deliberately not one exact network: a
    /// `tb1…` address is testnet3, testnet4 or signet, and a BOLT11 `tb`
    /// invoice is either public testnet, so naming one would be a guess.
    NetworkMismatch,
    /// The federation handle is closed, either because it was individually
    /// closed while retaining its data, or because the whole SDK instance
    /// was shut down.
    FederationClosed,
    /// The operation did not complete within an internal time budget.
    Timeout,
    /// The platform's secure random source was unavailable or failed, so no
    /// entropy could be drawn, as when
    /// [`Mnemonic::generate`](crate::Mnemonic::generate) creates a fresh seed,
    /// directly or through [`SdkBuilder::build`](crate::SdkBuilder::build)
    /// establishing one for empty storage.
    ///
    /// This is the one failure a caller can do nothing about: there is no
    /// input to correct, no permission to grant, and no retry that reliably
    /// helps. It is still surfaced rather than papered over, because the
    /// alternatives are worse: panicking would take down a binding layer
    /// that must not panic, and falling back to a weaker source would mint a
    /// guessable seed and lose funds silently. Report it and stop; do not
    /// substitute entropy of your own.
    Entropy,
    /// The local storage backend failed to read or write.
    ///
    /// A fault in the backend itself: the I/O failed, the browser store
    /// rejected the operation, the device is out of space, and therefore
    /// potentially **transient**: retrying, or retrying once the underlying
    /// condition is gone, is a reasonable thing for a caller to do.
    ///
    /// It does **not** cover a store whose contents are wrong. Storage that
    /// holds state but no usable seed is [`StorageOrphaned`], which is
    /// permanent and needs a human;
    /// [`SeedMismatch`](ErrorCode::SeedMismatch) and
    /// [`StorageInUse`](ErrorCode::StorageInUse) are the other two conditions
    /// that are about the store rather than the backend.
    ///
    /// [`StorageOrphaned`]: ErrorCode::StorageOrphaned
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
                compatible: vec![Network::Testnet, Network::Testnet4, Network::Signet],
                observed_prefix: "tb".to_owned(),
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
            ErrorDetails::StorageOrphaned {
                location: "/var/app/wallet".to_owned(),
                seed_present: false,
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
        match err.detail() {
            Some(ErrorDetails::InsufficientBalance {
                required,
                available,
            }) => {
                assert_eq!(*required, Amount::from_msats(1_500));
                assert_eq!(*available, Amount::from_msats(1_200));
                // The shortfall a UI wants is the caller's subtraction, and
                // it is exact rather than a third field that could drift.
                assert_eq!(
                    required.checked_sub(*available),
                    Some(Amount::from_msats(300))
                );
            }
            other => panic!("expected an InsufficientBalance detail, got {other:?}"),
        }
    }

    #[test]
    fn network_mismatch_carries_what_was_observed_not_an_invented_network() {
        // A `tb1…` address, rejected by a mainnet federation. Its encoding
        // proves "some test network" and no more, so that is what is carried.
        let err = Error::with_details(
            ErrorCode::NetworkMismatch,
            "wrong network",
            ErrorDetails::NetworkMismatch {
                expected: Network::Bitcoin,
                compatible: vec![Network::Testnet, Network::Testnet4, Network::Signet],
                observed_prefix: "tb".to_owned(),
            },
        );
        match err.detail() {
            Some(ErrorDetails::NetworkMismatch {
                expected,
                compatible,
                observed_prefix,
            }) => {
                assert_eq!(*expected, Network::Bitcoin);
                assert_eq!(
                    compatible,
                    &vec![Network::Testnet, Network::Testnet4, Network::Signet]
                );
                assert_eq!(observed_prefix, "tb");
                // The mismatch is exactly this, and it needs no exact
                // `actual` to be decidable.
                assert!(!compatible.contains(expected));
            }
            other => panic!("expected a NetworkMismatch detail, got {other:?}"),
        }
    }

    #[test]
    fn network_mismatch_expresses_a_network_this_crate_cannot_name() {
        // A BOLT11 `sb` (simnet) invoice. `Network` has no variant for it, so
        // the compatible set is empty and the prefix is the only ground
        // truth, which is a real answer, not a missing one.
        let err = Error::with_details(
            ErrorCode::NetworkMismatch,
            "wrong network",
            ErrorDetails::NetworkMismatch {
                expected: Network::Bitcoin,
                compatible: Vec::new(),
                observed_prefix: "sb".to_owned(),
            },
        );
        match err.detail() {
            Some(ErrorDetails::NetworkMismatch {
                expected,
                compatible,
                observed_prefix,
            }) => {
                assert!(compatible.is_empty());
                assert_eq!(observed_prefix, "sb");
                // An empty set still proves the mismatch.
                assert!(!compatible.contains(expected));
            }
            other => panic!("expected a NetworkMismatch detail, got {other:?}"),
        }
    }

    #[test]
    fn network_mismatch_narrows_to_one_network_where_the_encoding_pins_it() {
        // A BOLT11 `tbs` invoice does pin signet, and a single-entry set says
        // so, the shape carries precision where precision exists.
        let detail = ErrorDetails::NetworkMismatch {
            expected: Network::Bitcoin,
            compatible: vec![Network::Signet],
            observed_prefix: "tbs".to_owned(),
        };
        match &detail {
            ErrorDetails::NetworkMismatch { compatible, .. } => {
                assert_eq!(compatible, &vec![Network::Signet]);
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
        match err.detail() {
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
        match lapsed.detail() {
            Some(ErrorDetails::QuoteExpired {
                expires_at,
                already_executed,
            }) => {
                assert_eq!(*expires_at, Timestamp::from_epoch_millis(1_700_000_000_000));
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
        match reused.detail() {
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
        match err.detail() {
            Some(ErrorDetails::QuoteTermsChanged {
                quoted_total,
                current_total,
            }) => {
                assert_eq!(*quoted_total, Amount::from_msats(101_000));
                assert_eq!(*current_total, Amount::from_msats(103_500));
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
        match err.detail() {
            Some(ErrorDetails::BalanceNotEmpty { remaining }) => {
                assert_eq!(*remaining, Amount::from_msats(7_000));
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
        match in_use.detail() {
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
        match mismatch.detail() {
            Some(ErrorDetails::SeedMismatch { location }) => {
                assert_eq!(location, "/var/app/wallet");
            }
            other => panic!("expected a SeedMismatch detail, got {other:?}"),
        }
    }

    #[test]
    fn orphaned_storage_has_its_own_code_distinct_from_a_backend_fault() {
        // The point of the code: "state with no usable seed" is permanent and
        // needs a human; a failed read or write is transient and worth
        // retrying. A caller separates them on `code` alone, never by
        // reading `message`.
        fn worth_retrying(err: &Error) -> bool {
            // A backend fault may be gone next time; nothing else here is.
            matches!(err.code, ErrorCode::Storage)
        }

        let orphaned = Error::with_details(
            ErrorCode::StorageOrphaned,
            "storage holds federation state but no usable seed",
            ErrorDetails::StorageOrphaned {
                location: "/var/app/wallet".to_owned(),
                seed_present: false,
            },
        );
        let backend_fault = Error::new(ErrorCode::Storage, "write failed: device is full");

        assert_ne!(orphaned.code, backend_fault.code);
        assert!(!worth_retrying(&orphaned));
        assert!(worth_retrying(&backend_fault));
    }

    #[test]
    fn storage_orphaned_detail_names_the_location_and_which_condition() {
        // No seed entry at all: the state came from a seed that is not here.
        let absent = Error::with_details(
            ErrorCode::StorageOrphaned,
            "no seed beside the state",
            ErrorDetails::StorageOrphaned {
                location: "/var/app/wallet".to_owned(),
                seed_present: false,
            },
        );
        match absent.detail() {
            Some(ErrorDetails::StorageOrphaned {
                location,
                seed_present,
            }) => {
                assert_eq!(location, "/var/app/wallet");
                assert!(!seed_present);
            }
            other => panic!("expected a StorageOrphaned detail, got {other:?}"),
        }

        // An entry that exists and this build cannot use: possibly a newer
        // on-disk format, which is why the two conditions are told apart: the
        // first thing to try here is a newer build, not a fresh seed.
        let unreadable = Error::with_details(
            ErrorCode::StorageOrphaned,
            "the seed entry did not decode",
            ErrorDetails::StorageOrphaned {
                location: "/var/app/wallet".to_owned(),
                seed_present: true,
            },
        );
        match unreadable.detail() {
            Some(ErrorDetails::StorageOrphaned {
                location,
                seed_present,
            }) => {
                assert_eq!(location, "/var/app/wallet");
                assert!(seed_present);
            }
            other => panic!("expected a StorageOrphaned detail, got {other:?}"),
        }
    }

    #[test]
    fn a_diagnostic_carries_the_same_structured_detail_an_error_would() {
        // What a quarantined federation records: the code an equivalent
        // `Error` would carry, and the very same details envelope. The
        // modules that disagree are readable without parsing `message`, which
        // is the requirement the free-form-text version could not meet.
        let why = Diagnostic::with_details(
            ErrorCode::UnsupportedFederation,
            "mixed module generations",
            ErrorDetails::MixedModuleGenerations {
                modules: vec![
                    ModuleGeneration::new("mint", 1),
                    ModuleGeneration::new("ln", 2),
                ],
            },
        );
        assert_eq!(why.code, ErrorCode::UnsupportedFederation);
        assert_eq!(
            why.to_string(),
            "UnsupportedFederation: mixed module generations"
        );
        match why.detail() {
            Some(ErrorDetails::MixedModuleGenerations { modules }) => {
                let named: Vec<(&str, u32)> = modules
                    .iter()
                    .map(|m| (m.kind.as_str(), m.generation))
                    .collect();
                assert_eq!(named, vec![("mint", 1), ("ln", 2)]);
            }
            other => panic!("expected a MixedModuleGenerations detail, got {other:?}"),
        }

        // The envelope's own accessors read the same on a diagnostic as on an
        // error, which is what lets one boundary projection serve both.
        let envelope = why.details.as_ref().expect("a detail");
        assert_eq!(envelope.kind(), "MixedModuleGenerations");
        assert_eq!(envelope.version(), 1);
        assert!(envelope.is_interpreted());

        // And a diagnosis with nothing structured to add is still a complete
        // one: `code` is authoritative on its own.
        let bare = Diagnostic::new(ErrorCode::FederationUnreachable, "no guardian answered");
        assert!(bare.details.is_none());
        assert!(bare.detail().is_none());
    }

    #[test]
    fn a_diagnostic_and_an_error_convert_both_ways_unchanged() {
        let err = Error::with_details(
            ErrorCode::UnsupportedFederation,
            "mixed module generations",
            ErrorDetails::MixedModuleGenerations {
                modules: vec![
                    ModuleGeneration::new("mint", 1),
                    ModuleGeneration::new("ln", 2),
                ],
            },
        );
        let details = err.details.clone();

        // How the SDK records the failure that stopped a federation opening.
        let recorded = Diagnostic::from(err);
        assert_eq!(recorded.code, ErrorCode::UnsupportedFederation);
        assert_eq!(recorded.message, "mixed module generations");
        assert_eq!(recorded.details, details);

        // And how a caller raises a recorded diagnosis back as an error.
        let raised: Error = recorded.clone().into();
        assert_eq!(raised.code, recorded.code);
        assert_eq!(raised.message, recorded.message);
        assert_eq!(raised.details, recorded.details);
        assert_eq!(raised.to_string(), recorded.to_string());

        // A round trip is lossless in the public surface, which is what makes
        // the two types one mechanism rather than two taxonomies.
        assert_eq!(Diagnostic::from(raised), recorded);
    }

    #[test]
    fn diagnostics_compare_by_their_public_fields() {
        // A status holding one of these is diffed to decide whether anything
        // changed, so equality has to mean what a reader expects: the reason
        // the shared piece is this type and not `Error`, which carries an
        // internal source chain and is deliberately not comparable.
        let one = Diagnostic::with_details(
            ErrorCode::StorageOrphaned,
            "storage holds state but no usable seed",
            ErrorDetails::StorageOrphaned {
                location: "/var/app/wallet".to_owned(),
                seed_present: false,
            },
        );
        assert_eq!(one, one.clone());
        assert_ne!(
            one,
            Diagnostic::new(
                ErrorCode::StorageOrphaned,
                "storage holds state but no usable seed"
            )
        );
        assert_ne!(
            one,
            Diagnostic::with_details(
                ErrorCode::StorageOrphaned,
                "storage holds state but no usable seed",
                ErrorDetails::StorageOrphaned {
                    location: "/var/app/wallet".to_owned(),
                    seed_present: true,
                },
            )
        );
    }

    #[test]
    fn a_diagnostic_keeps_an_uninterpretable_detail_as_a_value() {
        // The skippable-payload property has to hold here too: a quarantine
        // written by a newer SDK degrades to "there is a detail I cannot
        // interpret", stated with a kind and a version, and the code and
        // message still describe the situation completely.
        let why = Diagnostic::with_raw_details(
            ErrorCode::UnsupportedFederation,
            "this federation's configuration is refused",
            RawErrorDetails::new(9, "SomethingNewerThanThisBuild", vec![0x01, 0x02]),
        );
        assert_eq!(why.code, ErrorCode::UnsupportedFederation);
        assert!(why.detail().is_none());

        let envelope = why.details.expect("the detail is preserved, not dropped");
        assert!(!envelope.is_interpreted());
        assert_eq!(envelope.kind(), "SomethingNewerThanThisBuild");
        assert_eq!(envelope.version(), 9);
        assert!(envelope.version() > RawErrorDetails::CURRENT_VERSION);
        assert_eq!(
            envelope
                .raw()
                .expect("an opaque envelope keeps its bytes")
                .payload,
            vec![0x01, 0x02]
        );
    }

    #[test]
    fn every_known_detail_belongs_to_a_shipped_envelope_version() {
        for detail in every_known_detail() {
            let version = detail.version();
            assert!(
                version >= 1 && version <= RawErrorDetails::CURRENT_VERSION,
                "{detail:?} reports version {version}, outside 1..={}",
                RawErrorDetails::CURRENT_VERSION
            );
        }
    }

    #[test]
    fn every_known_detail_has_a_distinct_stable_kind() {
        // The kind string is the wire discriminator, so two cases sharing one
        // would make the projection ambiguous.
        let details = every_known_detail();
        let mut kinds: Vec<&str> = details.iter().map(|detail| detail.kind()).collect();
        kinds.sort_unstable();
        let count = kinds.len();
        kinds.dedup();
        assert_eq!(kinds.len(), count, "two cases share a kind string");
        for detail in &details {
            assert!(!detail.kind().is_empty());
            assert!(detail.kind().is_ascii());
        }
    }

    #[test]
    fn an_interpreted_envelope_keeps_the_producer_version_through_projection() {
        // A version-7 producer emitting a version-1 kind: the case's own
        // introduction version must not overwrite how far ahead the producer
        // declared itself to be.
        let envelope = DetailEnvelope::Interpreted {
            detail: ErrorDetails::BalanceNotEmpty {
                remaining: Amount::from_msats(7_000),
            },
            producer_version: 7,
        };
        assert_eq!(envelope.kind(), "BalanceNotEmpty");
        assert_eq!(envelope.version(), 7);
        // The introduction version is a different question, still answerable
        // from the typed case.
        assert_eq!(envelope.typed().expect("interpreted").version(), 1);
        assert!(envelope.is_interpreted());
        // The bytes were spent decoding it; the boundary encoder re-derives
        // them from the typed case if it ever crosses again.
        assert!(envelope.raw().is_none());
    }

    #[test]
    fn an_uninterpretable_envelope_leaves_code_and_message_intact() {
        // A binding built against an older SDK meeting a kind it has no
        // projection for. It read the whole record (version, kind, and the
        // payload by its length) and simply has no typed case to build, so
        // the failure is still fully described and the detail is observable
        // rather than dropped or fatal.
        let err = Error::with_raw_details(
            ErrorCode::InsufficientBalance,
            "balance is short",
            RawErrorDetails::new(9, "SomethingNewerThanThisBuild", vec![0x01, 0x02, 0x03]),
        );

        // The code still branches, and the message still reads.
        assert_eq!(err.code, ErrorCode::InsufficientBalance);
        assert_eq!(err.message, "balance is short");
        assert_eq!(
            err.to_string(),
            "InsufficientBalance: balance is short",
            "an uninterpretable detail must not change how an error renders"
        );

        // No typed case, and that is not an error.
        assert!(err.detail().is_none());

        let envelope = err.details.expect("the detail is preserved, not dropped");
        assert!(!envelope.is_interpreted());
        // The producer's declared version is readable, so "this came from
        // something newer than me" is a statement rather than a guess, and it
        // reads off the envelope without caring which state it is in.
        assert_eq!(envelope.version(), 9);
        assert!(envelope.version() > RawErrorDetails::CURRENT_VERSION);
        assert_eq!(envelope.kind(), "SomethingNewerThanThisBuild");

        let raw = envelope.raw().expect("an opaque envelope keeps its bytes");
        // The opaque payload survived intact, skipped rather than parsed:
        // which is the whole reason an unknown kind is decodable at all.
        assert_eq!(raw.payload, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn an_envelope_accepts_an_undeclared_version_and_no_kind() {
        // A decoder that cannot tell what the producer was speaking, and got
        // no kind either, still has something honest to record: `0` is
        // reserved for "unstated" and is never a real envelope version.
        let envelope = DetailEnvelope::Opaque {
            raw: RawErrorDetails::new(0, "", Vec::new()),
        };
        assert!(!envelope.is_interpreted());
        assert_eq!(envelope.version(), 0);
        assert!(envelope.kind().is_empty());
        assert_ne!(0, RawErrorDetails::CURRENT_VERSION);
    }

    /// The canonical hand-written projection the docs describe: dispatch on
    /// the kind, then perform **exact, checked consumption** of the frozen
    /// layout. A payload that is short, or that has bytes left over, does
    /// not match the kind's frozen layout and is not projected: the caller
    /// keeps the raw envelope as `Opaque` instead. Nothing here can panic
    /// on boundary input, per the crate's no-panic rule.
    fn project(raw: &RawErrorDetails) -> Option<ErrorDetails> {
        match raw.kind.as_str() {
            // `InsufficientBalance` is exactly two big-endian u64
            // millisatoshi fields, required then available: 16 bytes, no
            // more and no fewer, checked before anything is read.
            "InsufficientBalance" => {
                if raw.payload.len() != 16 {
                    return None;
                }
                let required = u64::from_be_bytes(raw.payload[..8].try_into().ok()?);
                let available = u64::from_be_bytes(raw.payload[8..16].try_into().ok()?);
                Some(ErrorDetails::InsufficientBalance {
                    required: Amount::from_msats(required),
                    available: Amount::from_msats(available),
                })
            }
            // An unknown kind never looks inside the payload at all.
            _ => None,
        }
    }

    /// Builds the error a boundary would hand on: the typed case when the
    /// projection succeeded (with the producer's declared version carried
    /// through, never this build's), and the raw envelope kept opaque when
    /// it did not.
    fn project_or_keep_raw(raw: RawErrorDetails) -> Error {
        match project(&raw) {
            Some(detail) => Error::with_projected_details(
                ErrorCode::InsufficientBalance,
                "balance is short",
                detail,
                raw.version,
            ),
            None => {
                Error::with_raw_details(ErrorCode::InsufficientBalance, "balance is short", raw)
            }
        }
    }

    #[test]
    fn projection_preserves_the_producer_version_through_the_decoder_path() {
        // A version-7 producer emitting the version-1 kind, decoded through
        // the canonical path: what the envelope reports afterwards is the
        // producer's declared version, not this build's and not the case's
        // introduction version.
        let payload = [0u8, 0, 0, 0, 0, 0, 0x05, 0xDC, 0, 0, 0, 0, 0, 0, 0x04, 0xB0];
        let raw = RawErrorDetails::new(7, "InsufficientBalance", payload);

        let err = project_or_keep_raw(raw);
        let envelope = err.details.expect("projected");
        assert!(envelope.is_interpreted());
        assert_eq!(envelope.version(), 7);
        assert_eq!(envelope.typed().expect("interpreted").version(), 1);
    }

    #[test]
    fn a_payload_that_a_reader_understood_projects_to_the_typed_case() {
        let payload = [0u8, 0, 0, 0, 0, 0, 0x05, 0xDC, 0, 0, 0, 0, 0, 0, 0x04, 0xB0];
        let raw = RawErrorDetails::new(
            RawErrorDetails::CURRENT_VERSION,
            "InsufficientBalance",
            payload,
        );
        assert_eq!(raw.payload.len(), 16);
        let kind = raw.kind.clone();

        let err = project_or_keep_raw(raw);
        match err.detail() {
            Some(ErrorDetails::InsufficientBalance {
                required,
                available,
            }) => {
                assert_eq!(*required, Amount::from_msats(1_500));
                assert_eq!(*available, Amount::from_msats(1_200));
            }
            other => panic!("expected an InsufficientBalance detail, got {other:?}"),
        }
        // The kind the projection dispatched on is the kind the case reports.
        assert_eq!(err.details.expect("a detail").kind(), kind);
    }

    #[test]
    fn a_short_payload_for_a_known_kind_stays_opaque_without_panicking() {
        // Half a field short: uninterpretable. The decoder must neither
        // guess at the missing value nor panic: it keeps the envelope raw.
        let raw = RawErrorDetails::new(
            RawErrorDetails::CURRENT_VERSION,
            "InsufficientBalance",
            vec![0u8; 12],
        );
        let err = project_or_keep_raw(raw);
        assert_eq!(err.detail(), None);
        let envelope = err.details.expect("the raw envelope is kept");
        assert!(!envelope.is_interpreted());
        assert_eq!(
            envelope
                .raw()
                .expect("opaque keeps its bytes")
                .payload
                .len(),
            12
        );
    }

    #[test]
    fn a_trailing_payload_for_a_known_kind_stays_opaque() {
        // One byte too many: the layout is frozen, so this payload is not a
        // valid instance of the kind. Projecting it anyway would silently
        // discard the surplus; the reader rules require it to stay opaque,
        // with every byte intact.
        let mut payload = vec![0u8; 16];
        payload.push(0xFF);
        let raw = RawErrorDetails::new(
            RawErrorDetails::CURRENT_VERSION,
            "InsufficientBalance",
            payload,
        );
        let err = project_or_keep_raw(raw);
        assert_eq!(err.detail(), None);
        let envelope = err.details.expect("the raw envelope is kept");
        assert!(!envelope.is_interpreted());
        assert_eq!(
            envelope
                .raw()
                .expect("opaque keeps its bytes")
                .payload
                .len(),
            17
        );
    }

    #[test]
    fn an_error_stays_small_enough_to_return_by_value() {
        // `Result<T, Error>` is the return type of every fallible call in this
        // crate, so the details envelope must not bloat it. This is why
        // `DetailEnvelope` is a dichotomy and not a raw half beside a typed
        // half: holding both at once pushed `Error` past 128 bytes, which is
        // `clippy::result_large_err`'s threshold and a hard error here, at
        // every synchronous `Result`-returning call site in the crate.
        //
        // The bound is that threshold rather than today's size, so this fails
        // for the reason a reader would expect: a field added to `Error` or to
        // the envelope has made every fallible call more expensive to return.
        const CLIPPY_LARGE_ERR_THRESHOLD: usize = 128;
        assert!(
            core::mem::size_of::<Error>() <= CLIPPY_LARGE_ERR_THRESHOLD,
            "Error grew to {} bytes, over clippy::result_large_err's {CLIPPY_LARGE_ERR_THRESHOLD}",
            core::mem::size_of::<Error>()
        );

        // `Diagnostic` is those same three fields as a value, so it is the same
        // size, and a status that holds one costs no more than an error does.
        // Keeping them two types is what lets a status carry the envelope
        // without `Error` nesting a record inside itself to provide it.
        assert_eq!(
            core::mem::size_of::<Diagnostic>(),
            core::mem::size_of::<Error>(),
            "Diagnostic is meant to be Error's three fields and nothing more"
        );
    }

    #[test]
    fn details_are_matchable_with_a_wildcard_arm() {
        // How a forward-compatible caller reads the envelope: branch on
        // `code`, enrich from `detail()`, and fall through for anything else.
        fn shortfall(err: &Error) -> Option<Amount> {
            match err.detail() {
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
        let unknown = Error::with_raw_details(
            ErrorCode::InsufficientBalance,
            "balance is short",
            RawErrorDetails::new(9, "SomethingNewer", vec![0xFF]),
        );
        assert_eq!(shortfall(&unknown), None);
        assert_eq!(
            shortfall(&Error::new(ErrorCode::InsufficientBalance, "short")),
            None
        );
    }
}
