//! Lightning payment preimages.

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
// Implementation notes (delete once implemented):
// - v1's lightning module hands back the preimage as a hex string, lnv2 hands back raw bytes.
//   Normalise both to one `Preimage`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Preimage {
    preimage: String,
}

impl Preimage {
    /// Wraps an already-validated preimage string.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_raw(raw: String) -> Self {
        Self { preimage: raw }
    }
}

impl core::fmt::Display for Preimage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let _ = &self.preimage;
        unimplemented!()
    }
}

impl core::str::FromStr for Preimage {
    type Err = crate::Error;

    /// Parses a preimage from its canonical hex form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}
