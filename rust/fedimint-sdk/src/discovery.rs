//! Federation discovery: discover joinable federations either from a local
//! dev fixture or by querying Nostr relays for NIP-87 (`kind:38173`) announcements.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::types::discovery::{DiscoverySource, RawNip87Event};
use crate::{DiscoveredFederation, Error, ErrorCode, FederationId, Result};

/// Discovers federations from the requested source (Dev fixture or Nostr relays).
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

    let mut all_federations: BTreeMap<FederationId, DiscoveredFederation> = BTreeMap::new();
    let mut any_success = false;

    for res in results {
        match res {
            Ok(events) => {
                any_success = true;
                for event in events {
                    if let Ok(fed) = DiscoveredFederation::try_from(event) {
                        all_federations.entry(fed.id).or_insert(fed);
                    }
                }
            }
            Err(e) => {
                tracing::warn!(target: "fedimint_sdk", error = %e, "Failed to query Nostr relay");
            }
        }
    }

    if !any_success {
        return Err(Error::new(
            ErrorCode::FederationUnreachable,
            "Discovery error: all Nostr relays failed to respond",
        ));
    }

    Ok(all_federations.into_values().collect())
}

/// The Nostr subscription identifier sent to relays.
const SUB_ID: &str = "fedimint_discovery";

/// The maximum number of events to request from a relay or accept in one query.
const MAX_EVENTS: usize = 100;

fn make_req_message() -> String {
    serde_json::json!(["REQ", SUB_ID, { "kinds": [38173], "limit": MAX_EVENTS }]).to_string()
}

fn make_close_message() -> String {
    serde_json::json!(["CLOSE", SUB_ID]).to_string()
}

#[derive(Debug, PartialEq)]
enum RelayAction {
    Event(RawNip87Event),
    Eose,
    Ignore,
}

fn parse_relay_message(msg_str: &str) -> RelayAction {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(msg_str) else {
        return RelayAction::Ignore;
    };
    let Some(arr) = val.as_array() else {
        return RelayAction::Ignore;
    };

    let msg_type = arr.first().and_then(|v| v.as_str()).unwrap_or("");
    match msg_type {
        "EVENT" => {
            if let Some(event_val) = arr.get(2) {
                if let Ok(event) = serde_json::from_value::<RawNip87Event>(event_val.clone()) {
                    return RelayAction::Event(event);
                }
            }
            RelayAction::Ignore
        }
        "EOSE" => RelayAction::Eose,
        _ => RelayAction::Ignore,
    }
}

// ---------------------------------------------------------------------------
// Platform-specific relay query implementations
// ---------------------------------------------------------------------------

#[cfg(not(target_family = "wasm"))]
async fn query_single_relay(relay_url: String) -> Result<Vec<RawNip87Event>> {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    let timeout_duration = Duration::from_secs(5);
    tokio::time::timeout(timeout_duration, async {
        let (mut ws_stream, _) = connect_async(&relay_url).await.map_err(|e| {
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
                    ErrorCode::Internal,
                    format!("Discovery error: failed to send REQ to {relay_url}: {e}"),
                )
            })?;

        let mut events = Vec::new();
        let mut received_eose = false;
        while let Some(msg_result) = ws_stream.next().await {
            match msg_result {
                Ok(Message::Text(text)) => match parse_relay_message(&text) {
                    RelayAction::Event(ev) => {
                        events.push(ev);
                        if events.len() >= MAX_EVENTS {
                            break;
                        }
                    }
                    RelayAction::Eose => {
                        received_eose = true;
                        break;
                    }
                    RelayAction::Ignore => {}
                },
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    tracing::warn!(target: "fedimint_sdk", relay_url = %relay_url, error = %e, "Relay stream error");
                    break;
                }
                _ => {}
            }
        }

        let _ = ws_stream.send(Message::text(make_close_message())).await;

        if !received_eose && events.len() < MAX_EVENTS {
            return Err(Error::new(
                ErrorCode::FederationUnreachable,
                format!("Discovery error: relay {relay_url} terminated connection without EOSE"),
            ));
        }

        Ok(events)
    })
    .await
    .map_err(|_| {
        Error::new(
            ErrorCode::FederationUnreachable,
            format!("Discovery error: relay {relay_url} timed out after 5s"),
        )
    })?
}

