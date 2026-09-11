//! Federation metadata, from configuration and from consensus.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use crate::{Error, ErrorCode, Result};

/// How long `get`/`all`/`consensus_metadata` wait for the consensus data
/// before returning a timeout error.
const CONSENSUS_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

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
    /// [`Timeout`](crate::ErrorCode::Timeout), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        let merged = self.merged_view().await?;
        Ok(merged.get(key).cloned())
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
        self.merged_view().await
    }

    /// The raw configuration metadata, exactly as the federation's
    /// configuration declares it.
    ///
    /// Synchronous and infallible: this comes from configuration the SDK
    /// already holds locally, so there is nothing to fetch and nothing to
    /// fail. No consensus values are merged in.
    pub fn config_metadata(&self) -> BTreeMap<String, String> {
        self.inner.federation.config_meta()
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
        let client = self.inner.federation.client(false).await?;
        let Ok(module) = client.get_first_module::<fedimint_meta_client::MetaClientModule>() else {
            return Ok(None);
        };

        let result = fedimint_core::runtime::timeout(
            CONSENSUS_FETCH_TIMEOUT,
            module.get_consensus_value(fedimint_meta_common::DEFAULT_META_KEY),
        )
        .await
        .map_err(|_| Error::new(ErrorCode::Timeout, "consensus metadata fetch timed out"))?;

        let maybe_mcv = result.map_err(|err| {
            Error::new(
                ErrorCode::FederationUnreachable,
                format!("failed to fetch consensus metadata: {err}"),
            )
        })?;

        Ok(maybe_mcv.map(|mcv| ConsensusMetadata {
            revision: mcv.revision,
            value: mcv.value.as_slice().to_vec(),
        }))
    }

    /// Builds the facade for one federation. Handed out by `Federation::meta`.
    pub(crate) fn new(federation: Arc<crate::federation::FederationInner>) -> Meta {
        Meta {
            inner: Arc::new(MetaInner { federation }),
        }
    }

    async fn merged_view(&self) -> Result<BTreeMap<String, String>> {
        let consensus = self.consensus_metadata().await;
        let config = self.inner.federation.config_meta();

        match consensus {
            Ok(Some(mcv)) => Ok(apply_consensus_bytes(&config, &mcv.value)),
            Ok(None) => Ok(config),
            Err(err) => Err(err),
        }
    }
}

