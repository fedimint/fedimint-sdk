//! UniFFI bindings for `fedimint-sdk`.
//!
//! This crate wraps the SDK's public API into types that `uniffi` can
//! generate Swift, Kotlin and Python bindings from. Every type here is a
//! thin wrapper; the logic lives in `fedimint-sdk` and this crate adds only
//! the FFI annotations and the `Arc<Self>` ownership that UniFFI requires.
//!
//! # Why a separate crate?
//!
//! `fedimint-sdk` carries `wasm-bindgen` for its WASM target and enforces
//! `#![deny(missing_docs)]` plus a `wasm32-unknown-unknown` check gate.
//! UniFFI's proc-macro expansions generate items that break both of those
//! gates, and adding UniFFI as even an optional dependency would pull its
//! proc-macro tree into the SDK's `Cargo.lock`, bloating every contributor's
//! build whether they touch FFI or not. Keeping the boundary here means the
//! SDK crate stays lean and portable, and this crate can set its own lint
//! policy.

use std::sync::Arc;

use fedimint_sdk::Mnemonic as SdkMnemonic;

uniffi::setup_scaffolding!();

// ── Error ────────────────────────────────────────────────────────────────

/// Errors surfaced to the binding layer.
///
/// Each variant maps onto one or more [`fedimint_sdk::ErrorCode`] cases;
/// the full error taxonomy stays in the SDK, and this enum is the
/// coarsened view the binding sees.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    /// The platform's secure random source was unavailable.
    #[error("Entropy error: {msg}")]
    Entropy { msg: String },

    /// The input did not parse or was otherwise invalid.
    #[error("Invalid input: {msg}")]
    InvalidInput { msg: String },

    /// Catch-all for errors that don't have a specific variant yet.
    #[error("SDK error: {msg}")]
    Sdk { msg: String },
}

impl From<fedimint_sdk::Error> for FfiError {
    fn from(e: fedimint_sdk::Error) -> Self {
        match e.code {
            fedimint_sdk::ErrorCode::Entropy => FfiError::Entropy {
                msg: e.message.clone(),
            },
            fedimint_sdk::ErrorCode::InvalidInput => FfiError::InvalidInput {
                msg: e.message.clone(),
            },
            _ => FfiError::Sdk {
                msg: format!("{}: {}", e.code, e.message),
            },
        }
    }
}

// ── Mnemonic ─────────────────────────────────────────────────────────────

/// A BIP-39 seed phrase, wrapped for FFI.
///
/// The SDK's `Mnemonic` deliberately omits `Debug` and `Display` to
/// prevent accidental logging. This wrapper preserves that discipline:
/// the only way to get the words out is [`Mnemonic::words`].
#[derive(uniffi::Object)]
pub struct Mnemonic {
    inner: SdkMnemonic,
}

#[uniffi::export]
impl Mnemonic {
    /// Generates a fresh 12-word BIP-39 mnemonic.
    #[uniffi::constructor]
    pub fn generate() -> Result<Arc<Self>, FfiError> {
        let inner = SdkMnemonic::generate()?;
        Ok(Arc::new(Self { inner }))
    }

    /// Parses a whitespace-separated BIP-39 phrase.
    #[uniffi::constructor]
    pub fn from_phrase(phrase: String) -> Result<Arc<Self>, FfiError> {
        let inner: SdkMnemonic = phrase.parse()?;
        Ok(Arc::new(Self { inner }))
    }

    /// Returns the mnemonic's words in order.
    pub fn words(&self) -> Vec<String> {
        self.inner.words()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon \
                          abandon abandon abandon about";

    #[test]
    fn generate_produces_twelve_words() {
        let m = Mnemonic::generate().expect("entropy available");
        assert_eq!(m.words().len(), 12);
    }

    #[test]
    fn from_phrase_round_trips() {
        let m = Mnemonic::from_phrase(PHRASE.to_owned()).expect("valid phrase");
        assert_eq!(m.words().join(" "), PHRASE);
    }

    #[test]
    fn from_phrase_rejects_garbage() {
        let result = Mnemonic::from_phrase("not a mnemonic".to_owned());
        assert!(matches!(result, Err(FfiError::InvalidInput { .. })));
    }
}
