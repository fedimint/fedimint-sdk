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
///   [`DetailEnvelope`], and [`Error::detail`] is the short path from it to a
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
/// now, before the surface freezes: it arrives as a **new kind inside the
/// details envelope**, carried in the `details` field that exists from day
/// one. It does *not* arrive as a new field on `Error`, and it does not
/// arrive as a new field on a kind that already exists.
///
/// Reserving the field up front is a deliberate correction of an earlier plan
/// to defer detail into later fields on `Error`. `Error` is
/// `#[non_exhaustive]`, which makes adding a field non-breaking *for Rust
/// callers* — but this crate is also the single surface the Swift, Kotlin and
/// TypeScript SDKs are generated from, and there a public struct is a
/// generated record. Growing a record is not safely additive across all three
/// targets at once: a pre-generated binding pinned to an older SDK decodes a
/// record it was generated against, and a producer that added a field to it
/// is a producer it can no longer read.
///
/// ## Why the envelope is raw bytes and not a data enum
///
/// A first draft of this envelope was a plain data enum with an
/// `Unrecognized` case, on the theory that a binding meeting a case it did
/// not know would map itself onto that case. **That does not work, and the
/// case is gone.** A generated decoder fails on the unknown *tag* before
/// anything could map it anywhere — UniFFI's Swift decoder throws
/// `unexpectedEnumCase` — and even if it did not, it could not skip an
/// associated-value layout it has never seen in order to reach whatever
/// follows. A case cannot be the fallback for a tag that is rejected before
/// it is read, so "add a case" is not by itself a forward-compatibility
/// story.
///
/// What crosses a boundary is therefore not the enum. It is a **raw,
/// length-delimited envelope**, [`RawErrorDetails`]: a version, a kind
/// discriminator, and an opaque payload whose byte length precedes it. Every
/// field of that record is a fixed-width primitive or a length-delimited
/// string of bytes, so a reader of any vintage consumes the whole record
/// without understanding any of it, and a kind it has never heard of costs it
/// one skipped payload instead of a thrown error. The typed [`ErrorDetails`]
/// cases are then **projected locally**, by the side doing the reading, from
/// the `(kind, payload)` pair — which is what keeps an unknown tag away from
/// a generated enum decoder entirely, because each side only ever constructs
/// the cases it already knows. [`RawErrorDetails`] carries the full encoding
/// contract.
///
/// ## The rules
///
/// Three rules govern the envelope, and all three are part of the stability
/// contract:
///
/// 1. **`code` is authoritative; `details` only sharpens it.** `details` is
///    always `Option`, and `None` never means the error is less real — it
///    means this failure had no numbers worth reporting, or the layer that
///    raised it had none to hand. `details` therefore never holds the only
///    copy of something a caller must act on: a caller that ignores
///    `details` entirely still branches correctly on `code`. What `details`
///    buys is what a caller can *show* — "you need 1,500 msat and have
///    1,200" instead of "insufficient balance".
/// 2. **An uninterpretable detail is a value, not a failure.** A side that
///    meets a `kind` it has no vocabulary for keeps the raw envelope as
///    [`DetailEnvelope::Opaque`]. Nothing is dropped — the version, the kind
///    and the payload are all still there to log — nothing is fatal, nothing
///    is misread as a different kind, and `code` and `message` are
///    unaffected: they still describe the failure completely and correctly.
/// 3. **A kind's meaning and payload layout never drift.** Both are fixed in
///    the envelope version that introduced the kind and are never redefined;
///    [`ErrorDetails::version`] reports that version, and
///    [`RawErrorDetails::CURRENT_VERSION`] is the newest version this build
///    speaks. Reinterpreting a situation means adding a new kind at a new
///    version, leaving the old kind meaning exactly what it always meant.
///
/// Detail never arrives as a payload on [`ErrorCode`] either: that enum is a
/// fieldless `Copy` enum so it maps onto a plain Swift, Kotlin, or TypeScript
/// enum, and giving a variant a payload would break both `Copy` and every
/// unit-pattern match. And no addition ever removes or repurposes `code`,
/// `details`, or `message`.
///
/// The envelope is the *only* place in this crate where something a binding
/// has never heard of is genuinely decodable by that binding. It buys nothing
/// for [`ErrorCode`] or for any other `#[non_exhaustive]` enum here; see the
/// crate-level *Forward compatibility* section for what those do and do not
/// promise.
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
    /// `None` means no detail was attached — see rule 1 on the type. `Some`
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
    /// numbers behind the failure are known. The envelope's kind and version
    /// come from the case itself, so nothing else has to be stated.
    ///
    /// `details` should describe the same failure as `code`; the pairing
    /// documented on each [`ErrorDetails`] case is the intended one. Nothing
    /// enforces it, because `code` remains authoritative either way: a
    /// caller branches on `code` and reads `details` only to enrich what it
    /// shows.
    ///
    /// A decoder rebuilding an error that crossed a boundary reaches for this
    /// when it recognised the kind, and for [`Error::with_raw_details`] when it
    /// did not.
    pub fn with_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: ErrorDetails,
    ) -> Error {
        Error {
            code,
            message: message.into(),
            details: Some(DetailEnvelope::Interpreted { detail: details }),
        }
    }

    /// Builds an SDK error from a code, a human-readable message, and a
    /// [`RawErrorDetails`] the caller could not project into a typed case.
    ///
    /// This is the decoder's other constructor, and the one that makes the
    /// envelope forward-compatible in practice. A binding layer that reads a
    /// raw envelope off the wire, finds a `kind` from a newer SDK, and skips
    /// the payload by its length passes what it read through here — so the
    /// detail survives as an observable value with a version and a kind,
    /// instead of being dropped or turned into a failure of its own.
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
    /// The short path for the common case: `None` covers all three of "no
    /// detail was attached", "a detail was attached whose kind this build does
    /// not know", and "the payload did not decode". A caller that wants to
    /// tell those apart — to log the second, which says the producer knew more
    /// than this build can express — matches the
    /// [`details`](Error::details) field, whose
    /// [`Opaque`](DetailEnvelope::Opaque) case is exactly that situation.
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

