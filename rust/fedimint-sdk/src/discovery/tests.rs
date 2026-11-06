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

#[tokio::test]
async fn test_discover_federations_single_malformed_url_returns_invalid_input() {
    let res = discover_federations(DiscoverySource::Nostr {
        relays: vec!["not-a-valid-url".to_string()],
    })
    .await;
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(err.message.contains("malformed relay URL"));
}

#[tokio::test]
async fn test_discover_federations_single_invalid_scheme_returns_invalid_input() {
    let res = discover_federations(DiscoverySource::Nostr {
        relays: vec!["http://127.0.0.1:8080".to_string()],
    })
    .await;
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(err.message.contains("scheme must be ws:// or wss://"));
}

fn make_signed_test_event_with_key(
    fed_id: &str,
    invite: &str,
    created_at: u64,
    content: &str,
    key_byte: u8,
) -> RawNip87Event {
    let secp = fedimint_core::secp256k1::Secp256k1::new();
    let sk = fedimint_core::secp256k1::SecretKey::from_slice(&[key_byte; 32]).unwrap();
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

fn make_signed_test_event(
    fed_id: &str,
    invite: &str,
    created_at: u64,
    content: &str,
) -> RawNip87Event {
    make_signed_test_event_with_key(fed_id, invite, created_at, content, 42)
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
    let mut state = PageState::default();
    let outcome = state.on_frame(&raw_msg);
    assert_eq!(outcome, FrameOutcome::Continue);
    assert_eq!(state.events.len(), 1);
    assert_eq!(state.events[0].kind, Some(38173));
    assert_eq!(state.events[0].pubkey, ev.pubkey);
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
    let mut state = PageState::default();
    let outcome = state.on_frame(&raw_msg);
    assert_eq!(outcome, FrameOutcome::Continue);
    assert!(state.events.is_empty());
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
    let mut state = PageState::default();
    let outcome = state.on_frame(&raw_msg);
    assert_eq!(outcome, FrameOutcome::Continue);
    assert!(state.events.is_empty());
}

#[test]
fn test_parse_relay_eose_message() {
    let raw_msg = format!(r#"["EOSE", "{SUB_ID}"]"#);
    let mut state = PageState::default();
    let outcome = state.on_frame(&raw_msg);
    assert_eq!(outcome, FrameOutcome::Finished);
    assert!(state.received_eose);
}

#[test]
fn test_parse_relay_ignore_message() {
    let notice_msg = r#"["NOTICE", "rate-limited"]"#;
    let mut state = PageState::default();
    assert_eq!(state.on_frame(notice_msg), FrameOutcome::Continue);
    assert!(state.events.is_empty());

    let garbage_msg = r#"not json"#;
    let mut state = PageState::default();
    assert_eq!(state.on_frame(garbage_msg), FrameOutcome::Continue);
    assert!(state.events.is_empty());

    let wrong_sub_eose = r#"["EOSE", "wrong_sub_id"]"#;
    let mut state = PageState::default();
    assert_eq!(state.on_frame(wrong_sub_eose), FrameOutcome::Continue);
    assert!(!state.received_eose);

    let bare_eose = r#"["EOSE"]"#;
    let mut state = PageState::default();
    assert_eq!(state.on_frame(bare_eose), FrameOutcome::Continue);
    assert!(!state.received_eose);

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
    let mut state = PageState::default();
    assert_eq!(state.on_frame(&wrong_sub_event), FrameOutcome::Continue);
    assert!(state.events.is_empty());

    let oversized_msg = format!(
        r#"["EVENT", "{SUB_ID}", {{ "content": "{}" }}]"#,
        "x".repeat(MAX_MESSAGE_BYTES + 1)
    );
    let mut state = PageState::default();
    let outcome = state.on_frame(&oversized_msg);
    assert_eq!(outcome, FrameOutcome::Continue);
    // Oversized EVENT frames are counted in raw_events_in_page so spam doesn't
    // prevent pagination continuation, but are not added to parsed events.
    assert_eq!(state.raw_events_in_page, 1);
    assert_eq!(state.events.len(), 0);
    assert_eq!(state.oldest_created_at_in_page, None);
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
fn test_aggregate_discovered_events_anti_shadowing_preserves_distinct_publishers() {
    let ev1 = make_signed_test_event_with_key(
        CODE_FEDERATION_ID,
        CODE,
        100,
        r#"{"name": "Alice's Fed"}"#,
        42,
    );
    let ev2 = make_signed_test_event_with_key(
        CODE_FEDERATION_ID,
        CODE,
        200,
        r#"{"name": "Bob's Fed"}"#,
        43,
    );

    let results = vec![Ok(vec![ev1, ev2])];
    let feds = aggregate_discovered_events(results).expect("aggregation should succeed");
    assert_eq!(
        feds.len(),
        2,
        "distinct publishers for the same federation/invite must both be preserved"
    );
}

#[test]
fn test_discovered_federation_try_from_rejects_future_timestamp() {
    let now_secs = fedimint_core::time::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Event 20 minutes in future (> 900s tolerance) must be rejected
    let future_ev = make_signed_test_event(
        CODE_FEDERATION_ID,
        CODE,
        now_secs + 1200,
        r#"{"name": "Future Fed"}"#,
    );
    let res = DiscoveredFederation::try_from(future_ev);
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err(),
        "event created_at is too far in the future"
    );

    // Event 5 minutes in future (<= 900s tolerance) must be accepted
    let valid_ev = make_signed_test_event(
        CODE_FEDERATION_ID,
        CODE,
        now_secs + 300,
        r#"{"name": "Valid Clock Drift Fed"}"#,
    );
    let res = DiscoveredFederation::try_from(valid_ev);
    assert!(res.is_ok());
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
fn test_aggregate_discovered_events_preserves_single_invalid_input() {
    let results: Vec<Result<Vec<RawNip87Event>>> = vec![Err(Error::new(
        ErrorCode::InvalidInput,
        "Discovery error: malformed relay URL",
    ))];
    let err = aggregate_discovered_events(results).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(err.message.contains("malformed relay URL"));
}

#[test]
fn test_aggregate_discovered_events_preserves_single_timeout() {
    let results: Vec<Result<Vec<RawNip87Event>>> = vec![Err(Error::new(
        ErrorCode::Timeout,
        "Discovery error: relay timed out after 5s",
    ))];
    let err = aggregate_discovered_events(results).unwrap_err();
    assert_eq!(err.code, ErrorCode::Timeout);
    assert!(err.message.contains("timed out after 5s"));
}

#[test]
fn test_aggregate_discovered_events_precedence_invalid_input_over_timeout_and_unreachable() {
    let results: Vec<Result<Vec<RawNip87Event>>> = vec![
        Err(Error::new(
            ErrorCode::FederationUnreachable,
            "connection refused",
        )),
        Err(Error::new(ErrorCode::Timeout, "timed out after 5s")),
        Err(Error::new(
            ErrorCode::InvalidInput,
            "Discovery error: malformed relay URL",
        )),
    ];
    let err = aggregate_discovered_events(results).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(err.message.contains("malformed relay URL"));
}

#[test]
fn test_aggregate_discovered_events_precedence_timeout_over_unreachable() {
    let results: Vec<Result<Vec<RawNip87Event>>> = vec![
        Err(Error::new(
            ErrorCode::FederationUnreachable,
            "connection refused",
        )),
        Err(Error::new(ErrorCode::Timeout, "timed out after 5s")),
    ];
    let err = aggregate_discovered_events(results).unwrap_err();
    assert_eq!(err.code, ErrorCode::Timeout);
}

#[test]
fn test_aggregate_discovered_events_all_timeouts() {
    let results: Vec<Result<Vec<RawNip87Event>>> = vec![
        Err(Error::new(ErrorCode::Timeout, "relay 1 timed out")),
        Err(Error::new(ErrorCode::Timeout, "relay 2 timed out")),
    ];
    let err = aggregate_discovered_events(results).unwrap_err();
    assert_eq!(err.code, ErrorCode::Timeout);
}

#[test]
fn test_aggregate_discovered_events_mixed_success_with_invalid_input() {
    // Note: this combination cannot occur in production because fetch_from_nostr
    // validates all URLs upfront and returns InvalidInput before any connections.
    // The test exercises the aggregation function's defensive behaviour in isolation.
    let ev = make_test_raw_event(CODE_FEDERATION_ID, CODE);
    let results = vec![
        Err(Error::new(ErrorCode::InvalidInput, "bad relay url")),
        Ok(vec![ev]),
    ];
    let feds = aggregate_discovered_events(results)
        .expect("should succeed if at least one relay responds despite invalid url");
    assert_eq!(feds.len(), 1);
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
    let mut state = PageState::default();

    let ev = make_signed_test_event(
        CODE_FEDERATION_ID,
        CODE,
        12345,
        r#"{"name": "Community Fed"}"#,
    );
    let ev_str = make_signed_event_frame(&ev);
    let outcome = state.on_frame(&ev_str);
    assert_eq!(outcome, FrameOutcome::Continue);
    assert_eq!(state.events.len(), 1);
    assert!(!state.received_eose);

    let eose_str = format!(r#"["EOSE", "{SUB_ID}"]"#);
    let outcome = state.on_frame(&eose_str);
    assert_eq!(outcome, FrameOutcome::Finished);
    assert!(state.received_eose);

    let finalized =
        finalize_relay_query("wss://relay.example.com", state.events, state.received_eose).unwrap();
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

#[test]
fn test_pagination_state_triggers_next_page_when_raw_limit_hit() {
    let mut state = PageState::default();

    // Simulate 100 raw events that are invalid/spam
    for i in 0..MAX_EVENTS {
        let ts = 2000 - i as u64;
        let spam_frame = format!(
            r#"["EVENT", "{SUB_ID}", {{"id": "spam{i}", "kind": 38173, "created_at": {ts}, "tags": [], "content": ""}}]"#
        );
        let outcome = state.on_frame(&spam_frame);
        assert_eq!(outcome, FrameOutcome::Continue);
    }
    assert_eq!(state.raw_events_in_page, MAX_EVENTS);
    assert_eq!(state.events.len(), 0);
    assert_eq!(state.oldest_created_at_in_page, Some(1901));

    // EOSE frame should trigger NextPage with until = 1901 (inclusive)
    let eose_frame = format!(r#"["EOSE", "{SUB_ID}"]"#);
    let outcome = state.on_frame(&eose_frame);
    assert_eq!(
        outcome,
        FrameOutcome::NextPage {
            until: 1901,
            limit: MAX_EVENTS,
        }
    );
    assert_eq!(state.pages_queried, 1);
    assert_eq!(state.raw_events_in_page, 0);
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_bounded_backfill_recovers_announcement_behind_crowdout() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (url, server) = spawn_mock_relay(|mut ws| async move {
        // Page 1: Receive first REQ
        let req1 = ws.next().await.unwrap().unwrap();
        assert!(req1.to_text().unwrap().contains("REQ"));

        // Send 100 spam events (created_at = 1100 down to 1001)
        for i in 0..MAX_EVENTS {
            let ts = 1100 - i as u64;
            let spam_event = format!(
                r#"["EVENT", "{SUB_ID}", {{"id": "spam_{i}", "kind": 38173, "created_at": {ts}, "tags": [], "content": "spam"}}]"#
            );
            ws.send(Message::text(spam_event)).await.unwrap();
        }
        ws.send(Message::text(format!(r#"["EOSE", "{SUB_ID}"]"#)))
            .await
            .unwrap();

        // Client should send CLOSE then second REQ with until = 1001 (inclusive)
        let close1 = ws.next().await.unwrap().unwrap();
        assert!(close1.to_text().unwrap().contains("CLOSE"));

        let req2 = ws.next().await.unwrap().unwrap();
        let req2_text = req2.to_text().unwrap();
        assert!(req2_text.contains("REQ"));
        assert!(req2_text.contains(r#""until":1001"#));

        // Page 2: Send 1 valid event (created_at = 900) followed by EOSE
        let valid_ev = make_signed_test_event(
            CODE_FEDERATION_ID,
            CODE,
            900,
            r#"{"name": "Valid Fed"}"#,
        );
        ws.send(Message::text(make_signed_event_frame(&valid_ev)))
            .await
            .unwrap();
        ws.send(Message::text(format!(r#"["EOSE", "{SUB_ID}"]"#)))
            .await
            .unwrap();

        // Client should send final CLOSE
        let close2 = ws.next().await.unwrap().unwrap();
        assert!(close2.to_text().unwrap().contains("CLOSE"));
    })
    .await;

    let res = query_single_relay(url)
        .await
        .expect("backfill should successfully recover valid announcement behind crowdout");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].created_at, Some(900));
    server.await.unwrap();
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_bounded_backfill_recovers_announcement_tied_at_boundary() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (url, server) = spawn_mock_relay(|mut ws| async move {
        // Page 1: Receive first REQ
        let req1 = ws.next().await.unwrap().unwrap();
        assert!(req1.to_text().unwrap().contains("REQ"));

        // Send 99 spam events (created_at = 2000 down to 1002)
        for i in 0..(MAX_EVENTS - 1) {
            let ts = 2000 - i as u64;
            let spam_event = format!(
                r#"["EVENT", "{SUB_ID}", {{"id": "spam_{i}", "kind": 38173, "created_at": {ts}, "tags": [], "content": "spam"}}]"#
            );
            ws.send(Message::text(spam_event)).await.unwrap();
        }

        // 100th event is spam at boundary timestamp 1001
        let spam_boundary = format!(
            r#"["EVENT", "{SUB_ID}", {{"id": "spam_boundary_1001", "kind": 38173, "created_at": 1001, "tags": [], "content": "spam"}}]"#
        );
        ws.send(Message::text(spam_boundary.clone())).await.unwrap();
        ws.send(Message::text(format!(r#"["EOSE", "{SUB_ID}"]"#)))
            .await
            .unwrap();

        // Client should send CLOSE then second REQ with until = 1001 (inclusive)
        let close1 = ws.next().await.unwrap().unwrap();
        assert!(close1.to_text().unwrap().contains("CLOSE"));

        let req2 = ws.next().await.unwrap().unwrap();
        let req2_text = req2.to_text().unwrap();
        assert!(req2_text.contains("REQ"));
        assert!(req2_text.contains(r#""until":1001"#));

        // Page 2: Send duplicate boundary spam event first
        ws.send(Message::text(spam_boundary)).await.unwrap();

        // Followed by valid announcement tied at timestamp 1001 (101st event overall)
        let valid_ev = make_signed_test_event(
            CODE_FEDERATION_ID,
            CODE,
            1001,
            r#"{"name": "Boundary Tied Fed"}"#,
        );
        ws.send(Message::text(make_signed_event_frame(&valid_ev)))
            .await
            .unwrap();
        ws.send(Message::text(format!(r#"["EOSE", "{SUB_ID}"]"#)))
            .await
            .unwrap();

        // Client should send final CLOSE
        let close2 = ws.next().await.unwrap().unwrap();
        assert!(close2.to_text().unwrap().contains("CLOSE"));
    })
    .await;

    let res = query_single_relay(url)
        .await
        .expect("backfill should successfully recover announcement tied at boundary");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].created_at, Some(1001));
    server.await.unwrap();
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_deduplicates_overlapping_events_across_pages() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (url, server) = spawn_mock_relay(|mut ws| async move {
        // Page 1: Receive first REQ
        let req1 = ws.next().await.unwrap().unwrap();
        assert!(req1.to_text().unwrap().contains("REQ"));

        let valid_ev = make_signed_test_event(
            CODE_FEDERATION_ID,
            CODE,
            1001,
            r#"{"name": "Shared Fed"}"#,
        );
        // Page 1: 1 valid event at 1001, plus 99 spam events at 1001
        ws.send(Message::text(make_signed_event_frame(&valid_ev)))
            .await
            .unwrap();
        for i in 0..(MAX_EVENTS - 1) {
            let spam_event = format!(
                r#"["EVENT", "{SUB_ID}", {{"id": "spam_{i}", "kind": 38173, "created_at": 1001, "tags": [], "content": "spam"}}]"#
            );
            ws.send(Message::text(spam_event)).await.unwrap();
        }
        ws.send(Message::text(format!(r#"["EOSE", "{SUB_ID}"]"#)))
            .await
            .unwrap();

        // Page 2: Client sends CLOSE and second REQ with until = 1001
        let close1 = ws.next().await.unwrap().unwrap();
        assert!(close1.to_text().unwrap().contains("CLOSE"));

        let req2 = ws.next().await.unwrap().unwrap();
        assert!(req2.to_text().unwrap().contains(r#""until":1001"#));

        // Re-send the same valid event on Page 2
        ws.send(Message::text(make_signed_event_frame(&valid_ev)))
            .await
            .unwrap();
        ws.send(Message::text(format!(r#"["EOSE", "{SUB_ID}"]"#)))
            .await
            .unwrap();

        let close2 = ws.next().await.unwrap().unwrap();
        assert!(close2.to_text().unwrap().contains("CLOSE"));
    })
    .await;

    let res = query_single_relay(url)
        .await
        .expect("overlapping events across pages should be deduplicated");
    assert_eq!(res.len(), 1, "identical valid event must not be duplicated");
    server.await.unwrap();
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_bounded_backfill_stops_at_max_pages() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (url, server) = spawn_mock_relay(|mut ws| async move {
        for page in 0..MAX_PAGES {
            let req = ws.next().await.unwrap().unwrap();
            assert!(req.to_text().unwrap().contains("REQ"));

            let base_ts = 3000 - (page * 100) as u64;
            for i in 0..MAX_EVENTS {
                let ts = base_ts - i as u64;
                let spam_event = format!(
                    r#"["EVENT", "{SUB_ID}", {{"id": "spam_{page}_{i}", "kind": 38173, "created_at": {ts}, "tags": [], "content": "spam"}}]"#
                );
                ws.send(Message::text(spam_event)).await.unwrap();
            }
            ws.send(Message::text(format!(r#"["EOSE", "{SUB_ID}"]"#)))
                .await
                .unwrap();

            let close = ws.next().await.unwrap().unwrap();
            assert!(close.to_text().unwrap().contains("CLOSE"));
        }
    })
    .await;

    let res = query_single_relay(url)
        .await
        .expect("bounded backfill should complete after max pages even with zero valid events");
    assert_eq!(res.len(), 0);
    server.await.unwrap();
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_skips_oversized_frame_without_failing() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (url, server) = spawn_mock_relay(|mut ws| async move {
        let req = ws.next().await.unwrap().unwrap();
        assert!(req.to_text().unwrap().contains("REQ"));

        // Send a 100 KiB EVENT frame (above MAX_MESSAGE_BYTES = 64 KiB, but
        // below MAX_TRANSPORT_BYTES = 1 MiB). The transport should accept it,
        // and the application-level size check should skip it without killing
        // the connection.
        let oversized_content = "x".repeat(100 * 1024);
        let oversized_event = format!(
            r#"["EVENT", "{SUB_ID}", {{"id": "big", "kind": 38173, "created_at": 999, "tags": [], "content": "{oversized_content}"}}]"#
        );
        ws.send(Message::text(oversized_event)).await.unwrap();

        // Follow with a valid event and EOSE
        let valid_ev = make_signed_test_event(
            CODE_FEDERATION_ID,
            CODE,
            1000,
            r#"{"name": "After Oversized"}"#,
        );
        ws.send(Message::text(make_signed_event_frame(&valid_ev)))
            .await
            .unwrap();
        ws.send(Message::text(format!(r#"["EOSE", "{SUB_ID}"]"#)))
            .await
            .unwrap();

        let close = ws.next().await.unwrap().unwrap();
        assert!(close.to_text().unwrap().contains("CLOSE"));
    })
    .await;

    let res = query_single_relay(url)
        .await
        .expect("oversized frame should be skipped, not fail the query");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].created_at, Some(1000));
    server.await.unwrap();
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_full_timestamp_bucket_expands_limit() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    // Scenario: 100 signed-but-invalid events ALL at timestamp 1001.
    // A valid announcement sits at timestamp 1000 (the 101st event).
    // With newest == oldest on page 1, the client detects the single-timestamp
    // bucket immediately (no wasted round trip) and expands the limit on page 2.

    // Pre-build the 100 spam frames (valid JSON events but won't pass
    // federation invite parsing, so they're counted as raw but not accepted).
    let spam_frames: Vec<String> = (0..MAX_EVENTS)
        .map(|i| {
            format!(
                r#"["EVENT", "{SUB_ID}", {{"id": "bucket_{i}", "kind": 38173, "created_at": 1001, "tags": [], "content": "spam"}}]"#
            )
        })
        .collect();

    let valid_ev = make_signed_test_event(
        CODE_FEDERATION_ID,
        CODE,
        1000,
        r#"{"name": "Hidden Behind Bucket"}"#,
    );
    let valid_frame = make_signed_event_frame(&valid_ev);

    let (url, server) = spawn_mock_relay(move |mut ws| async move {
        // Page 1: first REQ (limit=100, no until)
        let req1 = ws.next().await.unwrap().unwrap();
        assert!(req1.to_text().unwrap().contains("REQ"));

        // Return all 100 spam at ts=1001
        for frame in &spam_frames {
            ws.send(Message::text(frame.clone())).await.unwrap();
        }
        ws.send(Message::text(format!(r#"["EOSE", "{SUB_ID}"]"#)))
            .await
            .unwrap();

        // Client detects newest==oldest==1001 on first page → bucket detected
        // immediately. Sends CLOSE + REQ with until=1001, limit=200 (expanded).
        let close1 = ws.next().await.unwrap().unwrap();
        assert!(close1.to_text().unwrap().contains("CLOSE"));
        let req2 = ws.next().await.unwrap().unwrap();
        let req2_text = req2.to_text().unwrap();
        assert!(req2_text.contains(r#""until":1001"#));
        assert!(
            req2_text.contains(r#""limit":200"#),
            "expected expanded limit:200 on page 2 (immediate bucket detection), got: {req2_text}"
        );

        // Page 2: relay returns all 100 spam PLUS the valid event at ts=1000
        for frame in &spam_frames {
            ws.send(Message::text(frame.clone())).await.unwrap();
        }
        ws.send(Message::text(valid_frame)).await.unwrap();
        ws.send(Message::text(format!(r#"["EOSE", "{SUB_ID}"]"#)))
            .await
            .unwrap();

        // Client sends final CLOSE
        let close2 = ws.next().await.unwrap().unwrap();
        assert!(close2.to_text().unwrap().contains("CLOSE"));
    })
    .await;

    let res = query_single_relay(url)
        .await
        .expect("full timestamp bucket should be resolved by expanding limit");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].created_at, Some(1000));
    server.await.unwrap();
}

// ---------------------------------------------------------------------------
// CLOSED Message Handling Tests
// ---------------------------------------------------------------------------

#[test]
fn test_parse_relay_closed_message() {
    let closed_msg = format!(r#"["CLOSED", "{SUB_ID}", "auth-required: please authenticate"]"#);
    let mut state = PageState::default();
    let outcome = state.on_frame(&closed_msg);
    assert_eq!(outcome, FrameOutcome::Finished);
    assert!(
        !state.received_eose,
        "CLOSED must not set received_eose so finalize_relay_query reports failure"
    );
}

#[test]
fn test_parse_relay_closed_wrong_sub_ignored() {
    let closed_msg = r#"["CLOSED", "wrong_sub_id", "auth-required: ..."]"#;
    let mut state = PageState::default();
    let outcome = state.on_frame(closed_msg);
    assert_eq!(outcome, FrameOutcome::Continue);
    assert!(!state.received_eose);
}

#[test]
fn test_process_frame_closed_returns_finished_without_eose() {
    let mut state = PageState::default();

    let closed_msg = format!(r#"["CLOSED", "{SUB_ID}", "rate-limited: slow down"]"#);
    let outcome = state.on_frame(&closed_msg);
    assert_eq!(outcome, FrameOutcome::Finished);
    assert!(
        !state.received_eose,
        "CLOSED must not set received_eose so finalize_relay_query reports failure"
    );
}

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_closed_returns_unreachable() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (url, server) = spawn_mock_relay(|mut ws| async move {
        let _ = ws.next().await; // REQ
        ws.send(Message::text(format!(
            r#"["CLOSED", "{SUB_ID}", "auth-required: please authenticate"]"#
        )))
        .await
        .unwrap();
        // Let the client close
        while ws.next().await.is_some() {}
    })
    .await;

    let err = query_single_relay(url)
        .await
        .expect_err("CLOSED from relay must fail the query");
    assert_eq!(err.code, ErrorCode::FederationUnreachable);
    server.await.unwrap();
}

// ---------------------------------------------------------------------------
// Sticky EOSE / Query Deadline Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[cfg(not(target_family = "wasm"))]
async fn test_query_single_relay_page1_eose_page2_stall_returns_page1_events() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (url, server) = spawn_mock_relay(|mut ws| async move {
        // Page 1: REQ → 100 spam events + 1 valid announcement at varied timestamps → EOSE
        let _ = ws.next().await; // REQ
        for i in 0..(MAX_EVENTS - 1) {
            let ts = 2000 - i as u64;
            let spam = format!(
                r#"["EVENT", "{SUB_ID}", {{"id": "spam_{i}", "kind": 38173, "created_at": {ts}, "tags": [], "content": "spam"}}]"#
            );
            ws.send(Message::text(spam)).await.unwrap();
        }
        let valid_ev = make_signed_test_event(
            CODE_FEDERATION_ID,
            CODE,
            1900,
            r#"{"name": "Page1 Fed"}"#,
        );
        ws.send(Message::text(make_signed_event_frame(&valid_ev)))
            .await
            .unwrap();
        ws.send(Message::text(format!(r#"["EOSE", "{SUB_ID}"]"#)))
            .await
            .unwrap();

        // Client sends CLOSE + second REQ for page 2
        let close = ws.next().await.unwrap().unwrap();
        assert!(close.to_text().unwrap().contains("CLOSE"));
        let _ = ws.next().await; // REQ page 2

        // Page 2: stall — don't send anything, let the client time out
        // Keep connection alive until client drops
        while ws.next().await.is_some() {}
    })
    .await;

    // Use a short timeout so the test doesn't hang
    let res = query_single_relay_with_timeout(url, std::time::Duration::from_millis(1500))
        .await
        .expect("page-1 events must be returned when page-2 stalls (sticky EOSE + query deadline)");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].created_at, Some(1900));
    server.await.unwrap();
}

// ---------------------------------------------------------------------------
// Immediate Bucket Detection (newest == oldest) Unit Test
// ---------------------------------------------------------------------------

#[test]
fn test_pagination_immediate_bucket_detection_first_page() {
    let mut state = PageState::default();

    // Simulate 100 raw events ALL at the same timestamp
    for i in 0..MAX_EVENTS {
        let spam_frame = format!(
            r#"["EVENT", "{SUB_ID}", {{"id": "bucket_{i}", "kind": 38173, "created_at": 5000, "tags": [], "content": "spam"}}]"#
        );
        let outcome = state.on_frame(&spam_frame);
        assert_eq!(outcome, FrameOutcome::Continue);
    }
    assert_eq!(state.oldest_created_at_in_page, Some(5000));
    assert_eq!(state.newest_created_at_in_page, Some(5000));

    // EOSE: bucket detected on first page via newest==oldest, limit expanded immediately
    let eose = format!(r#"["EOSE", "{SUB_ID}"]"#);
    let outcome = state.on_frame(&eose);
    assert_eq!(
        outcome,
        FrameOutcome::NextPage {
            until: 5000,
            limit: MAX_EVENTS + MAX_EVENTS, // 200 = expanded immediately
        }
    );
}

#[test]
fn test_pagination_short_page_with_expanded_limit_terminates() {
    let mut state = PageState::default();

    // Page 1: 100 events with identical timestamp -> full bucket
    for i in 0..MAX_EVENTS {
        let spam_frame = format!(
            r#"["EVENT", "{SUB_ID}", {{"id": "b1_{i}", "kind": 38173, "created_at": 5000, "tags": [], "content": "spam"}}]"#
        );
        assert_eq!(state.on_frame(&spam_frame), FrameOutcome::Continue);
    }
    let eose = format!(r#"["EOSE", "{SUB_ID}"]"#);
    let outcome = state.on_frame(&eose);
    assert_eq!(
        outcome,
        FrameOutcome::NextPage {
            until: 5000,
            limit: 200,
        }
    );
    assert_eq!(state.current_limit, 200);

    // Page 2: relay only returns 150 events (fewer than current_limit of 200)
    for i in 0..150 {
        let spam_frame = format!(
            r#"["EVENT", "{SUB_ID}", {{"id": "b2_{i}", "kind": 38173, "created_at": 5000, "tags": [], "content": "spam"}}]"#
        );
        assert_eq!(state.on_frame(&spam_frame), FrameOutcome::Continue);
    }

    // Page 2 EOSE arrives: since raw_events_in_page (150) < current_limit (200),
    // the page is short and pagination MUST terminate with FrameOutcome::Finished.
    let outcome2 = state.on_frame(&eose);
    assert_eq!(outcome2, FrameOutcome::Finished);
}

// ---------------------------------------------------------------------------
// Oversized EVENT Counted for Pagination
// ---------------------------------------------------------------------------

#[test]
fn test_oversized_event_frame_counted_for_pagination() {
    let mut state = PageState::default();

    // An EVENT frame larger than MAX_MESSAGE_BYTES should be counted in raw_events_in_page
    let oversized_content = "x".repeat(MAX_MESSAGE_BYTES + 1);
    let oversized_event = format!(
        r#"["EVENT", "{SUB_ID}", {{"id": "big", "kind": 38173, "created_at": 999, "tags": [], "content": "{oversized_content}"}}]"#
    );
    let outcome = state.on_frame(&oversized_event);
    assert_eq!(outcome, FrameOutcome::Continue);
    assert_eq!(
        state.raw_events_in_page, 1,
        "oversized EVENT must be counted for pagination"
    );
    assert_eq!(
        state.oldest_created_at_in_page,
        Some(999),
        "oversized EVENT must track created_at for pagination cursor"
    );
    assert_eq!(
        state.newest_created_at_in_page,
        Some(999),
        "oversized EVENT must track newest created_at for bucket expansion"
    );
    assert!(
        state.events.is_empty(),
        "oversized EVENT must not be added to valid events"
    );

    // A non-EVENT oversized frame should NOT be counted
    let mut state2 = PageState::default();
    let oversized_notice = format!(r#"["NOTICE", "{}"]"#, "y".repeat(MAX_MESSAGE_BYTES + 1));
    let outcome2 = state2.on_frame(&oversized_notice);
    assert_eq!(outcome2, FrameOutcome::Continue);
    assert_eq!(
        state2.raw_events_in_page, 0,
        "oversized non-EVENT must not be counted"
    );
    assert_eq!(state2.oldest_created_at_in_page, None);
}

#[test]
fn test_scan_created_at_escaped_strings_and_whitespace() {
    use super::scan_created_at;

    // Standard formatting
    assert_eq!(
        scan_created_at(r#"{"created_at": 1700000000}"#),
        Some(1700000000)
    );
    // Whitespace between key and colon, colon and value
    assert_eq!(
        scan_created_at(r#"{"created_at"   :   12345}"#),
        Some(12345)
    );
    // Escaped key inside content must NOT match
    let content_attack = r#"{"content": "foo \"created_at\": 0 bar", "created_at": 9999}"#;
    assert_eq!(scan_created_at(content_attack), Some(9999));
    // Escaped with triple backslash (\") inside content
    let triple_backslash = r#"{"content": "foo \\\"created_at\": 0 bar", "created_at": 8888}"#;
    assert_eq!(scan_created_at(triple_backslash), Some(8888));
    // Key inside tag array (followed by comma, not colon)
    let tag_frame = r#"{"tags": [["created_at", "tag_val"]], "created_at": 7777}"#;
    assert_eq!(scan_created_at(tag_frame), Some(7777));
    // Key prefix match should be ignored
    let prefix_match = r#"{"created_at_ms": 1111, "created_at": 2222}"#;
    assert_eq!(scan_created_at(prefix_match), Some(2222));
    // Missing created_at
    assert_eq!(scan_created_at(r#"{"content": "hello"}"#), None);
    // Non-numeric created_at
    assert_eq!(scan_created_at(r#"{"created_at": "invalid"}"#), None);
}

#[test]
fn test_is_our_event_frame() {
    use super::is_our_event_frame;

    assert!(is_our_event_frame(r#"["EVENT", "fedimint_discovery", {}]"#));
    assert!(is_our_event_frame(r#"["EVENT","fedimint_discovery",{}]"#));
    assert!(is_our_event_frame(
        r#"  [  "EVENT"  ,  "fedimint_discovery"  , {}]"#
    ));
    assert!(!is_our_event_frame(r#"["EVENT", "other_sub_id", {}]"#));
    assert!(!is_our_event_frame(
        r#"["EVENT", "fedimint_discovery_suffix", {}]"#
    ));
    assert!(!is_our_event_frame(r#"["NOTICE", "fedimint_discovery"]"#));
    assert!(!is_our_event_frame(r#"["EOSE", "fedimint_discovery"]"#));
    assert!(!is_our_event_frame("malformed non-json"));
}

#[test]
fn test_full_page_oversized_events_triggers_next_page() {
    let mut state = PageState::default();
    let oversized_pad = "x".repeat(MAX_MESSAGE_BYTES + 1);

    // 100 oversized EVENT frames with decreasing timestamps (from 2000 down to 1901)
    for i in 0..MAX_EVENTS {
        let ts = 2000 - i as u64;
        let frame = format!(
            r#"["EVENT", "{SUB_ID}", {{"id": "big_{i}", "kind": 38173, "created_at": {ts}, "tags": [], "content": "{oversized_pad}"}}]"#
        );
        let outcome = state.on_frame(&frame);
        assert_eq!(outcome, FrameOutcome::Continue);
    }

    assert_eq!(state.raw_events_in_page, 100);
    assert_eq!(state.oldest_created_at_in_page, Some(1901));
    assert_eq!(state.newest_created_at_in_page, Some(2000));
    assert_eq!(state.events.len(), 0);

    // EOSE arrives: must paginate rather than returning empty Finished!
    let eose = format!(r#"["EOSE", "{SUB_ID}"]"#);
    let outcome = state.on_frame(&eose);
    assert_eq!(
        outcome,
        FrameOutcome::NextPage {
            until: 1901,
            limit: MAX_EVENTS,
        }
    );
    assert_eq!(state.pages_queried, 1);
    assert_eq!(state.previous_until, Some(1901));
}

#[test]
fn test_full_page_oversized_events_bucket_expansion() {
    let mut state = PageState::default();
    let oversized_pad = "x".repeat(MAX_MESSAGE_BYTES + 1);

    // 100 oversized EVENT frames all sharing timestamp 1500 -> full bucket
    for i in 0..MAX_EVENTS {
        let frame = format!(
            r#"["EVENT", "{SUB_ID}", {{"id": "bucket_{i}", "kind": 38173, "created_at": 1500, "tags": [], "content": "{oversized_pad}"}}]"#
        );
        let outcome = state.on_frame(&frame);
        assert_eq!(outcome, FrameOutcome::Continue);
    }

    assert_eq!(state.raw_events_in_page, 100);
    assert_eq!(state.oldest_created_at_in_page, Some(1500));
    assert_eq!(state.newest_created_at_in_page, Some(1500));

    let eose = format!(r#"["EOSE", "{SUB_ID}"]"#);
    let outcome = state.on_frame(&eose);
    assert_eq!(
        outcome,
        FrameOutcome::NextPage {
            until: 1500,
            limit: MAX_EVENTS + MAX_EVENTS, // 200 = expanded limit
        }
    );
}

#[test]
fn test_aggregate_sort_deterministic_tiebreak_publisher() {
    let ev_alice = make_signed_test_event_with_key(
        CODE_FEDERATION_ID,
        CODE,
        100,
        r#"{"name": "Alice's Fed"}"#,
        42,
    );
    let ev_bob = make_signed_test_event_with_key(
        CODE_FEDERATION_ID,
        CODE,
        100,
        r#"{"name": "Bob's Fed"}"#,
        43,
    );

    let pk_alice = ev_alice.pubkey.clone().unwrap();
    let pk_bob = ev_bob.pubkey.clone().unwrap();
    assert_ne!(pk_alice, pk_bob);

    // Feed in [Alice, Bob]
    let res1 = aggregate_discovered_events(vec![Ok(vec![ev_alice.clone(), ev_bob.clone()])])
        .expect("aggregation should succeed");
    // Feed in [Bob, Alice]
    let res2 = aggregate_discovered_events(vec![Ok(vec![ev_bob, ev_alice])])
        .expect("aggregation should succeed");

    assert_eq!(res1.len(), 2);
    assert_eq!(res2.len(), 2);

    let pubkeys1: Vec<_> = res1
        .iter()
        .map(|f| f.metadata.get("publisher_pubkey").cloned())
        .collect();
    let pubkeys2: Vec<_> = res2
        .iter()
        .map(|f| f.metadata.get("publisher_pubkey").cloned())
        .collect();

    // The order must be strictly identical regardless of input order, sorted by publisher_pubkey
    assert_eq!(pubkeys1, pubkeys2);
    let mut expected = pubkeys1.clone();
    expected.sort();
    assert_eq!(pubkeys1, expected);
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
//    - Relay CLOSED message handling (auth-required, rate-limited)
//    - Sticky EOSE with query deadline (page-1 events survive page-2 stall)
//    - Oversized frame skip (100 KiB EVENT skipped, subsequent events collected)
//    - Full-timestamp bucket detection with immediate limit expansion
//
// 2. Shared Completion Policy & Untested WebAssembly Boundary:
//    The completion rules (event accumulation cutoff, EOSE enforcement, and premature
//    disconnect rejection) are encapsulated in `PageState::on_frame` and
//    `finalize_relay_query`. These functions are target-independent and shared directly
//    between native (`tokio-tungstenite`) and WebAssembly (`gloo-net`).
//    - Tested boundary: All state-machine transitions, error codes, and cutoff thresholds
//      are exercised both in unit isolation and through the native transport loop.
//    - Untested browser boundary: The browser-native `web_sys::WebSocket` / `gloo_net`
//      socket binding itself is not executed in standard unit tests because headless
//      browser DOM runtimes (`wasm-bindgen-test`) are decoupled from unit testing.
//      Because `gloo-net` simply forwards incoming stream items to the shared completion
//      functions, the completion logic running in the browser is the identical verified policy.
