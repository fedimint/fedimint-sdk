use super::*;

const CODE: &str = "fed11qgqpqrnhwden5te0vehk7tnzv9ez7qqpyq4z52329g4z52329g4z52329g4z52329g4z52329g4z52329g4z5wa8phk";
const CODE_FEDERATION_ID: &str = "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a";

#[test]
fn parses_valid_nip87_event() {
    let json = format!(
        r#"{{
            "kind": 38173,
            "pubkey": "deadbeef1234",
            "content": "{{\"name\": \"Test Fed\", \"picture\": \"https://example.com/logo.png\"}}",
            "tags": [
                ["d", "{CODE_FEDERATION_ID}"],
                ["u", "{CODE}"],
                ["modules", "lightning,wallet,mint"],
                ["n", "signet"]
            ]
        }}"#
    );
    let raw: RawNip87Event = serde_json::from_str(&json).unwrap();
    let fed = DiscoveredFederation::try_from(raw).expect("should parse");
    assert_eq!(fed.id, CODE_FEDERATION_ID.parse::<FederationId>().unwrap());
    assert_eq!(fed.invite, CODE.parse::<InviteCode>().unwrap());
    assert_eq!(fed.name.as_deref(), Some("Test Fed"));
    assert_eq!(fed.network, Some(Network::Signet));
    assert_eq!(
        fed.modules,
        vec![
            "lightning".to_string(),
            "wallet".to_string(),
            "mint".to_string()
        ]
    );
    assert_eq!(fed.metadata.get("name").unwrap(), "Test Fed");
    assert_eq!(
        fed.metadata.get("picture").unwrap(),
        "https://example.com/logo.png"
    );
    assert_eq!(
        fed.metadata.get("modules").unwrap(),
        "lightning,wallet,mint"
    );
    assert_eq!(fed.metadata.get("network").unwrap(), "signet");
    assert_eq!(
        fed.metadata.get("publisher_pubkey").unwrap(),
        "deadbeef1234"
    );
}

#[test]
fn parses_nip87_event_with_multiple_u_tags() {
    let invalid_code = "fed11invalidinvitecodethatfails";
    let json = format!(
        r#"{{
            "kind": 38173,
            "tags": [
                ["d", "{CODE_FEDERATION_ID}"],
                ["u", "{invalid_code}"],
                ["u", "{CODE}"]
            ]
        }}"#
    );
    let raw: RawNip87Event = serde_json::from_str(&json).unwrap();
    let fed = DiscoveredFederation::try_from(raw).expect("should pick valid matching invite code");
    assert_eq!(fed.id, CODE_FEDERATION_ID.parse::<FederationId>().unwrap());
    assert_eq!(fed.invite, CODE.parse::<InviteCode>().unwrap());
}

#[test]
fn parses_nip87_event_with_plain_text_content() {
    let json = format!(
        r#"{{
            "kind": 38173,
            "content": "This is a plain text community mint description",
            "tags": [
                ["d", "{CODE_FEDERATION_ID}"],
                ["u", "{CODE}"]
            ]
        }}"#
    );
    let raw: RawNip87Event = serde_json::from_str(&json).unwrap();
    let fed = DiscoveredFederation::try_from(raw).expect("should parse");
    assert_eq!(
        fed.metadata.get("description").unwrap(),
        "This is a plain text community mint description"
    );
}

#[test]
fn drops_event_with_wrong_kind() {
    let json = format!(
        r#"{{
            "kind": 1,
            "tags": [
                ["d", "{CODE_FEDERATION_ID}"],
                ["u", "{CODE}"]
            ]
        }}"#
    );
    let raw: RawNip87Event = serde_json::from_str(&json).unwrap();
    assert!(DiscoveredFederation::try_from(raw).is_err());
}

#[test]
fn drops_event_with_missing_kind() {
    let json = format!(
        r#"{{
            "tags": [
                ["d", "{CODE_FEDERATION_ID}"],
                ["u", "{CODE}"]
            ]
        }}"#
    );
    let raw: RawNip87Event = serde_json::from_str(&json).unwrap();
    assert!(DiscoveredFederation::try_from(raw).is_err());
}