/// The raw, length-delimited form of an error's structured detail: the only
/// form that ever crosses a language boundary.
///
/// A version, a kind, and an opaque payload. That is the whole record, and it
/// never grows another field — which is the point of it. Growing a generated
/// record is not safely additive across Swift, Kotlin and TypeScript at once,
/// and adding a case to a generated data enum is no better, because a
/// generated decoder throws on the unknown tag before any fallback case can
/// be reached. A frozen record whose only variable part is a byte string with
/// its length in front is the one shape a reader of *any* vintage can consume
/// completely, so this is that record. Everything that varies varies inside
/// [`payload`](RawErrorDetails::payload), and everything that varies is
/// skippable.
///
/// It is deliberately **not** `#[non_exhaustive]`, unlike every other public
/// struct in this crate. `#[non_exhaustive]` says "this may grow fields",
/// which is the opposite of this type's contract; leaving it off makes the
/// promise a compile-time one and lets a binding layer destructure the record
/// exhaustively, secure that a later release cannot add a field it would then
/// silently ignore. [`RawErrorDetails::new`] exists for convenience, not
/// because a struct literal is unavailable.
///
/// # The encoding contract
///
/// This crate has no serialization dependency and will not grow one, so the
/// payload is opaque bytes here and the encoding is a *documented contract*
/// that each boundary implements. Framing the record's three fields is the
/// boundary's own business — UniFFI's record encoding, wasm-bindgen's, a JSON
/// object, anything — subject to one requirement, which is the requirement
/// this whole design rests on:
///
/// > `version` and the *length* of `kind` and `payload` are readable without
/// > interpreting either, so `payload` can be consumed or skipped by its
/// > length by a reader that has never heard of its kind.
///
/// ## Kinds
///
/// `kind` is a stable ASCII identifier, spelled exactly as the
/// [`ErrorDetails`] variant it projects to — `"InsufficientBalance"`,
/// `"NetworkMismatch"`, and so on; [`ErrorDetails::kind`] is the mapping.
/// Unlike [`Error::message`], these strings **are** part of the stability
/// contract: they are the discriminator, so one is never renamed, never
/// reused for a different meaning, and never case-shifted. A kind a reader
/// does not know is not an error — it skips the payload and keeps the
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
/// not know is skipped by its own length like any other string — which is the
/// same trick as the payload itself, one level down.
///
/// ## Payload layout per kind
///
/// | `kind` | Since | Payload fields, in order |
/// |--------|-------|--------------------------|
/// | `InsufficientBalance` | 1 | `required: u64`, `available: u64` — millisatoshis |
/// | `NetworkMismatch` | 1 | `expected: str`, `compatible: list<str>`, `observed_prefix: str` |
/// | `MixedModuleGenerations` | 1 | `modules: list<record { kind: str, generation: u32 }>` |
/// | `QuoteExpired` | 1 | `expires_at: u64` — epoch milliseconds, `already_executed: bool` |
/// | `QuoteTermsChanged` | 1 | `quoted_total: u64`, `current_total: u64` — millisatoshis |
/// | `BalanceNotEmpty` | 1 | `remaining: u64` — millisatoshis |
/// | `StorageInUse` | 1 | `location: str` |
/// | `SeedMismatch` | 1 | `location: str` |
///
/// ## What a reader must do
///
/// - **Unknown kind:** consume `payload` by its length, do not look inside it,
///   and keep the envelope as [`DetailEnvelope::Opaque`].
/// - **Known kind:** read that kind's fields in order. **Ignore any bytes
///   left over.** Trailing bytes are not an error; they are a newer producer
///   saying something this build has no field for.
/// - **Short or invalid payload:** a payload that ends before the last field,
///   holds invalid UTF-8, or holds a `bool` that is neither `0` nor `1` is
///   *uninterpretable*. Keep it as [`DetailEnvelope::Opaque`] and never guess
///   at a missing value — a fabricated amount in an error about amounts is
///   worse than no amount at all.
/// - **Never gate on `version`.** The kind alone decides whether a projection
///   is possible. A producer at envelope version 7 still emits version-1
///   kinds, so refusing to read a payload because `version` is unfamiliar
///   would throw away details this build understands perfectly well.
///   `version` is for saying how far ahead the producer is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RawErrorDetails {
    /// The envelope version the producing side declared it speaks — normally
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
    /// The stable kind discriminator — the [`ErrorDetails`] variant name this
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
    /// It may still be empty — a producer that stated a kind and nothing else
    /// is legal, and a reader treats the missing fields as an uninterpretable
    /// payload rather than guessing at them.
    pub payload: Vec<u8>,
}

