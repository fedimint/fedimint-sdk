//! The Bitcoin network a federation operates on.

/// The Bitcoin network a federation's on-chain module is configured for.
///
/// Every federation operates on exactly one network; this value is read
/// from federation configuration and reported on
/// [`FederationPreview`](crate::FederationPreview) and the federation handle.
/// It is also what an [`Address`](crate::Address) is checked against when an
/// on-chain quote is requested, failing with
/// [`ErrorCode::NetworkMismatch`](crate::ErrorCode::NetworkMismatch) on
/// disagreement.
///
/// This enum is `#[non_exhaustive]`: Bitcoin has occasionally grown new test
/// networks, and a new variant here is an additive change, not a breaking
/// one. Rust callers must write a non-exhaustive match (with a wildcard
/// arm); foreign bindings map an unrecognized future variant to an explicit
/// unknown case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Network {
    /// Bitcoin mainnet.
    Bitcoin,
    /// The long-running public Bitcoin testnet.
    Testnet,
    /// Signet: a public test network secured by a signer rather than
    /// proof-of-work, generally more stable than testnet.
    Signet,
    /// A privately operated regression-test network, typically used for
    /// local development.
    Regtest,
}
