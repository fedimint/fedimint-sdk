//! Out-of-band ecash notes.

use super::Amount;

/// An out-of-band ecash token string, handed from a sender to a receiver
/// outside the federation (over a message, a QR code, a file).
///
/// `Notes` bundles one or more signed ecash tokens together with enough
/// federation context for a receiver to redeem them. It is opaque: callers
/// treat it as a value to display, copy, transmit, and hand to a receive
/// call, not as something to parse apart. It round-trips through
/// [`Display`](core::fmt::Display) and [`FromStr`](core::str::FromStr) with a
/// validating parse.
///
/// Notes obtained from a sender should be redeemed promptly: unredeemed
/// notes that a sender created are subject to that sender's automatic
/// reclaim policy, after which they stop being redeemable.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Notes {
    notes: String,
}

impl Notes {
    /// Returns the total value carried by these notes.
    ///
    /// This reads the value encoded in the notes themselves and does not
    /// contact the federation, so it does not confirm the notes are still
    /// redeemable (they could already have been spent or reclaimed) — only
    /// a receive call does that.
    pub fn value(&self) -> Amount {
        unimplemented!()
    }
}

impl core::fmt::Display for Notes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let _ = &self.notes;
        unimplemented!()
    }
}

impl core::str::FromStr for Notes {
    type Err = crate::Error;

    /// Parses ecash notes from their canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}
