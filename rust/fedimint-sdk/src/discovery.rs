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
/// bounded continuation backfill (up to [`MAX_PAGES`] pages, or 300 raw events total per relay)
/// using `until` timestamp filters whenever initial pages yield fewer than the maximum
/// accepted events.
///
/// Multi-relay aggregation concurrently queries all specified relays, deduplicating
/// announcements by `(FederationId, InviteCode)` and retaining the newest announcement.
///
/// # Errors
///
/// When querying Nostr relays, partial success returns the discovered federations.
/// If all relays fail:
/// - [`InvalidInput`](ErrorCode::InvalidInput) if no relays were specified, or if any
///   relay URL was malformed.
/// - [`Timeout`](ErrorCode::Timeout) if all queries failed, none returned
///   [`InvalidInput`](ErrorCode::InvalidInput), and at least one timed out.
/// - [`FederationUnreachable`](ErrorCode::FederationUnreachable) if all relays failed to
///   connect or terminated without responding.
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
    let mut all_federations: HashMap<(FederationId, InviteCode), AggregatedEntry> = HashMap::new();
    let mut any_success = false;
    let mut errors = Vec::new();

    for res in results {
        match res {
            Ok(events) => {
                any_success = true;
                for event in events {
                    let created_at = event.created_at.unwrap_or(0);
                    if let Ok(fed) = DiscoveredFederation::try_from(event) {
                        let key = (fed.id.clone(), fed.invite.clone());
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
pub(crate) const MAX_MESSAGE_BYTES: usize = 65_536;

pub(crate) fn make_req_message() -> String {
    make_req_message_with_until(None)
}

pub(crate) fn make_req_message_with_until(until: Option<u64>) -> String {
    let mut filter = serde_json::json!({ "kinds": [38173], "limit": MAX_EVENTS });
    if let Some(until_ts) = until {
        filter["until"] = serde_json::json!(until_ts);
    }
    serde_json::json!(["REQ", SUB_ID, filter]).to_string()
}

pub(crate) fn make_close_message() -> String {
    serde_json::json!(["CLOSE", SUB_ID]).to_string()
}

#[derive(Debug, PartialEq)]
pub(crate) enum RelayAction {
    Event(RawNip87Event),
    Eose,
    Ignore,
}

pub(crate) fn parse_relay_message(msg_str: &str) -> RelayAction {
    if msg_str.len() > MAX_MESSAGE_BYTES {
        return RelayAction::Ignore;
    }
    let Ok(val) = serde_json::from_str::<serde_json::Value>(msg_str) else {
        return RelayAction::Ignore;
    };
    let Some(arr) = val.as_array() else {
        return RelayAction::Ignore;
    };

    let msg_type = arr.first().and_then(|v| v.as_str()).unwrap_or("");
    let sub_id = arr.get(1).and_then(|v| v.as_str());
    match msg_type {
        "EVENT" => {
            if sub_id != Some(SUB_ID) {
                return RelayAction::Ignore;
            }
            if let Some(event) = arr
                .get(2)
                .and_then(|v| serde_json::from_value::<RawNip87Event>(v.clone()).ok())
                .filter(|ev| {
                    ev.verify_signature() && DiscoveredFederation::try_from(ev.clone()).is_ok()
                })
            {
                return RelayAction::Event(event);
            }
            RelayAction::Ignore
        }
        "EOSE" => {
            if sub_id == Some(SUB_ID) {
                RelayAction::Eose
            } else {
                RelayAction::Ignore
            }
        }
        _ => RelayAction::Ignore,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FrameOutcome {
    Continue,
    NextPage { until: u64 },
    Finished,
}

fn inspect_raw_event_frame(text: &str) -> Option<Option<u64>> {
    if text.len() > MAX_MESSAGE_BYTES {
        return None;
    }
    let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(text) else {
        return None;
    };
    if arr.first().and_then(|v| v.as_str()) != Some("EVENT")
        || arr.get(1).and_then(|v| v.as_str()) != Some(SUB_ID)
    {
        return None;
    }
    let ts = arr
        .get(2)
        .and_then(|v| v.get("created_at"))
        .and_then(|v| v.as_u64());
    Some(ts)
}

pub(crate) fn process_relay_message_frame_with_pagination(
    text: &str,
    events: &mut Vec<RawNip87Event>,
    received_eose: &mut bool,
    pages_queried: &mut usize,
    raw_events_in_page: &mut usize,
    oldest_created_at_in_page: &mut Option<u64>,
) -> FrameOutcome {
    if text.len() > MAX_MESSAGE_BYTES {
        return FrameOutcome::Continue;
    }

    if let Some(ts_opt) = inspect_raw_event_frame(text) {
        *raw_events_in_page += 1;
        if let Some(ts) = ts_opt {
            *oldest_created_at_in_page =
                Some(oldest_created_at_in_page.map_or(ts, |min| min.min(ts)));
        }
    }

    match parse_relay_message(text) {
        RelayAction::Event(ev) => {
            if !events.iter().any(|existing| existing.id == ev.id) {
                events.push(ev);
                if events.len() >= MAX_EVENTS {
                    return FrameOutcome::Finished;
                }
            }
            FrameOutcome::Continue
        }
        RelayAction::Eose => {
            *received_eose = true;
            if *raw_events_in_page < MAX_EVENTS
                || events.len() >= MAX_EVENTS
                || *pages_queried + 1 >= MAX_PAGES
            {
                return FrameOutcome::Finished;
            }

            let Some(oldest_ts) = *oldest_created_at_in_page else {
                return FrameOutcome::Finished;
            };

            let until = oldest_ts;
            *pages_queried += 1;
            *raw_events_in_page = 0;
            *oldest_created_at_in_page = None;
            *received_eose = false;
            FrameOutcome::NextPage { until }
        }
        RelayAction::Ignore => FrameOutcome::Continue,
    }
}

/// Processes a raw relay message text frame, appending any valid event to `events`
/// and checking for EOSE or the `MAX_EVENTS` cutoff.
///
/// Returns `true` if the message loop should break (either due to EOSE or hitting `MAX_EVENTS`).
pub(crate) fn process_relay_message_frame(
    text: &str,
    events: &mut Vec<RawNip87Event>,
    received_eose: &mut bool,
) -> bool {
    let mut pages_queried = 0;
    let mut raw_events_in_page = 0;
    let mut oldest_created_at_in_page = None;
    process_relay_message_frame_with_pagination(
        text,
        events,
        received_eose,
        &mut pages_queried,
        &mut raw_events_in_page,
        &mut oldest_created_at_in_page,
    ) != FrameOutcome::Continue
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
pub(crate) fn ensure_rustls_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

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
    ensure_rustls_crypto_provider();

    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async_with_config;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

    tokio::time::timeout(timeout_duration, async {
        let ws_config = WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_MESSAGE_BYTES));
        let (mut ws_stream, _) =
            connect_async_with_config(&relay_url, Some(ws_config), false)
                .await
                .map_err(|e| {
                    Error::new(
                        ErrorCode::FederationUnreachable,
                        format!("Discovery error: failed to connect to relay {relay_url}: {e}"),
                    )
                })?;

        ws_stream
            .send(Message::text(make_req_message()))
            .await
            .map_err(|e| {
                Error::new(
                    ErrorCode::FederationUnreachable,
                    format!("Discovery error: failed to send REQ to {relay_url}: {e}"),
                )
            })?;

        let mut events = Vec::new();
        let mut received_eose = false;
        let mut pages_queried = 0;
        let mut raw_events_in_page = 0;
        let mut oldest_created_at_in_page = None;

        while let Some(msg_result) = ws_stream.next().await {
            match msg_result {
                Ok(Message::Text(text)) => {
                    match process_relay_message_frame_with_pagination(
                        &text,
                        &mut events,
                        &mut received_eose,
                        &mut pages_queried,
                        &mut raw_events_in_page,
                        &mut oldest_created_at_in_page,
                    ) {
                        FrameOutcome::Continue => {}
                        FrameOutcome::NextPage { until } => {
                            if ws_stream.send(Message::text(make_close_message())).await.is_err() {
                                break;
                            }
                            if ws_stream
                                .send(Message::text(make_req_message_with_until(Some(until))))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        FrameOutcome::Finished => break,
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    tracing::warn!(target: "fedimint_sdk", relay_url = %relay_url, error = %e, "Relay stream error");
                    break;
                }
                _ => {}
            }
        }

        let _ = ws_stream.send(Message::text(make_close_message())).await;
        finalize_relay_query(&relay_url, events, received_eose)
    })
    .await
    .map_err(|_| {
        Error::new(
            ErrorCode::Timeout,
            if timeout_duration == std::time::Duration::from_secs(5) {
                format!("Discovery error: relay {relay_url} timed out after 5s")
            } else {
                format!("Discovery error: relay {relay_url} timed out")
            },
        )
    })?
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

    let mut events = Vec::new();
    let mut received_eose = false;
    let mut timeout_expired = false;
    let mut pages_queried = 0;
    let mut raw_events_in_page = 0;
    let mut oldest_created_at_in_page = None;

    loop {
        tokio::select! {
            msg = ws.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match process_relay_message_frame_with_pagination(
                            &text,
                            &mut events,
                            &mut received_eose,
                            &mut pages_queried,
                            &mut raw_events_in_page,
                            &mut oldest_created_at_in_page,
                        ) {
                            FrameOutcome::Continue => {}
                            FrameOutcome::NextPage { until } => {
                                let _ = ws.send(Message::Text(make_close_message())).await;
                                if ws.send(Message::Text(make_req_message_with_until(Some(until)))).await.is_err() {
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
    let _ = ws.send(Message::Text(make_close_message())).await;
    if timeout_expired {
        return Err(Error::new(
            ErrorCode::Timeout,
            format!("Discovery error: relay {relay_url} timed out on WASM"),
        ));
    }
    finalize_relay_query(&relay_url, events, received_eose)
}
