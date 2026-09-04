//! BIP-39 seed phrase handling.
//!
//! One [`Mnemonic`] is the root secret for an entire SDK instance. Every
//! federation the SDK joins derives its own client secret from that one
//! root, isolated per federation: a compromised or leaked secret for one
//! federation reveals nothing about any other federation derived from the
//! same seed. A seed phrase exported from this SDK is also portable to other
//! Fedimint clients restoring the same words against the same federation.
//
// Implementation notes (delete once implemented):
// - Reuse fedimint's existing, deployed derivation rather than inventing a new one: turn the
//   BIP-39 mnemonic into a root secret the way `fedimint-bip39` does, then obtain each
//   federation's child secret via the standard per-federation child derivation
//   (`get_default_client_secret`), which domain-separates by federation id.
// - Versioned starting at v1; pin the exact derivation path with cross-implementation test
//   vectors once implemented, so portability with `fedimint-cli`, multimint and Fedi is
//   verified, not just documented.

use fedimint_bip39::Language;

use crate::{Error, ErrorCode};

/// A BIP-39 seed phrase: the root secret an SDK instance is built from.
///
/// `Mnemonic` implements neither [`Debug`] nor
/// [`Display`](core::fmt::Display), unlike every other data type in this
/// crate. A seed phrase must never be formattable by accident: `Debug`
/// output routinely ends up in logs, crash reports and `assert!` failure
/// messages, and `Display` would let a `Mnemonic` leak through generic
/// string formatting. The words are obtainable only through the explicit
/// [`Mnemonic::words`] call, which is the deliberate point at which a caller
/// chooses to have them as a plain string.
///
/// This type's backing memory is zeroized when the value is dropped, so it
/// does not linger in memory after use.
///
/// Once [`Mnemonic::words`] hands the phrase across a language boundary as a
/// plain string, this type's guarantees end: keeping that copy from
/// lingering in memory, being logged, or being written somewhere insecure is
/// the responsibility of the application embedding the SDK. Protecting the
/// *at-rest* copy inside the SDK's persistent storage, for example by
/// encrypting it or integrating with a platform keychain, is not provided by
/// this crate yet.
// The crate lints on `#[warn(missing_debug_implementations)]`, promoted to a hard error by CI;
// `#[allow(missing_debug_implementations)]` on this type records the omission as intentional
// rather than an oversight.
#[allow(missing_debug_implementations)]
#[derive(Clone)]
pub struct Mnemonic {
    phrase: fedimint_bip39::Mnemonic,
}

impl Mnemonic {
    /// Generates a fresh 12-word English BIP-39 mnemonic.
    ///
    /// Uses the platform's cryptographically secure random source, which can
    /// genuinely be unavailable, so this reports a failure rather than
    /// panicking or falling back to a weaker source.
    ///
    /// # Errors
    ///
    /// [`Entropy`](crate::ErrorCode::Entropy) if the platform's secure random
    /// source was unavailable or failed. That is the only failure: nothing
    /// here reads storage or contacts a federation.
    pub fn generate() -> crate::Result<Mnemonic> {
        use fedimint_core::secp256k1::rand::RngCore;

        // 128 bits of entropy is exactly a 12-word phrase
        // (bip39-2.2.2/src/lib.rs:207). The buffer zeroizes itself on drop: it
        // holds the seed before the mnemonic does.
        let mut entropy = zeroize::Zeroizing::new([0u8; 16]);
        // `secp256k1` re-exports the same `rand` 0.8 that bip39's `rand_core`
        // bound expects (secp256k1-0.29.1/src/lib.rs:186), so no direct `rand`
        // dependency is needed. `try_fill_bytes` rather than `fill_bytes`:
        // `fill_bytes` panics when the source fails, and this crate's binding
        // layers are built with `panic = "abort"`, so a panic here would take
        // the host application down instead of surfacing `Entropy`.
        let mut rng = fedimint_core::secp256k1::rand::rngs::OsRng;
        rng.try_fill_bytes(entropy.as_mut_slice()).map_err(|_| {
            Error::new(
                ErrorCode::Entropy,
                "the platform's secure random source was unavailable",
            )
        })?;
        let phrase =
            fedimint_bip39::Mnemonic::from_entropy_in(Language::English, entropy.as_slice())
                .map_err(|_| {
                    Error::new(
                        ErrorCode::Entropy,
                        "the platform's secure random source was unavailable",
                    )
                })?;
        Ok(Self { phrase })
    }

