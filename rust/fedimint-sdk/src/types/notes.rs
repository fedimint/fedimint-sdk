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
///
/// # `Display` prints the notes, `Debug` never does
///
/// This value **is** the money: anyone holding the string can redeem it, so
/// it is a bearer instrument in exactly the way a banknote is.
/// [`Display`](core::fmt::Display) prints the notes and is the deliberate,
/// visible way to get the value out. [`Debug`] is redacted instead, because
/// it is what logging, crash reporters and `assert!` failures reach for, and
/// a struct holding a `Notes` (such as [`EcashSend`](crate::EcashSend))
/// would otherwise print the token merely by being logged.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Notes {
    notes: String,
}

impl Notes {
    /// Wraps an already-validated ecash note string.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_raw(raw: String) -> Self {
        Self { notes: raw }
    }

    /// Returns the total value carried by these notes.
    ///
    /// This reads the value encoded in the notes themselves and does not
    /// contact the federation, so it does not confirm the notes are still
    /// redeemable (they could already have been spent or reclaimed), only
    /// a receive call does that.
    pub fn value(&self) -> Amount {
        unimplemented!()
    }
}

impl core::fmt::Debug for Notes {
    /// Prints `Notes(<redacted>)`: the type name and nothing else, never the
    /// token. The value is still reachable, deliberately and visibly,
    /// through [`Display`](core::fmt::Display).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Notes(<redacted>)")
    }
}

impl core::fmt::Display for Notes {
    /// Writes the ecash token itself, in its canonical string form. This is
    /// the deliberate way to get the value out; see the type-level
    /// documentation for why [`Debug`] is not.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for a real token: no part of this string may ever appear
    /// in `Debug` output.
    const TOKEN: &str = "notes-secret-bearer-value-0123456789";

    #[test]
    fn debug_prints_the_marker_and_nothing_else() {
        let notes = Notes::from_raw(TOKEN.to_owned());
        let rendered = format!("{notes:?}");
        // Not merely "does not contain the token": the whole rendering is
        // the type name and the redaction marker, so there is nowhere for a
        // prefix, suffix, or truncated fragment of the value to hide.
        assert_eq!(rendered, "Notes(<redacted>)");
        assert!(!rendered.contains(TOKEN));
    }

    #[test]
    fn debug_stays_redacted_when_nested_in_another_value() {
        // The transitive case is the dangerous one: a `Notes` inside a struct
        // that derives `Debug` (`EcashSend` does) must not print the token
        // just because the outer value was logged.
        let nested = Some(Notes::from_raw(TOKEN.to_owned()));
        let rendered = format!("{nested:?}");
        assert_eq!(rendered, "Some(Notes(<redacted>))");
        assert!(!rendered.contains(TOKEN));
    }
}