#[test]
fn drops_event_with_missing_d_tag() {
    let json = format!(
        r#"{{
            "kind": 38173,
            "tags": [
                ["u", "{CODE}"]
            ]
        }}"#
    );
    let raw: RawNip87Event = serde_json::from_str(&json).unwrap();
    assert!(DiscoveredFederation::try_from(raw).is_err());
}

#[test]
fn drops_event_with_invalid_federation_id() {
    let json = format!(
        r#"{{
            "kind": 38173,
            "tags": [
                ["d", "not-a-valid-hex-id"],
                ["u", "{CODE}"]
            ]
        }}"#
    );
    let raw: RawNip87Event = serde_json::from_str(&json).unwrap();
    assert!(DiscoveredFederation::try_from(raw).is_err());
}

#[test]
fn drops_event_with_id_mismatch() {
    let other_id = "1111111111111111111111111111111111111111111111111111111111111111";
    let json = format!(
        r#"{{
            "kind": 38173,
            "tags": [
                ["d", "{other_id}"],
                ["u", "{CODE}"]
            ]
        }}"#
    );
    let raw: RawNip87Event = serde_json::from_str(&json).unwrap();
    let res = DiscoveredFederation::try_from(raw);
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err(),
        "no matching invite code in u tags for declared federation id"
    );
}

#[test]
fn test_dev_federation_fixture() {
    let dev = dev_federation();
    assert_eq!(dev, DiscoveredFederation::dev());
    assert_eq!(dev.id, CODE_FEDERATION_ID.parse::<FederationId>().unwrap());
    assert_eq!(dev.invite, CODE.parse::<InviteCode>().unwrap());
    assert_eq!(dev.name.as_deref(), Some("Dev Test Federation"));
    assert_eq!(dev.network, Some(Network::Regtest));
    assert_eq!(
        dev.modules,
        vec![
            "lightning".to_string(),
            "wallet".to_string(),
            "mint".to_string()
        ]
    );
    assert_eq!(
        dev.metadata.get("federation_name").unwrap(),
        "Dev Test Federation"
    );
    assert_eq!(dev.metadata.get("name").unwrap(), "Dev Test Federation");
    assert_eq!(dev.metadata.get("network").unwrap(), "regtest");
}

#[test]
fn test_content_does_not_overwrite_reserved_metadata_keys() {
    let json = format!(
        r#"{{
            "kind": 38173,
            "pubkey": "real_pubkey_123",
            "content": "{{\"publisher_pubkey\": \"spoofed_pubkey\", \"modules\": \"spoofed_module\", \"network\": \"mainnet\", \"custom_attr\": \"ok\"}}",
            "tags": [
                ["d", "{CODE_FEDERATION_ID}"],
                ["u", "{CODE}"],
                ["modules", "mint,wallet"],
                ["n", "signet"]
            ]
        }}"#
    );
    let raw: RawNip87Event = serde_json::from_str(&json).unwrap();
    let fed = DiscoveredFederation::try_from(raw).unwrap();
    assert_eq!(
        fed.metadata.get("publisher_pubkey").unwrap(),
        "real_pubkey_123"
    );
    assert_eq!(fed.metadata.get("modules").unwrap(), "mint,wallet");
    assert_eq!(fed.metadata.get("network").unwrap(), "signet");
    assert_eq!(fed.metadata.get("custom_attr").unwrap(), "ok");
}

#[test]
fn test_reserved_metadata_keys_rejected_from_content_when_tags_absent() {
    let json = format!(
        r#"{{
            "kind": 38173,
            "content": "{{\"publisher_pubkey\": \"spoofed_pubkey\", \"modules\": \"spoofed_module\", \"network\": \"spoofed_net\", \"custom_attr\": \"ok\"}}",
            "tags": [
                ["d", "{CODE_FEDERATION_ID}"],
                ["u", "{CODE}"]
            ]
        }}"#
    );
    let raw: RawNip87Event = serde_json::from_str(&json).unwrap();
    let fed = DiscoveredFederation::try_from(raw).unwrap();
    assert!(!fed.metadata.contains_key("publisher_pubkey"));
    assert!(!fed.metadata.contains_key("modules"));
    assert!(!fed.metadata.contains_key("network"));
    assert_eq!(fed.metadata.get("custom_attr").unwrap(), "ok");
}

