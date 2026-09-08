//! Integration tests against a live `devimint` federation.
//!
//! # Integration over mocking
//!
//! This crate deliberately tests against a real federation instead of mocking `fedimint-client`.
//! The client's behavior — consensus rounds, peg-ins, gateway routing, module state machines — is
//! not something a hand-rolled mock can stand in for without silently drifting from the real
//! thing, and this SDK's whole purpose is to be a thin, faithful facade over that client.
//!
//! # Running them
//!
//! ```sh
//! nix develop --accept-flake-config .#wasm-tests -c scripts/run-sdk-integration-tests.sh v1
//! ```
//!
//! That shell is the only one with `devimint`, `fedimintd`, `gatewayd`, `bitcoind`, `lnd`,
//! `esplora` and the `recurringd` binaries on `PATH`. Without a federation every test in this
//! file returns early with a notice, so `cargo test --locked` stays green in a plain checkout;
//! the wrapper sets `FM_SDK_REQUIRE_DEVIMINT=1`, which turns a missing federation into a failure
//! instead.
//!
//! # Module generation matters for this SDK specifically
//!
//! Today's fedimint defaults a federation to its v2 mint, wallet and lightning modules, and this
//! SDK enforces a rule devimint does not: a federation must be all-v1 or all-v2, and one mixing
//! generations is rejected outright rather than merely mishandled. The wrapper stands up three
//! shapes for that reason, and reports which one it built in `FM_SDK_SHAPE`.

use std::io::{Read, Write};

use fedimint_sdk::{ErrorCode, ErrorDetails, Sdk, Storage};

/// Everything devimint hands this process. `None` when not running under devimint.
#[derive(Debug)]
struct Devimint {
    invite: String,
    shape: String,
}

impl Devimint {
    /// Finds the federation, in devimint's own order of preference.
    fn detect() -> Option<Devimint> {
        let shape = std::env::var("FM_SDK_SHAPE").unwrap_or_else(|_| "v1".to_owned());
        let invite = invite_from_env()
            .or_else(invite_from_client_dir)
            .or_else(invite_from_faucet)?;
        Some(Devimint {
            invite: invite.trim().to_owned(),
            shape,
        })
    }
}

/// Set by `dev-fed --exec`, but not by `wasm-test-setup`.
fn invite_from_env() -> Option<String> {
    std::env::var("FM_INVITE_CODE")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// devimint's own accessor: the file it copies out of peer 0's data directory.
fn invite_from_client_dir() -> Option<String> {
    let dir = std::env::var("FM_CLIENT_DIR").ok()?;
    std::fs::read_to_string(std::path::Path::new(&dir).join("invite-code")).ok()
}

/// The faucet's `GET /connect-string`, for a setup that exposes only that.
///
/// A four-line HTTP/1.0 request rather than a client crate: this is the one network call the test
/// harness makes, and it is not worth a dependency in the lockfile the crate ships.
fn invite_from_faucet() -> Option<String> {
    use std::net::ToSocketAddrs;
    use std::time::Duration;

    // "localhost" rather than a hardcoded 127.0.0.1: a devimint host that only listens on ::1
    // still connects, and each candidate address gets its own bounded attempt.
    let port: u16 = std::env::var("FM_PORT_FAUCET").ok()?.parse().ok()?;
    let mut stream = ("localhost", port)
        .to_socket_addrs()
        .ok()?
        .find_map(|addr| {
            std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(5)).ok()
        })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .ok()?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .ok()?;
    stream
        .write_all(b"GET /connect-string HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let (_headers, body) = response.split_once("\r\n\r\n")?;
    Some(body.to_owned())
}

/// Returns the federation, or leaves the test early with a notice.
///
/// An early return rather than `#[ignore]`: a genuinely broken harness would look exactly like
/// "not requested" if the wrapper had to pass `--ignored`, and this way one `cargo test --locked`
/// covers both situations. `FM_SDK_REQUIRE_DEVIMINT` is what keeps the skip from being silent
/// where a federation was meant to be there.
macro_rules! devimint {
    () => {
        match Devimint::detect() {
            Some(devimint) => devimint,
            None => {
                assert!(
                    std::env::var_os("FM_SDK_REQUIRE_DEVIMINT").is_none(),
                    "FM_SDK_REQUIRE_DEVIMINT is set but no devimint federation was found; \
                     run scripts/run-sdk-integration-tests.sh"
                );
                eprintln!("skipping: not running under devimint");
                return;
            }
        }
    };
}

#[tokio::test(flavor = "multi_thread")]
async fn joins_previews_and_reads_zero_balance() {
    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    let invite = devimint
        .invite
        .parse()
        .expect("devimint's invite code parses");

    let storage = tempfile::tempdir().expect("a temporary directory");
    let path = storage.path().to_str().expect("a utf-8 path");
    let sdk = Sdk::builder()
        .storage(Storage::at(path).expect("a valid path"))
        .build()
        .await
        .expect("an instance opens on a fresh directory");

    // Preview writes nothing and does not join.
    let preview = sdk.preview(&invite).await.expect("the federation previews");
    assert!(preview.guardians >= 1);
    assert!(!preview.modules.is_empty());
    assert!(
        sdk.stored_federations().is_empty(),
        "a preview joins nothing"
    );

    let federation = sdk.join(&invite).await.expect("the federation joins");
    assert_eq!(federation.id(), preview.id);
    assert_eq!(federation.network(), preview.network);
    assert_eq!(
        federation
            .balance()
            .await
            .expect("a fresh wallet reports a balance"),
        fedimint_sdk::Amount::from_msats(0)
    );

    // Joining twice is refused, closed or not.
    let err = sdk
        .join(&invite)
        .await
        .expect_err("a second join is refused");
    assert_eq!(err.code, ErrorCode::AlreadyJoined);

    sdk.shutdown().await.expect("the instance shuts down");
}

#[tokio::test(flavor = "multi_thread")]
async fn stored_federation_survives_restart() {
    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    let invite: fedimint_sdk::InviteCode = devimint
        .invite
        .parse()
        .expect("devimint's invite code parses");

    let storage = tempfile::tempdir().expect("a temporary directory");
    let path = storage.path().to_str().expect("a utf-8 path").to_owned();

    let (id, words) = {
        let sdk = Sdk::builder()
            .storage(Storage::at(&path).expect("a valid path"))
            .build()
            .await
            .expect("an instance opens");
        let federation = sdk.join(&invite).await.expect("the federation joins");
        let id = federation.id();
        let words = sdk.export_mnemonic().words();
        sdk.shutdown().await.expect("the instance shuts down");
        // Dropped before the second build: the embedded store's own file lock lives with the
        // handle, and `shutdown` releases this SDK's lock, not that one.
        drop(federation);
        drop(sdk);
        (id, words)
    };

    let reopened = Sdk::builder()
        .storage(Storage::at(&path).expect("a valid path"))
        .build()
        .await
        .expect("the instance reopens");
    assert_eq!(
        reopened.export_mnemonic().words(),
        words,
        "the seed is the same one"
    );
    let stored = reopened.stored_federations();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id, id);
    assert_eq!(stored[0].status, fedimint_sdk::FederationStatus::Running);
    let federation = reopened
        .federation(&id)
        .expect("the federation came back open");
    assert_eq!(
        federation
            .balance()
            .await
            .expect("the reopened wallet reports a balance"),
        fedimint_sdk::Amount::from_msats(0)
    );
    reopened.shutdown().await.expect("the instance shuts down");
}