impl RawErrorDetails {
    /// The details-envelope version this build speaks.
    ///
    /// Bumped when kinds are added, and never for any other reason — a kind
    /// that exists neither changes meaning nor changes payload layout, so
    /// nothing else can change what this number means. `0` is not a version:
    /// it is reserved for "the producer declared none".
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
/// This is the type of [`Error::details`], and it is a dichotomy rather than a
/// pair of half-filled fields because there are genuinely only two things that
/// can have happened. Either this side knew the kind and decoded the payload,
/// in which case the typed case is the whole truth and the bytes are spent; or
/// it did not, in which case the raw envelope is all there is and is worth
/// keeping. There is no third state, and no state in which both halves matter
/// at once.
///
/// Either way [`kind`](DetailEnvelope::kind) and
/// [`version`](DetailEnvelope::version) answer, so "what was this detail, and
/// how far ahead was the producer" is always loggable — the difference between
/// graceful degradation and a dropped diagnostic. A caller after the numbers
/// goes through [`Error::detail`] and never touches bytes.
///
/// Like [`RawErrorDetails`], and unlike every other public enum in this crate,
/// this one is deliberately **not** `#[non_exhaustive]`: "interpreted" and "not
/// interpreted" exhausts the possibilities for all time, so there is no third
/// case to reserve room for, and a Rust caller should get a total match instead
/// of a wildcard arm it can never reach. It is also, for the same reason, the
/// one data enum here that would be safe to transport as a generated
/// enum-with-associated-values — two frozen cases can never present a decoder
/// with an unknown tag. The recommended boundary shape is still an optional
/// [`RawErrorDetails`] with each side projecting locally, since that is the
/// form the encoding contract is written against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailEnvelope {
    /// This side knew the kind and read the payload, so the detail is
    /// available as a typed [`ErrorDetails`] case.
    Interpreted {
        /// The typed detail. Its [`kind`](ErrorDetails::kind) and
        /// [`version`](ErrorDetails::version) are what the envelope reports,
        /// so nothing has to be carried alongside it.
        detail: ErrorDetails,
    },
    /// This side could not project the detail — an unrecognized kind, or a
    /// payload that did not decode — so the raw envelope is kept as it
    /// arrived.
    ///
    /// This is the graceful-degradation state and an ordinary value, not a
    /// failure of its own: [`Error::code`] and [`Error::message`] still
    /// describe the failure completely and correctly. Log it, because it says
    /// the producer knew more than this build can express, and carry on
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
    /// [`RawErrorDetails::kind`], which may be a name from a newer SDK — or
    /// empty, where the producing side stated none.
    pub fn kind(&self) -> &str {
        match self {
            DetailEnvelope::Interpreted { detail } => detail.kind(),
            DetailEnvelope::Opaque { raw } => &raw.kind,
        }
    }

