//! Depositing bitcoin into the federation and withdrawing it back out.
//!
//! Usage:
//!
//! `onchain <data-dir> <invite-code> receive`
//! `onchain <data-dir> <invite-code> send <address> <sats>`
//!
//! Builds an instance over `<data-dir>`, joining `<invite-code>` the first time and
//! reopening the same federation on every later run, then prints a deposit address for a
//! counterparty to pay, or withdraws to an address outside the federation.
//!
//! `scripts/run-sdk-examples.sh` runs this example against a federation it starts with
//! devimint, paying the deposit address and mining the confirmations itself.

mod common;

use fedimint_sdk::{Address, OnchainReceiveState, OnchainSendState, Sats};

const USAGE: &str = "usage: onchain <data-dir> <invite-code> receive\n       \
    onchain <data-dir> <invite-code> send <address> <sats>";

/// What this run was asked to do, parsed from the command line before anything is opened.
enum Action {
    Receive,
    Send { address: Address, sats: u64 },
}

#[tokio::main]
async fn main() -> fedimint_sdk::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data_dir, invite, rest @ ..] = args.as_slice() else {
        common::usage(USAGE);
    };
    let action = parse_action(rest)?;

    let sdk = common::build(data_dir).await?;
    let federation = common::open(&sdk, invite).await?;
    let onchain = federation
        .onchain()
        .expect("this federation has no wallet module");

    match action {
        Action::Receive => {
            // There is nothing to quote on this side: the sender pays whatever network fee
            // their own wallet charges, and the federation's own charge for claiming the
            // deposit is only knowable once an amount has arrived.
            let receive = onchain.receive().await?;
            println!("address: {}", receive.address);
            println!("send a deposit to this address");
            let mut updates = receive.operation.updates();
            let mut last_state = None;
            while let Some(state) = updates.next().await? {
                println!("state: {state:?}");
                last_state = Some(state);
            }
            // `Claimed` is the only terminal state whose deposit was actually credited:
            // `Failed` carries no transaction and no amount even when one was seen, so
            // there is no receipt to show.
            match last_state {
                Some(OnchainReceiveState::Claimed { .. }) => {
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
                }
                other => println!("did not receive a deposit: {other:?}"),
            }
            println!("balance: {}", federation.balance().await?);
        }
        Action::Send { address, sats } => {
            // Quote first, so the fee is known before anything is debited.
            let quote = onchain.quote(&address, Sats::from_sats(sats)).await?;
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
            println!("state: {outcome:?}");
            if let OnchainSendState::Succeeded { txid } = outcome {
                println!("confirmed on chain, txid {txid}");
            }
            println!("balance: {}", federation.balance().await?);
        }
    }

    sdk.shutdown().await
}

fn parse_action(rest: &[String]) -> fedimint_sdk::Result<Action> {
    match rest {
        [subcommand] if subcommand == "receive" => Ok(Action::Receive),
        [subcommand, address, sats] if subcommand == "send" => {
            let Ok(sats) = sats.parse::<u64>() else {
                common::usage(USAGE);
            };
            Ok(Action::Send {
                address: address.parse()?,
                sats,
            })
        }
        _ => common::usage(USAGE),
    }
}
