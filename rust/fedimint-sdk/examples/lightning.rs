//! Receiving and sending a lightning payment.
//!
//! Run it with `scripts/run-sdk-examples.sh`, which starts a federation through devimint first:
//! this example builds an instance over a fresh temporary directory, joins that federation, has
//! the faucet pay an invoice it issues, then pays an invoice the faucet issues back.

use devimint_support::{Devimint, faucet};
use fedimint_sdk::{Amount, Bolt11Invoice, Federation, InviteCode, Sdk, Storage};

#[tokio::main]
async fn main() -> fedimint_sdk::Result<()> {
    let devimint = Devimint::detect()
        .expect("no devimint federation found; run this example via scripts/run-sdk-examples.sh");
    let (_storage, sdk, federation) = join(&devimint).await?;

    let lightning = federation.lightning().expect(
        "devimint runs a lightning module; run this example via scripts/run-sdk-examples.sh",
    );

    // Receive: show the invoice before anyone pays it.
    let receive = lightning
        .receive(Amount::from_msats(200_000), "an example")
        .await?;
    println!("pay this invoice: {}", receive.invoice);
    faucet("POST", "/pay", &receive.invoice.to_string())
        .expect("the faucet pays the invoice; run this example via scripts/run-sdk-examples.sh");
    let mut updates = receive.operation.updates();
    while let Some(state) = updates.next().await? {
        println!("{state:?}");
    }
    let details = receive.operation.details().await?;
    println!("received {}", details.net_credit);
    println!("balance: {}", federation.balance().await?);

    // Send: an invoice from the faucet stands in for one a real payee would hand over. Quote
    // first, so the user sees the cost before agreeing to it.
    let invoice: Bolt11Invoice = faucet("POST", "/invoice", "50000")
        .expect("the faucet issues an invoice; run this example via scripts/run-sdk-examples.sh")
        .trim()
        .parse()?;
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
        println!("{state:?}");
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
