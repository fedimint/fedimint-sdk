//! Federation discovery: the list an application shows before anything is joined.
//!
//! The contract a caller reads lives on [`Sdk::discover_federations`] and
//! [`DiscoveredFederation`], since this module is private and its documentation reaches
//! nobody. What is here is the shape of the implementation: fetch, parse, then drop every
//! entry this SDK cannot hand to `join`. Everything after the fetch is pure, which is what
//! lets the rules that matter be tested without a server.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::{Error, ErrorCode, FederationId, InviteCode, Result, Sdk, Timestamp};

/// The key carrying the entry's invite code. The one field discovery requires.
const INVITE_CODE_KEY: &str = "invite_code";

/// The key read as an end-of-life time, in UNIX seconds.
///
/// The only one, deliberately: it is what fedimint documents in `docs/meta_fields/`. A
/// publisher's own end-of-life key (Fedi's `fedi:popup_end_timestamp`, say) reaches the
/// caller untouched in [`DiscoveredFederation::meta`] and is theirs to filter on, because
/// this SDK cannot know what a vendor-namespaced key means.
const EXPIRY_KEY: &str = "federation_expiry_timestamp";

/// One federation from a discovery document, ready to preview or join.
///
/// Obtained from [`Sdk::discover_federations`]. The [`invite`](Self::invite) is what
/// [`Sdk::preview`] and [`Sdk::join`] take.
///
/// # What this is and is not evidence of
///
/// Everything here except the [`id`](Self::id) is **what the publisher of the list
/// claims**. No guardian has been contacted: a name, an icon URL or a welcome message is
/// advertising copy until [`Sdk::preview`] says otherwise. Populate a browse screen from
/// this, and show the user `preview`'s answer for what they are actually about to join.
///
/// The one thing the SDK checks is that an entry is internally consistent: the invite code
/// parses, and the federation id it carries is the id the entry was filed under. An entry
/// failing either check never reaches a caller.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DiscoveredFederation {
    /// The federation's identifier, as the document filed it under.
    ///
    /// Always equal to the id carried by [`invite`](Self::invite): an entry where the two
    /// disagree is dropped, so this is never merely the publisher's word for it.
    pub id: FederationId,
    /// The invite code for this federation, parsed and therefore well-formed.
    pub invite: InviteCode,
    /// Every other field of the entry, as the publisher wrote it.
    ///
    /// Same shape and key conventions as
    /// [`FederationPreview::meta`](crate::FederationPreview::meta), but a different
    /// provenance: the publisher's, not the guardians'.
    pub meta: BTreeMap<String, String>,
}

