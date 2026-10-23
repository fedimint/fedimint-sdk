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

fn make_signed_test_event(
    fed_id: &str,
    invite: &str,
    created_at: u64,
    content: &str,
) -> RawNip87Event {
    let secp = fedimint_core::secp256k1::Secp256k1::new();
    let sk = fedimint_core::secp256k1::SecretKey::from_slice(&[42u8; 32]).unwrap();
    let keypair = fedimint_core::secp256k1::Keypair::from_secret_key(&secp, &sk);
    let (xonly, _) = keypair.x_only_public_key();
    let pubkey_hex = xonly.to_string();

    let tags = vec![
        vec!["d".to_string(), fed_id.to_string()],
        vec!["u".to_string(), invite.to_string()],
    ];

    let serialized =
        serde_json::to_string(&(0, &pubkey_hex, created_at, 38173u64, &tags, content)).unwrap();

    use fedimint_core::bitcoin::hashes::{Hash, sha256};
    let id_hash = sha256::Hash::hash(serialized.as_bytes());
    let id_hex = format!("{id_hash:x}");
    let msg = fedimint_core::secp256k1::Message::from_digest(id_hash.to_byte_array());
    let sig = secp.sign_schnorr(&msg, &keypair);

    RawNip87Event {
        id: Some(id_hex),
        pubkey: Some(pubkey_hex),
        created_at: Some(created_at),
        kind: Some(38173),
        tags,
        content: content.to_string(),
        sig: Some(sig.to_string()),
    }
}

fn make_signed_event_frame(event: &RawNip87Event) -> String {
    let ev_val = serde_json::json!({
        "id": event.id,
        "pubkey": event.pubkey,
        "created_at": event.created_at,
        "kind": event.kind,
        "tags": event.tags,
        "content": event.content,
        "sig": event.sig,
    });
    serde_json::json!(["EVENT", SUB_ID, ev_val]).to_string()
}

fn make_test_raw_event(fed_id: &str, invite: &str) -> RawNip87Event {
    make_signed_test_event(fed_id, invite, 12345, r#"{"name": "Test Fed"}"#)
}

#[test]
fn test_parse_relay_event_message() {
    let ev = make_signed_test_event(
        CODE_FEDERATION_ID,
        CODE,
        12345,
        r#"{"name": "Community Fed"}"#,
    );
    let raw_msg = make_signed_event_frame(&ev);
    match parse_relay_message(&raw_msg) {
        RelayAction::Event(parsed) => {
            assert_eq!(parsed.kind, Some(38173));
            assert_eq!(parsed.pubkey, ev.pubkey);
        }
        other => panic!("expected RelayAction::Event, got {other:?}"),
    }
}

#[test]
fn test_parse_relay_rejects_invalid_signature() {
    let mut ev = make_signed_test_event(
        CODE_FEDERATION_ID,
        CODE,
        12345,
        r#"{"name": "Spoofed Fed"}"#,
    );
    ev.sig = Some("00".repeat(64));
    let raw_msg = make_signed_event_frame(&ev);
    assert_eq!(parse_relay_message(&raw_msg), RelayAction::Ignore);
}

#[test]
fn test_parse_relay_rejects_tampered_id() {
    let mut ev = make_signed_test_event(
        CODE_FEDERATION_ID,
        CODE,
        12345,
        r#"{"name": "Spoofed Fed"}"#,
    );
    ev.id = Some("11".repeat(32));
    let raw_msg = make_signed_event_frame(&ev);
    assert_eq!(parse_relay_message(&raw_msg), RelayAction::Ignore);
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

    let ev = make_signed_test_event(
        CODE_FEDERATION_ID,
        CODE,
        12345,
        r#"{"name": "Community Fed"}"#,
    );
    let wrong_sub_event = serde_json::json!(["EVENT", "wrong_sub_id", {
        "id": ev.id,
        "pubkey": ev.pubkey,
        "created_at": ev.created_at,
        "kind": ev.kind,
        "tags": ev.tags,
        "content": ev.content,
        "sig": ev.sig,
    }])
    .to_string();
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
fn test_aggregate_discovered_events_anti_shadowing_preserves_multiple_invites() {
    let invite1 = CODE;
    let fed_id = CODE_FEDERATION_ID.parse::<FederationId>().unwrap();
    let other_url = fedimint_core::util::SafeUrl::parse("wss://secondary.guardian.net/").unwrap();
    let invite2_str = fedimint_core::invite_code::InviteCode::new(
        other_url,
        fedimint_core::PeerId::from(1),
        fed_id.inner(),
        None,
    )
    .to_string();

    let ev1 = make_signed_test_event(
        CODE_FEDERATION_ID,
        invite1,
        100,
        r#"{"name": "Primary Fed"}"#,
    );
    let ev2 = make_signed_test_event(
        CODE_FEDERATION_ID,
        &invite2_str,
        100,
        r#"{"name": "Secondary Fed"}"#,
    );

    let results = vec![Ok(vec![ev1, ev2])];
    let feds = aggregate_discovered_events(results).expect("aggregation should succeed");
    assert_eq!(
        feds.len(),
        2,
        "distinct invite codes for the same federation must both be preserved"
    );
}

#[test]
fn test_aggregate_discovered_events_updates_to_latest_timestamp() {
    let ev_old =
        make_signed_test_event(CODE_FEDERATION_ID, CODE, 100, r#"{"name": "Old Fed Name"}"#);
    let ev_new = make_signed_test_event(
        CODE_FEDERATION_ID,
        CODE,
        200,
        r#"{"name": "Updated Fed Name"}"#,
    );

    let results = vec![Ok(vec![ev_old]), Ok(vec![ev_new])];
    let feds = aggregate_discovered_events(results).expect("aggregation should succeed");
    assert_eq!(feds.len(), 1);
    assert_eq!(feds[0].name.as_deref(), Some("Updated Fed Name"));
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

    let ev = make_signed_test_event(
        CODE_FEDERATION_ID,
        CODE,
        12345,
        r#"{"name": "Community Fed"}"#,
    );
    let ev_str = make_signed_event_frame(&ev);
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
        let ev = make_signed_test_event(
            CODE_FEDERATION_ID,
            CODE,
            12345,
            r#"{"name": "Community Fed"}"#,
        );
        let event_json = make_signed_event_frame(&ev);
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
        let ev = make_signed_test_event(
            CODE_FEDERATION_ID,
            CODE,
            12345,
            r#"{"name": "Partial Fed"}"#,
        );
        let event_json = make_signed_event_frame(&ev);
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
            let ev = make_signed_test_event(
                CODE_FEDERATION_ID,
                CODE,
                1000 + i as u64,
                &format!(r#"{{"name": "Fed {i}"}}"#),
            );
            let event_json = make_signed_event_frame(&ev);
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
    assert_eq!(err.code, ErrorCode::Timeout);
    assert!(err.message.contains("timed out"));
    assert!(
        req_rx.await.is_ok(),
        "query handshake must complete before timeout"
    );
    server.await.unwrap();
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_rejects_malformed_url() {
    let err = query_single_relay("not-a-valid-url".to_string())
        .await
        .expect_err("malformed URL must fail");
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(err.message.contains("malformed relay URL"));
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_rejects_non_ws_scheme() {
    let err = query_single_relay("http://127.0.0.1:8080".to_string())
        .await
        .expect_err("http scheme must fail");
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(err.message.contains("scheme must be ws:// or wss://"));
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
