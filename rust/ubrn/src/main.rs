//! Runs uniffi-bindgen-react-native's command line, at the version this crate's Cargo.lock pins.
//!
//! See this crate's Cargo.toml for what it generates and how it reaches the dev shells.

fn main() -> ubrn_cli::Result<()> {
    <ubrn_cli::cli::CliArgs as clap::Parser>::parse().run()
}