impl Sdk {
    /// Fetches a list of joinable federations from `url`.
    ///
    /// The step before everything else in this crate: an application points the SDK at a
    /// URL it controls and gets back entries [`Sdk::preview`] and [`Sdk::join`] accept
    /// directly. **The SDK ships no list and no default URL**, and never will: which
    /// federations a user is offered is a decision for the application, not for this crate.
    ///
    /// Entries come back ordered by federation id rather than in the document's own order,
    /// which JSON does not preserve anyway. Entries that do not parse, whose invite code
    /// disagrees with the id they are filed under, or that announce an end already past are
    /// left out: a list is a screen of choices, and one bad row is not a reason to offer
    /// none.
    ///
    /// Nothing is cached or persisted. Each call is a fresh fetch, leaving the caching
    /// policy to the application, which is the only layer that knows how stale is too stale.
    ///
    /// # The document
    ///
    /// A JSON object whose keys are federation ids and whose values are that federation's
    /// fields. `invite_code` is required; every other key is carried through into
    /// [`DiscoveredFederation::meta`] unchanged, so a publisher already serving a
    /// `meta_override_url` document can reuse the same key conventions (`federation_name`,
    /// `welcome_message`, `federation_icon_url`, ...) that fedimint documents in
    /// `docs/meta_fields/`.
    ///
    /// ```json
    /// {
    ///   "2a2a2a2a…": {
    ///     "invite_code": "fed11qgqpqrnhwden5te0vehk7tnzv9ez7qqpyq4z52329g4z52329g4z5…",
    ///     "federation_name": "Example Federation",
    ///     "welcome_message": "Ecash for the example community.",
    ///     "federation_expiry_timestamp": "1790000000"
    ///   }
    /// }
    /// ```
    ///
    /// Values may be JSON strings or numbers; a number is carried through as the string a
    /// reader would have written, so `1790000000` and `"1790000000"` are the same entry.
    /// Keys whose value is an array, an object or `null` are dropped, since `meta` is a map
    /// of strings and there is no lossless spelling for those.
    ///
    /// # Federations that have ended
    ///
    /// An entry announcing its own end is dropped once that moment has passed, so a browse
    /// screen does not offer a federation that has shut down. One key is honoured:
    /// `federation_expiry_timestamp`, which fedimint documents as "a UNIX timestamp in
    /// seconds after which the federation will shut down".
    ///
    /// It is **seconds**, while [`Timestamp`](crate::Timestamp) is epoch milliseconds, so
    /// the conversion happens once, where the document is parsed. A value that is not a
    /// base-10 count of seconds is ignored rather than treated as an expiry: the publisher
    /// said something this SDK does not understand, which is not a reason to withhold the
    /// federation.
    ///
    /// A publisher with an end-of-life key of its own (Fedi's `fedi:popup_end_timestamp`,
    /// for instance) is not read here, because this SDK cannot know what a vendor's key
    /// means. Such keys arrive intact in [`DiscoveredFederation::meta`], where the
    /// application that recognises them can filter on them itself.
    ///
    /// # Errors
    ///
    /// - [`ErrorCode::EndpointUnreachable`] if the endpoint could not be reached or answered
    ///   with anything other than a success status. Distinct from
    ///   [`ErrorCode::FederationUnreachable`], which is about guardians; this is an ordinary
    ///   web server the application chose.
    /// - [`ErrorCode::Timeout`] if the request did not complete in time.
    /// - [`ErrorCode::InvalidInput`] if the endpoint answered with something that is not a
    ///   discovery document. A document whose *entries* are malformed is not an error; those
    ///   entries are skipped.
    /// - [`ErrorCode::FederationClosed`] if the SDK instance has been shut down.
    ///
    /// Returning an empty list is a success: it means the publisher currently offers nothing
    /// this SDK can use, which an application should render as such rather than as a failure.
    pub async fn discover_federations(&self, url: &str) -> Result<Vec<DiscoveredFederation>> {
        self.inner().alive()?;
        let now = Timestamp::from_epoch_millis(crate::db::now_millis());
        discover_with(fetch, url.to_owned(), now).await
    }
}

/// The fetch-then-parse pipeline, with the fetching left to the caller.
///
/// The seam the tests use: they serve a fixture document instead of standing up a web
/// server, and everything below this point is exercised exactly as production runs it.
// The url crosses as an owned `String` rather than a `&str`: a closure generic over the
// lifetime of a borrowed argument needs a higher-ranked bound that an `async fn` does not
// satisfy, and one allocation per call is not worth the ceremony of working around that.
pub(crate) async fn discover_with<F, Fut>(
    fetch: F,
    url: String,
    now: Timestamp,
) -> Result<Vec<DiscoveredFederation>>
where
    F: FnOnce(String) -> Fut,
    Fut: core::future::Future<Output = Result<String>>,
{
    let body = fetch(url).await?;
    Ok(entries_of(&parse_document(&body)?, now))
}

/// Parses the document into its entries, without interpreting any of them.
///
/// An unparseable document is the one shape of failure that is *not* skipped: there are no
/// entries to salvage, and reporting an empty list would tell the application that the
/// publisher offers nothing rather than that its endpoint is broken.
fn parse_document(body: &str) -> Result<BTreeMap<String, BTreeMap<String, Value>>> {
    serde_json::from_str(body).map_err(|err| {
        Error::new(
            ErrorCode::InvalidInput,
            format!("this endpoint did not answer with a discovery document: {err}"),
        )
    })
}

