//! Federation metadata, from configuration and from consensus.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::Result;

/// The metadata facade for one federation.
///
/// Obtained from [`Federation::meta`](crate::Federation::meta). Unlike the
/// capability facades this is unconditional: every federation has
/// configuration metadata, so there is always something here to read.
///
/// # Two sources, one merged view
///
/// A federation can describe itself in two places, and they are genuinely
/// different things:
///
/// - **Configuration metadata** is baked into the federation's consensus
///   configuration. It is fixed for the life of that configuration, is
///   available locally without asking anyone, and is what a
///   [`FederationPreview`](crate::FederationPreview) shows before joining.
/// - **Consensus metadata** lives in the federation's meta module, is
///   agreed by the guardians at runtime, and is *revisioned*: the
///   guardians can change it, and each change bumps a revision number.
///   Not every federation runs a meta module.
///
/// Most applications want neither of those specifically; they want to know
/// "what is this federation's welcome message" and to get the current
/// answer. [`Meta::get`] and [`Meta::all`] provide that as a merged view,
/// with a single precedence rule: **consensus metadata overrides
/// configuration metadata, per key**. A key present in both takes its value
/// from consensus, because consensus metadata is the one the guardians can
/// update; keys present in only one source appear unchanged.
///
/// The raw sources stay available separately:
/// [`Meta::config_metadata`] and [`Meta::consensus_metadata`], so an
/// application that needs to know *where* a value came from, or that needs
/// the consensus revision, is not forced to work backwards from the merged
/// result.
///
/// # The merged view is a lossy projection, and these are its exact rules
///
/// The meta module stores arbitrary bytes ([`ConsensusMetadata::value`]),
/// in practice a UTF-8 JSON document but with no guarantee of either.
/// Turning those bytes into the flat `BTreeMap<String, String>` that
/// [`Meta::get`] and [`Meta::all`] return is a defined, lossy projection.
/// Every binding must behave identically here, so the rules are written down
/// rather than left to each implementation to settle:
///
/// 1. **Decode as UTF-8.** If the bytes are not valid UTF-8, the consensus
///    document contributes **nothing**: no keys at all, and the merged view
///    is the configuration metadata alone.
/// 2. **Parse as JSON.** If the decoded text does not parse as JSON, the
///    consensus document again contributes nothing.
/// 3. **Require a top-level object.** If the document parses to anything
///    other than a JSON object (an array, a bare string, a number, a
///    boolean, `null`), it has no top-level entries to project, so it
///    contributes nothing.
/// 4. **Project each top-level entry by its value's type.** A **string**
///    value contributes its contents, unquoted. A **number**, **boolean**,
///    or **`null`** contributes its JSON text (`42`, `true`, `null`) and
///    stops being distinguishable from a string that happens to read the
///    same. A **nested object or array** cannot be projected to a string and
///    is **skipped**: that key contributes nothing from consensus.
/// 5. **Skipped is "not defined", never "defined as empty".** A key skipped
///    by rule 4 is treated exactly as though consensus had not defined it:
///    the configuration value for that key stands if there is one, and if
///    there is none the key is absent from the merged view entirely. An
///    unprojectable value never surfaces as an empty string and never blanks
///    out a configuration value it would otherwise have overridden.
///
/// Note what rules 1 to 3 mean in practice: a consensus document that is not
/// UTF-8 JSON with an object at its root is *invisible* to the merged view.
/// That is deliberate: a partial or guessed projection would be worse than
/// none, and it is also why the merged view is never evidence that the meta
/// module is empty.
///
/// None of this destroys anything. Everything the projection skips or
/// flattens is still in [`ConsensusMetadata::value`] exactly as consensus
/// stores it, which is what to read for anything that depends on the
/// document's structure, its scalar types, or its precise bytes.
#[derive(Debug, Clone)]
pub struct Meta {
    inner: Arc<MetaInner>,
}

