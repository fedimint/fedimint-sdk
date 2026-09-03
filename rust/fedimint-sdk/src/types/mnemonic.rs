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
/// This type's backing memory is meant to be zeroized when the value is
/// dropped, so it does not linger in memory after use; the skeleton does not
/// yet implement this and callers should not rely on it until an
/// implementation lands.
///
/// Once [`Mnemonic::words`] hands the phrase across a language boundary as a
/// plain string, this type's guarantees end: keeping that copy from
/// lingering in memory, being logged, or being written somewhere insecure is
/// the responsibility of the application embedding the SDK. Protecting the
/// *at-rest* copy inside the SDK's persistent storage, for example by
/// encrypting it or integrating with a platform keychain, is not provided by
/// this crate yet.
// Implementation notes (delete once implemented):
// - The crate lints on `#[warn(missing_debug_implementations)]`, promoted to a hard error by
//   CI; `#[allow(missing_debug_implementations)]` on this type records the omission as
//   intentional rather than an oversight.
#[allow(missing_debug_implementations)]
#[derive(Clone)]
pub struct Mnemonic {
    phrase: String,
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
        // Implementation notes (delete once implemented):
        // - Sources: `getrandom` and friends natively, the Web Crypto API on wasm. Both can
        //   fail: a sandbox without `/dev/urandom`, an exhausted file-descriptor table, a
        //   browsing context where `crypto.getRandomValues` is not exposed.
        // - This crate's binding layers hold a strict no-panic discipline (the UniFFI layer is
        //   built with `panic = "abort"`), so a panic here would take the host application
        //   down; report `Entropy` instead.
        unimplemented!()
    }

    /// Returns the mnemonic's words, in order, as owned strings.
    ///
    /// Calling this is the deliberate act of exporting the seed out of the
    /// SDK's control (for backup display, for example); see the type-level
    /// documentation for what protections do and don't extend past this
    /// point.
    pub fn words(&self) -> Vec<String> {
        unimplemented!()
    }
}

impl core::str::FromStr for Mnemonic {
    type Err = crate::Error;

    /// Parses and validates a whitespace-separated BIP-39 phrase (checksum
    /// included). Returns [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput)
    /// for a malformed phrase, wrong word count, or checksum failure.
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}