/// Turns parsed entries into the ones this SDK can offer, dropping the rest.
fn entries_of(
    document: &BTreeMap<String, BTreeMap<String, Value>>,
    now: Timestamp,
) -> Vec<DiscoveredFederation> {
    document
        .iter()
        .filter_map(|(id, fields)| entry_of(id, fields, now))
        .collect()
}

/// One entry, or `None` if it is not one this SDK can hand to `join`.
fn entry_of(
    id: &str,
    fields: &BTreeMap<String, Value>,
    now: Timestamp,
) -> Option<DiscoveredFederation> {
    let id: FederationId = id.parse().ok()?;

    let invite: InviteCode = fields.get(INVITE_CODE_KEY)?.as_str()?.parse().ok()?;

    // The publisher filed this entry under an id; the invite code carries one of its own.
    // Disagreement means at least one of them is wrong, and joining the wrong federation is
    // not a recoverable mistake.
    if invite.federation_id() != id {
        return None;
    }

    let meta: BTreeMap<String, String> = fields
        .iter()
        .filter(|(key, _)| key.as_str() != INVITE_CODE_KEY)
        .filter_map(|(key, value)| Some((key.clone(), string_of(value)?)))
        .collect();

    if has_ended(&meta, now) {
        return None;
    }

    Some(DiscoveredFederation { id, invite, meta })
}

