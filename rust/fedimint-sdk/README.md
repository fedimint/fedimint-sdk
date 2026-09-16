# fedimint-sdk

The high-level Rust SDK over `fedimint-client`: one ergonomic API for wallets
and apps to join federations, hold ecash, and send/receive over Lightning and
on-chain. It is also the single surface every language binding (Swift,
Kotlin, JS/wasm) is meant to generate from.

## Status

The surface documented throughout this crate is implemented behind every
facade: `Ecash`, `Lightning`, `Onchain`, `Meta`, and recovery on `Sdk`. This
crate tracks fedimint `master` at one pinned revision, the same one the
repo's `flake.nix` pins for devimint and the wasm client, so the SDK and the
federation its tests run against always come from one commit. What remains
is the language bindings: the Swift, Kotlin and JavaScript SDKs this surface
is meant to generate.

The wasm layer is not wired up to this crate yet. The UniFFI layer is,
behind the `uniffi` feature: a `#[uniffi::export]` block in `src/sdk.rs`
exposes the calls whose bodies are real (open an instance, show its
mnemonic, preview a federation, join one), and [the Android SDK](../../android)
is generated from them. The exports hand out this crate's own types
(`Sdk`, `FederationPreview`, `FederationId`, `Network`, `Mnemonic`,
`Error`, `ErrorCode`), so a binding is a view of this API rather than a
copy that can drift.

The design is tracked in
[fedimint-sdk#344](https://github.com/fedimint/fedimint-sdk/issues/344), the
RFC this crate implements.

## Examples

`examples/` has four runnable examples, one per facade:

- `walkthrough`: the crate documentation's walkthrough, made runnable.
- `ecash`: sending and redeeming out-of-band ecash.
- `lightning`: receiving and sending a lightning payment.
- `onchain`: depositing bitcoin into the federation and withdrawing it back
  out.

Each one joins a federation devimint stands up, funds itself, does its work,
and prints what happened. Like the integration tests, they need the
`.#wasm-tests` dev shell, the only one with devimint and the rest of the
federation's binaries on PATH. Run all four against a fresh federation with:

```
scripts/run-sdk-examples.sh
```

or a single one by name, optionally on a specific module shape:

```
scripts/run-sdk-examples.sh v2 lightning
```

The default and recommended shape is `v2`. On `v1`, the pinned fedimint
revision's lnv1 client cannot decode a successful send's preimage
([fedimint/fedimint#8969](https://github.com/fedimint/fedimint/issues/8969)),
so the `walkthrough` and `lightning` examples end with an `Internal` error
after the payment has actually gone through; `ecash` and `onchain` are
unaffected.
