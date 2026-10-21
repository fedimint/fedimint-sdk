//! Queries Nostr relays (NIP-87) or a dev fixture to discover federations.
//!
//! Usage: `cargo run --example discovery <data-dir> [dev | nostr [relay...]]`
//!
//! Examples:
//! ```bash
//! # Dev mode (returns display-only placeholder fixture without network queries; not joinable)
//! cargo run --example discovery /tmp/fedimint-test dev
//!
//! # Nostr production mode with default relays
//! cargo run --example discovery /tmp/fedimint-test nostr
//!
//! # Nostr mode with custom relay(s)
//! cargo run --example discovery /tmp/fedimint-test nostr wss://relay.damus.io wss://nos.lol
//! ```

use fedimint_sdk::{DiscoverySource, Sdk, Storage};

#[tokio::main]
async fn main() -> fedimint_sdk::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: discovery <data-dir> [dev | nostr [relay...]]";
    let (data_dir, source) = match args.as_slice() {
        [data_dir] => (data_dir.as_str(), DiscoverySource::Dev),
        [data_dir, mode] if mode == "dev" => (data_dir.as_str(), DiscoverySource::Dev),
        [data_dir, mode] if mode == "nostr" => {
            (data_dir.as_str(), DiscoverySource::nostr_default())
        }
        [data_dir, mode, relays @ ..] if mode == "nostr" => (
            data_dir.as_str(),
            DiscoverySource::Nostr {
                relays: relays.to_vec(),
            },
        ),
        _ => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    };

    let sdk = Sdk::builder()
        .storage(Storage::at(data_dir)?)
        .build()
        .await?;

    match &source {
        DiscoverySource::Dev => println!("Discovering federations using dev source...\n"),
        DiscoverySource::Nostr { relays } => {
            println!("Querying Nostr NIP-87 relays: {}...\n", relays.join(", "))
        }
    }

    let federations = sdk.discover_federations(source).await?;

    if federations.is_empty() {
        println!("No federations found.");
        return Ok(());
    }

    println!("Discovered {} federation(s):", federations.len());
    for (i, fed) in federations.iter().enumerate() {
        println!("\n[{}] Federation ID: {}", i + 1, fed.id);
        println!("    Invite Code:   {}", fed.invite.display());
        if !fed.metadata.is_empty() {
            println!("    Metadata:");
            for (k, v) in &fed.metadata {
                println!("      - {k}: {v}");
            }
        }
    }

    Ok(())
}
