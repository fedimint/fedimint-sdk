//! Federation invite codes and join previews.

use std::collections::BTreeMap;

use super::{FederationId, Network};

/// An invite code for a federation.
///
/// An invite code carries everything needed to locate and connect to a
/// federation's guardians before anything has been persisted locally. It is
/// opaque: callers pass it to a preview or join call rather than picking it
/// apart. It round-trips through [`Display`](core::fmt::Display) and
/// [`FromStr`](core::str::FromStr) with a validating parse, so it can be
/// entered as text, scanned from a QR code, or shared as a link and
/// reconstructed on the other end without any federation-specific parsing
/// logic outside this crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InviteCode {
    code: String,
}

impl core::fmt::Display for InviteCode {
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
