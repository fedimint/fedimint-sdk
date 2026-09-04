//! Lightning payment preimages.

use crate::{Error, ErrorCode};

/// The proof that a lightning payment settled.
///
/// A preimage is the 32-byte value whose hash is the payment hash a bolt11
/// invoice commits to. Releasing it is what settles a lightning payment, so
/// holding it is proof to anyone who has the invoice that that invoice was
/// paid. It is a receipt rather than an identifier, which is why it is a
/// first-class type here and not a loose string: it is the value an
/// application stores, displays, and may later have to show to someone who
/// disputes the payment.
///
/// Like the rest of the crate's string-shaped values it is opaque and has a
/// canonical hex form, round-tripping through
/// [`Display`](core::fmt::Display) and [`FromStr`](core::str::FromStr) with
/// a validating parse. A binding therefore carries it as a plain string:
/// a Swift `String`, a Kotlin `String`, a JavaScript string, without
/// needing hex handling or a length check of its own.
///
/// The SDK normalises this value to one hex form regardless of how a given
/// federation's lightning module reports it internally.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Preimage {
    preimage: [u8; 32],
}

impl Preimage {
    /// Wraps the 32 raw bytes of a preimage.
    ///
    /// Crate-internal: this performs no validation of its own beyond the
    /// length the type enforces, so it is not part of the public API. Parsing
    /// belongs in [`FromStr`](core::str::FromStr), which is the only way a
    /// caller outside this crate can build one.
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { preimage: bytes }
    }
}

impl core::fmt::Display for Preimage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The same forward-order lowercase hex formatter fedimint uses for its
        // own ids (fedimint-core/src/lib.rs:671-684), so a preimage
        // reads the same here as it does in a fedimint log line.
        fedimint_core::format_hex(&self.preimage, f)
    }
}

impl core::str::FromStr for Preimage {
    type Err = crate::Error;

    /// Parses a preimage from its canonical hex form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // `hex` 0.4.3 through fedimint's re-export, the same decoder upstream
        // uses for its own preimage parsing. Decoding into `[u8; 32]` is what
        // enforces the length, and the error names a bad character or a bad
        // length, never the settled secret itself.
        let bytes: [u8; 32] = fedimint_core::hex::FromHex::from_hex(s.trim()).map_err(|err| {
            Error::new(ErrorCode::InvalidInput, format!("invalid preimage: {err}"))
        })?;
        Ok(Self { preimage: bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZEROS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn a_preimage_round_trips_through_display_and_from_str() {
        let preimage = ZEROS.parse::<Preimage>().expect("a valid preimage");
        assert_eq!(preimage.to_string(), ZEROS);
        assert_eq!(preimage, Preimage::from_bytes([0; 32]));
    }

    #[test]
    fn display_normalises_to_one_lowercase_hex_form() {
        // The type promises one hex form regardless of how a module reported
        // the value, so an uppercase input has to come back lowercased.
        let mixed = "00112233445566778899AABBCCDDEEFF00112233445566778899aabbccddeeff";
        let preimage = mixed.parse::<Preimage>().expect("uppercase hex is valid");
        assert_eq!(
            preimage.to_string(),
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
        );
    }

    #[test]
    fn bytes_and_hex_reach_the_same_value() {
        // v1's lightning module reports a hex string and lnv2 reports raw
        // bytes; both have to land on one value.
        let bytes = [0x11u8; 32];
        assert_eq!(
            Preimage::from_bytes(bytes),
            "11".repeat(32).parse::<Preimage>().expect("valid")
        );
    }

    #[test]
    fn parsing_ignores_surrounding_whitespace() {
        let padded = format!("  {ZEROS}\n");
        assert_eq!(
            padded.parse::<Preimage>().expect("trimmed").to_string(),
            ZEROS
        );
    }

    #[test]
    fn a_malformed_preimage_is_invalid_input() {
        // A preimage is exactly 32 bytes, so a short or odd-length string is
        // not merely unusual, it is a different kind of value.
        let not_hex = "zz".repeat(32);
        let too_short = "00".repeat(31);
        for rejected in ["", "00", not_hex.as_str(), too_short.as_str()] {
            let error = rejected
                .parse::<Preimage>()
                .expect_err("a malformed preimage is rejected");
            assert_eq!(error.code, crate::ErrorCode::InvalidInput);
        }
    }
}