fn sign_test_event(
    keypair: &fedimint_core::secp256k1::Keypair,
    created_at: u64,
    kind: u64,
    tags: Vec<Vec<String>>,
    content: String,
) -> RawNip87Event {
    let (xonly, _) = keypair.x_only_public_key();
    let pubkey_hex = xonly.to_string();
    let serialized =
        serde_json::to_string(&(0, &pubkey_hex, created_at, kind, &tags, &content)).unwrap();
    use fedimint_core::bitcoin::hashes::{Hash, sha256};
    let id_hash = sha256::Hash::hash(serialized.as_bytes());
    let id_hex = format!("{id_hash:x}");
    let msg = fedimint_core::secp256k1::Message::from_digest(id_hash.to_byte_array());
    let sig = fedimint_core::secp256k1::SECP256K1.sign_schnorr(&msg, keypair);

    RawNip87Event {
        id: Some(id_hex),
        pubkey: Some(pubkey_hex),
        created_at: Some(created_at),
        kind: Some(kind),
        tags,
        content,
        sig: Some(sig.to_string()),
    }
}

#[test]
fn test_verify_signature_valid() {
    let secp = fedimint_core::secp256k1::Secp256k1::new();
    let sk = fedimint_core::secp256k1::SecretKey::from_slice(&[42u8; 32]).unwrap();
    let keypair = fedimint_core::secp256k1::Keypair::from_secret_key(&secp, &sk);

    let event = sign_test_event(
        &keypair,
        1700000000,
        38173,
        vec![
            vec!["d".to_string(), CODE_FEDERATION_ID.to_string()],
            vec!["u".to_string(), CODE.to_string()],
        ],
        r#"{"name": "Valid Signed Fed"}"#.to_string(),
    );

    assert!(event.verify_signature());
}

#[test]
fn test_verify_signature_tampered_id() {
    let secp = fedimint_core::secp256k1::Secp256k1::new();
    let sk = fedimint_core::secp256k1::SecretKey::from_slice(&[42u8; 32]).unwrap();
    let keypair = fedimint_core::secp256k1::Keypair::from_secret_key(&secp, &sk);

    let mut event = sign_test_event(
        &keypair,
        1700000000,
        38173,
        vec![vec!["d".to_string(), CODE_FEDERATION_ID.to_string()]],
        "content".to_string(),
    );
    event.id = Some("1111111111111111111111111111111111111111111111111111111111111111".to_string());
    assert!(!event.verify_signature());
}

#[test]
fn test_verify_signature_tampered_content() {
    let secp = fedimint_core::secp256k1::Secp256k1::new();
    let sk = fedimint_core::secp256k1::SecretKey::from_slice(&[42u8; 32]).unwrap();
    let keypair = fedimint_core::secp256k1::Keypair::from_secret_key(&secp, &sk);

    let mut event = sign_test_event(
        &keypair,
        1700000000,
        38173,
        vec![vec!["d".to_string(), CODE_FEDERATION_ID.to_string()]],
        "legitimate content".to_string(),
    );
    event.content = "tampered content".to_string();
    assert!(!event.verify_signature());
}

#[test]
fn test_verify_signature_corrupted_sig() {
    let secp = fedimint_core::secp256k1::Secp256k1::new();
    let sk = fedimint_core::secp256k1::SecretKey::from_slice(&[42u8; 32]).unwrap();
    let keypair = fedimint_core::secp256k1::Keypair::from_secret_key(&secp, &sk);

    let mut event = sign_test_event(
        &keypair,
        1700000000,
        38173,
        vec![vec!["d".to_string(), CODE_FEDERATION_ID.to_string()]],
        "content".to_string(),
    );
    event.sig = Some("00".repeat(64));
    assert!(!event.verify_signature());
}

#[test]
fn test_verify_signature_missing_sig() {
    let raw = RawNip87Event {
        id: Some("11".repeat(32)),
        pubkey: Some("22".repeat(32)),
        created_at: Some(12345),
        kind: Some(38173),
        tags: vec![],
        content: "".to_string(),
        sig: None,
    };
    assert!(!raw.verify_signature());
}
