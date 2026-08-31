# fedimint-sdk

The high-level Rust SDK over `fedimint-client`: one ergonomic API for wallets
and apps to join federations, hold ecash, and send/receive over Lightning and
on-chain. It is also the single surface every language binding (Swift,
Kotlin, JS/wasm) is meant to generate from.

## Status: API skeleton

This crate currently contains the **public API skeleton only**:

- Every effectful or fallible method body is `unimplemented!()`.
- No `fedimint-*` dependencies yet — the skeleton has zero dependencies.
- The FFI (UniFFI) and wasm layers are not wired up to this crate yet.

The design is tracked in
[fedimint-sdk#344](https://github.com/fedimint/fedimint-sdk/issues/344), the
RFC this crate implements.

Implementation is being split per module across contributors, following the
API defined here.
