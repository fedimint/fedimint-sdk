//! Federation discovery types for Nostr NIP-87 announcements and local dev fixtures.
//!
//! A [`DiscoveredFederation`] is a lightweight record representing a federation
//! discovered either from a local dev mock ([`DiscoverySource::Dev`])
//! or from Nostr NIP-87 (`kind:38173`) federation announcement events
//! ([`DiscoverySource::Nostr`]).
//!
//! Note that the dev mock ([`DiscoverySource::Dev`]) is a display-only placeholder
//! with a dummy test vector invite pointing at `wss://foo.bar/`; attempting to
//! [`Sdk::preview`](crate::Sdk::preview) or [`Sdk::join`](crate::Sdk::join) it
//! will fail to connect.
//!
//! Discovered entries are *advertisements*, not proven federation configuration.
//! To verify a federation, feed its [`invite`](DiscoveredFederation::invite)
//! to [`Sdk::preview`](crate::Sdk::preview) or [`Sdk::join`](crate::Sdk::join).

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{FederationId, InviteCode, Network};

#[cfg(test)]
mod tests;

/// Specifies the source for discovering federations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoverySource {
    /// Dev mode: returns a hardcoded display-only test federation without network queries.
    ///
    /// This fixture uses a dummy test invite (`wss://foo.bar/`) intended for UI
    /// layout and offline rendering; previewing or joining it will fail to connect.
    Dev,
    /// Production mode: queries Nostr relays for NIP-87 (`kind:38173`) federation announcements.
    Nostr {
        /// Nostr relay websocket URLs (e.g. `["wss://relay.damus.io"]`).
        relays: Vec<String>,
    },
}

impl DiscoverySource {
    /// Default relays to query in production mode when none are explicitly provided.
    pub const DEFAULT_RELAYS: &'static [&'static str] = &[
        "wss://relay.damus.io",
        "wss://nos.lol",
        "wss://relay.primal.net",
    ];

    /// Convenient constructor for production mode using default community relays.
    pub fn nostr_default() -> Self {
        Self::Nostr {
            relays: Self::DEFAULT_RELAYS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// One federation entry discovered from Nostr NIP-87 or a dev fixture.
///
/// This is an *advertisement*, not verified federation state. The `invite`
/// field has been parsed and its embedded federation ID cross-checked against
/// `id`, but the metadata is whatever the publisher claims — not what
/// the guardians serve. Use [`Sdk::preview`](crate::Sdk::preview) or
/// [`Sdk::join`](crate::Sdk::join) to verify.
///
/// This type is `#[non_exhaustive]`: new fields may be added in future releases.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DiscoveredFederation {
    /// The federation's identifier, parsed and validated.
    pub id: FederationId,
    /// The federation's invite code, parsed, validated, and cross-checked
    /// against `id`.
    pub invite: InviteCode,
    /// The federation's advertised human-readable name, if provided.
    pub name: Option<String>,
    /// The Bitcoin network this federation advertises operating on, if recognized.
    pub network: Option<Network>,
    /// The kind names of modules advertised by the federation (e.g. `["lightning", "wallet", "mint"]`).
    pub modules: Vec<String>,
    /// Publisher-supplied metadata for the entry.
    ///
    /// Extracted from NIP-87 event tags (e.g. `modules`, `network`, `publisher_pubkey`)
    /// and event content (e.g. `name`, `picture`, `about` / `description`).
    pub metadata: BTreeMap<String, String>,
}

fn parse_network(s: &str) -> Option<Network> {
    match s.trim().to_ascii_lowercase().as_str() {
        "bitcoin" | "mainnet" => Some(Network::Bitcoin),
        "testnet" | "testnet3" => Some(Network::Testnet),
        "testnet4" => Some(Network::Testnet4),
        "signet" => Some(Network::Signet),
        "regtest" => Some(Network::Regtest),
        _ => None,
    }
}

impl DiscoveredFederation {
    /// Returns the hardcoded display-only test federation for dev/offline mode.
    ///
    /// The returned entry contains placeholder metadata and a dummy invite code
    /// that cannot be previewed or joined.
    pub fn dev() -> Self {
        dev_federation()
    }
}

/// Returns the hardcoded display-only test federation for dev/offline mode.
///
/// See [`DiscoveredFederation::dev`].
pub(crate) fn dev_federation() -> DiscoveredFederation {
    let id: FederationId = "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a"
        .parse()
        .expect("valid dev federation id");
    let invite = InviteCode::parse(
        "fed11qgqpqrnhwden5te0vehk7tnzv9ez7qqpyq4z52329g4z52329g4z52329g4z52329g4z52329g4z52329g4z5wa8phk"
            .to_string(),
    )
    .expect("valid dev invite code");
    let mut metadata = BTreeMap::new();
    metadata.insert("name".to_string(), "Dev Test Federation".to_string());
    metadata.insert(
        "federation_name".to_string(),
        "Dev Test Federation".to_string(),
    );
    metadata.insert("network".to_string(), "regtest".to_string());
    metadata.insert("modules".to_string(), "lightning,wallet,mint".to_string());
    DiscoveredFederation {
        id,
        invite,
        name: Some("Dev Test Federation".to_string()),
        network: Some(Network::Regtest),
        modules: vec![
            "lightning".to_string(),
            "wallet".to_string(),
            "mint".to_string(),
        ],
        metadata,
    }
}

/// Raw representation of a Nostr NIP-87 `kind:38173` event.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawNip87Event {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub pubkey: Option<String>,
    #[serde(default)]
    pub created_at: Option<u64>,
    #[serde(default)]
    pub kind: Option<u64>,
    #[serde(default)]
    pub tags: Vec<Vec<String>>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub sig: Option<String>,
}

