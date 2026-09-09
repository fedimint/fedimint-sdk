# fedimint-sdk

The high-level Rust SDK over `fedimint-client`: one ergonomic API for wallets
and apps to join federations, hold ecash, and send/receive over Lightning and
on-chain. It is also the single surface every language binding (Swift,
Kotlin, JS/wasm) is meant to generate from.

## Status: API skeleton

This crate currently contains the **public API skeleton only**:

- Every effectful or fallible method body is `unimplemented!()`.
- The `fedimint-*` client crates are declared but not used yet.
- The crate tracks fedimint `master` at one pinned revision, the same one
  the repo's `flake.nix` pins for devimint and the wasm client, so the SDK
  and the federation its tests run against always come from one commit.
- The wasm layer is not wired up to this crate yet. The UniFFI layer is,
  behind the `uniffi` feature: a `#[uniffi::export]` block in `src/sdk.rs`
  exposes the calls whose bodies are real — open an instance, show its
  mnemonic, preview a federation, join one — and [the Android SDK](../../android)
  is generated from them. The exports hand out this crate's own types
  (`Sdk`, `FederationPreview`, `FederationId`, `Network`, `Mnemonic`,
  `Error`, `ErrorCode`), so a binding is a view of this API rather than a
  copy that can drift.

The design is tracked in
[fedimint-sdk#344](https://github.com/fedimint/fedimint-sdk/issues/344), the
RFC this crate implements.

Implementation is being split per module across contributors, following the
API defined here.
