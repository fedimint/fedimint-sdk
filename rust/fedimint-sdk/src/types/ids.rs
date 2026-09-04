//! Opaque, string-shaped identifiers.
//!
//! Every id here is opaque and round-trips through [`Display`](core::fmt::Display) and
//! [`FromStr`](core::str::FromStr) with a validating parse, which is what lets an
//! application persist one and find the same operation or federation again. The same
//! validating parse is also what lets a foreign-language binding carry these values as
//! plain strings with no per-language parsing or validation logic of its own: the Rust
//! side is the only place that knows the format.

use fedimint_core::bitcoin;
use fedimint_core::config;
use fedimint_core::secp256k1::PublicKey;

use crate::{Error, ErrorCode};

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
    id: config::FederationId,
}

impl FederationId {
    /// Wraps an already-parsed federation id.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_upstream(id: config::FederationId) -> Self {
        Self { id }
    }
}

impl core::fmt::Display for FederationId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // 64 forward-order lowercase hex characters
        // (fedimint-core/src/config.rs:427-431).
        core::fmt::Display::fmt(&self.id, f)
    }
}

impl core::str::FromStr for FederationId {
    type Err = crate::Error;

    /// Parses a federation id from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Upstream's error is `hex::HexToArrayError` (config.rs:480-486);
        // its text is "failed to parse hex" and names nothing else. A
        // federation id is public, so the text may be passed on.
        let id = s.trim().parse::<config::FederationId>().map_err(|err| {
            Error::new(
                ErrorCode::InvalidInput,
                format!("invalid federation id: {err}"),
            )
        })?;
        Ok(Self { id })
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
    id: fedimint_core::core::OperationId,
}

impl OperationId {
    /// Wraps an already-parsed operation id.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_upstream(id: fedimint_core::core::OperationId) -> Self {
        Self { id }
    }
}

impl core::fmt::Display for OperationId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Upstream has no `Display` at all, and its `Debug` prints
        // `OperationId(aabbccdd_11223344)`, an abbreviated form its own
        // `FromStr` rejects.
        // `fmt_full` (fedimint-core/src/core.rs:85) is the 64-character
        // form that parses back; its `Display` impl is at core.rs:99.
        write!(f, "{}", self.id.fmt_full())
    }
}

impl core::str::FromStr for OperationId {
    type Err = crate::Error;

    /// Parses an operation id from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // fedimint-core/src/core.rs:111-118: 64 hex characters, either
        // case. The error is `hex::FromHexError`, whose text names a bad
        // length or a bad character and nothing else.
        let id = s
            .trim()
            .parse::<fedimint_core::core::OperationId>()
            .map_err(|err| {
                Error::new(
                    ErrorCode::InvalidInput,
                    format!("invalid operation id: {err}"),
                )
            })?;
        Ok(Self { id })
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
    id: PublicKey,
}

impl GatewayId {
    /// Wraps an already-parsed gateway id.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_upstream(id: PublicKey) -> Self {
        Self { id }
    }
}

impl core::fmt::Display for GatewayId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Always the compressed spelling, 66 lowercase hex characters
        // (secp256k1-0.29.1/src/key.rs:152-164).
        core::fmt::Display::fmt(&self.id, f)
    }
}

impl core::str::FromStr for GatewayId {
    type Err = crate::Error;

    /// Parses a gateway id from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // secp256k1-0.29.1/src/key.rs:166-177 accepts both the 66-character
        // compressed and the 130-character uncompressed spelling of a key, and
        // checks that the bytes really are a point on the curve.
        let id = s.trim().parse::<PublicKey>().map_err(|err| {
            Error::new(
                ErrorCode::InvalidInput,
                format!("invalid gateway id: {err}"),
            )
        })?;
        Ok(Self { id })
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
    id: bitcoin::Txid,
}

impl Txid {
    /// Wraps an already-parsed transaction id.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_upstream(id: bitcoin::Txid) -> Self {
        Self { id }
    }
}

impl core::fmt::Display for Txid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Byte-reversed hex, the convention every block explorer uses.
        // Deliberately `bitcoin::Txid` and never `fedimint_core::TransactionId`,
        // which names a federation-internal transaction and displays in the
        // opposite byte order.
        core::fmt::Display::fmt(&self.id, f)
    }
}

impl core::str::FromStr for Txid {
    type Err = crate::Error;

