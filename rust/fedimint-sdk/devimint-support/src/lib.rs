//! The harness side of `scripts/run-sdk-integration-tests.sh` and `scripts/run-sdk-examples.sh`:
//! how a test or example finds the federation devimint stood up, talks to devimint's faucet, and
//! drives its bitcoind.

use std::io::{Read, Write};

/// Everything devimint hands this process. `None` when not running under devimint.
#[derive(Debug)]
pub struct Devimint {
    pub invite: String,
    pub shape: String,
}

impl Devimint {
    /// Finds the federation, in devimint's own order of preference.
    pub fn detect() -> Option<Devimint> {
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
pub fn faucet(method: &str, path: &str, body: &str) -> Option<String> {
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
pub fn bitcoin_cli(args: &[&str]) -> String {
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
pub fn mine_blocks(n: u32) {
    let address = bitcoin_cli(&["getnewaddress"]);
    bitcoin_cli(&["generatetoaddress", &n.to_string(), &address]);
}

/// Sends `sats` satoshis to `address` from bitcoind's own wallet, and returns the txid.
pub fn send_to_address(address: &str, sats: u64) -> String {
    let btc = format!("{}.{:08}", sats / 100_000_000, sats % 100_000_000);
    bitcoin_cli(&["sendtoaddress", address, &btc])
}
