//! Bitcoin addresses.

use fedimint_core::bitcoin;
use fedimint_core::bitcoin::address::NetworkUnchecked;

use super::Network;
use crate::{Error, ErrorCode};

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
    address: bitcoin::Address<NetworkUnchecked>,
}

impl Address {
    /// Wraps an already-parsed Bitcoin address.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_upstream(address: bitcoin::Address<NetworkUnchecked>) -> Self {
        Self { address }
    }

    /// Every network this address could have been intended for.
    ///
    /// Crate-internal: this is what fills
    /// [`ErrorDetails::NetworkMismatch::compatible`](crate::ErrorDetails::NetworkMismatch)
    /// when an on-chain quote rejects an address. Deliberately a set rather
    /// than one network, because an encoding often does not pin one: a base58
    /// test-family address is valid for testnet3, testnet4, signet and regtest
    /// alike.
    pub(crate) fn compatible_networks(&self) -> Vec<Network> {
        // A validated `bitcoin::Address` keeps only a `NetworkKind` or a
        // `KnownHrp`, neither of which is exposed, so the question can only be
        // answered by asking each candidate
        // (bitcoin-0.32.102/src/address/mod.rs:721-728).
        Network::ALL
            .into_iter()
            .filter(|network| self.address.is_valid_for_network(network.to_bitcoin()))
            .collect()
    }

    /// The network prefix this address was actually spelled with, lowercased.
    ///
    /// Crate-internal: this is what fills
    /// [`ErrorDetails::NetworkMismatch::observed_prefix`](crate::ErrorDetails::NetworkMismatch).
    /// A segwit address reports its bech32 HRP (`"bc"`, `"tb"`, `"bcrt"`); a
    /// base58 address reports its leading version character (`"1"`, `"3"`,
    /// `"m"`, `"n"`, `"2"`).
    pub(crate) fn observed_prefix(&self) -> String {
        let rendered = self.address.assume_checked_ref().to_string().to_lowercase();
        // `KnownHrp` (bitcoin-0.32.102/src/address/mod.rs:194-236) has exactly
        // these three values, so scanning for them is exhaustive rather than a
        // guess, and the `1` guards against a base58 address that merely starts
        // with the same letters.
        for hrp in ["bc", "tb", "bcrt"] {
            if let Some(rest) = rendered.strip_prefix(hrp)
                && rest.starts_with('1')
            {
                return hrp.to_owned();
            }
        }
        rendered
            .chars()
            .next()
            .map(String::from)
            .unwrap_or_default()
    }
}

impl core::fmt::Display for Address {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // bitcoin 0.32 deliberately implements `Display` only for
        // `Address<NetworkChecked>` (address/mod.rs:797). `assume_checked_ref`
        // borrows the checked view without consuming or revalidating anything
        // (address/mod.rs:696-698), which is right here: this type is
        // documented as network-unchecked until a federation is in hand.
        core::fmt::Display::fmt(self.address.assume_checked_ref(), f)
    }
}

impl core::str::FromStr for Address {
    type Err = crate::Error;

    /// Parses a Bitcoin address from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value. Does not check network agreement; see the type
    /// documentation.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // `bitcoin::address::ParseError` (address/error.rs:110-132) names no
        // secret, so its text is safe to pass on to a human.
        let address = s
            .trim()
            .parse::<bitcoin::Address<NetworkUnchecked>>()
            .map_err(|err| {
                Error::new(
                    ErrorCode::InvalidInput,
                    format!("invalid bitcoin address: {err}"),
                )
            })?;
        Ok(Self { address })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Address strings taken verbatim from `bitcoin`'s own test suite
    /// (`bitcoin-0.32.102/src/address/mod.rs`), so each is a real address of
    /// the stated kind rather than something shaped like one.
    const MAINNET_P2WPKH: &str = "bc1qvzvkjn4q3nszqxrv3nraga2r822xjty3ykvkuw";
    const MAINNET_P2PKH: &str = "132F25rTsvBdp9JzLLBHP5mvGY66i1xdiM";
    const MAINNET_P2SH: &str = "33iFwdLuRpW1uK1RTRqsoi8rR4NpDzk66k";
    const TESTNETS_P2WSH: &str = "tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3q0sl5k7";
    const TESTNETS_P2SH: &str = "2N83imGV3gPwBzKJQvWJ7cRUY2SpUyU6A5e";
    const TESTNET_P2PKH: &str = "mqkhEMH6NCeYjFybv7pvFC22MFeaNT9AQC";
    const REGTEST_P2WPKH: &str = "bcrt1q2nfxmhd4n3c8834pj72xagvyr9gl57n5r94fsl";

