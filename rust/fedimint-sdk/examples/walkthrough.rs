//! The crate documentation's walkthrough, made runnable.
//!
//! Run it with `scripts/run-sdk-examples.sh`, which starts a federation through devimint first:
//! this example builds an instance over a fresh temporary directory, joins that federation, funds
//! itself through devimint's faucet, then spends and receives the way `src/lib.rs` describes.

use devimint_support::{Devimint, faucet};
use fedimint_sdk::{Amount, Bolt11Invoice, InviteCode, LnSendState, OperationKind, Sdk, Storage};

#[tokio::main]
async fn main() -> fedimint_sdk::Result<()> {
    let devimint = Devimint::detect()
        .expect("no devimint federation found; run this example via scripts/run-sdk-examples.sh");

    // One storage, one seed, as many federations as the user joins. Leaving `.mnemonic(..)` off
    // entirely uses the seed already in this storage, or, since a fresh temporary directory is
    // always empty, generates a new one, which `build` reports as `ErrorCode::Entropy` in the
    // rare case the platform's random source fails.
    let storage = tempfile::tempdir().expect("a temporary directory");
    let path = storage.path().to_str().expect("a utf-8 path");
    let sdk = Sdk::builder().storage(Storage::at(path)?).build().await?;
    // This is what an application would back up: the words are the only way to restore this
    // wallet.
    println!("seed phrase: {}", sdk.export_mnemonic().words().join(" "));

    // Show the user what they are about to join, before joining it.
    let invite: InviteCode = devimint.invite.parse()?;
    let preview = sdk.preview(&invite).await?;
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

    let federation = sdk.join(&invite).await?;
    println!("balance: {}", federation.balance().await?);

    // What a federation can do is a value to branch on, never an error to
    // provoke: `capabilities()` to lay out a screen, the facade accessors
    // to actually do the work.
    let capabilities = federation.capabilities();
    println!("{capabilities:?}");

    // A runnable walkthrough needs a balance to spend from, which the doc version could skip.
    // The faucet stands in for any payer: it pays an invoice this SDK issues, the way a customer
    // or a friend would.
    let lightning = federation
        .lightning()
        .expect("devimint runs a lightning module");
    let receive = lightning
        .receive(Amount::from_msats(200_000), "funding")
        .await?;
    faucet("POST", "/pay", &receive.invoice.to_string()).expect("the faucet pays the invoice");
    receive.operation.await_final().await?;
    println!("balance: {}", federation.balance().await?);

    // Ecash: notes to hand over out of band, plus an operation that says
    // whether they were redeemed or came back. Quote first here too. The
    // mint rounds the request up to a denomination it can issue, and note
    // selection can cost a fee, so the debit is not the amount asked for.
    let mut ecash_send_id = None;
    if let Some(ecash) = federation.ecash() {
        let quote = ecash.quote(Amount::from_msats(50_000)).await?;
        println!(
            "{} of notes plus {} fee ({} debited), good until {}",
            quote.notes_value(),
            quote.fee(),
            quote.total(),
            quote.expires_at(),
        );
        let sent = ecash.send(quote).await?;
        println!("give these to the receiver: {}", sent.notes);
        // Worth persisting, though not required: the notes are readable
        // again from `Operation::details` after a restart, and the id is
        // all it takes to find this send.
        println!("resume with {}", sent.operation.id());
        ecash_send_id = Some(sent.operation.id());
    }

    // Lightning: quote first, so the user sees the expected cost before
    // agreeing to it, and `send` refuses a quote whose terms have moved.
    // What finally left the balance is read from the operation's details.
    if let Some(lightning) = federation.lightning() {
        // An invoice from the faucet stands in for one a real payee would hand over.
        let invoice: Bolt11Invoice = faucet("POST", "/invoice", "50000")
            .expect("the faucet issues an invoice")
            .trim()
            .parse()?;
        // An invoice states its own amount. One that does not cannot be
        // paid at all, so there is nothing to override here.
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
                    println!("paid, fee {fee}, preimage {preimage}");
                }
                // Not an error: the payment did not go through, and the
                // money is back in the balance.
                LnSendState::Refunded => println!("refunded"),
                other => println!("{other:?}"),
            }
        }
    }

    // Reattaching after a restart: the operation kept running without us. This example
    // reattaches to the ecash send from a moment ago, standing in for an id kept from a
    // previous run.
    if let Some(id) = ecash_send_id {
        match federation.operation(&id).await? {
            Some(operation) => match operation.kind() {
                OperationKind::LnSend => {
                    if let Some(payment) = operation.as_ln_send() {
                        println!("still going: {:?}", payment.state().await?);
                    }
                }
                // Recorded by a version that understood something this one
                // does not. Still a real row, still listable.
                OperationKind::Unknown => println!("an operation from another version"),
                other => println!("{other:?}"),
            },
            None => println!("no operation with that id here"),
        }
    }

    // Local history, newest first, one page at a time.
    let page = federation.activity(None, 20).await?;
    for item in &page.items {
        println!("{} {:?} {:?}", item.time, item.kind, item.status);
    }

    sdk.shutdown().await
}
