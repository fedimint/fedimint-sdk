//! The Bitcoin network a federation operates on.

use fedimint_core::bitcoin;

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
/// networks, and a new variant here is an additive change for Rust callers,
/// who must write a non-exhaustive match with a wildcard arm.
///
/// It is not additive for a generated binding, though, and the crate-level
/// documentation's forward-compatibility section explains why: a decoder
/// pinned to an older SDK fails on a tag it has never seen rather than
/// mapping it to an unknown case. A binding is expected to be regenerated
/// alongside the SDK it talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Network {
    /// Bitcoin mainnet.
    Bitcoin,
    /// The long-running public Bitcoin testnet, testnet3.
    Testnet,
    /// Testnet4: the successor public testnet, introduced after testnet3's
    /// difficulty and supply problems made it unreliable to test against.
    /// A separate network with its own genesis block, not a continuation of
    /// [`Testnet`](Self::Testnet).
    Testnet4,
    /// Signet: a public test network secured by a signer rather than
    /// proof-of-work, generally more stable than testnet.
    Signet,
    /// A privately operated regression-test network, typically used for
    /// local development.
    Regtest,
}

impl Network {
    /// Every network this enum can name.
    ///
    /// Crate-internal. It exists so that "which networks is this value
    /// compatible with?" can be answered by asking each candidate in turn,
    /// which is the only way `bitcoin` offers: a validated address keeps no
    /// `Network` of its own.
    // Unlike `to_bitcoin` below, an exhaustive match the compiler enforces, nothing
    // catches a new variant left out of this list: keep the two in sync by hand.
    pub(crate) const ALL: [Network; 5] = [
        Network::Bitcoin,
        Network::Testnet,
        Network::Testnet4,
        Network::Signet,
        Network::Regtest,
    ];

    /// This network as the `bitcoin` crate spells it.
    ///
    /// Crate-internal: no conversion between this enum and an upstream type is
    /// part of the public API.
    pub(crate) fn to_bitcoin(self) -> bitcoin::Network {
        match self {
            Network::Bitcoin => bitcoin::Network::Bitcoin,
            Network::Testnet => bitcoin::Network::Testnet,
            Network::Testnet4 => bitcoin::Network::Testnet4,
            Network::Signet => bitcoin::Network::Signet,
            Network::Regtest => bitcoin::Network::Regtest,
        }
    }
}
