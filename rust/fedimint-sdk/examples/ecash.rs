//! Sending and redeeming out-of-band ecash.
//!
//! Usage:
//!
//! `ecash <data-dir> <invite-code> send <amount-msats>`
//! `ecash <data-dir> <invite-code> receive <notes>`
//! `ecash <data-dir> <invite-code> cancel <operation-id>`
//!
//! Builds an instance over `<data-dir>`, joining `<invite-code>` the first time and
//! reopening the same federation on every later run, then sends notes, redeems notes handed
//! to it out of band, or asks a send to be reclaimed.
//!
//! `scripts/run-sdk-examples.sh` runs this example twice against a federation it starts with
//! devimint, once as the sender and once as the receiver, funding the sender itself first.

mod common;

use fedimint_sdk::{Amount, EcashReceiveState, EcashSendState, Notes, OperationId};

const USAGE: &str = "usage: ecash <data-dir> <invite-code> send <amount-msats>\n       \
    ecash <data-dir> <invite-code> receive <notes>\n       \
    ecash <data-dir> <invite-code> cancel <operation-id>";

/// What this run was asked to do, parsed from the command line before anything is opened.
enum Action {
    Send { msats: u64 },
    Receive { notes: Notes },
    Cancel { id: OperationId },
}

#[tokio::main]
async fn main() -> fedimint_sdk::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data_dir, invite, subcommand, arg] = args.as_slice() else {
        common::usage(USAGE);
    };
    let action = parse_action(subcommand, arg)?;

    let sdk = common::build(data_dir).await?;
    let federation = common::open(&sdk, invite).await?;
    let ecash = federation
        .ecash()
        .expect("this federation has no mint module");

    match action {
        Action::Send { msats } => {
            // Quote first: a mint hands out notes in fixed denominations, so the receiver
            // can end up with slightly more value than asked for, and assembling that value
            // can itself cost a fee; both are known before anything leaves the balance.
            let quote = ecash.quote(Amount::from_msats(msats)).await?;
            println!(
                "{} of notes plus {} fee ({} debited), good until {}",
                quote.notes_value(),
                quote.fee(),
                quote.total(),
                quote.expires_at(),
            );
            let sent = ecash.send(quote).await?;
            println!("notes: {}", sent.notes);
            // The send stays open until the notes are redeemed or reclaimed: this id is what
            // to keep, to check on it later or to cancel it.
            println!("operation: {}", sent.operation.id());
        }
        Action::Receive { notes } => {
            let received = ecash.receive(&notes).await?;
            let mut updates = received.updates();
            let mut last_state = None;
            while let Some(state) = updates.next().await? {
                println!("state: {state:?}");
                last_state = Some(state);
            }
            // `Done` is the only terminal state whose notes became spendable: `Failed`
            // means they were already spent or reclaimed by whoever sent them, so
            // there is no receipt to show.
            match last_state {
                Some(EcashReceiveState::Done) => {
                    let details = received.details().await?;
                    println!(
                        "redeemed {} of notes minus {} fee ({} credited)",
                        details.notes_value, details.fee, details.net_credit,
                    );
                }
                other => println!("did not redeem the notes: {other:?}"),
            }
            println!("balance: {}", federation.balance().await?);
        }
        Action::Cancel { id } => {
            let Some(operation) = federation.operation(&id).await? else {
                common::usage("no operation with that id here");
            };
            let Some(send) = operation.as_ecash_send() else {
                common::usage("that operation is not an ecash send");
            };
            send.request_cancel().await?;
            // Only the federation decides who won: `Redeemed` means the receiver got there
            // first, `Canceled` means the notes came back.
            match send.await_final().await? {
                EcashSendState::Redeemed => println!("state: Redeemed"),
                other => println!("state: {other:?}"),
            }
            println!("balance: {}", federation.balance().await?);
        }
    }

    sdk.shutdown().await
}

fn parse_action(subcommand: &str, arg: &str) -> fedimint_sdk::Result<Action> {
    match subcommand {
        "send" => {
            let Ok(msats) = arg.parse::<u64>() else {
                common::usage(USAGE);
            };
            Ok(Action::Send { msats })
        }
        "receive" => Ok(Action::Receive {
            notes: arg.parse()?,
        }),
        "cancel" => Ok(Action::Cancel { id: arg.parse()? }),
        _ => common::usage(USAGE),
    }
}