/// A scalar as the string a reader would have written, or `None` for a value with no such
/// spelling.
fn string_of(value: &Value) -> Option<String> {
    match value {
        Value::String(string) => Some(string.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(boolean) => Some(boolean.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// Whether the entry announces an end that has already passed.
fn has_ended(meta: &BTreeMap<String, String>, now: Timestamp) -> bool {
    meta.get(EXPIRY_KEY)
        .and_then(|seconds| seconds.trim().parse::<u64>().ok())
        // Seconds in the document, milliseconds in a `Timestamp`. A count so large that it
        // cannot be milliseconds is a time no clock will reach, so treat it as not ended
        // rather than wrapping into the past.
        .and_then(|seconds| seconds.checked_mul(1_000))
        .is_some_and(|millis| millis <= now.epoch_millis())
}

/// Fetches the document over HTTP.
///
/// Deliberately plain: one request, no retries, no caching, no headers of its own. The
/// endpoint belongs to the application, so anything policy-shaped about how it is called
/// belongs to the application too.
async fn fetch(url: String) -> Result<String> {
    let response = reqwest::get(url).await.map_err(unreachable_endpoint)?;

    let status = response.status();
    if !status.is_success() {
        return Err(Error::new(
            ErrorCode::EndpointUnreachable,
            format!("this endpoint answered with {status}"),
        ));
    }

    response.text().await.map_err(unreachable_endpoint)
}

/// Maps a transport failure onto the crate's codes, keeping a timeout distinguishable from
/// an endpoint that is simply not there.
fn unreachable_endpoint(err: reqwest::Error) -> Error {
    if err.is_timeout() {
        Error::new(
            ErrorCode::Timeout,
            format!("this endpoint did not answer in time: {err}"),
        )
    } else {
        Error::new(
            ErrorCode::EndpointUnreachable,
            format!("this endpoint could not be reached: {err}"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same real invite code `types/invite.rs` tests with: guardian URL `wss://foo.bar`,
    /// peer 0, and the upstream dummy federation id. A made-up string does not survive the
    /// bech32m checksum the parse enforces.
    const CODE: &str = "fed11qgqpqrnhwden5te0vehk7tnzv9ez7qqpyq4z52329g4z52329g4z52329g4z52329g4z52329g4z52329g4z5wa8phk";
    /// The id that code invites to: 32 bytes of `0x2a`.
    const ID: &str = "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a";
    /// Some other federation's id, for the entry whose key and invite code disagree.
    const OTHER_ID: &str = "2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b";

    /// A fixed "now" every expiry case is measured against, so no test depends on the clock:
    /// 2023-11-14T22:13:20Z, which is 1_700_000_000 seconds.
    fn now() -> Timestamp {
        Timestamp::from_epoch_millis(1_700_000_000_000)
    }

    fn entries(document: &str) -> Vec<DiscoveredFederation> {
        entries_of(
            &parse_document(document).expect("a well-formed document"),
            now(),
        )
    }

    #[test]
    fn an_entry_becomes_something_join_can_take() {
        let found = entries(&format!(
            r#"{{"{ID}": {{"invite_code": "{CODE}", "federation_name": "Example"}}}}"#
        ));

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id.to_string(), ID);
        assert_eq!(found[0].invite.to_string(), CODE);
        assert_eq!(
            found[0].meta.get("federation_name").map(String::as_str),
            Some("Example")
        );
        // The invite code is a field of its own, not one more metadata entry to render.
        assert!(!found[0].meta.contains_key(INVITE_CODE_KEY));
    }

    #[test]
    fn scalars_cross_as_the_strings_a_reader_would_have_written() {
        let found = entries(&format!(
            r#"{{"{ID}": {{"invite_code": "{CODE}", "count": 7, "vetted": true,
                 "missing": null, "list": [1], "nested": {{"a": 1}}}}}}"#
        ));

        assert_eq!(found[0].meta.get("count").map(String::as_str), Some("7"));
        assert_eq!(
            found[0].meta.get("vetted").map(String::as_str),
            Some("true")
        );
        // No lossless string for these, so they are left out rather than guessed at.
        assert!(!found[0].meta.contains_key("missing"));
        assert!(!found[0].meta.contains_key("list"));
        assert!(!found[0].meta.contains_key("nested"));
    }

    #[test]
    fn an_entry_filed_under_an_id_its_invite_code_disagrees_with_is_dropped() {
        let found = entries(&format!(r#"{{"{OTHER_ID}": {{"invite_code": "{CODE}"}}}}"#));
        assert!(found.is_empty());
    }

    #[test]
    fn entries_without_a_usable_invite_code_are_dropped() {
        for entry in [
            r#"{"federation_name": "no invite code here"}"#,
            r#"{"invite_code": "not an invite code"}"#,
            r#"{"invite_code": 7}"#,
        ] {
            assert!(
                entries(&format!(r#"{{"{ID}": {entry}}}"#)).is_empty(),
                "{entry}"
            );
        }
    }

    #[test]
    fn one_bad_entry_does_not_take_the_rest_of_the_list_with_it() {
        let found = entries(&format!(
            r#"{{"{ID}": {{"invite_code": "{CODE}"}},
                "{OTHER_ID}": {{"invite_code": "nonsense"}},
                "not-a-federation-id": {{"invite_code": "{CODE}"}}}}"#
        ));

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id.to_string(), ID);
    }

    #[test]
    fn an_expiry_is_read_as_seconds_and_not_as_milliseconds() {
        // 500 seconds after `now`. Read as milliseconds it lands in January 1970, so a
        // reader that confused the two would drop this entry; that is the whole point of
        // the case.
        let ahead = 1_700_000_500u64;
        let behind = 1_699_999_000u64;
        let key = EXPIRY_KEY;

        let still_open = entries(&format!(
            r#"{{"{ID}": {{"invite_code": "{CODE}", "{key}": "{ahead}"}}}}"#
        ));
        assert_eq!(
            still_open.len(),
            1,
            "an expiry in the future should be offered"
        );

        let ended = entries(&format!(
            r#"{{"{ID}": {{"invite_code": "{CODE}", "{key}": "{behind}"}}}}"#
        ));
        assert!(ended.is_empty(), "an expiry in the past should be dropped");
    }

    #[test]
    fn an_expiry_falling_exactly_now_counts_as_ended() {
        // The boundary the comparison picks: a federation whose announced end is this very
        // millisecond has ended, rather than being offered for one last instant.
        let key = EXPIRY_KEY;
        let ended = entries(&format!(
            r#"{{"{ID}": {{"invite_code": "{CODE}", "{key}": "1700000000"}}}}"#
        ));
        assert!(ended.is_empty());
    }

    #[test]
    fn an_empty_document_is_an_empty_list_rather_than_a_failure() {
        assert!(entries("{}").is_empty());
    }

    #[test]
    fn entries_come_back_ordered_by_federation_id() {
        // Documented on `discover_federations`, so pinned here: a browse screen that
        // reshuffles between refreshes is one nobody can point at. The second code is built
        // rather than written out, because a hand-written one fails the checksum.
        let second_code = fedimint_core::invite_code::InviteCode::new(
            "wss://foo.bar".parse().expect("a url"),
            fedimint_core::PeerId::from(0),
            OTHER_ID.parse().expect("a federation id"),
            None,
        )
        .to_string();

        // The larger id is written first, so document order and id order disagree.
        let found = entries(&format!(
            r#"{{"{OTHER_ID}": {{"invite_code": "{second_code}"}},
                "{ID}": {{"invite_code": "{CODE}"}}}}"#
        ));

        let ids: Vec<String> = found.iter().map(|found| found.id.to_string()).collect();
        assert_eq!(ids, vec![ID.to_owned(), OTHER_ID.to_owned()]);
    }

    #[test]
    fn a_publishers_own_end_of_life_key_is_carried_through_rather_than_acted_on() {
        // Fedi's key, long past. The SDK does not know what it means, so the entry stays on
        // offer and the key reaches the caller intact for an application that does know.
        let found = entries(&format!(
            r#"{{"{ID}": {{"invite_code": "{CODE}", "fedi:popup_end_timestamp": "1699999000"}}}}"#
        ));

        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0]
                .meta
                .get("fedi:popup_end_timestamp")
                .map(String::as_str),
            Some("1699999000")
        );
    }

    #[test]
    fn an_expiry_written_as_a_number_is_read_the_same_as_one_written_as_a_string() {
        let key = EXPIRY_KEY;
        let ended = entries(&format!(
            r#"{{"{ID}": {{"invite_code": "{CODE}", "{key}": 1699999000}}}}"#
        ));
        assert!(ended.is_empty());
    }

    #[test]
    fn an_expiry_the_sdk_cannot_read_is_ignored_rather_than_treated_as_ended() {
        let key = EXPIRY_KEY;
        for value in [r#""soon""#, r#""""#, r#""-1""#, "18446744073709551615"] {
            let found = entries(&format!(
                r#"{{"{ID}": {{"invite_code": "{CODE}", "{key}": {value}}}}}"#
            ));
            assert_eq!(
                found.len(),
                1,
                "{value} should not decide the federation has ended"
            );
        }
    }

    #[test]
    fn an_answer_that_is_not_a_discovery_document_is_an_error() {
        for body in [
            "",
            "not json at all",
            "[]",
            r#"{"id": "a string, not an entry"}"#,
        ] {
            let err = parse_document(body).expect_err(body);
            assert_eq!(err.code, ErrorCode::InvalidInput);
        }
    }

    #[tokio::test]
    async fn the_fetch_failure_is_what_the_caller_sees() {
        let failed = discover_with(
            |_| async {
                Err(Error::new(
                    ErrorCode::EndpointUnreachable,
                    "the endpoint answered with 404",
                ))
            },
            "https://example.invalid/list.json".to_owned(),
            now(),
        )
        .await
        .expect_err("a fetch that failed");

        assert_eq!(failed.code, ErrorCode::EndpointUnreachable);
    }

    #[tokio::test]
    async fn a_document_is_fetched_parsed_and_filtered_in_one_pass() {
        let document = format!(
            r#"{{"{ID}": {{"invite_code": "{CODE}", "federation_name": "Example"}},
                "{OTHER_ID}": {{"invite_code": "{CODE}"}}}}"#
        );

        let found = discover_with(
            |url| {
                assert_eq!(url, "https://example.test/federations.json");
                async move { Ok(document) }
            },
            "https://example.test/federations.json".to_owned(),
            now(),
        )
        .await
        .expect("a document this SDK can read");

        // The mismatched entry is gone; the good one survives with its metadata.
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].meta.get("federation_name").map(String::as_str),
            Some("Example")
        );
    }
}