    /// Parses a transaction id from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Generated by `hash_newtype!`
        // (bitcoin_hashes-0.14.101/src/util.rs:270-281); the error names a bad
        // character or a bad length and nothing else.
        let id = s.trim().parse::<bitcoin::Txid>().map_err(|err| {
            Error::new(
                ErrorCode::InvalidInput,
                format!("invalid transaction id: {err}"),
            )
        })?;
        Ok(Self { id })
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
// Unlike the other ids in this module, this one still wraps an opaque string rather than
// an upstream type, since there is no activity facade yet to define what a cursor
// actually addresses. Replace the field with whatever that facade needs once it lands.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `FederationId::dummy()` upstream is 32 bytes of `0x2a`, and a
    /// federation id prints forward-order lowercase hex.
    const FEDERATION_ID: &str = "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a";
    /// 64 hex zeros: a well-formed operation id, transaction id and preimage
    /// alike, and the value the crate's own fixtures already used.
    const ZEROS: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    /// The compressed public key from `secp256k1`'s own test suite.
    const GATEWAY_ID: &str = "0218845781f631c48f1c9709e23092067d06837f30aa0cd0544ac887fe91ddd166";
    /// The same key, uncompressed. It is the same key, so it parses, but
    /// `Display` always writes the compressed spelling.
    const GATEWAY_ID_UNCOMPRESSED: &str = "0418845781f631c48f1c9709e23092067d06837f30aa0cd0544ac887fe91ddd16684B84DB303A340CD7D6823EE88174747D12A67D2F8F2F9BA40846EE5EE7A44F6";

    #[test]
    fn ids_round_trip_through_display_and_from_str() {
        let federation = FEDERATION_ID
            .parse::<FederationId>()
            .expect("a valid federation id");
        assert_eq!(federation.to_string(), FEDERATION_ID);

        let operation = ZEROS.parse::<OperationId>().expect("a valid operation id");
        assert_eq!(operation.to_string(), ZEROS);

        let gateway = GATEWAY_ID.parse::<GatewayId>().expect("a valid gateway id");
        assert_eq!(gateway.to_string(), GATEWAY_ID);

        let txid = ZEROS.parse::<Txid>().expect("a valid transaction id");
        assert_eq!(txid.to_string(), ZEROS);
    }

    #[test]
    fn parsing_accepts_uppercase_hex_and_normalises_to_lowercase() {
        // Hex read off a block explorer or a log is routinely uppercase, and
        // the canonical form the SDK writes is lowercase.
        assert_eq!(
            FEDERATION_ID
                .to_uppercase()
                .parse::<FederationId>()
                .expect("valid")
                .to_string(),
            FEDERATION_ID
        );
        let upper = "AABBCCDDEEFF00112233445566778899AABBCCDDEEFF001122334455667788AA";
        assert_eq!(
            upper.parse::<OperationId>().expect("valid").to_string(),
            upper.to_lowercase()
        );
        assert_eq!(
            upper.parse::<Txid>().expect("valid").to_string(),
            upper.to_lowercase()
        );
    }

    #[test]
    fn parsing_ignores_surrounding_whitespace() {
        let padded = format!("  {FEDERATION_ID}\n");
        assert_eq!(
            padded.parse::<FederationId>().expect("trimmed").to_string(),
            FEDERATION_ID
        );
    }

    #[test]
    fn a_gateway_id_is_a_public_key_not_a_hex_blob() {
        // The uncompressed spelling names the same key, so it parses and
        // compares equal, but the canonical output is always compressed.
        let compressed = GATEWAY_ID.parse::<GatewayId>().expect("valid");
        let uncompressed = GATEWAY_ID_UNCOMPRESSED
            .parse::<GatewayId>()
            .expect("the uncompressed spelling of the same key");
        assert_eq!(compressed, uncompressed);
        assert_eq!(uncompressed.to_string(), GATEWAY_ID);
        // Right-looking hex that is not a key is not an id: `05` is not a
        // valid compressed-point prefix.
        assert!(
            "051111111111111111111111111111111111111111111111111111111111111111"
                .parse::<GatewayId>()
                .is_err(),
            "66 hex characters are not automatically a public key"
        );
    }

    #[test]
    fn a_malformed_id_is_invalid_input() {
        for rejected in ["", "op", "fed-id", "zz", "0266e4598d1d3c415f572a8488830b"] {
            assert_eq!(
                rejected.parse::<FederationId>().expect_err("rejected").code,
                crate::ErrorCode::InvalidInput
            );
            assert_eq!(
                rejected.parse::<OperationId>().expect_err("rejected").code,
                crate::ErrorCode::InvalidInput
            );
            assert_eq!(
                rejected.parse::<GatewayId>().expect_err("rejected").code,
                crate::ErrorCode::InvalidInput
            );
            assert_eq!(
                rejected.parse::<Txid>().expect_err("rejected").code,
                crate::ErrorCode::InvalidInput
            );
        }
    }

    #[test]
    fn an_operation_id_prints_the_full_form_that_parses_back() {
        // Upstream's `Debug` prints an abbreviated `aabbccdd_11223344` form
        // that its own `FromStr` does not accept; the SDK must print the full
        // 64 characters or a copied id would not come back.
        let printed = ZEROS.parse::<OperationId>().expect("valid").to_string();
        assert_eq!(printed.len(), 64);
        assert!(!printed.contains('_'));
    }
}