#[tokio::test(flavor = "multi_thread")]
async fn reopening_replaces_the_handle_and_leaves_the_old_one_closed() {
    // `Sdk::reopen_federation` documents that handles taken before the federation stopped are
    // not revived and keep failing, and that the handle it returns is the live one. It is only
    // observable against a real federation, because a reopen has to open a client to succeed.
    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    let invite: fedimint_sdk::InviteCode = devimint
        .invite
        .parse()
        .expect("devimint's invite code parses");

    let storage = tempfile::tempdir().expect("a temporary directory");
    let path = storage.path().to_str().expect("a utf-8 path");
    let sdk = Sdk::builder()
        .storage(Storage::at(path).expect("a valid path"))
        .build()
        .await
        .expect("an instance opens");

    let stale = sdk.join(&invite).await.expect("the federation joins");
    let id = stale.id();
    sdk.close_federation(&id)
        .await
        .expect("the federation closes");
    let err = stale.balance().await.expect_err("a closed handle refuses");
    assert_eq!(err.code, ErrorCode::FederationClosed);

    let reopened = sdk
        .reopen_federation(&id)
        .await
        .expect("the federation reopens");
    assert_eq!(
        reopened.balance().await.expect("the live handle answers"),
        fedimint_sdk::Amount::from_msats(0)
    );

    // The handle from before the close is not brought back to life by the reopen.
    let err = stale
        .balance()
        .await
        .expect_err("the stale handle still refuses");
    assert_eq!(err.code, ErrorCode::FederationClosed);

    sdk.shutdown().await.expect("the instance shuts down");
}

#[tokio::test(flavor = "multi_thread")]
async fn mixed_generation_federation_is_rejected() {
    let devimint = devimint!();
    if devimint.shape != "mixed" {
        eprintln!("skipping: this needs the mixed shape, run the wrapper with `mixed`");
        return;
    }
    let invite: fedimint_sdk::InviteCode = devimint
        .invite
        .parse()
        .expect("devimint's invite code parses");

    let sdk = Sdk::builder()
        .storage(Storage::in_memory())
        .build()
        .await
        .expect("an in-memory instance opens");

    // The refusal happens at preview, not after joining: a federation this SDK could not operate
    // on is never previewed and then refused.
    let err = sdk
        .preview(&invite)
        .await
        .expect_err("a mixed federation is refused");
    assert_eq!(err.code, ErrorCode::UnsupportedFederation);
    match err.detail() {
        Some(ErrorDetails::MixedModuleGenerations { modules }) => {
            let named: Vec<(&str, u32)> = modules
                .iter()
                .map(|module| (module.kind.as_str(), module.generation))
                .collect();
            assert!(named.contains(&("ln", 1)), "{named:?}");
            assert!(named.contains(&("lnv2", 2)), "{named:?}");
        }
        other => panic!("expected the conflicting modules, got {other:?}"),
    }

    let err = sdk
        .join(&invite)
        .await
        .expect_err("and it is refused at join too");
    assert_eq!(err.code, ErrorCode::UnsupportedFederation);
    assert!(
        sdk.stored_federations().is_empty(),
        "a refused join writes nothing"
    );

    sdk.shutdown().await.expect("the instance shuts down");
}