fn apply_consensus_bytes(
    config: &BTreeMap<String, String>,
    consensus_bytes: &[u8],
) -> BTreeMap<String, String> {
    let mut merged = config.clone();
    if let Ok(json_str) = std::str::from_utf8(consensus_bytes) {
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(json_str) {
            for (key, value) in map {
                match value {
                    serde_json::Value::String(s) => {
                        merged.insert(key, s);
                    }
                    serde_json::Value::Number(n) => {
                        merged.insert(key, n.to_string());
                    }
                    serde_json::Value::Bool(b) => {
                        merged.insert(key, b.to_string());
                    }
                    serde_json::Value::Null => {
                        merged.insert(key, "null".to_owned());
                    }
                    // Object and Array are skipped, not projected to empty.
                    serde_json::Value::Object(_) | serde_json::Value::Array(_) => {}
                }
            }
        }
    }
    merged
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn apply_consensus_bytes_maps_types_and_prioritizes_consensus() {
        let mut config = BTreeMap::new();
        config.insert("only_config".to_owned(), "c1".to_owned());
        config.insert("overlap".to_owned(), "c2".to_owned());

        let payload = json!({
            "overlap": "m1",
            "only_consensus": "m2",
            "num": 42,
            "bool": true,
            "null": null,
            "obj": {"a": 1},
            "arr": [1, 2]
        })
        .to_string();

        let merged = apply_consensus_bytes(&config, payload.as_bytes());

        assert_eq!(merged.len(), 6);
        assert_eq!(merged.get("only_config"), Some(&"c1".to_owned()));
        assert_eq!(merged.get("overlap"), Some(&"m1".to_owned())); // consensus wins
        assert_eq!(merged.get("only_consensus"), Some(&"m2".to_owned()));
        assert_eq!(merged.get("num"), Some(&"42".to_owned()));
        assert_eq!(merged.get("bool"), Some(&"true".to_owned()));
        assert_eq!(merged.get("null"), Some(&"null".to_owned()));
        assert!(!merged.contains_key("obj"));
        assert!(!merged.contains_key("arr"));
    }

    #[test]
    fn apply_consensus_bytes_handles_invalid_utf8_by_returning_config() {
        let mut config = BTreeMap::new();
        config.insert("c1".to_owned(), "v1".to_owned());

        let invalid_utf8 = vec![0xff, 0xff, 0xff];
        let merged = apply_consensus_bytes(&config, &invalid_utf8);
        assert_eq!(merged, config);
    }

    #[test]
    fn apply_consensus_bytes_handles_non_json_by_returning_config() {
        let mut config = BTreeMap::new();
        config.insert("c1".to_owned(), "v1".to_owned());

        let invalid_json = b"not valid json";
        let merged = apply_consensus_bytes(&config, invalid_json);
        assert_eq!(merged, config);
    }

    #[test]
    fn apply_consensus_bytes_handles_non_object_json_by_returning_config() {
        let mut config = BTreeMap::new();
        config.insert("c1".to_owned(), "v1".to_owned());

        for payload in [
            b"[]".as_slice(),
            b"\"string\"".as_slice(),
            b"123".as_slice(),
            b"true".as_slice(),
            b"null".as_slice(),
        ] {
            let merged = apply_consensus_bytes(&config, payload);
            assert_eq!(merged, config);
        }
    }

    #[test]
    fn apply_consensus_bytes_preserves_config_when_key_skipped() {
        let mut config = BTreeMap::new();
        config.insert("server".to_owned(), "https://example.com".to_owned());

        // Nested object violates Rule 4 and should be skipped under Rule 5.
        let payload = br#"{"server": {"host": "example.com"}}"#;

        let merged = apply_consensus_bytes(&config, payload);

        assert_eq!(
            merged.get("server"),
            Some(&"https://example.com".to_owned())
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn public_api_get_and_all_return_federation_closed_error() {
        use crate::FederationStatus;
        use crate::db::{FederationRecord, StoredCapabilities, StoredNetwork, StoredStatus};
        use crate::federation::FederationInner;
        use fedimint_core::PeerId;
        use fedimint_core::config::FederationId;
        use fedimint_core::util::SafeUrl;
        use std::sync::{Arc, Weak};

        let id = FederationId::dummy();
        let capabilities = StoredCapabilities {
            ecash: true,
            lightning: true,
            onchain: true,
        };
        let record = FederationRecord {
            invite: fedimint_core::invite_code::InviteCode::new(
                SafeUrl::parse("wss://guardian.example:5000").expect("a valid url"),
                PeerId::from(0),
                id,
                None,
            ),
            network: StoredNetwork::Regtest,
            status: StoredStatus::Closed,
            capabilities,
            generation: Some(1),
            name: Some("Test Federation".to_owned()),
        };

        let root = crate::db::in_memory_root();
        let mut initial_config_meta = BTreeMap::new();
        initial_config_meta.insert("name".to_owned(), "Config Name".to_owned());
        let federation_inner = Arc::new(FederationInner::new(
            id,
            Weak::new(),
            root.with_prefix(crate::db::federation_prefix(&id).to_vec()),
            record,
            initial_config_meta.clone(),
            FederationStatus::Closed,
            None,
        ));

        let meta = Meta::new(federation_inner);

        // config_metadata remains synchronous and infallible even if the federation is closed
        assert_eq!(meta.config_metadata(), initial_config_meta);

        // consensus_metadata, get, and all return FederationClosed error
        let consensus_err = meta.consensus_metadata().await.expect_err("should fail");
        assert_eq!(consensus_err.code, ErrorCode::FederationClosed);

        let get_err = meta.get("any_key").await.expect_err("should fail");
        assert_eq!(get_err.code, ErrorCode::FederationClosed);

        let all_err = meta.all().await.expect_err("should fail");
        assert_eq!(all_err.code, ErrorCode::FederationClosed);
    }
}
