//! Depositing bitcoin into the federation and withdrawing it back out.
//!
//! Run it with `scripts/run-sdk-examples.sh`, which starts a federation through devimint first:
//! this example builds an instance over a fresh temporary directory, joins that federation, funds
//! itself with an on-chain deposit, then withdraws part of it back to bitcoind.

use devimint_support::{Devimint, bitcoin_cli, mine_blocks, send_to_address};
use fedimint_sdk::{Address, Federation, InviteCode, OnchainSendState, Sats, Sdk, Storage};

#[tokio::main]
async fn main() -> fedimint_sdk::Result<()> {
    let devimint = Devimint::detect()
        .expect("no devimint federation found; run this example via scripts/run-sdk-examples.sh");
    let (_storage, sdk, federation) = join(&devimint).await?;

    let onchain = federation
        .onchain()
        .expect("devimint runs a wallet module; run this example via scripts/run-sdk-examples.sh");

    // Receive: a fresh deposit address. There is nothing to quote on this side; the sender
    // pays whatever network fee their own wallet charges, and the federation's own charge for
    // claiming the deposit is only knowable once an amount has arrived.
    let receive = onchain.receive().await?;
    println!("deposit to this address: {}", receive.address);
    send_to_address(&receive.address.to_string(), 100_000);
    // Enough confirmations for either wallet generation to claim the deposit.
    mine_blocks(21);
    let mut updates = receive.operation.updates();
    while let Some(state) = updates.next().await? {
        println!("{state:?}");
    }
    let details = receive.operation.details().await?;
    println!(
        "{} gross, {} fee, {} credited",
        details
            .gross_deposited
            .expect("a claimed deposit knows what arrived"),
        details.fee.expect("a claimed deposit knows its fee"),
        details
            .net_credit
            .expect("a claimed deposit knows its net credit"),
    );
    println!("balance: {}", federation.balance().await?);

    // Send: withdraw part of it to an address outside the federation. Quote first, so the fee
    // is known before anything is debited.
    let destination: Address = bitcoin_cli(&["getnewaddress"]).parse()?;
    let quote = onchain.quote(&destination, Sats::from_sats(20_000)).await?;
    println!(
        "send {} plus {} fee ({:?}, {} total), good until {}",
        quote.amount(),
        quote.fee(),
        quote.fee_breakdown(),
        quote.total(),
        quote.expires_at(),
    );
    let send = onchain.send(quote).await?;
    let outcome = send.await_final().await?;
    println!("{outcome:?}");
    if let OnchainSendState::Succeeded { txid } = outcome {
        println!("txid: {txid}");
    }

    println!("balance: {}", federation.balance().await?);
    sdk.shutdown().await
}

/// Builds an instance over a fresh temporary directory and joins the federation devimint is
/// running.
async fn join(devimint: &Devimint) -> fedimint_sdk::Result<(tempfile::TempDir, Sdk, Federation)> {
    let invite: InviteCode = devimint.invite.parse()?;
    let storage = tempfile::tempdir().expect("a temporary directory");
    let path = storage.path().to_str().expect("a utf-8 path");
    let sdk = Sdk::builder().storage(Storage::at(path)?).build().await?;
    let federation = sdk.join(&invite).await?;
    Ok((storage, sdk, federation))
}
