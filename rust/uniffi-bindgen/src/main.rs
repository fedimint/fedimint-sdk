//! Runs uniffi-bindgen against an already-built library.
//!
//! Invoked by scripts/generate-android-so.sh; see that script and this
//! crate's Cargo.toml for why it is not a bin inside fedimint-sdk itself.

fn main() {
    uniffi::uniffi_bindgen_main()
}
