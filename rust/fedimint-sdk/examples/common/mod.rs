//! Shared wallet setup and federation attachment for the runnable examples.

use std::io::{IsTerminal as _, Write as _};

use fedimint_sdk::{
    Error, ErrorCode, Federation, InviteCode, Mnemonic, RecoveryState, Result, Sdk, Storage,
};

/// The open SDK and whether this invocation explicitly chose Restore.
pub struct Wallet {
    sdk: Sdk,
    restore: bool,
}

impl std::ops::Deref for Wallet {
    type Target = Sdk;

    fn deref(&self) -> &Sdk {
        &self.sdk
    }
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorCode::InvalidInput, message)
}

fn read_line(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    std::io::stderr()
        .flush()
        .map_err(|_| invalid("could not display wallet setup"))?;
    let mut line = String::new();
    if std::io::stdin()
        .read_line(&mut line)
        .map_err(|_| invalid("could not read wallet setup"))?
        == 0
    {
        return Err(invalid("wallet setup was cancelled"));
    }
    Ok(line.trim().to_owned())
}

/// Opens existing storage without creating a seed. On fresh storage, ask for Create or
/// Restore. Automation must explicitly select `FM_SDK_EXAMPLE_SETUP=create` (or `restore`).
pub async fn build(data_dir: &str) -> Result<Wallet> {
    let sdk = Sdk::builder()
        .storage(Storage::at(data_dir)?)
        .build()
        .await?;
    let choice = match std::env::var("FM_SDK_EXAMPLE_SETUP") {
        Ok(choice) => Some(choice),
        Err(std::env::VarError::NotPresent) if sdk.has_mnemonic() => None,
        Err(std::env::VarError::NotPresent) if std::io::stdin().is_terminal() => Some(read_line(
            "No wallet seed. Create or Restore? [create/restore]: ",
        )?),
        Err(_) => {
            return Err(invalid(
                "choose Create or Restore interactively, or set FM_SDK_EXAMPLE_SETUP explicitly",
            ));
        }
    };
    let restore = choice.as_deref() == Some("restore");
    match choice.as_deref() {
        Some("create") if !sdk.has_mnemonic() => {
            sdk.set_mnemonic(&Mnemonic::generate()?).await?;
            // Real applications show the words privately and require backup confirmation.
            eprintln!(
                "new wallet in {data_dir}: back up Sdk::export_mnemonic before using real funds"
            );
        }
        Some("restore") => {
            let words = read_line("Restore words (never include these in logs): ")?;
            // Always validate explicit Restore, including when this location has a seed.
            sdk.set_mnemonic(&words.parse::<Mnemonic>()?).await?;
        }
        None | Some("create") => {}
        Some(_) => return Err(invalid("wallet setup must be create or restore")),
    }
    Ok(Wallet { sdk, restore })
}

/// Joins a new wallet, or recovers one whose user explicitly selected Restore.
pub async fn open(wallet: &Wallet, invite: &str) -> Result<Federation> {
    let invite: InviteCode = invite.parse()?;
    if let Some(federation) = wallet.federation(&invite.federation_id()) {
        return Ok(federation);
    }
    if wallet.restore {
        let recovery = wallet.recover(&invite).await?;
        if !matches!(recovery.progress.await_final().await?, RecoveryState::Done) {
            return Err(invalid(
                "recovery did not complete; do not use this wallet yet",
            ));
        }
        Ok(recovery.federation)
    } else {
        wallet.join(&invite).await
    }
}

/// Prints a usage line and exits with status 2.
pub fn usage(text: &str) -> ! {
    eprintln!("{text}");
    std::process::exit(2);
}
