//! Federation invite codes and join previews.

use std::collections::BTreeMap;

use super::{FederationId, Network};

/// An invite code for a federation.
///
/// An invite code carries everything needed to locate and connect to a
/// federation's guardians before anything has been persisted locally. It is
/// opaque: callers pass it to a preview or join call rather than picking it
/// apart, with one deliberate exception —
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
/// An invite code is not always public. It can embed an `api_secret`, which
/// is the credential a private federation requires before its guardians will
/// answer at all — so the code is a bearer credential, and printing one can
/// hand a reader access to a federation that was meant to be closed. The two
/// formatting traits are therefore split deliberately:
///
/// - **[`Display`](core::fmt::Display) is the escape hatch**, and it is the
///   only one. Rendering the code is what the type is for — it has to be
///   shown, scanned, and shared — so `{invite}` is a visible, deliberate
///   choice, the same way [`Mnemonic::words`](crate::Mnemonic::words) is the
///   deliberate way to get a seed phrase out.
/// - **[`Debug`] is not an escape hatch** and is redacted. `{:?}` is what
///   logging, crash reporters, tracing spans, and `assert!` failure messages
///   reach for, none of which should receive a credential nobody chose to
///   publish. Derived `Debug` would also leak *transitively*: any struct
///   holding an `InviteCode` and deriving `Debug` would print it merely by
///   being logged, without anyone formatting the code on purpose.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct InviteCode {
    code: String,
}

impl InviteCode {
    /// The id of the federation this code invites to.
    ///
    /// Read from the code itself, which encodes it: no network round trip,
    /// no join, and the same value [`FederationPreview::id`] reports after
    /// one. This is the one thing a caller may take out of an otherwise
    /// opaque code, and it is exposed because it is the key every
    /// per-federation call on [`Sdk`](crate::Sdk) takes —
    /// [`federation_status`](crate::Sdk::federation_status) and
    /// [`reopen_federation`](crate::Sdk::reopen_federation) among them, and
    /// the seed-recovery calls of the `experimental` feature with them. An
    /// application holding only an invite code can therefore find out where
    /// that federation stands *before* joining, and — the case that makes
    /// this accessor necessary rather than convenient — find its way back to
    /// a federation that a failed seed recovery left committed but not
    /// open, since that call's error carries no id and enumerating stored
    /// federations cannot say which row a particular failed call produced.
    ///
    /// Not a credential: a federation id is public. The `Debug` redaction
    /// covers the code as a whole because of the `api_secret` it can embed,
    /// and reading the id out of it hands over nothing that was secret.
    pub fn federation_id(&self) -> FederationId {
        let _ = &self.code;
        unimplemented!()
    }

    /// Wraps an already-validated invite code string.
    ///
    /// Crate-internal: this performs no validation of its own, so it is not
    /// part of the public API. Validation belongs in
    /// [`FromStr`](core::str::FromStr), which is the only way a caller
    /// outside this crate can build one.
    pub(crate) fn from_raw(raw: String) -> Self {
        Self { code: raw }
    }
}

impl core::fmt::Debug for InviteCode {
    /// Prints `InviteCode(<redacted>)`: the type name and nothing else, never
    /// the code.
    ///
    /// Hand-written rather than derived because an invite code may embed a
    /// federation's `api_secret`, making it a credential rather than a public
    /// identifier. `Debug` output reaches log lines, crash reports, tracing
    /// spans, and `assert!` messages without anybody deciding that it should,
    /// and a derive would additionally leak the code through every struct
    /// that contains one and derives `Debug`. The value stays reachable,
    /// deliberately and visibly, through [`Display`](core::fmt::Display).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("InviteCode(<redacted>)")
    }
}

impl core::fmt::Display for InviteCode {
    /// Writes the invite code itself, in its canonical string form.
    ///
    /// This is the deliberate way to get the value out — to render a QR code,
    /// to share a link — and it is the *only* way; see the type-level
    /// documentation for why [`Debug`] is not.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let _ = &self.code;
        unimplemented!()
    }
}

impl core::str::FromStr for InviteCode {
    type Err = crate::Error;

    /// Parses an invite code from its canonical string form. Returns
    /// [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput) for a
    /// malformed value.
    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}

/// Everything needed to render a "join this federation?" screen before
/// committing to anything.
///
/// A `FederationPreview` is fetched (from the federation's guardians, over
/// the network) without joining or persisting any state locally — it lets an
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
    /// only, while this list is the federation's full module set — a
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

    /// A stand-in for a real invite code, including the credential an invite
    /// for a private federation can embed.
    const CODE: &str = "fed11-invite-code-with-api-secret-0123456789";

    #[test]
    fn debug_prints_the_marker_and_nothing_else() {
        let invite = InviteCode::from_raw(CODE.to_owned());
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
        let nested = Some(InviteCode::from_raw(CODE.to_owned()));
        let rendered = format!("{nested:?}");
        assert_eq!(rendered, "Some(InviteCode(<redacted>))");
        assert!(!rendered.contains(CODE));
    }
}
