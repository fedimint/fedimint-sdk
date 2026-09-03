//! Bitcoin addresses.

/// A Bitcoin address, for on-chain withdrawals.
///
/// Parsing an `Address` only checks that the string is a well-formed
/// address for *some* Bitcoin network: it does not yet know which
/// federation it will be used with, so it cannot check network agreement at
/// parse time. That check happens later, when the address is used to
/// request an on-chain quote against a specific federation: if the
/// address's network does not match that federation's network, the call
/// fails with [`ErrorCode::NetworkMismatch`](crate::ErrorCode::NetworkMismatch)
/// rather than silently sending to the wrong chain. Sending needs no second
/// check, because [`Onchain::send`](crate::Onchain::send) takes only the
/// quote the address was bound into.
///
/// `Address` is opaque and round-trips through
/// [`Display`](core::fmt::Display) and [`FromStr`](core::str::FromStr) with
/// a validating parse.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address {
    address: String,
}

impl Address {
    /// Wraps an already-validated Bitcoin address string.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_raw(raw: String) -> Self {
        Self { address: raw }
    }
}

impl core::fmt::Display for Address {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let _ = &self.address;
        unimplemented!()
    }
}

impl core::str::FromStr for Address {
    type Err = crate::Error;

    /// Parses a Bitcoin address from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value. Does not check network agreement; see the type
    /// documentation.
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}
