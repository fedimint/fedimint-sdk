//! Sending and redeeming out-of-band ecash.
//!
//! Run it with `scripts/run-sdk-examples.sh`, which starts a federation through devimint first:
//! this example builds two instances over two fresh temporary directories, joins both to that
//! federation, funds the sender through devimint's faucet, sends it some ecash notes, and
//! redeems them into the receiver.

use devimint_support::{Devimint, faucet};
use fedimint_sdk::{
    Amount, EcashReceiveState, EcashSendState, Federation, InviteCode, Lightning, Sdk, Storage,
};

#[tokio::main]
async fn main() -> fedimint_sdk::Result<()> {
    let devimint = Devimint::detect()
        .expect("no devimint federation found; run this example via scripts/run-sdk-examples.sh");
    let (_sender_storage, sender_sdk, sender) = join(&devimint).await?;
    let (_receiver_storage, receiver_sdk, receiver) = join(&devimint).await?;

    let sender_ecash = sender
        .ecash()
        .expect("devimint runs a mint module; run this example via scripts/run-sdk-examples.sh");
    let receiver_ecash = receiver
        .ecash()
        .expect("devimint runs a mint module; run this example via scripts/run-sdk-examples.sh");
    let lightning = sender.lightning().expect(
        "devimint runs a lightning module; run this example via scripts/run-sdk-examples.sh",
    );
    fund(&lightning, 200_000).await?;
    println!("sender balance: {}", sender.balance().await?);

    // Send: quote first. A mint hands out notes in fixed denominations, so the receiver can end
    // up with slightly more value than asked for, and assembling that value can itself cost a
    // fee; both are known before anything leaves the balance.
    let quote = sender_ecash.quote(Amount::from_msats(50_000)).await?;
    println!(
        "{} of notes plus {} fee ({} debited), good until {}",
        quote.notes_value(),
        quote.fee(),
        quote.total(),
        quote.expires_at(),
    );
    let sent = sender_ecash.send(quote).await?;
    println!("give these to the receiver: {}", sent.notes);
    // Worth persisting: the notes are readable again from the operation's details after a
    // restart, and the id is all it takes to find this send.
    println!("resume with {}", sent.operation.id());

    // A real receiver is a separate wallet, reached over a chat message, a QR code or a file:
    // `receiver` above is exactly that, its own instance over its own storage, joined to the
    // same federation as the sender.
    let received = receiver_ecash.receive(&sent.notes).await?;
    match received.await_final().await? {
        EcashReceiveState::Done => {}
        other => panic!("expected the redeemed notes to settle as Done, got {other:?}"),
    }
    let details = received.details().await?;
    println!(
        "redeemed {} of notes minus {} fee ({} credited)",
        details.notes_value, details.fee, details.net_credit,
    );
    println!("receiver balance: {}", receiver.balance().await?);

    // The sender does not hear about the redemption on its own: a send stays in limbo until a
    // reclaim is attempted, by the automatic timer past the record's `reclaim_at` or by asking
    // for one now. Only the federation decides who won, and the receiver got there first, so
    // the request settles the send as `Redeemed` rather than taking the notes back.
    sent.operation.request_cancel().await?;
    match sent.operation.await_final().await? {
        EcashSendState::Redeemed => println!("the receiver got there first; nothing to reclaim"),
        other => println!("{other:?}"),
    }

    println!("sender balance: {}", sender.balance().await?);
    println!("receiver balance: {}", receiver.balance().await?);
    sender_sdk.shutdown().await?;
    receiver_sdk.shutdown().await
}

/// Funds the wallet by having the faucet pay an invoice this SDK issues, standing in for a
/// payment from a customer or a friend.
async fn fund(lightning: &Lightning, msats: u64) -> fedimint_sdk::Result<()> {
    let receive = lightning
        .receive(Amount::from_msats(msats), "funding")
        .await?;
    faucet("POST", "/pay", &receive.invoice.to_string())
        .expect("the faucet pays the invoice; run this example via scripts/run-sdk-examples.sh");
    receive.operation.await_final().await?;
    Ok(())
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
