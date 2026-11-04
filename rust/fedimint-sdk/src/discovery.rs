//! Federation discovery: discover joinable federations either from a local
//! dev fixture or by querying Nostr relays for NIP-87 (`kind:38173`) announcements.

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::types::discovery::{DiscoverySource, RawNip87Event};
use crate::{DiscoveredFederation, Error, ErrorCode, FederationId, Result};

/// Discovers federations from the requested source (Dev fixture or Nostr relays).
///
/// # Nostr Discovery Policy & Continuation
///
/// When using [`DiscoverySource::Nostr`], discovery queries Nostr relays for NIP-87
/// kind-38173 announcements with up to [`MAX_EVENTS`] (100) raw events per page. To defend
/// against raw-window crowdout by spam or non-announcement events, the query executes a
/// bounded continuation backfill (up to [`MAX_PAGES`] (3) pages per relay) using `until`
/// timestamp filters whenever a page fills with raw events but yields fewer than the
/// maximum accepted announcements.
///
/// If a full page of raw events shares a single timestamp (a "full timestamp bucket"),
/// re-requesting the same window would return identical results. The client detects this
/// on the same page (without a wasted round trip) and expands the `limit` on the next
/// request to reach past the bucket. The per-request limit is capped at
/// [`MAX_RAW_EVENTS_BUDGET`] (300); buckets deeper than this are unrecoverable, which
/// is a reasonable bounded policy.
///
/// Relay messages larger than [`MAX_MESSAGE_BYTES`] (64 KiB) are skipped for payload
/// parsing but still contribute to raw event counts and pagination timestamps (scanned
/// without full JSON deserialization), so that oversized spam does not prevent continuation.
/// The native transport accepts messages up to 1 MiB so that one oversized event does not
/// kill the connection.
///
/// If a relay rejects the subscription with a NIP-01 `CLOSED` message (e.g.
/// `auth-required` or `rate-limited`), the query terminates immediately and the
/// relay's reason is logged.
///
/// Backfill is best-effort: once any page completes with EOSE, a timeout or
/// disconnect on a subsequent page returns the events already collected rather
/// than discarding them. Both native and WASM enforce an overall query deadline
/// while preserving completed pages via sticky EOSE.
///
/// Multi-relay aggregation concurrently queries all specified relays, deduplicating
/// announcements by `(FederationId, InviteCode, publisher_pubkey)` and retaining the newest
/// announcement per publisher.
///
/// # Errors
///
/// - [`InvalidInput`](ErrorCode::InvalidInput) if no relays were specified, or if any
///   relay URL was malformed. This is checked before any connections are opened.
/// - [`Timeout`](ErrorCode::Timeout) if all queries failed, none returned
///   [`InvalidInput`](ErrorCode::InvalidInput), and at least one timed out.
/// - [`FederationUnreachable`](ErrorCode::FederationUnreachable) if all relays failed to
///   connect, rejected the subscription, or terminated without responding. Note: relay
///   failures reuse this code; it does not imply that a federation's guardians are down.
pub async fn discover_federations(source: DiscoverySource) -> Result<Vec<DiscoveredFederation>> {
    match source {
        DiscoverySource::Dev => Ok(vec![DiscoveredFederation::dev()]),
        DiscoverySource::Nostr { relays } => fetch_from_nostr(relays).await,
    }
}

async fn fetch_from_nostr(relays: Vec<String>) -> Result<Vec<DiscoveredFederation>> {
    if relays.is_empty() {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "Discovery error: at least one Nostr relay must be specified",
        ));
    }

    // Validate all relay URLs upfront so a malformed URL is reported immediately
    // rather than being silently swallowed when another relay succeeds.
    for relay in &relays {
        validate_relay_url(relay)?;
    }

    let mut query_tasks = Vec::new();
    for relay in relays {
        query_tasks.push(query_single_relay(relay));
    }

    let results = futures::future::join_all(query_tasks).await;
    aggregate_discovered_events(results)
}

#[derive(Debug)]
struct AggregatedEntry {
    created_at: u64,
    federation: DiscoveredFederation,
}

