//! Out-of-band ecash notes.

use fedimint_core::encoding::Encodable;
use fedimint_mint_client::OOBNotes;

use super::Amount;
use crate::{Error, ErrorCode};

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
#[derive(Clone, PartialEq, Eq)]
pub struct Notes {
    notes: OOBNotes,
}

impl Notes {
    /// Returns the total value carried by these notes.
    ///
    /// This reads the value encoded in the notes themselves and does not
    /// contact the federation, so it does not confirm the notes are still
    /// redeemable (they could already have been spent or reclaimed), only
    /// a receive call does that.
    pub fn value(&self) -> Amount {
        // `total_amount()` and the `notes()` it calls both `.expect()` a `Notes` part.
        // Infallible for any value this type can hold: the decoder behind `FromStr`
        // refuses notes lacking one (modules/fedimint-mint-client/src/lib.rs:392-447).
        // Already millisatoshis upstream
        // (modules/fedimint-mint-client/src/lib.rs:509-514).
        Amount::from_msats(self.notes.total_amount().msats)
    }

    /// Wraps already-parsed out-of-band ecash notes.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_upstream(notes: OOBNotes) -> Self {
        Self { notes }
    }

    /// The hex prefix of the id of the federation that issued these notes.
    ///
    /// Crate-internal: this is what the ecash facade matches a token against a
    /// joined federation with, before it tries to redeem anything. Eight
    /// lowercase hex characters, the upstream `FederationIdPrefix` form.
    pub(crate) fn federation_id_prefix(&self) -> String {
        // Infallible for any value this type can hold: the decoder behind
        // `FromStr` refuses notes it cannot derive a federation id from
        // (modules/fedimint-mint-client/src/lib.rs:277-286, :392-447).
        self.notes.federation_id_prefix().to_string()
    }
}

impl core::hash::Hash for Notes {
    fn hash<H>(&self, state: &mut H)
    where
        H: core::hash::Hasher,
    {
        // `OOBNotes` is `PartialEq + Eq` but not `Hash` upstream, and this type
        // is documented as hashable. Hashing the consensus encoding keeps the
        // "equal values hash equally" law, because upstream's `PartialEq` is a
        // structural comparison of exactly the parts this encodes.
        core::hash::Hash::hash(&self.notes.consensus_encode_to_vec(), state);
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
        // Always URL-safe base64 with padding, whichever of the three accepted
        // encodings the value was parsed from
        // (modules/fedimint-mint-client/src/lib.rs:449-452, :482-488).
        core::fmt::Display::fmt(&self.notes, f)
    }
}

impl core::str::FromStr for Notes {
    type Err = crate::Error;

    /// Parses ecash notes from their canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Upstream strips every whitespace character before decoding, so a
        // token wrapped across lines parses; no trim is needed here. The
        // upstream error is dropped rather than reported: this value is the
        // money, so anything derived from the rejected string could put a
        // spendable token in a log.
        let notes = s
            .parse::<OOBNotes>()
            .map_err(|_| Error::new(ErrorCode::InvalidInput, "invalid ecash notes"))?;
        Ok(Self { notes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real out-of-band ecash token worth 1 satoshi, built once from the
    /// single spendable-note vector in `fedimint-mint-client`'s own tests and
    /// the upstream dummy federation id. The parse validates a consensus
    /// encoding, so a stand-in string no longer works here.
    const TOKEN: &str = "AgEEKioqKgBVAf0D6AGl3T66ytG8SL2HGO7VqNodaPkTI77yhIrE-i5vju1xDzF4_UrvBHzCNOaxEnCG8zzECLOYGHgdlSFHU2DeayBfMyjkkKbZnV4lU6RVMgfIvQ==";

    #[test]
    fn debug_prints_the_marker_and_nothing_else() {
        let notes = TOKEN.parse::<Notes>().expect("a valid ecash token");
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
        let nested = Some(TOKEN.parse::<Notes>().expect("a valid ecash token"));
        let rendered = format!("{nested:?}");
        assert_eq!(rendered, "Some(Notes(<redacted>))");
        assert!(!rendered.contains(TOKEN));
    }

    #[test]
    fn notes_round_trip_through_display_and_from_str() {
        let notes = TOKEN.parse::<Notes>().expect("a valid ecash token");
        assert_eq!(notes.to_string(), TOKEN);
    }

    #[test]
    fn parsing_tolerates_a_wrapped_token() {
        // A token copied out of an email or a chat message arrives with line
        // breaks in it, and it is still the same money.
        let wrapped = TOKEN
            .chars()
            .collect::<Vec<_>>()
            .chunks(20)
            .map(|chunk| chunk.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            wrapped.parse::<Notes>().expect("a wrapped token"),
            TOKEN.parse::<Notes>().expect("a valid ecash token")
        );
    }

    #[test]
    fn value_is_read_out_of_the_notes_themselves() {
        let notes = TOKEN.parse::<Notes>().expect("a valid ecash token");
        assert_eq!(notes.value(), Amount::from_msats(1_000));
    }

    #[test]
    fn federation_id_prefix_names_the_issuing_federation() {
        let notes = TOKEN.parse::<Notes>().expect("a valid ecash token");
        assert_eq!(notes.federation_id_prefix(), "2a2a2a2a");
    }

    #[test]
    fn equal_notes_hash_equally() {
        use std::collections::HashSet;

        let notes = TOKEN.parse::<Notes>().expect("a valid ecash token");
        let same = TOKEN.parse::<Notes>().expect("a valid ecash token");
        // `OOBNotes` is not `Hash` upstream, so this type writes its own; the
        // law that matters is that equal values hash equally.
        let mut set = HashSet::new();
        set.insert(notes);
        assert!(set.contains(&same));
    }

    #[test]
    fn a_malformed_token_is_invalid_input_and_is_not_echoed() {
        for rejected in ["", "notes-secret-bearer-value-0123456789", "not a token"] {
            let error = rejected
                .parse::<Notes>()
                .expect_err("a malformed token is rejected");
            assert_eq!(error.code, crate::ErrorCode::InvalidInput);
            // The value is the money: a rejected token must not reach a log
            // through the error message.
            assert!(
                !error.message.contains(rejected) || rejected.is_empty(),
                "the rejected token must not appear in the message"
            );
        }
    }
}
