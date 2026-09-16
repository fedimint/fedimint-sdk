//! Receiving and sending a lightning payment.
//!
//! Usage:
//!
//! `lightning <data-dir> <invite-code> receive <amount-msats> [description]`
//! `lightning <data-dir> <invite-code> send <invoice>`
//!
//! Builds an instance over `<data-dir>`, joining `<invite-code>` the first time and
//! reopening the same federation on every later run, then issues an invoice for a
//! counterparty to pay, or pays an invoice a counterparty handed to it.
//!
//! `scripts/run-sdk-examples.sh` runs this example against a federation it starts with
//! devimint, playing the payer on one run and the payee on the other.

mod common;

use fedimint_sdk::{Amount, Bolt11Invoice, Federation, Lightning, LnSendState};

const USAGE: &str = "usage: lightning <data-dir> <invite-code> receive <amount-msats> \
    [description]\n       lightning <data-dir> <invite-code> send <invoice>";

/// What this run was asked to do, parsed from the command line before anything is opened.
enum Action {
    Receive { msats: u64, description: String },
    Send { invoice: Bolt11Invoice },
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
    let lightning = federation
        .lightning()
        .expect("this federation has no lightning module");

    match action {
        Action::Receive { msats, description } => {
            receive(&lightning, &federation, msats, &description).await?;
        }
        Action::Send { invoice } => send(&lightning, &federation, invoice).await?,
    }

    sdk.shutdown().await
}

fn parse_action(rest: &[String]) -> fedimint_sdk::Result<Action> {
    match rest {
        [subcommand, amount] if subcommand == "receive" => {
            let Ok(msats) = amount.parse::<u64>() else {
                common::usage(USAGE);
            };
            Ok(Action::Receive {
                msats,
                description: String::new(),
            })
        }
        [subcommand, amount, description] if subcommand == "receive" => {
            let Ok(msats) = amount.parse::<u64>() else {
                common::usage(USAGE);
            };
            Ok(Action::Receive {
                msats,
                description: description.clone(),
            })
        }
        [subcommand, invoice] if subcommand == "send" => Ok(Action::Send {
            invoice: invoice.parse()?,
        }),
        _ => common::usage(USAGE),
    }
}

/// Issues an invoice, waits for it to be paid, and reports what arrived.
async fn receive(
    lightning: &Lightning,
    federation: &Federation,
    msats: u64,
    description: &str,
) -> fedimint_sdk::Result<()> {
    // Show the invoice before anyone pays it.
    let receive = lightning
        .receive(Amount::from_msats(msats), description)
        .await?;
    println!("invoice: {}", receive.invoice);
    println!("pay this invoice");
    let mut updates = receive.operation.updates();
    while let Some(state) = updates.next().await? {
        println!("state: {state:?}");
    }
    let details = receive.operation.details().await?;
    println!("received {}", details.net_credit);
    println!("balance: {}", federation.balance().await?);
    Ok(())
}

/// Quotes and pays an invoice a counterparty handed over, reporting each state until the
/// payment settles.
async fn send(
    lightning: &Lightning,
    federation: &Federation,
    invoice: Bolt11Invoice,
) -> fedimint_sdk::Result<()> {
    // Quote first, so the cost is known before agreeing to it.
    let quote = lightning.quote(&invoice).await?;
    println!(
        "pay {} plus {} fee ({} total) via {:?}, good until {}",
        quote.invoice_amount(),
        quote.fee(),
        quote.total(),
        quote.route(),
        quote.expires_at(),
    );
    let payment = lightning.send(quote).await?;
    let mut updates = payment.updates();
    while let Some(state) = updates.next().await? {
        match state {
            LnSendState::Success { preimage, fee, .. } => {
                println!("state: paid, fee {fee}, preimage {preimage}");
            }
            // A payment that does not succeed is not an error: `Refunded` means the money
            // is safe in the balance, `Failed` means it did not resolve into a clean refund.
            // Neither is the call failing, so both print as an ordinary observed state.
            other => println!("state: {other:?}"),
        }
    }

    println!("balance: {}", federation.balance().await?);
    Ok(())
}
