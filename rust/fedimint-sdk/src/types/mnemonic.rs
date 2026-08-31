//! BIP-39 seed phrase handling.
//!
//! # Per-federation key derivation
//!
//! One [`Mnemonic`] is the root secret for an entire SDK instance; every
//! federation the SDK joins derives its own client secret
//! from that one root, isolated per federation. The derivation reuses
//! fedimint's existing, already-deployed scheme rather than inventing a new
//! one: the BIP-39 mnemonic is turned into a root secret the same way
//! `fedimint-bip39` does today, and each federation's child secret is
//! obtained from that root through the standard per-federation child
//! derivation (`get_default_client_secret`), which domain-separates by
//! federation id. Two consequences follow directly from reusing that scheme:
//! a compromised or leaked secret for one federation reveals nothing about
//! any other federation derived from the same seed, and a seed phrase
//! exported from this SDK is portable — restoring the same words in
//! `fedimint-cli`, multimint, or Fedi against the same federation reproduces
//! the same client secret.
//!
//! This derivation is versioned (starting at v1); the exact derivation path
//! will be pinned by cross-implementation test vectors once it is
//! implemented, so that portability between clients is verified, not just
//! documented.

/// A BIP-39 seed phrase: the root secret an SDK instance is built from.
///
/// # Why no [`Debug`] or [`Display`](core::fmt::Display)
///
/// `Mnemonic` deliberately implements neither trait, which is unusual for a
/// data type in this crate (see the crate's derive conventions) and is
/// called out explicitly here rather than left to be discovered as a
/// missing impl:
///
/// - **No `Debug`.** Every other data type in this crate implements `Debug`
///   so it prints usefully in logs and test failures. A mnemonic must never
///   do that: application logging, crash reporters, and `assert!` failure
///   messages routinely capture `Debug` output, and a seed phrase reaching
///   any of those sinks is a fund-loss-severity leak. Omitting the impl
///   means the phrase simply cannot be formatted that way, by accident or
///   otherwise. Because the crate lints on `#[warn(missing_debug_implementations)]`
///   (promoted to a hard error under this crate's CI settings), the type
///   carries `#[allow(missing_debug_implementations)]` to record that the
///   absence is intentional rather than an oversight the lint should catch.
/// - **No `Display`.** Unlike `Debug`, `Display` is something callers invoke
///   on purpose — but a `Mnemonic` participating in generic string-formatting
///   contexts (`format!("{mnemonic}")`, `println!`, string interpolation
///   inside a template) is exactly the accidental-exposure path this type
///   exists to prevent. Recovering the words is available only through
///   [`Mnemonic::words`], which returns an owned, ordinary `Vec<String>`: at
///   that point the caller holds a plain string and is making an explicit,
///   visible choice to have it, which is the appropriate point for that
///   choice to happen.
///
/// # Zeroization contract
///
/// A `Mnemonic`'s backing memory is intended to be zeroized when the value
/// is dropped, so a stack or heap page that once held the seed does not keep
/// holding it indefinitely. This is documented here as a contract this type
/// is expected to uphold; the skeleton does not yet implement it (doing so
/// pulls in a zeroizing-memory dependency, which is out of scope while this
/// crate has zero dependencies) and callers should not rely on it being
/// enforced until an implementation lands.
///
/// # What this type does not protect
///
/// Once a caller calls [`Mnemonic::words`] and the phrase becomes a plain
/// string on the other side of a language boundary — a Swift `String`, a
/// Kotlin `String`, a JavaScript string — this type's guarantees end. Making
/// sure that exported copy doesn't linger in memory, get logged, or get
/// written to an insecure location is the responsibility of the application
/// embedding the SDK, not of this crate. Separately, protecting the *at-rest*
/// copy of the seed inside the SDK's persistent storage — for example,
/// encrypting it or integrating with a platform keychain — is not part of
/// the 0.1 contract; it is a recognized future additive capability of that
/// storage layer, to be documented there when it lands.
#[allow(missing_debug_implementations)]
#[derive(Clone)]
pub struct Mnemonic {
    phrase: String,
}

impl Mnemonic {
    /// Generates a fresh 12-word English BIP-39 mnemonic.
    ///
    /// This draws from a system source of entropy, so it has no meaningful
    /// behavior to specify at the type-skeleton stage.
    pub fn generate() -> Self {
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
