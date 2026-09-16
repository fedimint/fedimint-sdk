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
fn invite_from_faucet() -> Option<String> {
    faucet("GET", "/connect-string", "")
}

/// One request to devimint's faucet, which is also this suite's counterparty on the lightning
/// network: `POST /pay` pays an invoice from a node outside the federation, `POST /invoice`
/// issues one.
///
/// A hand-written HTTP/1.0 exchange rather than a client crate: these are the only network calls
/// the harness makes, and they are not worth a dependency in the lockfile the crate ships.
fn faucet(method: &str, path: &str, body: &str) -> Option<String> {
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
    // Paying an invoice waits for the payment to settle, which can take a while on a fresh
    // channel.
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .ok()?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .ok()?;
    let request = format!(
        "{method} {path} HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\
         Content-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let (headers, body) = response.split_once("\r\n\r\n")?;
    let status = headers.lines().next()?;
    assert!(
        status.contains(" 200 "),
        "faucet {method} {path} answered {status}: {body}"
    );
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

/// Builds an instance on a fresh directory and joins devimint's federation.
///
/// The directory is returned so it outlives the instance; a caller that reopens the same storage
/// keeps it and builds again over `path`.
async fn joined(devimint: &Devimint) -> (tempfile::TempDir, String, Sdk, fedimint_sdk::Federation) {
    let invite: fedimint_sdk::InviteCode = devimint
        .invite
        .parse()
        .expect("devimint's invite code parses");
    let storage = tempfile::tempdir().expect("a temporary directory");
    let path = storage.path().to_str().expect("a utf-8 path").to_owned();
    let sdk = Sdk::builder()
        .storage(Storage::at(&path).expect("a valid path"))
        .build()
        .await
        .expect("an instance opens on a fresh directory");
    let federation = sdk.join(&invite).await.expect("the federation joins");
    (storage, path, sdk, federation)
}

/// Funds the wallet by having the faucet pay an invoice this SDK issued, and returns what landed.
async fn fund(lightning: &fedimint_sdk::Lightning, msats: u64) -> fedimint_sdk::Amount {
    let receive = lightning
        .receive(fedimint_sdk::Amount::from_msats(msats), "funding")
        .await
        .expect("an invoice is issued");
    let paid = faucet("POST", "/pay", &receive.invoice.to_string());
    assert!(paid.is_some(), "the faucet pays the invoice");
    let state = receive
        .operation
        .await_final()
        .await
        .expect("the receive settles");
    assert_eq!(state, fedimint_sdk::LnReceiveState::Claimed);
    receive
        .operation
        .details()
        .await
        .expect("details")
        .net_credit
}

/// Waits until the federation's balance reads `expected`, through a fresh balance stream, and
/// panics with the last figure seen if it has not within a minute.
///
/// A recovered wallet's notes are re-signed by state machines that resume on the client the
/// recovery's end swaps in, so the balance can land a moment after the recovery reads `Done`.
async fn balance_settles_at(federation: &fedimint_sdk::Federation, expected: fedimint_sdk::Amount) {
    let mut updates = federation.balance_updates();
    settles_at(&mut updates, expected).await;
}

/// Waits on an existing balance stream until it yields `expected`, and panics with the last
/// figure seen if it has not within a minute.
async fn settles_at(updates: &mut fedimint_sdk::BalanceUpdates, expected: fedimint_sdk::Amount) {
    let mut last = None;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let next = tokio::time::timeout(remaining, updates.next()).await;
        match next {
            Ok(Ok(balance)) if balance == expected => return,
            Ok(Ok(balance)) => last = Some(balance),
            Ok(Err(err)) => panic!("the balance stream failed: {err:?}"),
            Err(_) => panic!("the balance did not reach {expected:?}; last seen {last:?}"),
        }
    }
}

/// On the lnv2 shape, leaves the LND gateway as the guardian's only lnv2 gateway, once per test
/// process.
///
/// The lnv2 client picks a gateway at random from the guardians' list on purpose, and devimint's
/// `wasm-test-setup` funds only the LND gateway's ecash, so a receive through either LDK gateway
/// cannot be funded and a send through the faucet's own LDK gateway to the faucet's invoice is a
/// self-payment. Removing the two LDK entries through the same admin call devimint used to add
/// them makes the SDK's choice the funded gateway, and leaves the faucet's LDK node as the
/// counterparty, as on the v1 shape.
fn pin_lnv2_gateway_to_lnd(devimint: &Devimint) {
    if devimint.shape != "v2" {
        return;
    }
    static PINNED: std::sync::Once = std::sync::Once::new();
    PINNED.call_once(|| {
        // The wrapper always sets these when it runs a federation for these tests; a run
        // outside it (a plain `cargo test`) never reaches this function, because every
        // caller checks `FM_SDK_SHAPE` through `Devimint::detect` first.
        let mint_client = std::env::var("FM_MINT_CLIENT")
            .unwrap_or_else(|err| panic!("FM_MINT_CLIENT is not set ({err}); run under devimint"));
        let lnd_port = std::env::var("FM_PORT_GW_LND")
            .unwrap_or_else(|err| panic!("FM_PORT_GW_LND is not set ({err}); run under devimint"));
        let mut words = mint_client.split_whitespace();
        let program = words
            .next()
            .expect("FM_MINT_CLIENT names at least a program");
        let base_args: Vec<&str> = words.collect();

        // `--our-id 0`: the harness always runs a single guardian (`FM_FED_SIZE=1`).
        // `pass` is the admin password devimint sets everywhere
        // (`fedimint_testing_core::config::API_AUTH`).
        let run_admin = |admin_args: &[&str]| -> std::process::Output {
            std::process::Command::new(program)
                .args(&base_args)
                .args(["--our-id", "0", "--password", "pass"])
                .args(admin_args)
                .output()
                .unwrap_or_else(|err| panic!("could not run `{program}`: {err}"))
        };
        let expect_success = |output: &std::process::Output, what: &str| {
            assert!(
                output.status.success(),
                "{what} failed: stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        };

        let listed = run_admin(&["module", "lnv2", "gateways", "list"]);
        expect_success(&listed, "listing the guardian's lnv2 gateways");
        let stdout = String::from_utf8_lossy(&listed.stdout).into_owned();
        // `serde_json` is not a dev-dependency of this crate, and the list is a JSON array of
        // plain URL strings, so it is cheaper to pick the quoted tokens out by hand than to add
        // one for this alone.
        let lnd_needle = format!(":{lnd_port}/");
        let urls: Vec<&str> = stdout
            .split('"')
            .filter(|token| token.starts_with("http"))
            .collect();
        assert!(
            urls.iter().any(|url| url.contains(&lnd_needle)),
            "the LND gateway is not among the guardian's lnv2 gateways: {stdout}"
        );
        for url in urls.into_iter().filter(|url| !url.contains(&lnd_needle)) {
            let removed = run_admin(&["module", "lnv2", "gateways", "remove", url]);
            expect_success(&removed, &format!("removing the lnv2 gateway {url}"));
        }
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn lightning_receive_is_paid_by_the_faucet_and_survives_a_restart() {
    use fedimint_sdk::{Amount, LnReceiveState, OperationKind};

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    pin_lnv2_gateway_to_lnd(&devimint);
    let (_storage, path, sdk, federation) = joined(&devimint).await;
    let lightning = federation
        .lightning()
        .expect("devimint runs a lightning module");

    let receive = lightning
        .receive(Amount::from_msats(100_000), "coffee")
        .await
        .expect("an invoice is issued");
    let id = receive.operation.id();
    let details = receive.operation.details().await.expect("details");
    assert_eq!(details.invoice, receive.invoice);
    assert_eq!(details.description, "coffee");
    assert_eq!(details.requested_amount, Amount::from_msats(100_000));
    assert_eq!(details.invoice_amount, Amount::from_msats(100_000));
    assert_eq!(
        details.net_credit.checked_add(details.fee),
        Some(details.invoice_amount)
    );
    assert!(details.gateway_id.is_some(), "a gateway took the invoice");
    assert!(details.expires_at > details.created_at);
    assert_eq!(receive.invoice.amount(), Some(Amount::from_msats(100_000)));
    assert_eq!(receive.invoice.description(), "coffee");

    let mut updates = receive.operation.updates();
    let first = updates.next().await.expect("a state").expect("a state");
    assert!(
        matches!(
            first,
            LnReceiveState::Created | LnReceiveState::WaitingForPayment
        ),
        "{first:?}"
    );

    assert!(faucet("POST", "/pay", &receive.invoice.to_string()).is_some());
    let last = receive.operation.await_final().await.expect("settles");
    assert_eq!(last, LnReceiveState::Claimed);
    assert_eq!(
        federation.balance().await.expect("balance"),
        details.net_credit
    );
    // A subscription opened before the payment ends cleanly after it.
    let mut seen = vec![first];
    while let Some(state) = updates.next().await.expect("a state") {
        seen.push(state);
    }
    assert_eq!(seen.last(), Some(&LnReceiveState::Claimed));

    // Reattaching by id, same process.
    let any = federation
        .operation(&id)
        .await
        .expect("lookup")
        .expect("recorded");
    assert_eq!(any.kind(), OperationKind::LnReceive);
    let typed = any.as_ln_receive().expect("a typed handle");
    assert_eq!(typed.details().await.expect("details"), details);
    assert_eq!(typed.state().await.expect("state"), LnReceiveState::Claimed);

    // And after a restart.
    let federation_id = federation.id();
    sdk.shutdown().await.expect("shuts down");
    // Every handle goes before the second build, as in `stored_federation_survives_restart`: a
    // subscriber and a reattached operation hold the federation's store open exactly as the
    // federation handle does, and the embedded store's own lock waits for all of them.
    drop(updates);
    drop(typed);
    drop(any);
    drop(lightning);
    drop(federation);
    drop(receive);
    drop(sdk);
    let reopened = Sdk::builder()
        .storage(Storage::at(&path).expect("a valid path"))
        .build()
        .await
        .expect("reopens");
    let federation = reopened.federation(&federation_id).expect("still there");
    let any = federation
        .operation(&id)
        .await
        .expect("lookup")
        .expect("still recorded");
    let typed = any.as_ln_receive().expect("a typed handle");
    assert_eq!(typed.details().await.expect("details"), details);
    assert_eq!(typed.state().await.expect("state"), LnReceiveState::Claimed);
    assert_eq!(
        federation.balance().await.expect("balance"),
        details.net_credit
    );
    reopened.shutdown().await.expect("shuts down");
}

#[tokio::test(flavor = "multi_thread")]
async fn lightning_send_pays_an_invoice_from_outside_the_federation() {
    use fedimint_sdk::{Amount, LightningRoute, LnSendState, OperationKind};

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    pin_lnv2_gateway_to_lnd(&devimint);
    let (_storage, _path, sdk, federation) = joined(&devimint).await;
    let lightning = federation
        .lightning()
        .expect("devimint runs a lightning module");
    let funded = fund(&lightning, 200_000).await;

    let invoice: fedimint_sdk::Bolt11Invoice = faucet("POST", "/invoice", "50000")
        .expect("the faucet issues an invoice")
        .trim()
        .parse()
        .expect("a bolt11 invoice");
    let quote = lightning.quote(&invoice).await.expect("a quote");
    assert_eq!(quote.invoice_amount(), Amount::from_msats(50_000));
    assert_eq!(
        quote.invoice_amount().checked_add(quote.fee()),
        Some(quote.total())
    );
    let breakdown = quote.fee_breakdown();
    let summed = [
        breakdown.gateway,
        breakdown.lightning_module,
        breakdown.primary_module,
        breakdown.dust,
    ]
    .into_iter()
    .try_fold(Amount::from_msats(0), Amount::checked_add);
    assert_eq!(summed, Some(quote.fee()));
    assert!(
        matches!(quote.route(), LightningRoute::Gateway { .. }),
        "an outside payee goes through a gateway"
    );
    assert!(quote.expires_at() > fedimint_sdk::Timestamp::from_epoch_millis(0));
    let total = quote.total();
    let fee = quote.fee();
    let route = quote.route();

    let operation = lightning.send(quote).await.expect("the payment starts");
    let id = operation.id();
    let details = operation.details().await.expect("details");
    assert_eq!(details.invoice, invoice);
    assert_eq!(details.invoice_amount, Amount::from_msats(50_000));
    assert_eq!(details.fee, fee);
    assert_eq!(details.total, total);
    assert_eq!(details.route, route);

    if devimint.shape == "v1" {
        // fedimint/fedimint#8969: the v1 client strips two characters off the preimage the
        // gateway returns, so the SDK cannot decode the success state. The payment itself goes
        // through; only its observation fails, and the record and the reattached handle are
        // still checked below. Remove this branch once the pin includes the fix.
        let err = operation.await_final().await.expect_err("fedimint#8969");
        assert_eq!(err.code, ErrorCode::Internal);
        assert!(err.message.contains("preimage"), "{}", err.message);
    } else {
        let last = operation.await_final().await.expect("settles");
        match last {
            LnSendState::Success {
                preimage,
                fee: reported_fee,
                route: reported_route,
            } => {
                assert_eq!(reported_fee, fee);
                assert_eq!(reported_route, route);
                assert_eq!(preimage.to_string().len(), 64);
            }
            other => panic!("expected Success, got {other:?}"),
        }
        assert_eq!(
            federation.balance().await.expect("balance"),
            funded.checked_sub(total).expect("the total was debited")
        );
    }

    let any = federation
        .operation(&id)
        .await
        .expect("lookup")
        .expect("recorded");
    assert_eq!(any.kind(), OperationKind::LnSend);
    let typed = any.as_ln_send().expect("a typed handle");
    assert_eq!(typed.details().await.expect("details"), details);
    if devimint.shape == "v1" {
        // Same cause as above: `state()` replays the same undecodable stream.
        let err = typed.state().await.expect_err("fedimint#8969");
        assert_eq!(err.code, ErrorCode::Internal);
    } else {
        assert!(matches!(
            typed.state().await.expect("state"),
            LnSendState::Success { .. }
        ));
    }
    sdk.shutdown().await.expect("shuts down");
}

#[tokio::test(flavor = "multi_thread")]
async fn lightning_quote_refuses_what_cannot_be_paid() {
    use fedimint_sdk::{Amount, Network};

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    pin_lnv2_gateway_to_lnd(&devimint);
    let (_storage, _path, sdk, federation) = joined(&devimint).await;
    let lightning = federation
        .lightning()
        .expect("devimint runs a lightning module");

    // A mainnet invoice with no amount: the amount is refused first.
    let amountless: fedimint_sdk::Bolt11Invoice = "lnbc1pj48ugqdq0dehjqctdda6kuaqpp5yg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3qsp5xvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxves9qrsgqcqzyswm4efuu52zkzgrcc35fra9fmvj7s9ppxmej85s83hjkh7crcy9vqlradwalsmq40knf3552panjvlhjlrfazmvs86krxuaygut8v30sq0y0422".parse().expect("valid");
    assert_eq!(
        lightning
            .quote(&amountless)
            .await
            .expect_err("refused")
            .code,
        ErrorCode::AmountlessInvoice
    );

    // A mainnet invoice with an amount: the network is refused, with details.
    let mainnet: fedimint_sdk::Bolt11Invoice = "lnbc25m1pvjluezpp5qqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqypqdq5vdhkven9v5sxyetpdeessp5zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zygs9q5sqqqqqqqqqqqqqqqpqsq67gye39hfg3zd8rgc80k32tvy9xk2xunwm5lzexnvpx6fd77en8qaq424dxgt56cag2dpt359k3ssyhetktkpqh24jqnjyw6uqd08sgptq44qu".parse().expect("valid");
    let err = lightning.quote(&mainnet).await.expect_err("refused");
    assert_eq!(err.code, ErrorCode::NetworkMismatch);
    match err.detail() {
        Some(ErrorDetails::NetworkMismatch {
            expected,
            compatible,
            observed_prefix,
        }) => {
            assert_eq!(*expected, Network::Regtest);
            assert_eq!(compatible, &vec![Network::Bitcoin]);
            assert_eq!(observed_prefix, "bc");
        }
        other => panic!("expected NetworkMismatch details, got {other:?}"),
    }

    // A fresh regtest invoice on an empty wallet: everything upstream succeeds and the balance
    // is what refuses it, naming both numbers.
    let invoice: fedimint_sdk::Bolt11Invoice = faucet("POST", "/invoice", "50000")
        .expect("the faucet issues an invoice")
        .trim()
        .parse()
        .expect("a bolt11 invoice");
    let err = lightning
        .quote(&invoice)
        .await
        .expect_err("nothing to pay with");
    assert_eq!(err.code, ErrorCode::InsufficientBalance);
    match err.detail() {
        Some(ErrorDetails::InsufficientBalance {
            required,
            available,
        }) => {
            assert!(*required >= Amount::from_msats(50_000));
            assert_eq!(*available, Amount::from_msats(0));
        }
        other => panic!("expected InsufficientBalance details, got {other:?}"),
    }

    // A zero-amount receive and an over-long description are refused before any gateway is
    // asked.
    assert_eq!(
        lightning
            .receive(Amount::from_msats(0), "nothing")
            .await
            .expect_err("refused")
            .code,
        ErrorCode::InvalidInput
    );
    assert_eq!(
        lightning
            .receive(Amount::from_msats(1_000), &"x".repeat(640))
            .await
            .expect_err("refused")
            .code,
        ErrorCode::InvalidInput
    );
    sdk.shutdown().await.expect("shuts down");
}

#[tokio::test(flavor = "multi_thread")]
async fn lightning_send_refuses_a_quote_used_twice() {
    use fedimint_sdk::LnSendState;

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    pin_lnv2_gateway_to_lnd(&devimint);
    let (_storage, _path, sdk, federation) = joined(&devimint).await;
    let lightning = federation
        .lightning()
        .expect("devimint runs a lightning module");
    let funded = fund(&lightning, 200_000).await;

    let invoice: fedimint_sdk::Bolt11Invoice = faucet("POST", "/invoice", "10000")
        .expect("the faucet issues an invoice")
        .trim()
        .parse()
        .expect("a bolt11 invoice");
    let first = lightning.quote(&invoice).await.expect("a quote");
    let second = lightning.quote(&invoice).await.expect("a second quote");
    let operation = lightning.send(first).await.expect("the payment starts");
    if devimint.shape == "v1" {
        // fedimint/fedimint#8969: the v1 client strips two characters off the preimage the
        // gateway returns, so `await_final` cannot decode the success state. The payment itself
        // goes through, so the second send below is still refused as already executed; the
        // balance drop confirms the first payment settled since `await_final` cannot.
        let _ = operation.await_final().await.expect_err("fedimint#8969");
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut balance = federation.balance().await.expect("balance");
        while balance >= funded && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            balance = federation.balance().await.expect("balance");
        }
        assert!(balance < funded, "the payment never debited the balance");
    } else {
        assert!(matches!(
            operation.await_final().await.expect("settles"),
            LnSendState::Success { .. }
        ));
    }
    // The invoice is paid; a second quote for it is refused as already executed.
    let err = lightning.send(second).await.expect_err("already paid");
    assert_eq!(err.code, ErrorCode::QuoteExpired);
    match err.detail() {
        Some(ErrorDetails::QuoteExpired {
            already_executed, ..
        }) => assert!(already_executed),
        other => panic!("expected QuoteExpired details, got {other:?}"),
    }
    sdk.shutdown().await.expect("shuts down");
}

#[tokio::test(flavor = "multi_thread")]
async fn activity_lists_what_the_federation_was_used_for() {
    use fedimint_sdk::{ActivityStatus, Amount, Cursor, Direction, OperationKind};

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    pin_lnv2_gateway_to_lnd(&devimint);
    let (_storage, path, sdk, federation) = joined(&devimint).await;
    let lightning = federation
        .lightning()
        .expect("devimint runs a lightning module");

    let empty = federation.activity(None, 10).await.expect("an empty page");
    assert!(empty.items.is_empty());
    assert!(empty.next.is_none());

    let funded = fund(&lightning, 200_000).await;

    let invoice: fedimint_sdk::Bolt11Invoice = faucet("POST", "/invoice", "50000")
        .expect("the faucet issues an invoice")
        .trim()
        .parse()
        .expect("a bolt11 invoice");
    let quote = lightning.quote(&invoice).await.expect("a quote");
    let fee = quote.fee();
    let send = lightning.send(quote).await.expect("the payment starts");
    let send_id = send.id();

    if devimint.shape == "v1" {
        // fedimint/fedimint#8969: the v1 client strips two characters off the preimage the
        // gateway returns, so the SDK cannot decode the success state. The payment itself goes
        // through; only its observation fails.
        let err = send.await_final().await.expect_err("fedimint#8969");
        assert_eq!(err.code, ErrorCode::Internal);
        assert!(err.message.contains("preimage"), "{}", err.message);
    } else {
        send.await_final().await.expect("settles");
    }

    let history = federation.activity(None, 10).await.expect("both rows");
    assert_eq!(history.items.len(), 2);
    assert!(history.next.is_none());
    let send_row = &history.items[0];
    let receive_row = &history.items[1];

    assert_eq!(send_row.operation_id, send_id);
    assert_eq!(send_row.kind, OperationKind::LnSend);
    if devimint.shape == "v1" {
        // The record has no final state and the current one cannot be read
        // (fedimint/fedimint#8969, as above): the outcome is unknown, not pending, and an
        // unknown outcome carries no figures.
        assert_eq!(send_row.status, ActivityStatus::Unknown);
        assert!(!send_row.is_final);
        assert_eq!(send_row.direction, None);
        assert_eq!(send_row.amount, None);
        assert_eq!(send_row.fee, None);
    } else {
        assert_eq!(send_row.status, ActivityStatus::Success);
        assert!(send_row.is_final);
        assert_eq!(send_row.direction, Some(Direction::Outgoing));
        assert_eq!(send_row.amount, Some(Amount::from_msats(50_000)));
        assert_eq!(send_row.fee, Some(fee));
    }

    assert_eq!(receive_row.kind, OperationKind::LnReceive);
    assert_eq!(receive_row.direction, Some(Direction::Incoming));
    assert_eq!(receive_row.amount, Some(Amount::from_msats(200_000)));
    assert_eq!(
        receive_row.fee,
        Amount::from_msats(200_000).checked_sub(funded)
    );
    assert_eq!(receive_row.status, ActivityStatus::Success);
    assert!(receive_row.is_final);
    assert!(send_row.time >= receive_row.time);

    // Paging: one row per page, the send first, then the receive with no cursor left after it.
    let page_one = federation.activity(None, 1).await.expect("first page");
    assert_eq!(page_one.items.len(), 1);
    assert_eq!(page_one.items[0].operation_id, send_id);
    let cursor = page_one.next.expect("the receive row remains");
    let round_tripped: Cursor = cursor.to_string().parse().expect("the cursor round-trips");
    let page_two = federation
        .activity(Some(round_tripped), 1)
        .await
        .expect("second page");
    assert_eq!(page_two.items.len(), 1);
    assert_eq!(page_two.items[0].kind, OperationKind::LnReceive);
    assert!(page_two.next.is_none());

    // Restart, every handle dropped first, as in
    // `lightning_receive_is_paid_by_the_faucet_and_survives_a_restart`.
    let federation_id = federation.id();
    sdk.shutdown().await.expect("shuts down");
    drop(send);
    drop(lightning);
    drop(federation);
    drop(sdk);

    let reopened = Sdk::builder()
        .storage(Storage::at(&path).expect("a valid path"))
        .build()
        .await
        .expect("reopens");
    let federation = reopened.federation(&federation_id).expect("still there");

    // The same two rows in the same order, with the same figures and buckets. `await_final`
    // above already persisted the send's final state before the first `activity` call, so both
    // that read and this one come off the record rather than a fresh one through the driver.
    let restarted = federation.activity(None, 10).await.expect("both rows");
    assert_eq!(restarted.items.len(), 2);
    let send_row = &restarted.items[0];
    let receive_row = &restarted.items[1];
    assert_eq!(send_row.operation_id, send_id);
    assert_eq!(send_row.kind, OperationKind::LnSend);
    if devimint.shape == "v1" {
        assert_eq!(send_row.status, ActivityStatus::Unknown);
        assert!(!send_row.is_final);
        assert_eq!(send_row.amount, None);
    } else {
        assert_eq!(send_row.status, ActivityStatus::Success);
        assert!(send_row.is_final);
        assert_eq!(send_row.direction, Some(Direction::Outgoing));
        assert_eq!(send_row.amount, Some(Amount::from_msats(50_000)));
        assert_eq!(send_row.fee, Some(fee));
    }
    assert_eq!(receive_row.kind, OperationKind::LnReceive);
    assert_eq!(receive_row.direction, Some(Direction::Incoming));
    assert_eq!(receive_row.amount, Some(Amount::from_msats(200_000)));
    assert_eq!(
        receive_row.fee,
        Amount::from_msats(200_000).checked_sub(funded)
    );
    assert_eq!(receive_row.status, ActivityStatus::Success);
    assert!(receive_row.is_final);

    assert_eq!(
        federation
            .activity(None, 0)
            .await
            .expect_err("zero is not a valid limit")
            .code,
        ErrorCode::InvalidInput
    );

    reopened.shutdown().await.expect("shuts down");
}

#[tokio::test(flavor = "multi_thread")]
async fn recovery_restores_a_wallet_with_history() {
    use fedimint_sdk::{
        ActivityStatus, FederationStatus, LnSendState, OperationKind, RecoveryState,
    };

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    pin_lnv2_gateway_to_lnd(&devimint);
    let invite: fedimint_sdk::InviteCode = devimint
        .invite
        .parse()
        .expect("devimint's invite code parses");

    // Instance A joins plainly and funds the wallet that the recovery below will restore.
    let storage_a = tempfile::tempdir().expect("a temporary directory");
    let path_a = storage_a.path().to_str().expect("a utf-8 path");
    let sdk_a = Sdk::builder()
        .storage(Storage::at(path_a).expect("a valid path"))
        .build()
        .await
        .expect("an instance opens on a fresh directory");
    let federation_a = sdk_a.join(&invite).await.expect("the federation joins");
    let id = federation_a.id();
    let lightning_a = federation_a
        .lightning()
        .expect("devimint runs a lightning module");
    let funded = fund(&lightning_a, 200_000).await;
    let mnemonic = sdk_a.export_mnemonic();

    assert_eq!(
        sdk_a.recovery_status(&id).await.expect("readable"),
        None,
        "a plainly joined federation has no recovery"
    );
    let err = sdk_a
        .resume_recovery(&id)
        .await
        .expect_err("this federation was joined, not recovered");
    assert_eq!(err.code, ErrorCode::InvalidInput);

    sdk_a.shutdown().await.expect("the instance shuts down");
    // Every handle dropped before the second instance builds, as in
    // `stored_federation_survives_restart`.
    drop(lightning_a);
    drop(federation_a);
    drop(sdk_a);

    // Instance B holds the same seed, in a fresh directory, and recovers the funded federation.
    let storage_b = tempfile::tempdir().expect("a temporary directory");
    let path_b = storage_b.path().to_str().expect("a utf-8 path").to_owned();
    let sdk_b = Sdk::builder()
        .storage(Storage::at(&path_b).expect("a valid path"))
        .mnemonic(mnemonic.clone())
        .build()
        .await
        .expect("an instance opens on a fresh directory");

    let recovery = sdk_b.recover(&invite).await.expect("the recovery starts");
    assert_eq!(recovery.federation.id(), id);
    // Subscribed while the rescan is still running, when the timing allows, and kept for the
    // rest of the test: the recovery's end retires the client this subscription was opened on,
    // and the subscription has to follow it to the successor rather than end or go quiet.
    let mut updates = recovery.federation.balance_updates();
    let provisional = updates
        .next()
        .await
        .expect("a balance is readable during recovery");
    assert!(
        provisional <= funded,
        "a provisional balance never exceeds the funded one"
    );
    let last = recovery
        .progress
        .await_final()
        .await
        .expect("the recovery finishes");
    assert_eq!(last, RecoveryState::Done);
    assert_eq!(
        sdk_b.recovery_status(&id).await.expect("readable"),
        Some(RecoveryState::Done)
    );
    assert_eq!(
        sdk_b.federation_status(&id),
        Some(FederationStatus::Running)
    );
    // The stream reports changes only, so a provisional reading that was already the funded
    // figure (a rescan that ended before the subscription) is not waited for a second time.
    if provisional != funded {
        settles_at(&mut updates, funded).await;
    }

    // Resuming a completed recovery hands back the same attempt rather than starting a new one.
    let resumed = sdk_b
        .resume_recovery(&id)
        .await
        .expect("a completed recovery is handed back rather than restarted");
    assert_eq!(resumed.progress.id(), recovery.progress.id());
    assert_eq!(
        resumed.progress.state().await.expect("state"),
        RecoveryState::Done
    );

    let err = sdk_b
        .recover(&invite)
        .await
        .expect_err("this instance already holds that federation");
    assert_eq!(err.code, ErrorCode::AlreadyJoined);

    let history = recovery
        .federation
        .activity(None, 10)
        .await
        .expect("activity reads");
    let recovery_row = history
        .items
        .iter()
        .find(|item| item.kind == OperationKind::Recovery)
        .expect("a recovery row is in the history");
    assert_eq!(recovery_row.status, ActivityStatus::Success);
    assert!(recovery_row.is_final);
    assert_eq!(recovery_row.direction, None);
    assert_eq!(recovery_row.amount, None);
    assert_eq!(recovery_row.fee, None);

    // A spend works after recovery: this proves the client swap made the mint usable.
    let lightning_b = recovery
        .federation
        .lightning()
        .expect("devimint runs a lightning module");
    let invoice: fedimint_sdk::Bolt11Invoice = faucet("POST", "/invoice", "10000")
        .expect("the faucet issues an invoice")
        .trim()
        .parse()
        .expect("a bolt11 invoice");
    let quote = lightning_b.quote(&invoice).await.expect("a quote");
    let send = lightning_b.send(quote).await.expect("the payment starts");
    if devimint.shape == "v1" {
        // fedimint/fedimint#8969: the v1 client strips two characters off the preimage the
        // gateway returns, so the SDK cannot decode the success state. The payment itself goes
        // through; only its observation fails, as in
        // `lightning_send_pays_an_invoice_from_outside_the_federation`.
        let err = send.await_final().await.expect_err("fedimint#8969");
        assert_eq!(err.code, ErrorCode::Internal);
        assert!(err.message.contains("preimage"), "{}", err.message);
    } else {
        assert!(matches!(
            send.await_final().await.expect("settles"),
            LnSendState::Success { .. }
        ));
    }
    // The subscription opened during recovery is the one that reports the payment.
    let paid = tokio::time::timeout(std::time::Duration::from_secs(60), updates.next())
        .await
        .expect("the balance moves within a minute of the payment")
        .expect("the balance stream is still live after the recovery");
    assert!(
        paid < funded,
        "the payment lowered the balance: {paid:?} < {funded:?}"
    );
    drop(updates);
    let balance_before_restart = recovery.federation.balance().await.expect("balance");
    let config_meta = recovery.federation.meta().config_metadata();
    assert!(
        !config_meta.is_empty(),
        "a recovered federation has config meta"
    );

    // Restart B: every handle dropped before rebuilding on the same storage, as in
    // `lightning_receive_is_paid_by_the_faucet_and_survives_a_restart`.
    sdk_b.shutdown().await.expect("the instance shuts down");
    drop(send);
    drop(lightning_b);
    drop(resumed);
    drop(recovery);
    drop(sdk_b);
    let reopened = Sdk::builder()
        .storage(Storage::at(&path_b).expect("a valid path"))
        .mnemonic(mnemonic)
        .build()
        .await
        .expect("the instance reopens");
    assert_eq!(
        reopened.federation_status(&id),
        Some(FederationStatus::Running)
    );
    assert_eq!(
        reopened.recovery_status(&id).await.expect("readable"),
        Some(RecoveryState::Done)
    );
    let federation = reopened.federation(&id).expect("still there");
    assert_eq!(
        federation.balance().await.expect("balance"),
        balance_before_restart
    );
    assert_eq!(federation.meta().config_metadata(), config_meta);

    reopened.shutdown().await.expect("the instance shuts down");
}

#[tokio::test(flavor = "multi_thread")]
async fn recovery_locks_the_federation_while_it_runs() {
    use fedimint_sdk::{Amount, FederationStatus, RecoveryState};

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    pin_lnv2_gateway_to_lnd(&devimint);
    let invite: fedimint_sdk::InviteCode = devimint
        .invite
        .parse()
        .expect("devimint's invite code parses");

    // Instance A joins plainly and funds the wallet that the recovery below will restore,
    // mirroring `recovery_restores_a_wallet_with_history`.
    let storage_a = tempfile::tempdir().expect("a temporary directory");
    let path_a = storage_a.path().to_str().expect("a utf-8 path");
    let sdk_a = Sdk::builder()
        .storage(Storage::at(path_a).expect("a valid path"))
        .build()
        .await
        .expect("an instance opens on a fresh directory");
    let federation_a = sdk_a.join(&invite).await.expect("the federation joins");
    let id = federation_a.id();
    let lightning_a = federation_a
        .lightning()
        .expect("devimint runs a lightning module");
    let funded = fund(&lightning_a, 200_000).await;
    let mnemonic = sdk_a.export_mnemonic();
    sdk_a.shutdown().await.expect("the instance shuts down");
    drop(lightning_a);
    drop(federation_a);
    drop(sdk_a);

    let storage_b = tempfile::tempdir().expect("a temporary directory");
    let path_b = storage_b.path().to_str().expect("a utf-8 path");
    let sdk_b = Sdk::builder()
        .storage(Storage::at(path_b).expect("a valid path"))
        .mnemonic(mnemonic)
        .build()
        .await
        .expect("an instance opens on a fresh directory");

    let recovery = sdk_b.recover(&invite).await.expect("the recovery starts");
    assert_eq!(recovery.federation.id(), id);

    // The rescan on devimint takes only seconds and may already be done by the time `recover`
    // returns, so the lock is asserted only when it can still be observed here; the end-state
    // checks below run either way, so this test never passes vacuously on the parts it can
    // check.
    if sdk_b.federation_status(&id) == Some(FederationStatus::Recovering) {
        let lightning = recovery
            .federation
            .lightning()
            .expect("devimint runs a lightning module");
        let err = lightning
            .receive(Amount::from_msats(1_000), "locked")
            .await
            .expect_err("a recovering federation refuses fund-touching calls");
        assert_eq!(err.code, ErrorCode::Recovering);
        recovery
            .federation
            .balance()
            .await
            .expect("reading a balance is not fund-touching");
        assert!(
            sdk_b
                .federations()
                .iter()
                .any(|federation| federation.id() == id),
            "a recovering federation is still listed as open"
        );
    } else {
        eprintln!("skipping the lock assertion: the rescan finished before recover returned");
    }

    let last = recovery
        .progress
        .await_final()
        .await
        .expect("the recovery finishes");
    assert_eq!(last, RecoveryState::Done);
    assert_eq!(
        sdk_b.recovery_status(&id).await.expect("readable"),
        Some(RecoveryState::Done)
    );
    assert_eq!(
        sdk_b.federation_status(&id),
        Some(FederationStatus::Running)
    );
    balance_settles_at(&recovery.federation, funded).await;

    sdk_b.shutdown().await.expect("the instance shuts down");
}

#[tokio::test(flavor = "multi_thread")]
async fn federation_metadata_persists_across_restart() {
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

    let mint_client = std::env::var("FM_MINT_CLIENT")
        .unwrap_or_else(|err| panic!("FM_MINT_CLIENT is not set ({err}); run under devimint"));
    let mut words = mint_client.split_whitespace();
    let program = words
        .next()
        .expect("FM_MINT_CLIENT names at least a program");
    let base_args: Vec<&str> = words.collect();

    let fed_size: usize = std::env::var("FM_FED_SIZE")
        .unwrap_or_else(|_| "1".to_string())
        .parse()
        .expect("FM_FED_SIZE must be a number");

    let run_admin = |admin_args: &[&str]| {
        for i in 0..fed_size {
            let our_id = i.to_string();
            let output = std::process::Command::new(program)
                .args(&base_args)
                .args(["--our-id", &our_id, "--password", "pass"])
                .args(admin_args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "admin command failed for guardian {i}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    };

    let (id, config_meta, consensus_meta, merged) = {
        let sdk = Sdk::builder()
            .storage(Storage::at(&path).expect("a valid path"))
            .build()
            .await
            .expect("an instance opens");
        let federation = sdk.join(&invite).await.expect("the federation joins");
        let id = federation.id();

        let meta = federation.meta();
        let config_meta = meta.config_metadata();
        assert!(!config_meta.is_empty(), "live federation has config meta");

        let overlap_key = config_meta.keys().next().expect("a key").clone();
        let initial_config_val = config_meta.get(&overlap_key).unwrap().clone();

        let mut payload1_map = serde_json::Map::new();
        payload1_map.insert(overlap_key.clone(), serde_json::json!("consensus_wins"));
        payload1_map.insert("only_consensus".to_owned(), serde_json::json!("new_value"));
        let payload1 = serde_json::Value::Object(payload1_map).to_string();

        run_admin(&["module", "meta", "submit", &payload1]);

        let mut current_consensus = None;
        for _ in 0..20 {
            if let Some(mcv) = meta.consensus_metadata().await.expect("consensus meta")
                && !mcv.value.is_empty()
            {
                current_consensus = Some(mcv);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        let consensus1 = current_consensus.expect("consensus metadata published");

        let all1 = meta.all().await.expect("merged meta");
        assert_eq!(all1.get(&overlap_key), Some(&"consensus_wins".to_owned()));
        assert_eq!(all1.get("only_consensus"), Some(&"new_value".to_owned()));
        assert_eq!(
            meta.config_metadata().get(&overlap_key),
            Some(&initial_config_val)
        );

        let mut payload2_map = serde_json::Map::new();
        payload2_map.insert(
            overlap_key.clone(),
            serde_json::json!("consensus_wins_again"),
        );
        payload2_map.insert(
            "only_consensus".to_owned(),
            serde_json::json!("newer_value"),
        );
        let payload2 = serde_json::Value::Object(payload2_map).to_string();

        run_admin(&["module", "meta", "submit", &payload2]);

        let mut current_consensus2 = None;
        for _ in 0..20 {
            if let Some(mcv) = meta.consensus_metadata().await.expect("consensus meta")
                && mcv.revision > consensus1.revision
            {
                current_consensus2 = Some(mcv);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        let consensus2 = current_consensus2.expect("consensus metadata updated");
        assert!(consensus2.revision > consensus1.revision);

        let all2 = meta.all().await.expect("merged meta 2");
        assert_eq!(
            all2.get(&overlap_key),
            Some(&"consensus_wins_again".to_owned())
        );

        let final_config = meta.config_metadata();
        let final_consensus = meta.consensus_metadata().await.expect("final consensus");
        let final_merged = meta.all().await.expect("final merged");

        sdk.shutdown().await.expect("the instance shuts down");
        drop(federation);
        drop(sdk);
        (id, final_config, final_consensus, final_merged)
    };

    let reopened = Sdk::builder()
        .storage(Storage::at(&path).expect("a valid path"))
        .build()
        .await
        .expect("the instance reopens");

    let federation = reopened
        .federation(&id)
        .expect("the federation came back open");
    let meta = federation.meta();

    assert_eq!(meta.config_metadata(), config_meta);
    assert_eq!(
        meta.consensus_metadata().await.expect("consensus meta"),
        consensus_meta
    );
    assert_eq!(meta.all().await.expect("merged meta"), merged);

    reopened.shutdown().await.expect("the instance shuts down");
}

/// Runs one `bitcoin-cli` command against devimint's regtest node and returns its trimmed
/// stdout, panicking with both output streams on a non-zero exit.
///
/// `FM_BTC_CLIENT` is the ready-to-run command line devimint exports for this
/// (`bitcoin-cli -regtest -rpcuser=... -rpcpassword=... -datadir=...`); this splits it on
/// whitespace and appends `args`. The faucet (see `faucet` above) has no on-chain endpoint at
/// all, so this is the only way this suite reaches bitcoind.
///
/// The wallet is named explicitly: devimint's bitcoind has several loaded (the gateways' among
/// them), and `bitcoin-cli` refuses a wallet call without a selection then. `default` is the one
/// devimint creates and funds for itself (`devimint/src/external.rs`).
fn bitcoin_cli(args: &[&str]) -> String {
    let client = std::env::var("FM_BTC_CLIENT")
        .unwrap_or_else(|err| panic!("FM_BTC_CLIENT is not set ({err}); run under devimint"));
    let mut words = client.split_whitespace();
    let program = words
        .next()
        .expect("FM_BTC_CLIENT names at least a program");
    let output = std::process::Command::new(program)
        .args(words)
        .arg("-rpcwallet=default")
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("could not run `{program}`: {err}"));
    assert!(
        output.status.success(),
        "bitcoin-cli {args:?} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Mines `n` blocks to a fresh address, as devimint's own peg-in helpers do.
fn mine_blocks(n: u32) {
    let address = bitcoin_cli(&["getnewaddress"]);
    bitcoin_cli(&["generatetoaddress", &n.to_string(), &address]);
}

/// Sends `sats` satoshis to `address` from bitcoind's own wallet, and returns the txid.
fn send_to_address(address: &str, sats: u64) -> String {
    let btc = format!("{}.{:08}", sats / 100_000_000, sats % 100_000_000);
    bitcoin_cli(&["sendtoaddress", address, &btc])
}

#[tokio::test(flavor = "multi_thread")]
async fn onchain_deposit_and_withdrawal_round_trip() {
    use fedimint_sdk::{
        ActivityStatus, Amount, Direction, OnchainReceiveState, OnchainSendState, OperationKind,
        Sats,
    };

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    let (_storage, _path, sdk, federation) = joined(&devimint).await;
    let onchain = federation.onchain().expect("devimint runs a wallet module");

    // A fresh deposit address starts empty: every fact fills in only once something arrives.
    let receive = onchain
        .receive()
        .await
        .expect("a deposit address is issued");
    let id = receive.operation.id();
    let details = receive.operation.details().await.expect("details");
    assert_eq!(details.address, receive.address);
    assert!(details.txid.is_none());
    assert!(details.gross_deposited.is_none());
    assert!(details.fee.is_none());
    assert!(details.fee_breakdown.is_none());
    assert!(details.net_credit.is_none());
    assert_eq!(
        receive.operation.state().await.expect("state"),
        OnchainReceiveState::WaitingForTransaction
    );

    let txid: fedimint_sdk::Txid = send_to_address(&receive.address.to_string(), 100_000)
        .parse()
        .expect("a well-formed txid");
    mine_blocks(1);

    // v1 reports the pending confirmation explicitly; walletv2 never does, because its scanner
    // only ever reports `Confirmed` once it has already claimed. See `src/onchain.rs`'s deposit
    // state-mapping notes.
    if devimint.shape == "v1" {
        let mut updates = receive.operation.updates();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let state = tokio::time::timeout(remaining, updates.next())
                .await
                .expect("a confirmation state within a minute")
                .expect("a state")
                .expect("a state");
            match state {
                OnchainReceiveState::WaitingForConfirmation {
                    txid: seen,
                    gross_deposited,
                } => {
                    assert_eq!(seen, txid);
                    assert_eq!(gross_deposited, Sats::from_sats(100_000));
                    break;
                }
                OnchainReceiveState::WaitingForTransaction => continue,
                other => panic!("unexpected state before confirmation: {other:?}"),
            }
        }
    }

    mine_blocks(21);
    let claimed = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        receive.operation.await_final(),
    )
    .await
    .expect("the deposit claims within two minutes")
    .expect("settles");
    let net_credit = match claimed {
        OnchainReceiveState::Claimed {
            txid: claimed_txid,
            gross_deposited,
            net_credit,
        } => {
            assert_eq!(claimed_txid, txid);
            assert_eq!(gross_deposited, Sats::from_sats(100_000));
            assert!(net_credit <= Amount::from_msats(100_000_000));
            net_credit
        }
        other => panic!("expected Claimed, got {other:?}"),
    };

    let details = receive.operation.details().await.expect("details");
    assert_eq!(details.txid, Some(txid));
    assert_eq!(details.gross_deposited, Some(Sats::from_sats(100_000)));
    let deposit_fee = details.fee.expect("a claimed deposit has a fee");
    let breakdown = details.fee_breakdown.as_ref().expect("a breakdown");
    let summed = [
        breakdown.peg_in,
        breakdown.network_claim,
        breakdown.primary_module,
        breakdown.dust,
    ]
    .into_iter()
    .try_fold(Amount::from_msats(0), Amount::checked_add)
    .expect("the parts do not overflow");
    assert_eq!(summed, deposit_fee);
    assert_eq!(details.net_credit, Some(net_credit));
    assert_eq!(
        Sats::from_sats(100_000)
            .to_amount()
            .and_then(|gross| gross.checked_sub(deposit_fee)),
        Some(net_credit)
    );

    balance_settles_at(&federation, net_credit).await;

    let history = federation.activity(None, 10).await.expect("one row");
    assert_eq!(history.items.len(), 1);
    let row = &history.items[0];
    assert_eq!(row.operation_id, id);
    assert_eq!(row.kind, OperationKind::OnchainReceive);
    assert_eq!(row.status, ActivityStatus::Success);
    assert_eq!(row.direction, Some(Direction::Incoming));
    assert_eq!(row.amount, Some(Amount::from_msats(100_000_000)));
    assert_eq!(row.fee, Some(deposit_fee));

    // A withdrawal spends part of what was just deposited.
    let destination = bitcoin_cli(&["getnewaddress"]);
    let destination_address: fedimint_sdk::Address = destination.parse().expect("a valid address");
    let quote = onchain
        .quote(&destination_address, Sats::from_sats(30_000))
        .await
        .expect("a quote");
    let amount = quote.amount().to_amount().expect("30,000 sats fits");
    let withdrawal_fee = quote.fee();
    let total = quote.total();
    assert_eq!(amount.checked_add(withdrawal_fee), Some(total));
    let send_breakdown = quote.fee_breakdown();
    let summed = [
        send_breakdown.wallet_output,
        send_breakdown.funding,
        send_breakdown.change,
    ]
    .into_iter()
    .try_fold(Amount::from_msats(0), Amount::checked_add)
    .expect("the parts do not overflow");
    assert_eq!(summed, withdrawal_fee);

    let send = onchain.send(quote).await.expect("the withdrawal starts");
    let send_id = send.id();
    let send_details = send.details().await.expect("details");
    assert_eq!(send_details.address, destination_address);
    assert_eq!(send_details.amount, Sats::from_sats(30_000));
    assert_eq!(send_details.fee, withdrawal_fee);
    assert_eq!(send_details.total, total);

    let send_txid = match send.await_final().await.expect("settles") {
        OnchainSendState::Succeeded { txid } => txid,
        other => panic!("expected Succeeded, got {other:?}"),
    };

    // `Succeeded` is the federation's broadcast, and the transaction reaches bitcoind's mempool
    // a moment later, so the destination is polled rather than read once. Unconfirmed first
    // (`minconf` 0), then confirmed by a block of our own.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        if bitcoin_cli(&["getreceivedbyaddress", &destination, "0"]) == "0.00030000" {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the withdrawal {send_txid} did not reach the destination within a minute"
        );
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    mine_blocks(1);
    assert_eq!(
        bitcoin_cli(&["getreceivedbyaddress", &destination, "1"]),
        "0.00030000"
    );

    // `Succeeded` is the federation accepting the withdrawal; the change from the notes it
    // consumed is minted afterwards, so the balance is waited for rather than read once.
    balance_settles_at(
        &federation,
        net_credit
            .checked_sub(total)
            .expect("the total was debited"),
    )
    .await;

    let history = federation.activity(None, 10).await.expect("two rows");
    assert_eq!(history.items.len(), 2);
    let newest = &history.items[0];
    assert_eq!(newest.operation_id, send_id);
    assert_eq!(newest.kind, OperationKind::OnchainSend);
    assert_eq!(newest.status, ActivityStatus::Success);
    assert_eq!(newest.direction, Some(Direction::Outgoing));
    assert_eq!(newest.amount, Some(Amount::from_msats(30_000_000)));
    assert_eq!(newest.fee, Some(withdrawal_fee));

    // A second deposit lands in a wallet that already holds notes. The claim the mint builds for
    // it may consolidate or rebalance what is there, so its transaction can carry mint inputs
    // beside the wallet input; the credit and the record's identity have to hold all the same.
    let after_withdrawal = net_credit
        .checked_sub(total)
        .expect("the total was debited");
    let second = claim_deposit(&onchain, 20_000).await;
    balance_settles_at(
        &federation,
        after_withdrawal
            .checked_add(second)
            .expect("no overflow at this magnitude"),
    )
    .await;
    let history = federation.activity(None, 10).await.expect("three rows");
    assert_eq!(history.items.len(), 3);
    assert_eq!(history.items[0].kind, OperationKind::OnchainReceive);
    assert_eq!(history.items[0].status, ActivityStatus::Success);

    // The whole balance cannot be withdrawn: the amount fits, the amount plus its fees does
    // not, and that shortfall surfaces from inside the funding quote.
    let balance = federation.balance().await.expect("balance");
    let err = onchain
        .quote(&destination_address, balance.sats_floor())
        .await
        .expect_err("the fees do not fit");
    assert_eq!(err.code, ErrorCode::InsufficientBalance);

    sdk.shutdown().await.expect("shuts down");
}

/// Pays a fresh deposit address `sats` from bitcoind, mines it in, waits for the claim and
/// checks the record's identity, returning the net credit the deposit made.
async fn claim_deposit(onchain: &fedimint_sdk::Onchain, sats: u64) -> fedimint_sdk::Amount {
    use fedimint_sdk::{OnchainReceiveState, Sats};

    let receive = onchain.receive().await.expect("a deposit address");
    send_to_address(&receive.address.to_string(), sats);
    mine_blocks(21);
    let claimed = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        receive.operation.await_final(),
    )
    .await
    .expect("the deposit is claimed within two minutes")
    .expect("the deposit settles");
    let net_credit = match claimed {
        OnchainReceiveState::Claimed {
            gross_deposited,
            net_credit,
            ..
        } => {
            assert_eq!(gross_deposited, Sats::from_sats(sats));
            net_credit
        }
        other => panic!("expected Claimed, got {other:?}"),
    };
    let details = receive.operation.details().await.expect("details");
    let fee = details.fee.expect("a claimed deposit has a fee");
    assert_eq!(
        Sats::from_sats(sats)
            .to_amount()
            .and_then(|gross| gross.checked_sub(fee)),
        Some(net_credit)
    );
    net_credit
}

/// A subscriber that stopped polling must not keep the federation from closing: the stream it
/// holds is parked wherever its last wait returned pending, and nothing in there is ever polled
/// again, so whatever that wait held is held for as long as the subscriber lives.
#[tokio::test(flavor = "multi_thread")]
async fn onchain_parked_deposit_subscriber_does_not_block_a_close() {
    use fedimint_sdk::OnchainReceiveState;

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    let (_storage, _path, sdk, federation) = joined(&devimint).await;
    let id = federation.id();
    let onchain = federation.onchain().expect("devimint runs a wallet module");
    let receive = onchain.receive().await.expect("a deposit address");

    let mut updates = receive.operation.updates();
    let first = updates.next().await.expect("a state").expect("a state");
    assert_eq!(first, OnchainReceiveState::WaitingForTransaction);
    // Nobody pays, so the next wait does not resolve; give it up and keep the subscriber.
    let parked = tokio::time::timeout(std::time::Duration::from_millis(200), updates.next()).await;
    assert!(parked.is_err(), "an unpaid address yields nothing further");

    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        sdk.close_federation(&id),
    )
    .await
    .expect("the close is not held up by the parked subscriber")
    .expect("the federation closes");
    drop(updates);
    drop(receive);
    sdk.shutdown().await.expect("shuts down");
}

/// Two calls hand out two addresses, as `Onchain::receive` promises.
///
/// walletv2 returns the same address until its background scanner has advanced its index
/// (fedimint/fedimint#9101), so on that shape the second of two calls made in quick succession
/// is refused instead (`onchain_receive_never_hands_out_a_watched_address` covers that). Ignored
/// until a fresh address per call is available upstream.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "fedimint/fedimint#9101: walletv2 hands out the same address until its scanner advances"]
async fn onchain_receive_hands_out_a_fresh_address_each_time() {
    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    let (_storage, _path, sdk, federation) = joined(&devimint).await;
    let onchain = federation.onchain().expect("devimint runs a wallet module");

    let first = onchain.receive().await.expect("a deposit address");
    let second = onchain.receive().await.expect("another deposit address");
    assert_ne!(first.address, second.address);
    assert_ne!(first.operation.id(), second.operation.id());

    sdk.shutdown().await.expect("shuts down");
}

/// An address is never handed out twice while an operation is still following it: a second
/// call either yields a different address or, where the federation's wallet offers only one
/// unused address at a time (walletv2, fedimint/fedimint#9101), refuses rather than recording a
/// second operation on the first address. Either way the first operation keeps its address
/// alone.
#[tokio::test(flavor = "multi_thread")]
async fn onchain_receive_never_hands_out_a_watched_address() {
    use fedimint_sdk::{ErrorCode, OnchainReceiveState};

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    let (_storage, _path, sdk, federation) = joined(&devimint).await;
    let onchain = federation.onchain().expect("devimint runs a wallet module");

    let first = onchain.receive().await.expect("a deposit address");
    match onchain.receive().await {
        Ok(second) => {
            assert_ne!(first.address, second.address);
            assert_ne!(first.operation.id(), second.operation.id());
        }
        Err(err) => {
            eprintln!("the second call was refused: {err}");
            assert_eq!(devimint.shape, "v2", "only walletv2 refuses: {err}");
            assert_eq!(err.code, ErrorCode::Internal, "{err}");
        }
    }
    assert_eq!(
        first
            .operation
            .state()
            .await
            .expect("the first still reads"),
        OnchainReceiveState::WaitingForTransaction
    );

    sdk.shutdown().await.expect("shuts down");
}

/// Two calls racing each other never record one address twice: the check for an address an
/// operation already follows and the commit of the new operation's record are one step. Without
/// that, two concurrent calls handed the same walletv2 address would both pass the check before
/// either record existed, and one payment would be adopted twice.
#[tokio::test(flavor = "multi_thread")]
async fn onchain_concurrent_receives_never_share_an_address() {
    use fedimint_sdk::ErrorCode;

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    let (_storage, _path, sdk, federation) = joined(&devimint).await;
    let onchain = federation.onchain().expect("devimint runs a wallet module");

    // Separate tasks on the multi-threaded runtime, so the calls really overlap.
    let racing: Vec<_> = (0..8)
        .map(|_| {
            let onchain = onchain.clone();
            tokio::spawn(async move { onchain.receive().await })
        })
        .collect();
    let mut handed_out = Vec::new();
    for task in racing {
        match task.await.expect("the call does not panic") {
            Ok(receive) => handed_out.push(receive),
            Err(err) => {
                eprintln!("a racing call was refused: {err}");
                assert_eq!(devimint.shape, "v2", "only walletv2 refuses: {err}");
                assert_eq!(err.code, ErrorCode::Internal, "{err}");
            }
        }
    }
    assert!(
        !handed_out.is_empty(),
        "at least one call hands out an address"
    );
    for (i, a) in handed_out.iter().enumerate() {
        for b in &handed_out[i + 1..] {
            assert_ne!(a.address, b.address, "one address was recorded twice");
            assert_ne!(a.operation.id(), b.operation.id());
        }
    }

    sdk.shutdown().await.expect("shuts down");
}

#[tokio::test(flavor = "multi_thread")]
async fn onchain_deposit_survives_a_restart() {
    use fedimint_sdk::OnchainReceiveState;

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    let (_storage, path, sdk, federation) = joined(&devimint).await;
    let onchain = federation.onchain().expect("devimint runs a wallet module");

    let receive = onchain
        .receive()
        .await
        .expect("a deposit address is issued");
    let id = receive.operation.id();
    let address = receive.address.clone();
    let federation_id = federation.id();

    sdk.shutdown().await.expect("shuts down");
    // Every handle dropped before the second build, as in
    // `lightning_receive_is_paid_by_the_faucet_and_survives_a_restart`.
    drop(receive);
    drop(onchain);
    drop(federation);
    drop(sdk);

    let reopened = Sdk::builder()
        .storage(Storage::at(&path).expect("a valid path"))
        .build()
        .await
        .expect("reopens");
    let federation = reopened.federation(&federation_id).expect("still there");
    let any = federation
        .operation(&id)
        .await
        .expect("lookup")
        .expect("still recorded");
    let typed = any.as_onchain_receive().expect("a typed handle");
    let details = typed.details().await.expect("details");
    assert_eq!(details.address, address);

    let txid: fedimint_sdk::Txid = send_to_address(&address.to_string(), 100_000)
        .parse()
        .expect("a well-formed txid");
    mine_blocks(21);

    let claimed = tokio::time::timeout(std::time::Duration::from_secs(120), typed.await_final())
        .await
        .expect("the deposit claims within two minutes")
        .expect("settles");
    match claimed {
        OnchainReceiveState::Claimed {
            txid: claimed_txid, ..
        } => assert_eq!(claimed_txid, txid),
        other => panic!("expected Claimed, got {other:?}"),
    }
    assert_eq!(typed.id(), id);

    reopened.shutdown().await.expect("shuts down");
}

#[tokio::test(flavor = "multi_thread")]
async fn onchain_quote_refuses_what_cannot_be_withdrawn() {
    use fedimint_sdk::{Amount, Network, Sats};

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    let (_storage, _path, sdk, federation) = joined(&devimint).await;
    let onchain = federation.onchain().expect("devimint runs a wallet module");

    // A well-formed address for the wrong chain, from `bitcoin`'s own test suite.
    let mainnet: fedimint_sdk::Address = "bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq"
        .parse()
        .expect("a valid address");
    let err = onchain
        .quote(&mainnet, Sats::from_sats(50_000))
        .await
        .expect_err("refused");
    assert_eq!(err.code, ErrorCode::NetworkMismatch);
    match err.detail() {
        Some(ErrorDetails::NetworkMismatch {
            expected,
            observed_prefix,
            ..
        }) => {
            assert_eq!(*expected, Network::Regtest);
            assert_eq!(observed_prefix, "bc");
        }
        other => panic!("expected NetworkMismatch details, got {other:?}"),
    }

    let destination: fedimint_sdk::Address = bitcoin_cli(&["getnewaddress"])
        .parse()
        .expect("a valid address");

    assert_eq!(
        onchain
            .quote(&destination, Sats::from_sats(0))
            .await
            .expect_err("a zero withdrawal is refused")
            .code,
        ErrorCode::InvalidInput
    );

    // Below the dust threshold on both wallet module generations.
    assert_eq!(
        onchain
            .quote(&destination, Sats::from_sats(100))
            .await
            .expect_err("dust is refused")
            .code,
        ErrorCode::InvalidInput
    );

    // Above what an empty wallet holds.
    let err = onchain
        .quote(&destination, Sats::from_sats(50_000))
        .await
        .expect_err("nothing to withdraw with");
    assert_eq!(err.code, ErrorCode::InsufficientBalance);
    match err.detail() {
        Some(ErrorDetails::InsufficientBalance {
            required,
            available,
        }) => {
            assert!(*required > *available);
            assert_eq!(*available, Amount::from_msats(0));
        }
        other => panic!("expected InsufficientBalance details, got {other:?}"),
    }

    sdk.shutdown().await.expect("shuts down");
}

// # A rejected funding is not an ending
//
// The three tests below cover the contract `crate::inputs` implements: an upstream send
// subscription reporting that its funding transaction was rejected does not end the operation.
// Submitting that transaction took primary-module notes out of the spendable set before
// consensus said anything, putting them back is a later transaction of the mint's own, and the
// SDK reports `Refunded` only once that recovery has settled in a way that establishes the value
// is spendable again, `Failed` when it cannot.
//
// ## Why they are `#[ignore]` rather than early-return
//
// Every other test in this file returns early when there is no federation, for the reason the
// `devimint!` macro documents. These are ignored for a different reason, which no federation
// fixes: on the pinned fedimint the scenario cannot be produced and its outcome cannot be
// proven.
//
// - Provoking the rejection needs two clients holding the same notes, one of which spends them
//   first. Restoring a second client from the same seed and racing it is the shape the bodies
//   below use, and it depends on recovery handing back notes that are still spendable on the
//   first client: fedimint/fedimint#6546.
// - Proving the ending exactly needs the mint's own input-recovery outcome. Its state machine
//   ends in `RefundSuccess` or `Error` and neither is nameable outside `fedimint-mint-client`,
//   so the SDK reads what is public instead, the recovery transaction's acceptance and whether
//   its outputs issued their notes, and deliberately reports `Failed` for the shapes that
//   leaves undecided: fedimint/fedimint#9099.
// - Knowing *when* the recovery has settled, rather than polling for it, needs an operation-level
//   quiescence signal: fedimint/fedimint#8421.
//
// The assertions are written out in full rather than stubbed, so closing the upstream gaps is
// most of the change. It is not the whole of it: the scenario these two need has no fixture yet,
// because it cannot be built on the pinned fedimint, and `rejected_funding` panics rather than
// quietly reporting that there is nothing to test. Removing `#[ignore]` on its own therefore
// fails at the missing piece instead of passing without having asserted anything — implement the
// fixture first. Run one with `cargo test --locked -- --ignored rejected_funding`.

/// The contract, on the lightning send path: a rejected funding holds the operation open until
/// the notes it selected have settled, and only then ends it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs fedimint/fedimint#6546 to provoke the rejection and #9099 to prove the ending"]
async fn lightning_send_with_rejected_funding_waits_for_input_recovery() {
    use fedimint_sdk::{ActivityStatus, LnSendState};

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    let (_storage, _path, sdk, federation) = joined(&devimint).await;
    let lightning = federation
        .lightning()
        .expect("devimint runs a lightning module");
    let funded = fund(&lightning, 200_000).await;

    let (spender, send) = rejected_funding(&devimint, &sdk, &federation, &lightning).await;

    // The rejection has been reported upstream by now. The operation must not be final on it:
    // the notes the funding selected are neither spent nor yet spendable, and `Refunded` would
    // promise the second.
    let state = send.state().await.expect("a state");
    assert!(
        matches!(state, LnSendState::Created | LnSendState::Funded),
        "a rejected funding ended the send at {state:?} before its inputs settled"
    );
    let row = activity_row(&federation, send.id()).await;
    assert_eq!(
        row.status,
        ActivityStatus::Pending,
        "the history row called a send final while its inputs were still being recovered"
    );

    // Once the recovery settles the ending is chosen from what it established. Restored value
    // is `Refunded`; anything the SDK cannot establish as a clean return is `Failed`, and both
    // are legitimate endings for this scenario. What is not legitimate is `Success`.
    let last = send.await_final().await.expect("the send settles");
    match last {
        LnSendState::Refunded => {
            // The promise `Refunded` makes: no part of the authorised total is still standing
            // against the balance.
            balance_settles_at(&federation, funded).await;
        }
        LnSendState::Failed { .. } => {
            eprintln!("the input recovery settled without establishing a clean return");
        }
        other => panic!("a payment whose funding was rejected ended in {other:?}"),
    }

    drop(spender);
    sdk.shutdown().await.expect("shuts down");
}

/// The same contract across a restart. The gate is not in-memory state: the record carries no
/// final state while the recovery runs, so a reattached operation re-enters the gate and reaches
/// the same ending.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs fedimint/fedimint#6546 to provoke the rejection and #9099 to prove the ending"]
async fn lightning_send_with_rejected_funding_resolves_after_a_restart() {
    use fedimint_sdk::{LnSendState, OperationKind};

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    let (_storage, path, sdk, federation) = joined(&devimint).await;
    let lightning = federation
        .lightning()
        .expect("devimint runs a lightning module");
    fund(&lightning, 200_000).await;

    let id = {
        let (spender, send) = rejected_funding(&devimint, &sdk, &federation, &lightning).await;
        let id = send.id();
        // Dropped before the restart below: the instance that caused the rejection has no part
        // in what a reattached operation reports.
        drop(spender);
        id
    };
    let federation_id = federation.id();
    sdk.shutdown().await.expect("the instance shuts down");
    drop(federation);
    drop(lightning);
    drop(sdk);

    let reopened = Sdk::builder()
        .storage(Storage::at(&path).expect("a valid path"))
        .build()
        .await
        .expect("the instance reopens");
    let federation = reopened
        .federation(&federation_id)
        .expect("the federation came back open");
    let any = federation
        .operation(&id)
        .await
        .expect("lookup")
        .expect("the send survived the restart");
    assert_eq!(any.kind(), OperationKind::LnSend);
    let send = any.as_ln_send().expect("a typed handle");

    // Nothing final was written while the recovery was running, so this is a real reattachment
    // to a live operation rather than a replay of a recorded ending.
    let last = send.await_final().await.expect("the send settles");
    assert!(
        matches!(last, LnSendState::Refunded | LnSendState::Failed { .. }),
        "a reattached send whose funding was rejected ended in {last:?}"
    );
    reopened.shutdown().await.expect("shuts down");
}

/// The contract on the on-chain send path, which reaches the same gate through the wallet
/// modules' own funding rejections (`WithdrawState::Failed` for the first, `Aborted` for the
/// second).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "blocked on the on-chain facade, plus fedimint/fedimint#6546 and #9099 as above"]
async fn onchain_send_with_rejected_funding_waits_for_input_recovery() {
    use fedimint_sdk::{OnchainSendState, Sats};

    let devimint = devimint!();
    if devimint.shape == "mixed" {
        eprintln!("skipping: the mixed shape is covered by its own test");
        return;
    }
    let (_storage, _path, sdk, federation) = joined(&devimint).await;
    let lightning = federation
        .lightning()
        .expect("devimint runs a lightning module");
    let funded = fund(&lightning, 500_000).await;
    let onchain = federation.onchain().expect("devimint runs a wallet module");

    let address = faucet("POST", "/address", "")
        .expect("the faucet issues an address")
        .trim()
        .parse()
        .expect("a bitcoin address");
    let quote = onchain
        .quote(&address, Sats::from_sats(100))
        .await
        .expect("a quote");
    let send = onchain.send(quote).await.expect("the withdrawal starts");

    // As above: the rejection is reported, the recovery of the notes the funding selected is
    // not finished, and the withdrawal must not claim either ending yet.
    let state = send.state().await.expect("a state");
    assert_eq!(
        state,
        OnchainSendState::Created,
        "a rejected funding ended the withdrawal before its inputs settled"
    );

    let last = send.await_final().await.expect("the withdrawal settles");
    match last {
        OnchainSendState::Refunded { .. } => balance_settles_at(&federation, funded).await,
        OnchainSendState::Failed { .. } => {
            eprintln!("the input recovery settled without establishing a clean return");
        }
        other => panic!("a withdrawal whose funding was rejected ended in {other:?}"),
    }
    sdk.shutdown().await.expect("shuts down");
}

/// Starts a lightning send whose funding transaction the federation rejects, and returns the
/// instance that caused the rejection alongside the doomed send.
///
/// The rejection is a double spend: a second instance restored from the same seed holds the same
/// notes, spends them first, and the send's own funding transaction is then refused for inputs
/// that are already gone. Keeping the returned `Sdk` alive keeps that instance from being torn
/// down while the send is still being observed.
///
/// This is the half that fedimint/fedimint#6546 blocks: a restored client's notes have to still
/// be spendable on the instance that was restored from for the race to be winnable at all.
///
/// Unimplemented until then, and it says so by panicking rather than by reporting that there is
/// nothing to provoke. Both callers are `#[ignore]`d, so nothing reaches here by accident; a
/// deliberate `--ignored` run against a live federation stops here, at the piece that is
/// actually missing, instead of passing through assertions it never made. Returning `None` for
/// the callers to skip on is what made those runs report success.
async fn rejected_funding(
    _devimint: &Devimint,
    _sdk: &Sdk,
    _federation: &fedimint_sdk::Federation,
    _lightning: &fedimint_sdk::Lightning,
) -> (Sdk, fedimint_sdk::Operation<fedimint_sdk::LnSendState>) {
    unimplemented!(
        "the double spend this needs is blocked on fedimint/fedimint#6546: restore a second \
         client onto the same notes, spend them there, then quote and send here so this send's \
         funding transaction is refused"
    )
}

/// The one activity row an operation wrote, for a test that wants the bucket rather than the
/// typed state.
async fn activity_row(
    federation: &fedimint_sdk::Federation,
    id: fedimint_sdk::OperationId,
) -> fedimint_sdk::ActivityItem {
    federation
        .activity(None, 50)
        .await
        .expect("activity reads")
        .items
        .into_iter()
        .find(|item| item.operation_id == id)
        .expect("the send has a history row")
}