    /// The envelope version this detail belongs to.
    ///
    /// For [`Interpreted`](DetailEnvelope::Interpreted) that is the version
    /// which introduced the case, from [`ErrorDetails::version`] — never later
    /// than [`RawErrorDetails::CURRENT_VERSION`], since this build could only
    /// project a case it knows. For [`Opaque`](DetailEnvelope::Opaque) it is
    /// the version the producing side declared it speaks, or `0` where it
    /// declared none, which is what makes "this came from something newer than
    /// me" a statement rather than a guess.
    pub fn version(&self) -> u32 {
        match self {
            DetailEnvelope::Interpreted { detail } => detail.version(),
            DetailEnvelope::Opaque { raw } => raw.version,
        }
    }

    /// The typed detail, where this side could project one.
    pub fn typed(&self) -> Option<&ErrorDetails> {
        match self {
            DetailEnvelope::Interpreted { detail } => Some(detail),
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
/// This is the typed half of the envelope described under [`Error`]'s *details
/// envelope* section: the reserved place where the numbers behind a failure
/// are reported, so that they are never available only by parsing
/// [`Error::message`]. Each case names the failure it accompanies and
/// carries exactly the values a caller needs to render or act on it.
///
/// A case here is always a **local projection** of a [`RawErrorDetails`],
/// never something decoded straight off a wire tag. That is what makes the
/// envelope forward-compatible, and it is why this enum has no "unrecognized"
/// case of its own: a kind with no projection is
/// [`DetailEnvelope::Opaque`], with the raw envelope still there to read.
///
/// # Shape, and why it is this shape
///
/// A flat data enum of plain records: no generics, no tuple variants, no
/// borrowed data, no trait objects, and no nesting beyond a list of a small
/// record. That is the intersection of what UniFFI and wasm can both express
/// directly, so this type generates into a Swift or Kotlin sealed
/// enum-with-associated-values and a TypeScript discriminated union
/// mechanically.
///
/// What is *not* mechanical, and is the cost of doing this honestly, is the
/// projection: each target hand-writes the map from a `kind` string plus
/// payload bytes to its own local case, and that map is what the boundary's
/// cross-version conformance tests exercise. The generated enum is only ever
/// built from cases the target already knows, so it never has to decode an
/// unknown tag — which is precisely the failure that made a bare data enum
/// unusable as the wire form.
///
/// The enum is `#[non_exhaustive]`; its **variants deliberately are not**.
/// Rust callers must therefore write a wildcard arm, while a binding layer
/// can still *construct* any case it needs (which [`Error::with_details`] and
/// [`DetailEnvelope::Interpreted`] exist for). The asymmetry is the point: a
/// case may
/// be added, but a case that exists never grows a field, because a generated
/// record that grows a field is exactly the thing that is not safely additive
/// across Swift, Kotlin and TypeScript at once. More detail about an existing
/// situation therefore arrives as a *new, more specific case* at a new
/// envelope version, not as an extra field here.
///
/// # Versioning
///
/// [`RawErrorDetails::CURRENT_VERSION`] is the envelope version this build
/// speaks, and [`ErrorDetails::version`] reports the version that introduced
/// a particular case. A case's meaning, and its payload layout, are frozen at
/// that version: never redefined, never given a wider or narrower reading,
/// never repurposed. Adding cases bumps `CURRENT_VERSION`; nothing else does.
///
/// A producer ahead of its consumer may emit a kind the consumer has no
/// projection for, and the consumer keeps the raw envelope as
/// [`DetailEnvelope::Opaque`] — which is why a version mismatch degrades to
/// "there is a detail here I cannot interpret", stated with a version and a
/// kind, rather than to a crash, a silent drop, or a payload read as the wrong
/// case.
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
    ///
    /// The two sides of this mismatch are known to very different precisions,
    /// and the case is shaped to say so rather than to look symmetric. The
    /// federation's network is read from its own configuration and is exact.
    /// The rejected value's is **not knowable exactly from the value**, and an
    /// earlier draft of this case that asked for an exact `actual: Network`
    /// could only have been satisfied by fabricating one:
    ///
    /// - testnet3, testnet4 and signet share address encodings in the cases
    ///   that matter — a `tb1…` address is evidence of "some test network",
    ///   not of which one;
    /// - BOLT11 exposes a single `tb` currency for both public testnets, so a
    ///   `tb` invoice narrows the answer to two networks and no further;
    /// - BOLT11 also has `sb` (simnet), which [`Network`] cannot represent at
    ///   all.
    ///
    /// So what is carried is what was observed: the set of networks the value
    /// could have been for, and the prefix it was actually spelled with. A
    /// diagnostic then says "a testnet invoice, and this federation is on
    /// mainnet" — true — instead of naming a network nobody measured.
    NetworkMismatch {
        /// The network the federation operates on — the one a value had to
        /// match. Read from federation configuration, so exact.
        expected: Network,
        /// Every network the rejected value could have been intended for,
        /// given what its encoding actually proves. Unordered and free of
        /// duplicates.
        ///
        /// The mismatch is exactly that `expected` is not among these. A
        /// single entry means the value's encoding pinned one network (`bc1…`,
        /// `bcrt1…`, a BOLT11 `tbs`); several mean it did not (`tb1…` is
        /// testnet3, testnet4 or signet; a BOLT11 `tb` is either public
        /// testnet).
        ///
        /// **Empty** means the value named a network this crate's [`Network`]
        /// enum cannot represent — a BOLT11 `sb` (simnet) invoice is the case
        /// that exists today. Empty is therefore a real, meaningful answer,
        /// not a missing one, and it still proves a mismatch: the federation's
        /// network is certainly not in an empty set.
        ///
        /// Do not treat the list as exhaustive of what the *producer* knew. A
        /// reader that cannot name every entry keeps the ones it can name and
        /// drops the rest, which can only shrink the set — so the conclusion
        /// "`expected` is not in here" survives, while completeness does not.
        /// `observed_prefix` is the ground truth for anything else.
        compatible: Vec<Network>,
        /// The network prefix or BOLT11 currency the rejected value was
        /// actually spelled with, verbatim and lowercased: `"bc"`, `"tb"`,
        /// `"tbs"`, `"bcrt"`, `"sb"`, or a base58 address's leading character.
        ///
        /// The ground truth of the whole case, and the only field that can
        /// describe a network this SDK has no name for. Show it in a
        /// diagnostic. Empty where the layer raising the error genuinely had
        /// no prefix to report — never a fabricated one.
        ///
        /// This is a diagnostic to display, not a discriminator to branch on:
        /// branch on `compatible` and `expected`, whose meanings are fixed.
        observed_prefix: String,
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
}

impl ErrorDetails {
    /// The stable kind identifier for this case: the discriminator that
    /// crosses a boundary in [`RawErrorDetails::kind`].
    ///
    /// Spelled exactly as the variant, and part of the stability contract —
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
            | ErrorDetails::SeedMismatch { .. } => 1,
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
/// marked `#[non_exhaustive]` so that Rust callers must write non-exhaustive
/// matches, with a wildcard arm; the compiler enforces that, and a variant
/// added later is therefore not a breaking change for a Rust caller.
///
/// # What that does *not* buy across a binding
///
/// `#[non_exhaustive]` is a Rust-only guarantee, and it is worth being blunt
/// about the limit because an earlier version of this documentation was not.
/// It does not make a generated Swift, Kotlin or TypeScript decoder tolerate a
/// tag it has never seen: UniFFI's generated Swift decoder throws
/// `unexpectedEnumCase` on an unknown discriminant, and no attribute on the
/// Rust side changes that. A pre-generated binding pinned to an older SDK,
/// meeting a code added since, fails to decode the error — it does not quietly
/// receive an "unknown" case.
///
/// There are exactly two ways to be safe, and both cost something:
///
/// - **Regenerate the binding against the SDK version it talks to.** This is
///   the default expectation for this crate, and the cheap answer: the
///   binding and the SDK ship together, so no vintage gap exists.
/// - **Hand-write an adapter for the boundary, and test it across
///   versions.** For a fieldless enum like this one that is genuinely cheap:
///   carry the code across as its stable variant *name* — a length-delimited
///   string, so an unfamiliar one is read and skipped like any other — and
///   project it into the target's own enum with an explicit unknown fallback.
///   What it costs is a per-target map that must be kept in step and a
///   cross-version conformance suite that decodes a newer producer's output
///   with an older consumer's adapter. Without those tests the tolerance is a
///   claim, not a property.
///
/// [`ErrorDetails`] is the one place in this crate where forward decodability
/// is built in rather than left to the boundary, because there the *payload*
/// is length-delimited opaque bytes; see [`RawErrorDetails`].
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
    ///
    /// [`ErrorDetails::NetworkMismatch`] carries the federation's network
    /// exactly, and, for the rejected value, what its encoding actually
    /// proves: the set of networks it could have been for, plus the prefix it
    /// was spelled with. That is deliberately not one exact network — a
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
        // the compatible set is empty and the prefix is the only ground truth
        // — which is a real answer, not a missing one.
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
        // so — the shape carries precision where precision exists.
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
    fn an_interpreted_envelope_reports_the_kind_and_version_of_its_case() {
        let envelope = DetailEnvelope::Interpreted {
            detail: ErrorDetails::BalanceNotEmpty {
                remaining: Amount::from_msats(7_000),
            },
        };
        // Kind and version come from the case, so nothing is carried beside it
        // and nothing can drift out of step with it.
        assert_eq!(envelope.kind(), "BalanceNotEmpty");
        assert_eq!(envelope.version(), 1);
        assert!(envelope.version() <= RawErrorDetails::CURRENT_VERSION);
        assert!(envelope.is_interpreted());
        assert!(envelope.typed().is_some());
        // The bytes were spent decoding it; the boundary encoder re-derives
        // them from the typed case if it ever crosses again.
        assert!(envelope.raw().is_none());
    }

    #[test]
    fn an_uninterpretable_envelope_leaves_code_and_message_intact() {
        // A binding built against an older SDK meeting a kind it has no
        // projection for. It read the whole record — version, kind, and the
        // payload by its length — and simply has no typed case to build, so
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
        // something newer than me" is a statement rather than a guess — and it
        // reads off the envelope without caring which state it is in.
        assert_eq!(envelope.version(), 9);
        assert!(envelope.version() > RawErrorDetails::CURRENT_VERSION);
        assert_eq!(envelope.kind(), "SomethingNewerThanThisBuild");

        let raw = envelope.raw().expect("an opaque envelope keeps its bytes");
        // The opaque payload survived intact, skipped rather than parsed —
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

    #[test]
    fn a_payload_that_a_reader_understood_projects_to_the_typed_case() {
        // The decoder's happy path, spelled out against the documented
        // encoding: `InsufficientBalance` is two big-endian u64 millisatoshi
        // fields, required then available.
        let payload = [0u8, 0, 0, 0, 0, 0, 0x05, 0xDC, 0, 0, 0, 0, 0, 0, 0x04, 0xB0];
        let raw = RawErrorDetails::new(
            RawErrorDetails::CURRENT_VERSION,
            "InsufficientBalance",
            payload,
        );
        assert_eq!(raw.payload.len(), 16);

        // What a boundary's projection does with it, by hand: dispatch on the
        // kind, read the fields in order.
        let projected = match raw.kind.as_str() {
            "InsufficientBalance" => {
                let required = u64::from_be_bytes(raw.payload[..8].try_into().unwrap());
                let available = u64::from_be_bytes(raw.payload[8..16].try_into().unwrap());
                Some(ErrorDetails::InsufficientBalance {
                    required: Amount::from_msats(required),
                    available: Amount::from_msats(available),
                })
            }
            // An unknown kind never looks inside the payload at all.
            _ => None,
        };

        let err = match projected {
            Some(detail) => {
                Error::with_details(ErrorCode::InsufficientBalance, "balance is short", detail)
            }
            None => Error::with_raw_details(
                ErrorCode::InsufficientBalance,
                "balance is short",
                raw.clone(),
            ),
        };
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
        assert_eq!(err.details.expect("a detail").kind(), raw.kind);
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
