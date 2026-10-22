use super::*;

const CODE: &str = "fed11qgqpqrnhwden5te0vehk7tnzv9ez7qqpyq4z52329g4z52329g4z52329g4z52329g4z52329g4z52329g4z5wa8phk";
const CODE_FEDERATION_ID: &str = "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a";

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

    let wrong_sub_eose = r#"["EOSE", "wrong_sub_id"]"#;
    assert_eq!(parse_relay_message(wrong_sub_eose), RelayAction::Ignore);

    let bare_eose = r#"["EOSE"]"#;
    assert_eq!(parse_relay_message(bare_eose), RelayAction::Ignore);

    let wrong_sub_event = format!(
        r#"["EVENT", "wrong_sub_id", {{
            "kind": 38173,
            "pubkey": "pubkey123",
            "content": "{{\"name\": \"Community Fed\"}}",
            "tags": [
                ["d", "{CODE_FEDERATION_ID}"],
                ["u", "{CODE}"]
            ]
        }}]"#
    );
    assert_eq!(parse_relay_message(&wrong_sub_event), RelayAction::Ignore);

    let oversized_msg = format!(
        r#"["EVENT", "{SUB_ID}", {{ "content": "{}" }}]"#,
        "x".repeat(MAX_MESSAGE_BYTES + 1)
    );
    assert_eq!(parse_relay_message(&oversized_msg), RelayAction::Ignore);
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

fn make_test_raw_event(fed_id: &str, invite: &str) -> RawNip87Event {
    RawNip87Event {
        id: Some("ev1".to_string()),
        pubkey: Some("pub1".to_string()),
        created_at: Some(12345),
        kind: Some(38173),
        tags: vec![
            vec!["d".to_string(), fed_id.to_string()],
            vec!["u".to_string(), invite.to_string()],
        ],
        content: r#"{"name": "Test Fed"}"#.to_string(),
    }
}

#[test]
fn test_aggregate_discovered_events_deduplication() {
    let ev1 = make_test_raw_event(CODE_FEDERATION_ID, CODE);
    let ev2 = make_test_raw_event(CODE_FEDERATION_ID, CODE);

    let results = vec![Ok(vec![ev1]), Ok(vec![ev2])];
    let feds = aggregate_discovered_events(results).expect("aggregation should succeed");
    assert_eq!(
        feds.len(),
        1,
        "identical federations should be deduplicated"
    );
    assert_eq!(
        feds[0].id,
        CODE_FEDERATION_ID.parse::<FederationId>().unwrap()
    );
}

#[test]
fn test_aggregate_discovered_events_mixed_success_failure() {
    let ev = make_test_raw_event(CODE_FEDERATION_ID, CODE);
    let results = vec![
        Err(Error::new(
            ErrorCode::FederationUnreachable,
            "relay 1 unreachable",
        )),
        Ok(vec![ev]),
    ];
    let feds = aggregate_discovered_events(results)
        .expect("should succeed if at least one relay responds");
    assert_eq!(feds.len(), 1);
}

#[test]
fn test_aggregate_discovered_events_all_failures() {
    let results: Vec<Result<Vec<RawNip87Event>>> = vec![
        Err(Error::new(
            ErrorCode::FederationUnreachable,
            "relay 1 failed",
        )),
        Err(Error::new(
            ErrorCode::FederationUnreachable,
            "relay 2 failed",
        )),
    ];
    let err = aggregate_discovered_events(results).unwrap_err();
    assert_eq!(err.code, ErrorCode::FederationUnreachable);
}

#[test]
fn test_aggregate_discovered_events_empty_results() {
    let results: Vec<Result<Vec<RawNip87Event>>> = vec![Ok(vec![]), Ok(vec![])];
    let feds = aggregate_discovered_events(results).expect("clean empty responses should succeed");
    assert!(feds.is_empty());
}

// ---------------------------------------------------------------------------
// Shared Completion Policy Unit Tests
// ---------------------------------------------------------------------------

