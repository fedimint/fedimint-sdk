//! Opaque, string-shaped identifiers.
//!
//! Every type in this module wraps an internal string representation that
//! callers never see directly. Each round-trips losslessly through
//! [`Display`](core::fmt::Display) and [`FromStr`](core::str::FromStr) with
//! validating parse, which is what lets a foreign-language binding carry
//! these values as plain strings with no per-language parsing or validation
//! logic of its own: the Rust side is the only place that knows the format.

/// Uniquely identifies a federation.
///
/// A `FederationId` is derived from the federation's consensus configuration
/// and is the same for every client and guardian of that federation. It is
/// opaque: callers should treat it as an identifier to compare, store, and
/// pass back to the SDK, not as a structured value to parse apart. It
/// round-trips through [`Display`](core::fmt::Display) and
/// [`FromStr`](core::str::FromStr), so a binding can persist or transmit it
/// as a plain string and reconstruct it later.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FederationId {
    id: String,
}

impl FederationId {
    /// Wraps an already-validated federation id string.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_raw(raw: String) -> Self {
        Self { id: raw }
    }
}

impl core::fmt::Display for FederationId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let _ = &self.id;
        unimplemented!()
    }
}

impl core::str::FromStr for FederationId {
    type Err = crate::Error;

    /// Parses a federation id from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}

/// Identifies one operation (a send, a receive, a recovery, ...) within a
/// federation.
///
/// `OperationId`s are generated when an operation is created and are stable
/// for that operation's entire lifetime, including across process restarts:
/// they are what operation lookup and activity history use to name a
/// specific piece of ongoing or past work. The id alone does not reveal what
/// kind of operation it names; a dedicated accessor elsewhere in the crate
/// reports that. It is opaque and round-trips through
/// [`Display`](core::fmt::Display) and [`FromStr`](core::str::FromStr).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationId {
    id: String,
}

impl OperationId {
    /// Wraps an already-validated operation id string.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_raw(raw: String) -> Self {
        Self { id: raw }
    }
}

impl core::fmt::Display for OperationId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let _ = &self.id;
        unimplemented!()
    }
}

impl core::str::FromStr for OperationId {
    type Err = crate::Error;

    /// Parses an operation id from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}

/// Identifies a lightning gateway registered with a federation.
///
/// Used to report which gateway routed a payment (see the lightning facade's
/// routing type) and, in principle, to reason about gateway choice in
/// diagnostics or UI. It is opaque and round-trips through
/// [`Display`](core::fmt::Display) and [`FromStr`](core::str::FromStr);
/// callers are not expected to construct one by hand outside of parsing a
/// value the SDK itself produced.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GatewayId {
    id: String,
}

impl GatewayId {
    /// Wraps an already-validated gateway id string.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_raw(raw: String) -> Self {
        Self { id: raw }
    }
}

impl core::fmt::Display for GatewayId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let _ = &self.id;
        unimplemented!()
    }
}

impl core::str::FromStr for GatewayId {
    type Err = crate::Error;

    /// Parses a gateway id from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}

/// A Bitcoin transaction id, used for on-chain peg-in and peg-out receipts.
///
/// This names an on-chain Bitcoin transaction (for linking out to a block
/// explorer, for example), not a federation-internal identifier. It is
/// opaque here rather than a fixed-size byte array so it can round-trip
/// through [`Display`](core::fmt::Display) and
/// [`FromStr`](core::str::FromStr) uniformly with the rest of this module;
/// the parse validates that the string is a well-formed transaction id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Txid {
    id: String,
}

impl Txid {
    /// Wraps an already-validated transaction id string.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_raw(raw: String) -> Self {
        Self { id: raw }
    }
}

impl core::fmt::Display for Txid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let _ = &self.id;
        unimplemented!()
    }
}

impl core::str::FromStr for Txid {
    type Err = crate::Error;

    /// Parses a transaction id from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}

/// An opaque pagination token for paginated activity history.
///
/// A `Cursor` is obtained only from a previous page of activity results and
/// is meant to be passed back verbatim to fetch the following page. Callers
/// must not construct one from an arbitrary string or attempt to interpret
/// its contents: its internal format is free to change between SDK
/// versions since it is never meant to be handled as anything but an opaque
/// value obtained from, and returned to, this crate. It still implements
/// [`Display`](core::fmt::Display) and [`FromStr`](core::str::FromStr) like
/// the other ids in this module, purely so it can be stored and reloaded
/// (e.g. to resume paging after an app restart) without a bespoke
/// serialization path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cursor {
    token: String,
}

impl Cursor {
    /// Wraps an already-validated cursor token.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_raw(raw: String) -> Self {
        Self { token: raw }
    }
}

impl core::fmt::Display for Cursor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let _ = &self.token;
        unimplemented!()
    }
}

impl core::str::FromStr for Cursor {
    type Err = crate::Error;

    /// Parses a cursor from a string previously produced by this type's
    /// `Display` impl. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}
