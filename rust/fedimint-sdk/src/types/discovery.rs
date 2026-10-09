//! Federation discovery types for the observer API.
//!
//! A [`DiscoveredFederation`] is a lightweight record returned by a
//! community-run federation index (such as `observer.fedimint.org`). It
//! carries just enough to render a browsable list of federations: a name,
//! an invite code, a health signal, and aggregate deposit and activity
//! numbers. It does *not* carry anything the SDK validates against its own
//! configuration rules — that check happens when the caller takes the
//! invite code and feeds it to [`Sdk::preview`](crate::Sdk::preview), or
//! directly to [`Sdk::join`](crate::Sdk::join).

use serde::Deserialize;

/// The default URL for the Fedimint observer API.
///
/// Exposed as a constant so that applications have a canonical default
/// and do not need to know the URL themselves, while still being free to
/// pass a different one to [`Sdk::discover_federations`](crate::Sdk::discover_federations).
pub const DEFAULT_DISCOVERY_URL: &str = "https://observer.fedimint.org/api/federations";

/// One federation as reported by a community-run federation index.
///
/// This is a plain data record parsed from the JSON the index returns. It
/// is not validated against the federation's own configuration: a
/// `DiscoveredFederation` is an *advertisement*, not a proof. Use its
/// [`invite`](DiscoveredFederation::invite) field to
/// [`preview`](crate::Sdk::preview) or [`join`](crate::Sdk::join) the
/// federation, at which point the SDK contacts the guardians and applies
/// its own validation.
///
/// Fields that the index may add in the future are silently ignored
/// (`#[serde(deny_unknown_fields)]` is deliberately absent), so a schema
/// change on the server side does not break existing SDK builds.
///
/// This type is `#[non_exhaustive]`: new fields may be added in future
/// releases of this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[non_exhaustive]
pub struct DiscoveredFederation {
    /// The federation's hex-encoded identifier, as the index reports it.
    pub id: String,
    /// The federation's human-readable name, when the index provides one.
    pub name: Option<String>,
    /// The federation's invite code, ready to be passed to
    /// [`Sdk::preview`](crate::Sdk::preview) or
    /// [`Sdk::join`](crate::Sdk::join).
    pub invite: Option<String>,
    /// Total deposits tracked by the index, in millisatoshis.
    pub deposits: Option<u64>,
    /// The health status as reported by the index (e.g. `"online"`,
    /// `"offline"`).
    pub health: Option<String>,
}

/// The raw JSON shape the observer API returns for each federation.
///
/// Kept `pub(crate)` because callers see [`DiscoveredFederation`], not
/// this intermediate form. Every field uses `default` so that a missing
/// or `null` value silently becomes `None` rather than failing the parse.
#[derive(Deserialize)]
pub(crate) struct RawDiscoveredFederation {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub invite: Option<String>,
    #[serde(default)]
    pub deposits: Option<u64>,
    #[serde(default)]
    pub health: Option<String>,
}

impl std::convert::TryFrom<RawDiscoveredFederation> for DiscoveredFederation {
    type Error = &'static str;

    fn try_from(raw: RawDiscoveredFederation) -> std::result::Result<Self, Self::Error> {
        Ok(DiscoveredFederation {
            id: raw.id.ok_or("missing id")?,
            name: raw.name,
            invite: raw.invite,
            deposits: raw.deposits,
            health: raw.health,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_entry() {
        let json = r#"{"id":"abc123"}"#;
        let raw: RawDiscoveredFederation = serde_json::from_str(json).unwrap();
        let fed = DiscoveredFederation::try_from(raw).expect("Failed to parse");
        assert_eq!(fed.id, "abc123");
        assert!(fed.name.is_none());
        assert!(fed.invite.is_none());
        assert!(fed.deposits.is_none());
        assert!(fed.health.is_none());
    }

    #[test]
    fn parses_a_full_entry() {
        let json = r#"{
            "id": "abc123",
            "name": "Test Federation",
            "invite": "fed11qgq...",
            "deposits": 1000000,
            "health": "online"
        }"#;
        let raw: RawDiscoveredFederation = serde_json::from_str(json).unwrap();
        let fed = DiscoveredFederation::try_from(raw).expect("Failed to parse");
        assert_eq!(fed.id, "abc123");
        assert_eq!(fed.name.as_deref(), Some("Test Federation"));
        assert_eq!(fed.invite.as_deref(), Some("fed11qgq..."));
        assert_eq!(fed.deposits, Some(1000000));
        assert_eq!(fed.health.as_deref(), Some("online"));
    }

    #[test]
    fn ignores_unknown_fields_gracefully() {
        let json = r#"{
            "id": "abc123",
            "name": "Test",
            "some_future_field": 42,
            "another_new_thing": {"nested": true}
        }"#;
        let raw: RawDiscoveredFederation = serde_json::from_str(json).unwrap();
        let fed = DiscoveredFederation::try_from(raw).expect("Failed to parse");
        assert_eq!(fed.id, "abc123");
        assert_eq!(fed.name.as_deref(), Some("Test"));
    }

    #[test]
    fn parses_an_empty_object_as_error() {
        let json = r#"{}"#;
        let raw: RawDiscoveredFederation = serde_json::from_str(json).unwrap();
        let fed_result = DiscoveredFederation::try_from(raw);
        assert!(fed_result.is_err());
    }
}
