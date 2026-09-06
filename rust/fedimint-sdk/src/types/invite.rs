//! Federation invite codes and join previews.

use std::collections::BTreeMap;

use fedimint_core::invite_code;

use super::{FederationId, Network};
use crate::{Error, ErrorCode};

/// An invite code for a federation.
///
/// An invite code carries everything needed to locate and connect to a
/// federation's guardians before anything has been persisted locally. It is
/// opaque: callers pass it to a preview or join call rather than picking it
/// apart, with one deliberate exception:
/// [`federation_id`](InviteCode::federation_id), the key that every
/// per-federation call takes, is readable from the code without a network
/// round trip. It round-trips through [`Display`](core::fmt::Display) and
/// [`FromStr`](core::str::FromStr) with a validating parse, so it can be
/// entered as text, scanned from a QR code, or shared as a link and
/// reconstructed on the other end without any federation-specific parsing
/// logic outside this crate.
///
/// # `Display` prints the code, `Debug` never does
///
/// An invite code is not always public: it can embed an `api_secret`, the
/// credential a private federation requires before its guardians will answer
/// at all, so the code is a bearer credential and printing one can hand a
/// reader access to a federation meant to be closed.
/// [`Display`](core::fmt::Display) prints the code and is the deliberate way
/// to render, scan or share it. [`Debug`] is redacted instead, because it is
/// what logging, crash reporters and `assert!` failures reach for, and any
/// struct holding an `InviteCode` would otherwise print it merely by being
/// logged.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct InviteCode {
    code: invite_code::InviteCode,
}

impl InviteCode {
    /// The id of the federation this code invites to.
    ///
    /// Read from the code itself, which encodes it: no network round trip,
    /// no join, and the same value [`FederationPreview::id`] reports after
    /// one. It is the key every per-federation call on [`Sdk`](crate::Sdk)
    /// takes, including [`federation_status`](crate::Sdk::federation_status),
    /// [`reopen_federation`](crate::Sdk::reopen_federation),
    /// [`recovery_status`](crate::Sdk::recovery_status) and
    /// [`resume_recovery`](crate::Sdk::resume_recovery). An application
    /// holding only an invite code can therefore check where that federation
    /// stands before joining, and find its way back to a federation that a
    /// failed seed recovery left committed but not open, when nothing else
    /// identifies which stored federation that failed call produced.
    ///
    /// Not a credential: a federation id is public, unlike the `api_secret`
    /// the code as a whole may embed.
    pub fn federation_id(&self) -> FederationId {
        // Infallible for any code this type can hold: the decoder behind
        // `FromStr` refuses a code without a federation id part
        // (fedimint-core/src/invite_code.rs:22-25, :178), and
        // `from_upstream` is only ever handed a code that came through it.
        FederationId::from_upstream(self.code.federation_id())
    }

    /// Wraps an already-parsed invite code.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_upstream(code: invite_code::InviteCode) -> Self {
        Self { code }
    }
}

impl core::fmt::Debug for InviteCode {
    /// Prints `InviteCode(<redacted>)`: the type name and nothing else, never
    /// the code. The value stays reachable, deliberately and visibly,
    /// through [`Display`](core::fmt::Display).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("InviteCode(<redacted>)")
    }
}

impl core::fmt::Display for InviteCode {
    /// Writes the invite code itself, in its canonical string form. This is
    /// the deliberate way to get the value out; see the type-level
    /// documentation for why [`Debug`] is not.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Always bech32m with the `fed1` human-readable part, whichever of the
        // two accepted encodings the value was parsed from
        // (fedimint-core/src/invite_code.rs:268-274).
        core::fmt::Display::fmt(&self.code, f)
    }
}

impl core::str::FromStr for InviteCode {
    type Err = crate::Error;

    /// Parses an invite code from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Upstream accepts bech32m and a `fedimint`-prefixed base32 form
        // (invite_code.rs:220-243) and reports `InviteCodeParseError`
        // (invite_code.rs:245-265). That error is dropped rather than
        // reported: a code can embed an api_secret, so anything derived from
        // the rejected string could carry a credential into a log, and every
        // variant of it is the same `InvalidInput` to a caller anyway.
        let code = s
            .trim()
            .parse::<invite_code::InviteCode>()
            .map_err(|_| Error::new(ErrorCode::InvalidInput, "invalid invite code"))?;
        Ok(Self { code })
    }
}

