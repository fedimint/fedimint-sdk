# fedimint-sdk

The high-level Rust SDK over `fedimint-client`: one ergonomic API for wallets
and apps to join federations, hold ecash, and send/receive over Lightning and
on-chain. It is also the single surface every language binding (Swift,
Kotlin, JS/wasm) is generated from.

## Status

The surface documented throughout this crate is implemented behind every
facade: `Ecash`, `Lightning`, `Onchain`, `Meta`, and recovery on `Sdk`. This
crate tracks fedimint `master` at one pinned revision, the same one the
repo's `flake.nix` pins for devimint and the wasm client, so the SDK and the
federation its tests run against always come from one commit.

The language bindings are generated from this surface with UniFFI, behind
the `uniffi` feature. The exports hand out this crate's own types, so a
binding is a view of this API rather than a copy that can drift:

- [the Android SDK](../../android), in Kotlin
- [the iOS SDK](../../ios), in Swift
- [the browser SDK](../../js/web/sdk-web), in TypeScript
- the React Native bindings
  ([`js/react-native/react-native-bindings`](../../js/react-native/react-native-bindings))
  and the [`@fedimint/react-native`](../../js/react-native/react-native)
  package built over them

The Kotlin and the Swift are generated when their SDK is built. The browser
and React Native bindings are committed, and are regenerated with
`just generate-sdk-web-bindings` and `just generate-sdk-rn-bindings` when
this surface changes.

The design is tracked in
[fedimint-sdk#344](https://github.com/fedimint/fedimint-sdk/issues/344), the
RFC this crate implements.

## Examples

`examples/` has four runnable examples, one per facade. Each is an ordinary command-line
program: `<data-dir> <invite-code>` first, then a subcommand of its own. It works against any
federation the invite code names, does its work, and prints what a person or another program
needs to act on next (an invoice, an address, some notes, an operation id), then waits.

- `walkthrough <data-dir> <invite-code> [invoice]`: the crate documentation's walkthrough, made
  runnable.
- `ecash <data-dir> <invite-code> send <amount-msats>`, `... receive <notes>`,
  `... cancel <operation-id>`: sending and redeeming out-of-band ecash.
- `lightning <data-dir> <invite-code> receive <amount-msats> [description]`,
  `... send <invoice>`: receiving and sending a lightning payment.
- `onchain <data-dir> <invite-code> receive`, `... send <address> <sats>`: depositing bitcoin
  into the federation and withdrawing it back out.

The data directory is created if it does not exist and is meant to be reused across runs: the
first run generates a seed and joins the federation, and every later run over the same directory
reopens it, which is how a send finds the balance an earlier receive brought in, and how an
operation id a previous run printed can be picked up again by a later one. Run one by hand
against any federation you already have an invite code for, for example:

```
cargo run --example lightning -- ./wallet <invite-code> receive 100000
```

`scripts/run-sdk-examples.sh` runs all four against a federation it stands up with devimint,
playing every counterparty itself: paying and issuing invoices through devimint's faucet, and
depositing to and confirming with devimint's bitcoind. It enters the `.#wasm-tests` dev shell
itself, the only one with devimint and the rest of the federation's binaries on PATH, so it can
be started from a plain shell. Run all four against a fresh federation with:

```
scripts/run-sdk-examples.sh
```

or a single one by name, optionally on a specific module shape:

```
scripts/run-sdk-examples.sh v2 lightning
```

or hand the federation to your own shell instead, to run the examples by hand against it with
the invite code, a fresh data directory and the counterparty commands in the environment:

```
scripts/run-sdk-examples.sh v2 shell
```

The default and recommended shape is `v2`. On `v1`, the pinned fedimint
revision's lnv1 client cannot decode a successful send's preimage
([fedimint/fedimint#8969](https://github.com/fedimint/fedimint/issues/8969)),
so the `walkthrough` and `lightning` examples end with an `Internal` error
after the payment has actually gone through; `ecash` and `onchain` are
unaffected.