    /// Returns the mnemonic's words, in order, as owned strings.
    ///
    /// Calling this is the deliberate act of exporting the seed out of the
    /// SDK's control (for backup display, for example); see the type-level
    /// documentation for what protections do and don't extend past this
    /// point.
    pub fn words(&self) -> Vec<String> {
        // bip39-2.2.2/src/lib.rs:337. `word_iter` is the deprecated spelling of
        // the same iterator.
        self.phrase.words().map(str::to_owned).collect()
    }

    /// Wraps an already-parsed BIP-39 mnemonic.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_upstream(mnemonic: fedimint_bip39::Mnemonic) -> Self {
        Self { phrase: mnemonic }
    }
}

impl core::str::FromStr for Mnemonic {
    type Err = crate::Error;

    /// Parses and validates a whitespace-separated BIP-39 phrase (checksum
    /// included). Returns [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput)
    /// for a malformed phrase, wrong word count, or checksum failure.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // bip39 matches words case-sensitively against a lowercase list
        // (bip39-2.2.2/src/language/mod.rs:169-192), so a phrase restored from
        // a capitalised paper backup would be rejected without this.
        // `split_whitespace` collapses runs of whitespace and line breaks.
        // The temporaries are not zeroized: the caller already holds the same
        // phrase in the `&str` it passed in, so there is nothing here that is
        // not already outside this crate's control.
        let normalised = s
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        // "Normalized" upstream means NFKD, which is a no-op for the ASCII-only
        // English word list, the only list compiled in under fedimint's feature
        // set. The upstream error is dropped rather than reported: it names a
        // word position, and a phrase is the root secret.
        let phrase = fedimint_bip39::Mnemonic::parse_in_normalized(Language::English, &normalised)
            .map_err(|_| Error::new(ErrorCode::InvalidInput, "invalid mnemonic"))?;
        Ok(Self { phrase })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical all-zero-entropy BIP-39 phrase.
    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon \
                          abandon abandon abandon about";

    #[test]
    fn a_phrase_parses_and_its_words_come_back_in_order() {
        let mnemonic = PHRASE.parse::<Mnemonic>().expect("a valid phrase");
        assert_eq!(mnemonic.words().len(), 12);
        assert_eq!(mnemonic.words().join(" "), PHRASE);
    }

    #[test]
    fn parsing_is_forgiving_about_case_and_whitespace() {
        // A phrase restored from a paper backup arrives capitalised, with
        // stray spaces, or with a line break in the middle of it, and it is
        // still the same seed.
        let messy = "  ABANDON abandon  Abandon\nabandon abandon abandon abandon abandon \
                     abandon abandon   abandon ABOUT  ";
        assert_eq!(
            messy.parse::<Mnemonic>().expect("normalised").words(),
            PHRASE.parse::<Mnemonic>().expect("a valid phrase").words()
        );
    }

    #[test]
    fn every_bip39_word_count_is_accepted() {
        // 15, 18, 21 and 24 words are valid BIP-39 phrases even though this
        // crate only generates 12, and a seed restored from another wallet may
        // well be one of them.
        let twenty_four = "abandon abandon abandon abandon abandon abandon abandon abandon \
                           abandon abandon abandon abandon abandon abandon abandon abandon \
                           abandon abandon abandon abandon abandon abandon abandon art";
        assert_eq!(
            twenty_four
                .parse::<Mnemonic>()
                .expect("a valid 24-word phrase")
                .words()
                .len(),
            24
        );
    }

    #[test]
    fn a_malformed_phrase_is_invalid_input_and_is_not_echoed() {
        let bad_checksum = "abandon abandon abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon abandon";
        let wrong_count = "abandon abandon about";
        let unknown_word = "abandon abandon abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon zzzzzz";
        for rejected in [
            bad_checksum,
            wrong_count,
            unknown_word,
            "",
            "not a mnemonic",
        ] {
            let Err(error) = rejected.parse::<Mnemonic>() else {
                panic!("a malformed phrase is rejected");
            };
            assert_eq!(error.code, crate::ErrorCode::InvalidInput);
            // The phrase is the root secret: nothing derived from a rejected
            // one may reach a log through the error message.
            assert!(
                !error.message.contains("abandon"),
                "the rejected phrase must not appear in the message"
            );
        }
    }

    #[test]
    fn generate_produces_a_fresh_twelve_word_phrase() {
        let first = Mnemonic::generate().expect("the platform has a random source");
        assert_eq!(first.words().len(), 12);
        // Every word is a real wordlist entry, which is what re-parsing the
        // phrase proves.
        let round_tripped = first
            .words()
            .join(" ")
            .parse::<Mnemonic>()
            .expect("a generated phrase parses");
        assert_eq!(round_tripped.words(), first.words());
        let second = Mnemonic::generate().expect("the platform has a random source");
        assert_ne!(
            first.words(),
            second.words(),
            "two generated seeds must not be the same seed"
        );
    }
}