#[test]
fn test_shared_completion_policy_eose_and_cutoff() {
    let mut events = Vec::new();
    let mut received_eose = false;

    let ev_str = format!(
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
    let should_break = process_relay_message_frame(&ev_str, &mut events, &mut received_eose);
    assert!(!should_break);
    assert_eq!(events.len(), 1);
    assert!(!received_eose);

    let eose_str = format!(r#"["EOSE", "{SUB_ID}"]"#);
    let should_break = process_relay_message_frame(&eose_str, &mut events, &mut received_eose);
    assert!(should_break);
    assert!(received_eose);

    let finalized = finalize_relay_query("wss://relay.example.com", events, received_eose).unwrap();
    assert_eq!(finalized.len(), 1);
}

#[test]
fn test_shared_completion_policy_disconnect_without_eose_fails() {
    let events = vec![make_test_raw_event(CODE_FEDERATION_ID, CODE)];
    let err = finalize_relay_query("wss://relay.example.com", events, false).unwrap_err();
    assert_eq!(err.code, ErrorCode::FederationUnreachable);
    assert!(err.message.contains("terminated connection without EOSE"));
}

#[test]
fn test_shared_completion_policy_event_cap_without_eose_succeeds() {
    let mut events = Vec::new();
    for _ in 0..MAX_EVENTS {
        events.push(make_test_raw_event(CODE_FEDERATION_ID, CODE));
    }
    let finalized = finalize_relay_query("wss://relay.example.com", events, false).unwrap();
    assert_eq!(finalized.len(), MAX_EVENTS);
}

// ---------------------------------------------------------------------------
// Native Controlled-Query Local Fixtures
// ---------------------------------------------------------------------------

#[cfg(not(target_family = "wasm"))]
mod local_fixture {
    use std::future::Future;
    use tokio::net::TcpListener;
    use tokio::net::TcpStream;
    use tokio_tungstenite::WebSocketStream;
    use tokio_tungstenite::accept_async;

    pub async fn spawn_mock_relay<F, Fut>(handler: F) -> (String, tokio::task::JoinHandle<()>)
    where
        F: FnOnce(WebSocketStream<TcpStream>) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind to ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        let relay_url = format!("ws://127.0.0.1:{port}");

        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept connection");
            let ws = accept_async(stream).await.expect("websocket handshake");
            handler(ws).await;
        });

        (relay_url, handle)
    }
}