/// Everything needed to render a "join this federation?" screen before
/// committing to anything.
///
/// A `FederationPreview` is fetched (from the federation's guardians, over
/// the network) without joining or persisting any state locally: it lets an
/// application show the user what they're about to join. Producing one also
/// validates the federation-wide rule that every module must share the same
/// generation (all v1 or all v2, never mixed); a federation that fails that
/// check fails with [`ErrorCode::UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation)
/// before a preview is ever returned, rather than returning a preview for
/// something the SDK cannot actually operate on.
///
/// This type is `#[non_exhaustive]`: new fields may be added in future
/// releases, so construct it only through the SDK and match it only with a
/// `..` pattern or by field access, never by exhaustive destructuring.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FederationPreview {
    /// The federation's identifier.
    pub id: FederationId,
    /// The federation's human-readable name, when its configuration
    /// provides one.
    pub name: Option<String>,
    /// The Bitcoin network this federation operates on.
    pub network: Network,
    /// The number of guardians in the federation.
    pub guardians: u16,
    /// The kind names of *every* module this federation runs, e.g.
    /// `"mint"`, `"ln"`, `"wallet"`.
    ///
    /// Presence here does not imply a corresponding facade after joining.
    /// The SDK exposes facades for the mint, lightning and wallet modules
    /// only, while this list is the federation's full module set: a
    /// federation may run modules this SDK has no facade for, and they
    /// appear here all the same.
    ///
    /// The single-generation rule is not a per-module gate and plays no
    /// part in this: it is federation-wide, and any preview that was
    /// returned at all has already satisfied it (see the type
    /// documentation).
    pub modules: Vec<String>,
    /// Config-level metadata (for example, a welcome message), keyed by
    /// arbitrary string keys as defined by the federation's configuration.
    pub meta: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real invite code, built once from fixed inputs: the guardian URL
    /// `wss://foo.bar`, peer 0, and the upstream dummy federation id. The
    /// parse now checks a bech32m checksum, so a placeholder string no longer
    /// works here.
    const CODE: &str = "fed11qgqpqrnhwden5te0vehk7tnzv9ez7qqpyq4z52329g4z52329g4z52329g4z52329g4z52329g4z52329g4z5wa8phk";
    /// The federation id that code invites to: the upstream dummy id, 32 bytes
    /// of `0x2a`.
    const CODE_FEDERATION_ID: &str =
        "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a";

    #[test]
    fn debug_prints_the_marker_and_nothing_else() {
        let invite = CODE.parse::<InviteCode>().expect("a valid invite code");
        let rendered = format!("{invite:?}");
        // Not merely "does not contain the code": the whole rendering is the
        // type name and the redaction marker, so there is nowhere for a
        // prefix, suffix, or truncated fragment of the value to hide.
        assert_eq!(rendered, "InviteCode(<redacted>)");
        assert!(!rendered.contains(CODE));
    }

    #[test]
    fn debug_stays_redacted_when_nested_in_another_value() {
        // The transitive case is the dangerous one: an `InviteCode` inside a
        // struct that derives `Debug` must not print the code just because
        // the outer value was logged.
        let nested = Some(CODE.parse::<InviteCode>().expect("a valid invite code"));
        let rendered = format!("{nested:?}");
        assert_eq!(rendered, "Some(InviteCode(<redacted>))");
        assert!(!rendered.contains(CODE));
    }

    #[test]
    fn an_invite_code_round_trips_through_display_and_from_str() {
        let invite = CODE.parse::<InviteCode>().expect("a valid invite code");
        assert_eq!(invite.to_string(), CODE);
        let padded = format!("  {CODE}\n");
        assert_eq!(padded.parse::<InviteCode>().expect("trimmed"), invite);
        // Bech32m is case-insensitive, and a QR code encoder emits uppercase
        // to stay in alphanumeric mode.
        assert_eq!(
            CODE.to_uppercase()
                .parse::<InviteCode>()
                .expect("uppercase bech32m is valid"),
            invite
        );
    }

    #[test]
    fn federation_id_is_readable_without_a_network_round_trip() {
        let invite = CODE.parse::<InviteCode>().expect("a valid invite code");
        assert_eq!(invite.federation_id().to_string(), CODE_FEDERATION_ID);
        // The same value `FederationPreview::id` reports after a preview, so
        // the two spellings have to agree exactly.
        assert_eq!(
            invite.federation_id(),
            CODE_FEDERATION_ID
                .parse::<FederationId>()
                .expect("a valid federation id")
        );
    }

    #[test]
    fn a_malformed_invite_code_is_invalid_input_and_is_not_echoed() {
        for rejected in [
            "",
            "fed11-invite-code-with-api-secret-0123456789",
            "not an invite code",
        ] {
            let error = rejected
                .parse::<InviteCode>()
                .expect_err("a malformed invite code is rejected");
            assert_eq!(error.code, crate::ErrorCode::InvalidInput);
            // A code can embed an api secret, so a rejected one must not reach
            // a log through the error message: the message is fixed, not built
            // from the rejected string.
            assert_eq!(error.message, "invalid invite code");
        }
    }
}