fn aggregate_discovered_events(
    results: Vec<Result<Vec<RawNip87Event>>>,
) -> Result<Vec<DiscoveredFederation>> {
    use crate::InviteCode;
    let mut all_federations: HashMap<(FederationId, InviteCode, Option<String>), AggregatedEntry> =
        HashMap::new();
    let mut any_success = false;
    let mut errors = Vec::new();

    for res in results {
        match res {
            Ok(events) => {
                any_success = true;
                for event in events {
                    let created_at = event.created_at.unwrap_or(0);
                    if let Ok(fed) = DiscoveredFederation::try_from(event) {
                        let publisher_pubkey = fed.metadata.get("publisher_pubkey").cloned();
                        let key = (fed.id.clone(), fed.invite.clone(), publisher_pubkey);
                        match all_federations.entry(key) {
                            std::collections::hash_map::Entry::Vacant(vacant) => {
                                vacant.insert(AggregatedEntry {
                                    created_at,
                                    federation: fed,
                                });
                            }
                            std::collections::hash_map::Entry::Occupied(mut occupied) => {
                                if created_at > occupied.get().created_at {
                                    occupied.insert(AggregatedEntry {
                                        created_at,
                                        federation: fed,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(target: "fedimint_sdk", error = %e, "Failed to query Nostr relay");
                errors.push(e);
            }
        }
    }

    if !any_success {
        return Err(select_aggregate_error(errors));
    }

    let mut federations: Vec<DiscoveredFederation> = all_federations
        .into_values()
        .map(|entry| entry.federation)
        .collect();
    federations.sort_by(|a, b| {
        a.id.to_string()
            .cmp(&b.id.to_string())
            .then_with(|| a.invite.to_string().cmp(&b.invite.to_string()))
            .then_with(|| {
                a.metadata
                    .get("publisher_pubkey")
                    .cmp(&b.metadata.get("publisher_pubkey"))
            })
    });
    Ok(federations)
}

fn select_aggregate_error(errors: Vec<Error>) -> Error {
    if errors.is_empty() {
        return Error::new(
            ErrorCode::FederationUnreachable,
            "Discovery error: all Nostr relays failed to respond",
        );
    }
    if errors.len() == 1 {
        return errors.into_iter().next().unwrap();
    }
    // Deterministic precedence for mixed failures:
    // 1. InvalidInput (caller configuration problem takes top precedence)
    // 2. Timeout (all queries failed, none returned InvalidInput, and at least one timed out)
    // 3. FederationUnreachable (network connection / communication failure)
    // 4. Any other error
    if let Some(err) = errors.iter().find(|e| e.code == ErrorCode::InvalidInput) {
        return err.clone();
    }
    if let Some(err) = errors.iter().find(|e| e.code == ErrorCode::Timeout) {
        return err.clone();
    }
    if let Some(err) = errors
        .iter()
        .find(|e| e.code == ErrorCode::FederationUnreachable)
    {
        return err.clone();
    }
    errors.into_iter().next().unwrap()
}

/// The Nostr subscription identifier sent to relays.
const SUB_ID: &str = "fedimint_discovery";

/// The maximum number of events to request from a relay or accept in one query.
pub(crate) const MAX_EVENTS: usize = 100;

/// The maximum number of pagination continuation pages to query per relay.
pub(crate) const MAX_PAGES: usize = 3;

/// Maximum allowed size for a single incoming relay message frame in bytes (64 KiB).
/// Messages larger than this are skipped at the application level (treated as ignored),
/// but the transport layer accepts up to [`MAX_TRANSPORT_BYTES`] so that one oversized
/// event does not kill the entire relay connection.
pub(crate) const MAX_MESSAGE_BYTES: usize = 65_536;

/// Largest message the transport will assemble; bounds memory per relay connection (1 MiB).
/// Nostr relays routinely forward events between 128 and 512 KiB, so this must be above
/// what a relay will send. Messages between `MAX_MESSAGE_BYTES` and this limit are received
/// by the transport but skipped by the application-level size check, matching WASM behavior.
#[cfg(not(target_family = "wasm"))]
pub(crate) const MAX_TRANSPORT_BYTES: usize = 1024 * 1024;

/// The maximum `limit` parameter requested in any single page subscription filter.
/// During pagination expansion (for full timestamp buckets), the per-request limit
/// increases up to this ceiling, bounding the size of any individual page request.
pub(crate) const MAX_RAW_EVENTS_BUDGET: usize = MAX_EVENTS * MAX_PAGES;

pub(crate) fn make_req_message() -> String {
    make_req_message_with_until(None)
}

pub(crate) fn make_req_message_with_until(until: Option<u64>) -> String {
    make_req_message_with_until_and_limit(until, MAX_EVENTS)
}

pub(crate) fn make_req_message_with_until_and_limit(until: Option<u64>, limit: usize) -> String {
    let mut filter = serde_json::json!({ "kinds": [38173], "limit": limit });
    if let Some(until_ts) = until {
        filter["until"] = serde_json::json!(until_ts);
    }
    serde_json::json!(["REQ", SUB_ID, filter]).to_string()
}

pub(crate) fn make_close_message() -> String {
    serde_json::json!(["CLOSE", SUB_ID]).to_string()
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FrameOutcome {
    Continue,
    NextPage { until: u64, limit: usize },
    Finished,
}

#[derive(Debug, PartialEq)]
pub(crate) struct PageState {
    pub events: Vec<RawNip87Event>,
    pub received_eose: bool,
    pub pages_queried: usize,
    pub raw_events_in_page: usize,
    pub oldest_created_at_in_page: Option<u64>,
    pub newest_created_at_in_page: Option<u64>,
    pub previous_until: Option<u64>,
    pub current_limit: usize,
}

impl Default for PageState {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            received_eose: false,
            pages_queried: 0,
            raw_events_in_page: 0,
            oldest_created_at_in_page: None,
            newest_created_at_in_page: None,
            previous_until: None,
            current_limit: MAX_EVENTS,
        }
    }
}

pub(crate) fn is_our_event_frame(text: &str) -> bool {
    let rest = text.trim_start();
    let Some(rest) = rest.strip_prefix('[') else {
        return false;
    };
    let rest = rest.trim_start();
    let Some(rest) = rest.strip_prefix("\"EVENT\"") else {
        return false;
    };
    let rest = rest.trim_start();
    let Some(rest) = rest.strip_prefix(',') else {
        return false;
    };
    let rest = rest.trim_start();
    let Some(rest) = rest.strip_prefix('"') else {
        return false;
    };
    let Some(rest) = rest.strip_prefix(SUB_ID) else {
        return false;
    };
    rest.starts_with('"')
}

pub(crate) fn scan_created_at(text: &str) -> Option<u64> {
    const KEY: &str = "\"created_at\"";
    let bytes = text.as_bytes();
    let mut search_start = 0;

    while let Some(rel_idx) = text[search_start..].find(KEY) {
        let key_idx = search_start + rel_idx;
        search_start = key_idx + KEY.len();

        // 1. Verify the opening quote is not escaped by an odd number of backslashes.
        let mut backslash_count = 0;
        let mut i = key_idx;
        while i > 0 && bytes[i - 1] == b'\\' {
            backslash_count += 1;
            i -= 1;
        }
        if backslash_count % 2 != 0 {
            continue;
        }

        // 2. Expect a colon with optional whitespace after the key.
        let after_key = text[search_start..].trim_start();
        let Some(after_colon) = after_key.strip_prefix(':') else {
            continue;
        };

        // 3. Expect integer digits with optional leading whitespace.
        let digits = after_colon.trim_start();
        let digit_end = digits
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(digits.len());

        if digit_end == 0 {
            continue;
        }

        if let Ok(ts) = digits[..digit_end].parse::<u64>() {
            return Some(ts);
        }
    }

    None
}

impl PageState {
    pub(crate) fn record_event_timestamp(&mut self, ts: u64) {
        self.oldest_created_at_in_page =
            Some(self.oldest_created_at_in_page.map_or(ts, |min| min.min(ts)));
        self.newest_created_at_in_page =
            Some(self.newest_created_at_in_page.map_or(ts, |max| max.max(ts)));
    }

    pub(crate) fn on_frame(&mut self, text: &str) -> FrameOutcome {
        // Count oversized EVENT frames and record their timestamps for pagination
        // so that oversized spam does not prevent pagination continuation.
        if text.len() > MAX_MESSAGE_BYTES {
            if is_our_event_frame(text) {
                self.raw_events_in_page += 1;
                if let Some(ts) = scan_created_at(text) {
                    self.record_event_timestamp(ts);
                }
            }
            return FrameOutcome::Continue;
        }

        // Parse JSON once for both pagination tracking and message processing,
        // avoiding the double-parse overhead of separate inspect + parse calls.
        let Ok(val) = serde_json::from_str::<serde_json::Value>(text) else {
            return FrameOutcome::Continue;
        };
        let Some(arr) = val.as_array() else {
            return FrameOutcome::Continue;
        };

        let msg_type = arr.first().and_then(|v| v.as_str()).unwrap_or("");
        let sub_id = arr.get(1).and_then(|v| v.as_str());

        match msg_type {
            "EVENT" => {
                if sub_id != Some(SUB_ID) {
                    return FrameOutcome::Continue;
                }

                // Track raw event count and timestamps for pagination.
                self.raw_events_in_page += 1;
                if let Some(ts) = arr
                    .get(2)
                    .and_then(|v| v.get("created_at"))
                    .and_then(|v| v.as_u64())
                {
                    self.record_event_timestamp(ts);
                }

                // Cheap structural checks (kind, d-tag, invite) before expensive
                // Schnorr signature verification so spam is rejected early.
                if let Some(ev) = arr
                    .get(2)
                    .and_then(|v| serde_json::from_value::<RawNip87Event>(v.clone()).ok())
                    .filter(|ev| {
                        DiscoveredFederation::try_from(ev.clone()).is_ok() && ev.verify_signature()
                    })
                    .filter(|ev| !self.events.iter().any(|existing| existing.id == ev.id))
                {
                    self.events.push(ev);
                    if self.events.len() >= MAX_EVENTS {
                        return FrameOutcome::Finished;
                    }
                }
                FrameOutcome::Continue
            }
            "EOSE" => {
                if sub_id != Some(SUB_ID) {
                    return FrameOutcome::Continue;
                }
                self.received_eose = true;
                if self.raw_events_in_page < self.current_limit
                    || self.events.len() >= MAX_EVENTS
                    || self.pages_queried + 1 >= MAX_PAGES
                {
                    return FrameOutcome::Finished;
                }

                let Some(oldest_ts) = self.oldest_created_at_in_page else {
                    return FrameOutcome::Finished;
                };

                let until = oldest_ts;

                // Detect full-timestamp-bucket: expand the limit when either
                // (a) every event in this page shares one timestamp (newest == oldest), or
                // (b) no temporal progress from the previous request (previous_until == oldest_ts).
                // Case (a) catches the bucket on the first occurrence without a wasted round trip.
                let bucket_detected = self.newest_created_at_in_page == Some(oldest_ts)
                    || self.previous_until == Some(oldest_ts);

                let next_limit = if bucket_detected {
                    (self.raw_events_in_page + MAX_EVENTS).min(MAX_RAW_EVENTS_BUDGET)
                } else {
                    MAX_EVENTS
                };

                self.pages_queried += 1;
                self.raw_events_in_page = 0;
                self.oldest_created_at_in_page = None;
                self.newest_created_at_in_page = None;
                self.previous_until = Some(until);
                self.current_limit = next_limit;

                // Keep received_eose sticky: once a page completes with EOSE, a
                // later-page failure (disconnect/timeout) should not discard events
                // already collected. finalize_relay_query checks this flag.
                FrameOutcome::NextPage {
                    until,
                    limit: next_limit,
                }
            }
            "CLOSED" => {
                if sub_id != Some(SUB_ID) {
                    return FrameOutcome::Continue;
                }
                let reason = arr.get(2).and_then(|v| v.as_str()).unwrap_or("unknown");
                tracing::warn!(
                    target: "fedimint_sdk",
                    reason = %reason,
                    "Relay closed subscription"
                );
                // Terminal: the relay rejected the subscription (auth-required,
                // rate-limited, etc). Don't set received_eose so finalize_relay_query
                // can distinguish a clean close from a rejection.
                FrameOutcome::Finished
            }
            _ => FrameOutcome::Continue,
        }
    }
}

/// Finalizes a relay query by enforcing EOSE completion semantics.
///
/// Under NIP-87 discovery:
/// - A relay query succeeds if it receives an EOSE frame (even with 0 events).
/// - A relay query succeeds if it reaches `MAX_EVENTS` (even if truncated before EOSE).
/// - A relay query terminates with `FederationUnreachable` if the connection closes
///   or stream ends before EOSE and fewer than `MAX_EVENTS` were received.
pub(crate) fn finalize_relay_query(
    relay_url: &str,
    events: Vec<RawNip87Event>,
    received_eose: bool,
) -> Result<Vec<RawNip87Event>> {
    if !received_eose && events.len() < MAX_EVENTS {
        return Err(Error::new(
            ErrorCode::FederationUnreachable,
            format!("Discovery error: relay {relay_url} terminated connection without EOSE"),
        ));
    }
    Ok(events)
}

// ---------------------------------------------------------------------------
// Platform-specific relay query implementations
// ---------------------------------------------------------------------------

fn validate_relay_url(relay_url: &str) -> Result<()> {
    let parsed = fedimint_core::util::SafeUrl::parse(relay_url).map_err(|e| {
        Error::new(
            ErrorCode::InvalidInput,
            format!("Discovery error: malformed relay URL {relay_url}: {e}"),
        )
    })?;
    let scheme = parsed.scheme();
    if scheme != "ws" && scheme != "wss" {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            format!("Discovery error: relay URL {relay_url} scheme must be ws:// or wss://"),
        ));
    }
    Ok(())
}

#[cfg(not(target_family = "wasm"))]
static TLS_CONFIG: std::sync::LazyLock<std::sync::Arc<rustls::ClientConfig>> =
    std::sync::LazyLock::new(|| {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("ring supports default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
        std::sync::Arc::new(tls)
    });

#[cfg(not(target_family = "wasm"))]
pub(crate) async fn query_single_relay(relay_url: String) -> Result<Vec<RawNip87Event>> {
    query_single_relay_with_timeout(relay_url, std::time::Duration::from_secs(5)).await
}

#[cfg(not(target_family = "wasm"))]
pub(crate) async fn query_single_relay_with_timeout(
    relay_url: String,
    timeout_duration: std::time::Duration,
) -> Result<Vec<RawNip87Event>> {
    validate_relay_url(&relay_url)?;

    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
    use tokio_tungstenite::{Connector, connect_async_tls_with_config};

    let deadline = tokio::time::Instant::now() + timeout_duration;

    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(MAX_TRANSPORT_BYTES))
        .max_frame_size(Some(MAX_TRANSPORT_BYTES));
    let is_wss = relay_url
        .split_once(':')
        .is_some_and(|(s, _)| s.eq_ignore_ascii_case("wss"));
    let connector = if is_wss {
        Some(Connector::Rustls(std::sync::Arc::clone(&TLS_CONFIG)))
    } else {
        None
    };
    let (mut ws_stream, _) = tokio::time::timeout_at(
        deadline,
        connect_async_tls_with_config(&relay_url, Some(ws_config), false, connector),
    )
    .await
    .map_err(|_| {
        Error::new(
            ErrorCode::Timeout,
            format!("Discovery error: relay {relay_url} timed out after {timeout_duration:.0?}"),
        )
    })?
    .map_err(|e| {
        Error::new(
            ErrorCode::FederationUnreachable,
            format!("Discovery error: failed to connect to relay {relay_url}: {e}"),
        )
    })?;

    tokio::time::timeout_at(deadline, ws_stream.send(Message::text(make_req_message())))
        .await
        .map_err(|_| {
            Error::new(
                ErrorCode::Timeout,
                format!(
                    "Discovery error: relay {relay_url} timed out after {timeout_duration:.0?}"
                ),
            )
        })?
        .map_err(|e| {
            Error::new(
                ErrorCode::FederationUnreachable,
                format!("Discovery error: failed to send REQ to {relay_url}: {e}"),
            )
        })?;

    let mut state = PageState::default();
    let mut timed_out = false;

    loop {
        let msg = match tokio::time::timeout_at(deadline, ws_stream.next()).await {
            Ok(Some(msg)) => msg,
            Ok(None) => break,
            Err(_) => {
                timed_out = true;
                break;
            }
        };
        match msg {
            Ok(Message::Text(text)) => match state.on_frame(&text) {
                FrameOutcome::Continue => {}
                FrameOutcome::NextPage { until, limit } => {
                    let close_res = tokio::time::timeout_at(
                        deadline,
                        ws_stream.send(Message::text(make_close_message())),
                    )
                    .await;
                    match close_res {
                        Ok(Ok(())) => {}
                        Ok(Err(_)) => break,
                        Err(_) => {
                            timed_out = true;
                            break;
                        }
                    }

                    let req_res = tokio::time::timeout_at(
                        deadline,
                        ws_stream.send(Message::text(make_req_message_with_until_and_limit(
                            Some(until),
                            limit,
                        ))),
                    )
                    .await;
                    match req_res {
                        Ok(Ok(())) => {}
                        Ok(Err(_)) => break,
                        Err(_) => {
                            timed_out = true;
                            break;
                        }
                    }
                }
                FrameOutcome::Finished => break,
            },
            Ok(Message::Close(_)) => break,
            Err(e) => {
                tracing::warn!(target: "fedimint_sdk", relay_url = %relay_url, error = %e, "Relay stream error");
                break;
            }
            _ => {}
        }
    }

    if !timed_out && tokio::time::Instant::now() < deadline {
        let _ = tokio::time::timeout_at(
            deadline,
            ws_stream.send(Message::text(make_close_message())),
        )
        .await;
    }

    // A timeout on a later page should not discard events from earlier
    // completed pages. Only report a timeout if no page ever finished.
    if timed_out && !state.received_eose {
        return Err(Error::new(
            ErrorCode::Timeout,
            format!("Discovery error: relay {relay_url} timed out after {timeout_duration:.0?}"),
        ));
    }
    finalize_relay_query(&relay_url, state.events, state.received_eose)
}

#[cfg(target_family = "wasm")]
async fn query_single_relay(relay_url: String) -> Result<Vec<RawNip87Event>> {
    validate_relay_url(&relay_url)?;

    use futures::{SinkExt, StreamExt};
    use gloo_net::websocket::Message;
    use gloo_net::websocket::futures::WebSocket;

    let mut ws = WebSocket::open(&relay_url).map_err(|e| {
        Error::new(
            ErrorCode::FederationUnreachable,
            format!("Discovery error: failed to open websocket {relay_url}: {e}"),
        )
    })?;

    let (tx, mut rx) = futures::channel::oneshot::channel();
    let timeout = gloo_timers::callback::Timeout::new(5000, move || {
        let _ = tx.send(());
    });

    tokio::select! {
        res = ws.send(Message::Text(make_req_message())) => {
            if let Err(e) = res {
                timeout.cancel();
                return Err(Error::new(
                    ErrorCode::FederationUnreachable,
                    format!("Discovery error: failed to send REQ to {relay_url}: {e}"),
                ));
            }
        }
        _ = &mut rx => {
            timeout.cancel();
            return Err(Error::new(
                ErrorCode::Timeout,
                format!("Discovery error: relay {relay_url} timed out on WASM"),
            ));
        }
    }

    let mut state = PageState::default();
    let mut timeout_expired = false;

    loop {
        tokio::select! {
            msg = ws.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match state.on_frame(&text) {
                            FrameOutcome::Continue => {}
                            FrameOutcome::NextPage { until, limit } => {
                                let close_send = ws.send(Message::Text(make_close_message()));
                                tokio::select! {
                                    res = close_send => {
                                        if res.is_err() {
                                            break;
                                        }
                                    }
                                    _ = &mut rx => {
                                        timeout_expired = true;
                                        break;
                                    }
                                }
                                if timeout_expired {
                                    break;
                                }

                                let req_send = ws.send(Message::Text(
                                    make_req_message_with_until_and_limit(Some(until), limit),
                                ));
                                tokio::select! {
                                    res = req_send => {
                                        if res.is_err() {
                                            break;
                                        }
                                    }
                                    _ = &mut rx => {
                                        timeout_expired = true;
                                        break;
                                    }
                                }
                                if timeout_expired {
                                    break;
                                }
                            }
                            FrameOutcome::Finished => break,
                        }
                    }
                    Some(Ok(Message::Bytes(_))) => {}
                    _ => break,
                }
            }
            _ = &mut rx => {
                tracing::warn!(target: "fedimint_sdk", relay_url = %relay_url, "Relay timed out on WASM");
                timeout_expired = true;
                break;
            }
        }
    }

    timeout.cancel();
    if !timeout_expired {
        let _ = ws.send(Message::Text(make_close_message())).await;
    }
    // A timeout on a later page should not discard events from earlier
    // completed pages. Only report a timeout if no page ever finished.
    if timeout_expired && !state.received_eose {
        return Err(Error::new(
            ErrorCode::Timeout,
            format!("Discovery error: relay {relay_url} timed out on WASM"),
        ));
    }
    finalize_relay_query(&relay_url, state.events, state.received_eose)
}