    #[test]
    fn addresses_round_trip_through_display_and_from_str() {
        for address in [
            MAINNET_P2WPKH,
            MAINNET_P2PKH,
            MAINNET_P2SH,
            TESTNETS_P2WSH,
            TESTNETS_P2SH,
            TESTNET_P2PKH,
            REGTEST_P2WPKH,
        ] {
            let parsed = address.parse::<Address>().expect("a valid address");
            assert_eq!(parsed.to_string(), address);
        }
    }

    #[test]
    fn parsing_normalises_case_and_ignores_surrounding_whitespace() {
        // BIP-173 allows an all-uppercase bech32 string, which is what a QR
        // code encoder emits to stay in alphanumeric mode.
        let upper = MAINNET_P2WPKH
            .to_uppercase()
            .parse::<Address>()
            .expect("uppercase bech32 is valid");
        assert_eq!(upper.to_string(), MAINNET_P2WPKH);
        let padded = format!("  {REGTEST_P2WPKH}\n");
        assert_eq!(
            padded.parse::<Address>().expect("trimmed").to_string(),
            REGTEST_P2WPKH
        );
    }

    #[test]
    fn a_malformed_address_is_invalid_input() {
        for rejected in [
            "bcrt1qexampleexampleexampleexampleexampleex",
            "not an address",
            "",
        ] {
            let error = rejected
                .parse::<Address>()
                .expect_err("a malformed address is rejected");
            assert_eq!(error.code, crate::ErrorCode::InvalidInput);
        }
    }

    #[test]
    fn compatible_networks_reports_every_network_the_encoding_allows() {
        // Mainnet is the only unambiguous case.
        assert_eq!(
            MAINNET_P2WPKH
                .parse::<Address>()
                .expect("valid")
                .compatible_networks(),
            vec![Network::Bitcoin]
        );
        // A `tb1` address is testnet3, testnet4 or signet, but never regtest,
        // which has its own `bcrt` prefix.
        assert_eq!(
            TESTNETS_P2WSH
                .parse::<Address>()
                .expect("valid")
                .compatible_networks(),
            vec![Network::Testnet, Network::Testnet4, Network::Signet]
        );
        assert_eq!(
            REGTEST_P2WPKH
                .parse::<Address>()
                .expect("valid")
                .compatible_networks(),
            vec![Network::Regtest]
        );
        // A base58 test-family address is valid for all four test networks:
        // they share one version byte. Naming a single network here would be a
        // guess, which is exactly why the accessor returns a set.
        assert_eq!(
            TESTNETS_P2SH
                .parse::<Address>()
                .expect("valid")
                .compatible_networks(),
            vec![
                Network::Testnet,
                Network::Testnet4,
                Network::Signet,
                Network::Regtest
            ]
        );
    }

    #[test]
    fn observed_prefix_is_the_hrp_or_the_base58_version_character() {
        for (address, prefix) in [
            (MAINNET_P2WPKH, "bc"),
            (TESTNETS_P2WSH, "tb"),
            (REGTEST_P2WPKH, "bcrt"),
            (MAINNET_P2PKH, "1"),
            (MAINNET_P2SH, "3"),
            (TESTNET_P2PKH, "m"),
            (TESTNETS_P2SH, "2"),
        ] {
            assert_eq!(
                address.parse::<Address>().expect("valid").observed_prefix(),
                prefix,
                "prefix of {address}"
            );
        }
    }
}
