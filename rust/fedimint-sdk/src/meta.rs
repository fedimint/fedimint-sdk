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
///   agreed by the guardians at runtime, and is *revisioned* — the
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
/// The raw sources stay available separately —
/// [`Meta::config_metadata`] and [`Meta::consensus_metadata`] — so an
/// application that needs to know *where* a value came from, or that needs
/// the consensus revision, is not forced to work backwards from the merged
/// result.
///
/// # The merged view is lossy
///
/// This matters enough to state plainly. The meta module stores arbitrary
/// bytes, in practice a JSON document. The merged view projects that
/// document to `String`-valued keys by taking its top-level entries, which
/// loses information in three ways: nested objects and arrays cannot be
/// represented as a flat string map, non-string scalars (numbers, booleans,
/// null) are rendered as text and stop being distinguishable from strings
/// that look like them, and a document that is not a top-level JSON object
/// at all has no top-level entries to project. Anything that depends on
/// the document's structure must read
/// [`ConsensusMetadata::value`] and parse it, not use the merged view.
#[derive(Debug, Clone)]
pub struct Meta {
    inner: Arc<MetaInner>,
}

impl Meta {
    /// Looks up one key in the merged view.
    ///
    /// Returns the consensus value if the meta module defines this key, the
    /// configuration value if only the configuration does, and `None` if
    /// neither does. Asynchronous and fallible because reading consensus
    /// metadata may require contacting the federation.
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
    /// Every key from either source, with consensus values winning where
    /// both define one. Ordered by key: the map is a
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
    /// `Option`-returning — the absence lives here, at the level of the one
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
    /// The metadata document as the meta module stores it, as a string.
    ///
    /// In practice this is JSON, but the module stores arbitrary bytes and
    /// the SDK does not require, validate, or reformat any particular
    /// structure — this is the raw value, for the application to interpret.
    /// The flat, string-valued projection used by [`Meta::get`] and
    /// [`Meta::all`] is derived from this and is lossy; see the type-level
    /// documentation on [`Meta`].
    pub value: String,
}

/// Placeholder for the metadata sources this facade reads.
#[derive(Debug)]
struct MetaInner;