#[cfg(target_family = "wasm")]
async fn query_single_relay(relay_url: String) -> Result<Vec<RawNip87Event>> {
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
                    ErrorCode::Internal,
                    format!("Discovery error: failed to send REQ to {relay_url}: {e}"),
                ));
            }
        }
        _ = &mut rx => {
            timeout.cancel();
            return Err(Error::new(
                ErrorCode::FederationUnreachable,
                format!("Discovery error: relay {relay_url} timed out on WASM"),
            ));
        }
    }

    let mut events = Vec::new();
    let mut received_eose = false;
    let mut timeout_expired = false;

    loop {
        tokio::select! {
            msg = ws.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => match parse_relay_message(&text) {
                        RelayAction::Event(ev) => {
                            events.push(ev);
                            if events.len() >= MAX_EVENTS {
                                break;
                            }
                        }
                        RelayAction::Eose => {
                            received_eose = true;
                            break;
                        }
                        RelayAction::Ignore => {}
                    },
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
            ErrorCode::FederationUnreachable,
            format!("Discovery error: relay {relay_url} timed out on WASM"),
        ));
    }
    if !received_eose && events.len() < MAX_EVENTS {
        return Err(Error::new(
            ErrorCode::FederationUnreachable,
            format!("Discovery error: relay {relay_url} terminated connection without EOSE"),
        ));
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "fed11qgqpqrnhwden5te0vehk7tnzv9ez7qqpyq4z52329g4z52329g4z52329g4z52329g4z52329g4z52329g4z5wa8phk";
    const CODE_FEDERATION_ID: &str =
        "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a";

    #[tokio::test]
    async fn test_discover_federations_dev() {
        let feds = discover_federations(DiscoverySource::Dev)
            .await
            .expect("dev mode should succeed");
        assert_eq!(feds.len(), 1);
        assert_eq!(
            feds[0].id,
            CODE_FEDERATION_ID.parse::<FederationId>().unwrap()
        );
        assert_eq!(
            feds[0].metadata.get("federation_name").unwrap(),
            "Dev Test Federation"
        );
    }

    #[tokio::test]
    async fn test_discover_federations_empty_relays_fails() {
        let res = discover_federations(DiscoverySource::Nostr { relays: vec![] }).await;
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, ErrorCode::InvalidInput);
    }

    #[test]
    fn test_parse_relay_event_message() {
        let raw_msg = format!(
            r#"["EVENT", "{SUB_ID}", {{
                "kind": 38173,
                "pubkey": "pubkey123",
                "content": "{{\"name\": \"Community Fed\"}}",
                "tags": [
                    ["d", "{CODE_FEDERATION_ID}"],
                    ["u", "{CODE}"]
                ]
            }}]"#
        );
        match parse_relay_message(&raw_msg) {
            RelayAction::Event(ev) => {
                assert_eq!(ev.kind, Some(38173));
                assert_eq!(ev.pubkey.as_deref(), Some("pubkey123"));
            }
            other => panic!("expected RelayAction::Event, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_relay_eose_message() {
        let raw_msg = format!(r#"["EOSE", "{SUB_ID}"]"#);
        assert_eq!(parse_relay_message(&raw_msg), RelayAction::Eose);
    }

    #[test]
    fn test_parse_relay_ignore_message() {
        let notice_msg = r#"["NOTICE", "rate-limited"]"#;
        assert_eq!(parse_relay_message(notice_msg), RelayAction::Ignore);

        let garbage_msg = r#"not json"#;
        assert_eq!(parse_relay_message(garbage_msg), RelayAction::Ignore);
    }

    #[test]
    fn test_make_req_message_includes_limit() {
        let req_str = make_req_message();
        let val: serde_json::Value = serde_json::from_str(&req_str).unwrap();
        let arr = val.as_array().unwrap();
        assert_eq!(arr[0], "REQ");
        assert_eq!(arr[1], SUB_ID);
        assert_eq!(arr[2]["kinds"].as_array().unwrap()[0], 38173);
        assert_eq!(arr[2]["limit"], MAX_EVENTS);
    }
}