#[cfg(not(target_family = "wasm"))]
use local_fixture::spawn_mock_relay;

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_matching_eose_empty() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (url, server) = spawn_mock_relay(|mut ws| async move {
        let req = ws.next().await.unwrap().unwrap();
        assert!(req.to_text().unwrap().contains("REQ"));
        ws.send(Message::text(format!(r#"["EOSE", "{SUB_ID}"]"#)))
            .await
            .unwrap();
        let close = ws.next().await.unwrap().unwrap();
        assert!(close.to_text().unwrap().contains("CLOSE"));
    })
    .await;

    let res = query_single_relay(url)
        .await
        .expect("empty EOSE should succeed");
    assert!(res.is_empty());
    server.await.unwrap();
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_matching_eose_with_events() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (url, server) = spawn_mock_relay(|mut ws| async move {
        let req = ws.next().await.unwrap().unwrap();
        assert!(req.to_text().unwrap().contains("REQ"));
        let event_json = format!(
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
        ws.send(Message::text(event_json)).await.unwrap();
        ws.send(Message::text(format!(r#"["EOSE", "{SUB_ID}"]"#)))
            .await
            .unwrap();
        let close = ws.next().await.unwrap().unwrap();
        assert!(close.to_text().unwrap().contains("CLOSE"));
    })
    .await;

    let res = query_single_relay(url)
        .await
        .expect("events with matching EOSE should succeed");
    assert_eq!(res.len(), 1);
    server.await.unwrap();
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_disconnect_before_eose_partial_events() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (url, server) = spawn_mock_relay(|mut ws| async move {
        let _ = ws.next().await;
        let event_json = format!(
            r#"["EVENT", "{SUB_ID}", {{
                "kind": 38173,
                "pubkey": "pubkey123",
                "content": "{{\"name\": \"Partial Fed\"}}",
                "tags": [
                    ["d", "{CODE_FEDERATION_ID}"],
                    ["u", "{CODE}"]
                ]
            }}]"#
        );
        ws.send(Message::text(event_json)).await.unwrap();
        let _ = ws.close(None).await;
    })
    .await;

    let err = query_single_relay(url)
        .await
        .expect_err("disconnect before EOSE must fail");
    assert_eq!(err.code, ErrorCode::FederationUnreachable);
    assert!(err.message.contains("terminated connection without EOSE"));
    server.await.unwrap();
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_disconnect_before_eose_zero_events() {
    use futures::StreamExt;

    let (url, server) = spawn_mock_relay(|mut ws| async move {
        let _ = ws.next().await;
        let _ = ws.close(None).await;
    })
    .await;

    let err = query_single_relay(url)
        .await
        .expect_err("immediate disconnect before EOSE must fail");
    assert_eq!(err.code, ErrorCode::FederationUnreachable);
    assert!(err.message.contains("terminated connection without EOSE"));
    server.await.unwrap();
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_completion_at_event_cap_without_eose() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (url, server) = spawn_mock_relay(|mut ws| async move {
        let _ = ws.next().await;
        for i in 0..MAX_EVENTS {
            let event_json = format!(
                r#"["EVENT", "{SUB_ID}", {{
                    "kind": 38173,
                    "pubkey": "pubkey_{i}",
                    "content": "{{\"name\": \"Fed {i}\"}}",
                    "tags": [
                        ["d", "{CODE_FEDERATION_ID}"],
                        ["u", "{CODE}"]
                    ]
                }}]"#
            );
            ws.send(Message::text(event_json)).await.unwrap();
        }
        let close = ws.next().await.unwrap().unwrap();
        assert!(close.to_text().unwrap().contains("CLOSE"));
    })
    .await;

    let events = query_single_relay(url)
        .await
        .expect("reaching MAX_EVENTS must complete successfully without EOSE");
    assert_eq!(events.len(), MAX_EVENTS);
    server.await.unwrap();
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_stalled_deadline() {
    use futures::StreamExt;
    use tokio::sync::oneshot;

    let (req_tx, req_rx) = oneshot::channel();
    let (url, server) = spawn_mock_relay(|mut ws| async move {
        let req = ws.next().await.unwrap().unwrap();
        assert!(req.to_text().unwrap().contains("REQ"));
        let _ = req_tx.send(());
        // Server stalls without sending EOSE or events until client disconnects
        while ws.next().await.is_some() {}
    })
    .await;

    let err = query_single_relay_with_timeout(url, std::time::Duration::from_millis(500))
        .await
        .expect_err("stalled relay must hit query timeout deadline");
    assert_eq!(err.code, ErrorCode::FederationUnreachable);
    assert!(err.message.contains("timed out"));
    assert!(
        req_rx.await.is_ok(),
        "query handshake must complete before timeout"
    );
    server.await.unwrap();
}

// ---------------------------------------------------------------------------
// Testing Boundary & Rationale
// ---------------------------------------------------------------------------
//
// The discovery test suite combines controlled native transport fixtures with
// target-agnostic shared-policy verification:
//
// 1. Native Controlled-Query Fixtures:
//    On native targets, `query_single_relay` is exercised against deterministic
//    in-process mock WebSocket servers bound to OS-assigned ephemeral ports
//    (`127.0.0.1:0`). This covers the full production transport lifecycle without
//    public relay dependencies or port collisions:
//    - Wire protocol handshake and request framing (`make_req_message`, `make_close_message`)
//    - Clean EOSE completion (both empty responses and populated events)
//    - Premature connection termination before EOSE (both partial and zero events)
//    - Automatic cutoff upon reaching `MAX_EVENTS` (100) without EOSE
//    - Enforced query timeout deadline on stalled or non-responsive relays
//
// 2. Shared Completion Policy & Untested WebAssembly Boundary:
//    The completion rules (event accumulation cutoff, EOSE enforcement, and premature
//    disconnect rejection) are encapsulated in `process_relay_message_frame` and
//    `finalize_relay_query`. These functions are target-independent and shared directly
//    between native (`tokio-tungstenite`) and WebAssembly (`gloo-net`).
//    - Tested boundary: All state-machine transitions, error codes, and cutoff thresholds
//      are exercised both in unit isolation and through the native transport loop.
//    - Untested browser boundary: The browser-native `web_sys::WebSocket` / `gloo_net`
//      socket binding itself is not executed in standard unit tests because headless
//      browser DOM runtimes (`wasm-bindgen-test`) are decoupled from unit testing.
//      Because `gloo-net` simply forwards incoming stream items to the shared completion
//      functions, the completion logic running in the browser is the identical verified policy.
