//! Shared setup every example uses.
//!
//! Every example takes a data directory and an invite code first, then a subcommand of its
//! own. This module is the two things every one of them does before that subcommand-specific
//! work starts: open the federation named by the invite code, persisting it under the data
//! directory, and reject a command line that could not be understood the same way.

use fedimint_sdk::{Federation, InviteCode, Result, Sdk, Storage};

/// Opens the federation named by `invite`, persisting it under `data_dir`.
///
/// `data_dir` is created if it does not exist and is meant to be reused across runs: the
/// first run against a fresh directory generates a seed and joins the federation, and each
/// later run over the same directory finds the federation already stored and reopens it
/// instead. That persistence is what lets the second half of an example, a send, find the
/// balance the first half, a receive, brought in, and what lets an operation id a previous
/// run printed be picked up again by a later one.
pub async fn open(data_dir: &str, invite: &str) -> Result<(Sdk, Federation)> {
    let invite: InviteCode = invite.parse()?;
    let sdk = Sdk::builder()
        .storage(Storage::at(data_dir)?)
        .build()
        .await?;

    // An empty list of stored federations is the cheapest way to tell "this run just
    // generated a fresh seed" from "this run reopened a wallet that already existed": an
    // application backs up the mnemonic exactly once, the first time it is generated, and
    // this is that moment.
    let fresh = sdk.stored_federations().is_empty();
    let federation = match sdk.federation(&invite.federation_id()) {
        Some(federation) => federation,
        None => sdk.join(&invite).await?,
    };
    if fresh {
        println!("mnemonic: {}", sdk.export_mnemonic().words().join(" "));
    }
    Ok((sdk, federation))
}

/// Prints `text` as a usage line on stderr and exits the process with status 2, the
/// conventional exit code for a command line that could not be understood.
pub fn usage(text: &str) -> ! {
    eprintln!("{text}");
    std::process::exit(2);
}
