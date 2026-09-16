//! Shared setup every example uses.
//!
//! Every example takes a data directory and an invite code first, then a subcommand of its
//! own. This module is what every one of them does before that subcommand-specific work
//! starts: build an instance over the data directory, open the federation named by the invite
//! code, and reject a command line that could not be understood the same way.

use fedimint_sdk::{Federation, InviteCode, Result, Sdk, Storage};

/// Builds an instance over `data_dir`, which is created if it does not exist and is meant to
/// be reused across runs.
///
/// The first run against a fresh directory generates a seed; each later run over the same
/// directory finds that seed and everything stored since. That persistence is what lets the
/// second half of an example, a send, find the balance the first half, a receive, brought in,
/// and what lets an operation id a previous run printed be picked up again by a later one.
pub async fn build(data_dir: &str) -> Result<Sdk> {
    let sdk = Sdk::builder()
        .storage(Storage::at(data_dir)?)
        .build()
        .await?;
    // An empty list of stored federations is the cheapest way to tell "this run just
    // generated a fresh seed" from "this run reopened a wallet that already existed": an
    // application backs up the mnemonic exactly once, the first time it is generated, and
    // this is that moment.
    if sdk.stored_federations().is_empty() {
        println!("mnemonic: {}", sdk.export_mnemonic().words().join(" "));
    }
    Ok(sdk)
}

/// The federation named by `invite`: joined the first time, reopened from storage on every
/// later run over the same data directory.
pub async fn open(sdk: &Sdk, invite: &str) -> Result<Federation> {
    let invite: InviteCode = invite.parse()?;
    match sdk.federation(&invite.federation_id()) {
        Some(federation) => Ok(federation),
        None => sdk.join(&invite).await,
    }
}

/// Prints `text` as a usage line on stderr and exits the process with status 2, the
/// conventional exit code for a command line that could not be understood.
pub fn usage(text: &str) -> ! {
    eprintln!("{text}");
    std::process::exit(2);
}