impl Meta {
    /// Looks up one key in the merged view.
    ///
    /// Returns the consensus value if the meta module defines this key with
    /// a value the projection can represent, the configuration value if only
    /// the configuration does, and `None` if neither does. Asynchronous and
    /// fallible because reading consensus metadata may require contacting the
    /// federation.
    ///
    /// **`None` is not proof the key is unset.** The projection is governed
    /// by the rules on [`Meta`] and it declines rather than guesses: a
    /// consensus document that is not valid UTF-8, does not parse as JSON, or
    /// is not a JSON object contributes no keys at all, and a key whose value
    /// is a nested object or array is skipped as though consensus had not
    /// defined it (the configuration value, if any, then stands). A caller
    /// that needs to distinguish "absent" from "present but not projectable"
    /// must read [`Meta::consensus_metadata`] and interpret
    /// [`ConsensusMetadata::value`] itself.
    ///
    /// # Errors
    ///
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        unimplemented!()
    }

    /// The whole merged view.
    ///
    /// Every key from either source, with consensus values winning where both
    /// define one, subject to the same projection rules as [`Meta::get`]. A
    /// key is present here only if the projection could
    /// represent it as a string: an undecodable, unparseable, or non-object
    /// consensus document contributes no keys, and a key whose consensus
    /// value is a nested object or array is skipped in favour of the
    /// configuration value, or omitted if there is none. This map is
    /// therefore a view for rendering, never an inventory of what the
    /// federation's metadata contains; [`Meta::consensus_metadata`] is that.
    ///
    /// Ordered by key: the map is a
    /// [`BTreeMap`](std::collections::BTreeMap) rather than a hash map so
    /// that iteration order is deterministic, which matters both for
    /// rendering a stable list and for tests. Bindings receive it as their
    /// host language's ordinary map or dictionary type.
    ///
    /// # Errors
    ///
    /// The same as [`Meta::get`].
    pub async fn all(&self) -> Result<BTreeMap<String, String>> {
        unimplemented!()
    }

    /// The raw configuration metadata, exactly as the federation's
    /// configuration declares it.
    ///
    /// Synchronous and infallible: this comes from configuration the SDK
    /// already holds locally, so there is nothing to fetch and nothing to
    /// fail. No consensus values are merged in.
    pub fn config_metadata(&self) -> BTreeMap<String, String> {
        unimplemented!()
    }

    /// The raw consensus metadata, or `None` if this federation has no meta
    /// module.
    ///
    /// `None` is an ordinary answer, not a failure: a federation without a
    /// meta module is perfectly well-formed, and this is why [`Meta`]
    /// itself is unconditional while the capability facades are
    /// `Option`-returning: the absence lives here, at the level of the one
    /// thing that can actually be absent.
    ///
    /// The returned value is unprojected and carries its revision, so an
    /// application can parse the document itself and can tell whether it
    /// has changed since it last looked.
    ///
    /// # Errors
    ///
    /// The same as [`Meta::get`].
    pub async fn consensus_metadata(&self) -> Result<Option<ConsensusMetadata>> {
        unimplemented!()
    }

    /// Builds the facade for one federation. Handed out by `Federation::meta`.
    pub(crate) fn new(federation: Arc<crate::federation::FederationInner>) -> Meta {
        Meta {
            inner: Arc::new(MetaInner { federation }),
        }
    }
}

/// A revision of a federation's consensus metadata.
///
/// The guardians can change consensus metadata while the federation runs;
/// each agreed change increments [`ConsensusMetadata::revision`]. Comparing
/// revisions is how an application detects a change without diffing the
/// document.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConsensusMetadata {
    /// The revision number of this metadata. Monotonically increasing;
    /// a larger number is a later version of the same document.
    pub revision: u64,
    /// The metadata document as raw bytes, exactly as consensus stores it.
    ///
    /// Commonly, but not necessarily, UTF-8 JSON. The meta module's value is
    /// an arbitrary byte string with no encoding guarantee, so this field is
    /// `Vec<u8>` rather than `String`: a `String` could not hold what the
    /// guardians actually agreed on, and anything that did not decode would
    /// have to be mangled or dropped to fit. The SDK does not require,
    /// validate, reformat, or re-encode any of it: this is the unprojected
    /// value, byte for byte, for the application to interpret.
    ///
    /// The flat, string-valued projection used by [`Meta::get`] and
    /// [`Meta::all`] is derived from these bytes and is lossy; see the
    /// type-level documentation on [`Meta`] for exactly what it drops. This
    /// field is what remains authoritative when it does.
    pub value: Vec<u8>,
}

/// The federation this facade reads metadata from.
///
/// Unconditional, unlike the three capability facades: every federation has configuration
/// metadata even when it runs no meta module.
#[derive(Debug)]
struct MetaInner {
    federation: Arc<crate::federation::FederationInner>,
}
