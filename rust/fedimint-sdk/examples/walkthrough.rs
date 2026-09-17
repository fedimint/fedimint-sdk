//! The crate documentation's walkthrough, made runnable.
//!
//! Usage: `walkthrough <data-dir> <invite-code> [invoice]`
//!
//! Builds an instance over `<data-dir>`, joining `<invite-code>` the first time and
//! reopening the same federation on every later run, then works through the walkthrough end
//! to end: preview the federation, read the balance, look at its capabilities, fund the
//! wallet with a lightning invoice if the balance is still zero, send some ecash, pay
//! `invoice` if one was given, reattach to the ecash send by the id it printed, list one page
//! of activity, and shut down.
//!
//! `scripts/run-sdk-examples.sh` runs this example against a federation it starts with
//! devimint, playing every counterparty itself.

mod common;

use fedimint_sdk::{Amount, Bolt11Invoice, InviteCode, LnSendState, OperationKind};

#[tokio::main]
async fn main() -> fedimint_sdk::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: walkthrough <data-dir> <invite-code> [invoice]";
    let (data_dir, invite, invoice) = match args.as_slice() {
        [data_dir, invite] => (data_dir.as_str(), invite.as_str(), None),
        [data_dir, invite, invoice] => (data_dir.as_str(), invite.as_str(), Some(invoice.as_str())),
        _ => common::usage(usage),
    };

    let sdk = common::build(data_dir).await?;

    // Show the user what they are about to join, before joining it.
    let preview = sdk.preview(&invite.parse::<InviteCode>()?).await?;
    println!(
        "{} on {:?}, {} guardians, modules {:?}",
        preview.name.as_deref().unwrap_or("unnamed federation"),
        preview.network,
        preview.guardians,
        preview.modules,
    );
    if let Some(welcome) = preview.meta.get("welcome_message") {
        println!("{welcome}");
    }

    let federation = common::open(&sdk, invite).await?;
    let balance = federation.balance().await?;
    println!("balance: {balance}");

    // What a federation can do is a value to branch on, never an error to
    // provoke: `capabilities()` to lay out a screen, the facade accessors
    // to actually do the work.
    println!("{:?}", federation.capabilities());

    // A fresh wallet has nothing to spend from yet: issue an invoice and wait for it to be
    // paid, the way any first payment into this wallet would arrive.
    if balance.msats() == 0 {
        let lightning = federation
            .lightning()
            .expect("this federation has no lightning module");
        let receive = lightning
            .receive(Amount::from_msats(200_000), "funding")
            .await?;
        println!("invoice: {}", receive.invoice);
        println!("pay this invoice to fund the wallet");
        receive.operation.await_final().await?;
    }

    // Ecash: notes to hand over out of band, plus an operation that says
    // whether they were redeemed or came back. Quote first here too. The
    // mint rounds the request up to a denomination it can issue, and note
    // selection can cost a fee, so the debit is not the amount asked for.
    let ecash = federation
        .ecash()
        .expect("this federation has no mint module");
    let quote = ecash.quote(Amount::from_msats(50_000)).await?;
    println!(
        "{} of notes plus {} fee ({} debited), good until {}",
        quote.notes_value(),
        quote.fee(),
        quote.total(),
        quote.expires_at(),
    );
    let sent = ecash.send(quote).await?;
    println!("notes: {}", sent.notes);
    // Worth persisting, though not required: the notes are readable
    // again from `Operation::details` after a restart, and the id is
    // all it takes to find this send.
    println!("operation: {}", sent.operation.id());
    let ecash_send_id = sent.operation.id();

    // Lightning: quote first, so the user sees the expected cost before
    // agreeing to it, and `send` refuses a quote whose terms have moved.
    if let Some(invoice) = invoice {
        let invoice: Bolt11Invoice = invoice.parse()?;
        let lightning = federation
            .lightning()
            .expect("this federation has no lightning module");
        let quote = lightning.quote(&invoice).await?;
        println!(
            "pay {} plus {} fee ({} total) via {:?}, good until {}",
            quote.invoice_amount(),
            quote.fee(),
            quote.total(),
            quote.route(),
            quote.expires_at(),
        );

        // `send` takes the quote by value: one quote, one payment.
        let payment = lightning.send(quote).await?;

        // The subscriber yields the current state first, then every
        // transition, then `None` once a final state has been seen.
        let mut updates = payment.updates();
        while let Some(state) = updates.next().await? {
            match state {
                LnSendState::Success { preimage, fee, .. } => {
                    // The fee the quote bound, and therefore the fee that
                    // was charged.
                    println!("state: paid, fee {fee}, preimage {preimage}");
                }
                // A payment that does not succeed is not an error: `Refunded` means the
                // money is safe in the balance, `Failed` means it did not resolve into a
                // clean refund. Neither is the call failing, so both print as an ordinary
                // observed state.
                other => println!("state: {other:?}"),
            }
        }
    }

    // Reattaching after a restart: the operation kept running without us. This looks up the
    // ecash send from a moment ago by its id, standing in for an id kept from a previous run.
    match federation.operation(&ecash_send_id).await? {
        Some(operation) => match operation.kind() {
            // The kind says which typed handle to ask for; the handle reads
            // the state the operation reached while nobody was watching.
            OperationKind::EcashSend => {
                if let Some(send) = operation.as_ecash_send() {
                    println!("state: {:?}", send.state().await?);
                }
            }
            OperationKind::LnSend => {
                if let Some(payment) = operation.as_ln_send() {
                    println!("state: {:?}", payment.state().await?);
                }
            }
            // Recorded by a version that understood something this one
            // does not. Still a real row, still listable.
            OperationKind::Unknown => println!("an operation from another version"),
            other => println!("{other:?}"),
        },
        None => println!("no operation with that id here"),
    }

    // Local history, newest first, one page at a time.
    let page = federation.activity(None, 20).await?;
    for item in &page.items {
        println!("{} {:?} {:?}", item.time, item.kind, item.status);
    }

    sdk.shutdown().await
}