impl RawNip87Event {
    /// Verifies the NIP-01 Schnorr signature of the event.
    ///
    /// Validates that:
    /// 1. `id`, `pubkey`, `created_at`, `kind`, and `sig` are present and well-formed.
    /// 2. Canonical NIP-01 serialization `[0, pubkey, created_at, kind, tags, content]`
    ///    SHA256 hashes to `id`.
    /// 3. `sig` is a valid BIP-340 Schnorr signature over `id` verified by `pubkey`.
    pub fn verify_signature(&self) -> bool {
        let (Some(id_hex), Some(pubkey_hex), Some(created_at), Some(kind), Some(sig_hex)) = (
            self.id.as_deref(),
            self.pubkey.as_deref(),
            self.created_at,
            self.kind,
            self.sig.as_deref(),
        ) else {
            return false;
        };

        let Ok(serialized) =
            serde_json::to_string(&(0, pubkey_hex, created_at, kind, &self.tags, &self.content))
        else {
            return false;
        };

        use fedimint_core::bitcoin::hashes::{Hash, sha256};
        let computed_id = sha256::Hash::hash(serialized.as_bytes());
        let computed_id_hex = format!("{computed_id:x}");
        if !computed_id_hex.eq_ignore_ascii_case(id_hex) {
            return false;
        }

        use std::str::FromStr;
        let Ok(pubkey) = fedimint_core::secp256k1::XOnlyPublicKey::from_str(pubkey_hex) else {
            return false;
        };
        let Ok(sig) = fedimint_core::secp256k1::schnorr::Signature::from_str(sig_hex) else {
            return false;
        };

        let msg = fedimint_core::secp256k1::Message::from_digest(computed_id.to_byte_array());
        fedimint_core::secp256k1::SECP256K1
            .verify_schnorr(&sig, &msg, &pubkey)
            .is_ok()
    }
}

impl std::convert::TryFrom<RawNip87Event> for DiscoveredFederation {
    type Error = &'static str;

    fn try_from(raw: RawNip87Event) -> std::result::Result<Self, Self::Error> {
        // 1. Verify event kind
        if raw.kind != Some(38173) {
            return Err("missing or unsupported event kind: expected 38173");
        }

        // 2. Extract federation ID from 'd' tag
        let d_tag = raw
            .tags
            .iter()
            .find(|t| t.first().map(String::as_str) == Some("d"))
            .ok_or("missing d tag (federation id)")?;
        let id_str = d_tag
            .get(1)
            .map(String::as_str)
            .map(str::trim)
            .ok_or("empty d tag value")?;
        let id: FederationId = id_str
            .parse()
            .map_err(|_| "invalid federation id in d tag")?;

        // 3. Find first matching invite code from 'u' tags
        let invite = raw
            .tags
            .iter()
            .filter(|t| t.first().map(String::as_str) == Some("u"))
            .filter_map(|tag| tag.get(1).map(String::as_str).map(str::trim))
            .filter_map(|val| InviteCode::parse(val.to_string()).ok())
            .find(|parsed| parsed.federation_id() == id)
            .ok_or("no matching invite code in u tags for declared federation id")?;

        // 4. Build metadata map
        let mut metadata = BTreeMap::new();

        // Parse content: kind:0-style JSON metadata or plain description.
        // Skip reserved protocol keys so untrusted content cannot spoof advertised event metadata.
        let content_trimmed = raw.content.trim();
        if !content_trimmed.is_empty() {
            if let Ok(serde_json::Value::Object(map)) =
                serde_json::from_str::<serde_json::Value>(content_trimmed)
            {
                for (k, v) in map {
                    if matches!(k.as_str(), "publisher_pubkey" | "modules" | "network") {
                        continue;
                    }
                    match v {
                        serde_json::Value::Null => {}
                        serde_json::Value::String(s) => {
                            metadata.insert(k, s);
                        }
                        other => {
                            metadata.insert(k, other.to_string());
                        }
                    }
                }
            } else {
                metadata.insert("description".to_string(), content_trimmed.to_string());
            }
        }

        // Event envelope publisher pubkey (unverified advertisement metadata)
        if let Some(pk) = raw.pubkey {
            metadata.insert("publisher_pubkey".to_string(), pk);
        }

        // Extract advertised NIP-87 tags
        for tag in &raw.tags {
            match tag.first().map(String::as_str) {
                Some("modules") => {
                    if let Some(val) = tag.get(1) {
                        metadata.insert("modules".to_string(), val.trim().to_string());
                    }
                }
                Some("n") => {
                    if let Some(val) = tag.get(1) {
                        metadata.insert("network".to_string(), val.trim().to_string());
                    }
                }
                _ => {}
            }
        }

        let name = metadata
            .get("name")
            .cloned()
            .or_else(|| metadata.get("federation_name").cloned());
        let network = metadata.get("network").and_then(|s| parse_network(s));
        let modules = metadata
            .get("modules")
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|m| !m.is_empty())
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();

        Ok(DiscoveredFederation {
            id,
            invite,
            name,
            network,
            modules,
            metadata,
        })
    }
}
