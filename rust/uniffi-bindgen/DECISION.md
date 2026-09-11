# FFI shape: in-crate feature vs. a separate `fedimint-sdk-ffi` crate

**Status: Accepted.** The two shapes below were discussed on the dev call on
2026-09-09; the maintainers agreed on Option B (in-crate feature), which is
what this branch implements. This document is the write-up of that
discussion and the evidence behind it, not an open proposal.

T4 asked for a decision between two shapes for the UniFFI surface, backed by a
prototype, before the FFI layer landed. That comparison didn't ship with the
implementation ([fedimint-sdk#384](https://github.com/fedimint/fedimint-sdk/pull/384));
this fills the gap after the fact, against the code as it actually exists on
this branch.

## The two options

**Option A — separate crate.** A `fedimint-sdk-ffi` crate, alongside
`fedimint-sdk`, that depends on it and re-exports a UniFFI-shaped wrapper API:
its own `#[uniffi::export]` blocks, its own error enum, its own `Cargo.lock`.
This is the shape [fedimint-client-uniffi](../fedimint-client-uniffi) already
uses, and what the `spike/ffi-shape` prototype on
[fedimint-sdk#380](https://github.com/fedimint/fedimint-sdk/issues/380) built
and recommended.

**Option B — in-crate feature (what's implemented).** A `uniffi` feature on
`fedimint-sdk` itself, off by default. `#[uniffi::export]` sits directly on
the real methods (`Sdk::preview`, `Sdk::join`, `Mnemonic::generate`, …) behind
`#[cfg_attr(feature = "uniffi", …)]`, and the crate's own `Error` is exported
as the FFI error interface. No wrapper layer exists.

## What was actually measured

Issue #380 raised three costs for Option B. Rechecked against this branch:

| #   | Claim (issue #380)                                                                                                       | Status on this branch                     | Evidence                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| --- | ------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Wasm fragility: UniFFI bounds don't compile on `wasm32-unknown-unknown` without care                                     | **Confirmed, and sharper than described** | `cargo check --target wasm32-unknown-unknown --features uniffi` does not fail on `Send + Sync` bounds — it fails immediately with tokio's own `compile_error!("Only features sync,macros,io-util,rt,time are supported on wasm.")`, because `uniffi/tokio` (which the `uniffi` feature turns on) needs tokio's `net` feature, which is off tokio's wasm allow-list. Reproduced locally; see the guardrail comment added at [`Cargo.toml`'s `[features]` block](../fedimint-sdk/Cargo.toml). |
| 2   | Lint suppression: `setup_scaffolding!()` generates undocumented items that would need to bypass `#![deny(missing_docs)]` | **Did not materialize**                   | No `#[allow(missing_docs)]` exists anywhere near `uniffi::setup_scaffolding!()` in [`lib.rs`](../fedimint-sdk/src/lib.rs), and `cargo clippy --features uniffi -- -D warnings` is clean in CI. UniFFI 0.32's generated items are already `#[doc(hidden)]` internally, which `missing_docs` respects.                                                                                                                                                                                        |
| 3   | Lockfile bloat: UniFFI's proc-macro tree lands in the SDK's committed `Cargo.lock` for every contributor                 | **Confirmed**                             | UniFFI and its proc-macro dependencies are resolved into `fedimint-sdk/Cargo.lock` even though the `uniffi` feature is optional, so every contributor's lockfile carries them whether or not they build with it.                                                                                                                                                                                                                                                                            |

Point 3's counterpart for Option A: a `fedimint-sdk-ffi` crate depending on
both `fedimint-sdk` and `uniffi` would not avoid uniffi's proc-macro tree. As
a separate resolution graph with its own `Cargo.lock`, it would just move
which lockfile carries it, and add a second copy of everything `fedimint-sdk`
itself already locks.

## Decision: Option B, accepted

The costs above are real but contained; the benefit Option A gives up is not.

- **No drift is the load-bearing property.** The SDK's own `lib.rs` states
  the design goal directly: it is "the single surface every language binding
  is meant to generate from," so "a binding is a view of this API, not a copy
  that can drift." Option A is structurally a hand-maintained copy — every
  new facade method needs a matching wrapper, by hand, forever. For a crate
  whose stated purpose is to be the one place the contract is defined, that
  is the bigger long-term risk, and it is exactly the risk `fedimint-client-uniffi`
  (an existing Option-A crate) already lives with today.
- **The wasm risk is fully contained today.** The feature is opt-in and off
  by default; nothing in this repo currently passes `--features uniffi` or
  `--all-features` together with `--target wasm32-unknown-unknown` —
  `scripts/build_wasm.sh` builds a different crate entirely
  (`fedimint-client-wasm` via `nix build .#wasmBundle`), and `fedimint-sdk`'s
  own wasm layer isn't wired up yet. The risk is a future contributor running
  `--all-features` locally or in a tightened CI matrix; the new guardrail
  comment on the feature declaration and this doc are the mitigation, not a
  code change, because the fix (never enable both) is a policy, not a bug.
- **The lint-suppression cost, the other reason Option A was preferred,
  turned out not to apply** to the UniFFI version actually pinned (0.32).
- **The lockfile cost is contained.** It shows up as lockfile entries only;
  none of it compiles unless a contributor enables the optional `uniffi`
  feature, which most never do.

If a second binding target needs its own error mapping or a materially
different shape than the real API later (the PR description already flags
`Federation`'s facades and iOS as out of scope for this stage), that's the
point to revisit Option A for that surface specifically — not a reason to
move the surface implemented so far.
