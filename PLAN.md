# Execution plan: `fedimint-sdk` crate scaffolding and API skeleton

This is the implementation plan for the **first part** of
[fedimint-sdk#344](https://github.com/fedimint/fedimint-sdk/issues/344) — the crate
scaffolding and the full public API skeleton. It is written for a Claude **Opus 5**
agent to execute end to end.

The staged approach was agreed in the issue thread: _"create full Rust sdk still with
the API we agree on here but w/o any implementation (`unimplemented!()` body in methods
everywhere) and not yet connect it to the FFI"_ (zeenix). Contributors then divide the
implementation work per module. This plan produces exactly that skeleton PR.

---

## 0. Instructions to the executing agent

- **Read the sources first.** Before writing any code, read issue
  [fedimint-sdk#344](https://github.com/fedimint/fedimint-sdk/issues/344) **and all of
  its comments** (via the GitHub MCP tools: `issue_read` with `get` and `get_comments`).
  The issue body is the _draft_ RFC; the comments contain a review whose corrections
  were **accepted point by point** by the issue author. Section 2 of this plan is the
  reconciled decision ledger — if you find a conflict between this plan and the thread,
  the thread wins for _decisions_, this plan wins for _scope_ (skeleton only). Flag any
  conflict you find in your final report.
- **Use subagents.** Fan work out with the Agent tool. Model selection rule:
  **Sonnet for mechanical tasks** (writing files whose exact content or signatures this
  plan already specifies, CI YAML edits, rustdoc formatting passes, running check
  loops); **Opus for judgment tasks** (the operation-model generics, doc-contract
  wording for invariants, the final adversarial audit). **Every subagent, regardless of
  model, must be launched with effort set to `max`.** Launch independent subagents in
  parallel in a single message.
- **Branch discipline.** Work on the branch your session/harness designates. If none
  is designated, create `feat/fedimint-sdk-skeleton` from the latest `main`. Never
  commit directly to `main`. Commit in the reviewable sequence given in section 7 and
  push that same branch with `git push -u origin <branch>`. Do not open a PR unless
  the user asks for one.
- **Do not implement behavior.** Every method that would touch a federation, storage,
  the network, entropy, or the system clock has an `unimplemented!()` body. The only
  Rust code with real logic is the trivial pure-type layer explicitly listed in
  section 5.13 (const-like constructors and accessors on `Amount`, `Sats`,
  `Timestamp`). When in doubt: `unimplemented!()`.
- **No `anyhow`.** The crate must not depend on `anyhow` at all, and no public
  signature may ever expose `anyhow::Error` (or any other foreign error type). The
  crate's own `Error`/`ErrorCode` (section 5.2) is the only error surface. Context:
  upstream is separately working on structured errors
  ([fedimint#8821](https://github.com/fedimint/fedimint/issues/8821)); **do not assume
  that work** — the SDK's error type must stand on its own either way. When
  implementation starts (later phase, not yours), upstream `anyhow` errors get mapped
  to `ErrorCode` in one internal seam so #8821 landing later is a cheap swap.
- **No fedimint-\* dependencies yet.** The skeleton defines all of its own types
  (a design rule from the RFC: no `fedimint-*` type crosses this API). Dependencies on
  the published `fedimint-*` 0.12 crates arrive with the implementation phase, not in
  this PR. Target dependency set for the skeleton: **none** (std only).

---

## 1. Scope

**In scope (this PR):**

1. New crate `fedimint-sdk` at `rust/fedimint-sdk/` — standalone package with its own
   committed `Cargo.lock`, mirroring how `rust/fedimint-client-uniffi` is set up.
2. The complete public API from section 5, every fallible/effectful body
   `unimplemented!()`, with **full rustdoc on every public item** documenting the
   contract (the thread agreed docs are part of the API skeleton).
3. CI coverage for the new crate via a **new** workflow file
   `.github/workflows/rust-sdk-ci.yaml` (section 6.1). The existing `rust-ci.yaml`
   stays untouched.
4. A test-harness stub: `tests/integration.rs` with one `#[ignore]`d test documenting
   the devimint-based integration-test plan (section 6.3). No real tests — agreed in
   the thread: harness only until the API is implemented.

**Explicitly out of scope (do not touch):**

- Any implementation against `fedimint-client`, storage backends, or networking.
- The FFI/UniFFI layer and the wasm layer. The thread's open question (separate ffi
  crate vs. uniffi custom types in-crate) stays open; nothing in this PR decides it.
  Do not restructure or edit `rust/fedimint-client-uniffi/`.
- A cargo workspace at `rust/`. `nix/ffi.nix` hardcodes
  `crateDir = ../rust/fedimint-client-uniffi` and the react-native `ubrn` builds
  consume that crate's own lockfile; converting `rust/` into a workspace would move
  `Cargo.lock` and break those. Keep the new crate standalone.
- `flake.nix`, `nix/`, `Justfile`, anything under `js/`, and **every existing
  workflow file** — including `rust-ci.yaml`: the new crate gets its own workflow
  (section 6.1), so existing check names and path filters stay stable.
- Publishing to crates.io. Set `publish = false` for now; reserving the `fedimint-sdk`
  name under the fedimint org is a maintainer action, not yours.
- Creating follow-up GitHub issues for the per-module implementation split (maintainers
  will do this per the thread).

---

## 2. Decision ledger (RFC + accepted thread corrections)

The API in section 5 is the RFC **as amended by the thread**. Every item below was
explicitly accepted by the issue author and is **binding**; the executing agent's final
audit (section 7, phase 6) must verify each one against the diff.

| #   | Decision                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | Origin                                        |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------- |
| D1  | Executable quotes: `quote()` returns an opaque, expiring quote binding invoice, amount override, gateway, fee, total debit, config context; `send(quote)` consumes it; `QuoteExpired`/`QuoteChanged` errors. Applies to Lightning **and** Onchain. Gives amountless bolt11 invoices a home (`amount: Option<Amount>` on `quote`).                                                                                                                                                                                                                                               | Review pt. 1, accepted                        |
| D2  | Fallible observation: `updates()` returns a distinct `OperationUpdates<S>` subscriber; `next() -> Result<Option<S>>`; `Ok(None)` means exactly "final state observed, closed cleanly"; each `updates()` call is an independent subscriber (no shared hidden cursor); `state()` returns `Result<S>`. Dropping a subscriber never aborts the operation.                                                                                                                                                                                                                           | Review pt. 2, accepted                        |
| D3  | Reattach is async and fallible: `Federation::operation(&self, id) -> Result<Option<AnyOperation>>`; `OperationKind` gains `Unknown` (persisted operations outlive SDK versions).                                                                                                                                                                                                                                                                                                                                                                                                | Review pt. 3, accepted                        |
| D4  | `leave()` is replaced by `close_federation()` (retain data) + `forget_federation()` (destructive; gated on zero balance, no non-final operations, no reclaimable outgoing value, no recovery in progress). Outstanding handles fail with `FederationClosed`.                                                                                                                                                                                                                                                                                                                    | Review pt. 4, accepted                        |
| D5  | Recovery is **not** in the stable 0.1 surface: `Sdk::recover` + `Recovery` + `RecoveryState` live behind an off-by-default `experimental` cargo feature, documented as unstable, citing fedimint#8908 and fedimint#8934.                                                                                                                                                                                                                                                                                                                                                        | Review pt. 5, accepted                        |
| D6  | "One shape" means one semantic model + mechanical per-target adapters sharing conformance tests, not literal type identity. (Affects docs wording only in this PR.)                                                                                                                                                                                                                                                                                                                                                                                                             | Review, accepted                              |
| D7  | Capability facades are `Option`: `ecash()`, `lightning()`, `onchain()` return `Option<...>`, plus a `capabilities()` accessor for UI capability discovery. `ErrorCode::NotSupported` remains only for config changes after a facade was obtained.                                                                                                                                                                                                                                                                                                                               | Review, accepted; settles RFC open question 2 |
| D8  | Lightning route is exposed: `LightningRoute::{Internal, Gateway { gateway_id }}`, with the final fee, available from quote and final states; docs must state that both upstream LN state machines (internal + gateway) map into the one `LnSendState`.                                                                                                                                                                                                                                                                                                                          | Review, accepted                              |
| D9  | Ecash reclamation: a documented default reclaim-after period is retained on `Ecash::send`; cancellation is `request_cancel()` (accepted-request semantics; the actual outcome lands in `EcashSendState`, since redemption can win the race).                                                                                                                                                                                                                                                                                                                                    | Review, accepted                              |
| D10 | Activity is documented as **local** history (not complete history); `ActivityItem` carries `amount`, `fee`, and `direction` as separate fields; `ActivityStatus` includes `Refunded` and `Canceled`; timestamps are locally recorded, stated as such.                                                                                                                                                                                                                                                                                                                           | Review, accepted                              |
| D11 | Seed/storage lifecycle contract, documented on `SdkBuilder::build`/`Storage`: opening existing storage with a different mnemonic fails `SeedMismatch` before mutation; a generated mnemonic is durably stored before any federation state; concurrent open of the same native storage fails `StorageInUse`; explicit `Sdk::shutdown()` exists; a federation that cannot open is reported, not silently dropped; the seed→federation-secret derivation is versioned and pinned by test vectors (documented now, vectors land with implementation).                               | Review, accepted                              |
| D12 | `mnemonic()` is renamed `export_mnemonic()`; `Mnemonic` does **not** implement `Debug` ("the type won't implement `Debug`" — verbatim acceptance) and has a zeroize-on-drop contract. This plan's reading of the secure-storage point: protecting _exported_ copies (Swift/Kotlin/JS strings) is documented as the app's responsibility, while encryption/secure-storage integration for the _persisted_ seed is recorded in `Storage` rustdoc as a recognized future additive design point (same treatment as D19) — surface disagreement in review if this split reads wrong. | Review, accepted                              |
| D13 | Distinct `Sats` type for on-chain amounts (whole satoshis; no silent flooring anywhere); wasm `u64` representation (BigInt) is recorded as a binding-layer rule in docs; SDK `Timestamp` in integer epoch units replaces `SystemTime` in the public API.                                                                                                                                                                                                                                                                                                                        | Review, accepted                              |
| D14 | `ErrorCode` additionally includes `SeedMismatch`, `StorageInUse`, `QuoteExpired`, `QuoteChanged`, `AmountRequired`, `NetworkMismatch`, `PendingOperations`, `FederationClosed`, `UnsupportedOperation`; `message` is documented as non-stable; enums are `#[non_exhaustive]` with a documented unknown-variant strategy for generated foreign enums.                                                                                                                                                                                                                            | Review, accepted                              |
| D15 | Strict federation-wide module-version rule, **no override**: all modules of a federation must be the same generation; validated at preview, join/open, and config update; mixed federations fail `UnsupportedFederation` with diagnostics naming the conflicting modules; validation covers all modules, not only exposed facades.                                                                                                                                                                                                                                              | Review, accepted; settles RFC open question 5 |
| D16 | Metadata: separated raw accessors `config_metadata()` and `consensus_metadata()` (revisioned) **plus** a merged convenience view (`get`/`all`) with documented precedence (consensus meta overrides config meta).                                                                                                                                                                                                                                                                                                                                                               | Review + author nuance                        |
| D17 | Naming: crate is `fedimint-sdk`; consistent `send`/`receive` verbs across facades (settles RFC open question 6).                                                                                                                                                                                                                                                                                                                                                                                                                                                                | Thread                                        |
| D18 | Per-federation key derivation reuses upstream's existing scheme (`fedimint-bip39` + the standard per-federation child derivation, `get_default_client_secret`, domain-separated by federation id) for isolation **and** seed portability with fedimint-cli/multimint/Fedi. Named and versioned in docs now; test vectors with implementation. Do not invent a new derivation path.                                                                                                                                                                                              | sadeeqrabiu + zeenix                          |
| D19 | Cross-process DB-lock delegation is out of the 0.1 contract; concurrent open is a clean `StorageInUse` error; lock delegation is recorded in docs as an additive future option behind `Storage`.                                                                                                                                                                                                                                                                                                                                                                                | zeenix                                        |
| D20 | Binding-layer requirements recorded as design rules (docs in this PR, enforcement later): wasm entry point installs a panic hook from day one; the uniffi response path keeps no-panic discipline (`panic = "abort"` there); every in-flight operation must fail observably on transport/worker death.                                                                                                                                                                                                                                                                          | zeenix (from #330/#350 experience)            |
| D21 | Replay semantics: `updates()` yields the current state first, then transitions — never a promise of full historical transition replay (settles RFC open question 3).                                                                                                                                                                                                                                                                                                                                                                                                            | Review, accepted                              |
| D22 | FFI streams: async pull on a distinct subscriber object (settles RFC open question 1) — this PR reflects it only in the Rust shape (`OperationUpdates` / `BalanceUpdates` objects with `async next`).                                                                                                                                                                                                                                                                                                                                                                           | Review, accepted                              |

RFC open question 4 (recovery promises) is settled by D5. The remaining genuinely open
item — whether the FFI layer is a separate crate or in-crate via uniffi custom types —
is **not** decided by this PR (see out-of-scope).

---

## 3. Repository facts the agent must respect

Verified against the repo as of this writing; re-verify cheaply before relying on them.

- The tree is grouped by platform: `rust/`, `js/`, `nix/`, `scripts/`. Rust code lives
  in per-crate directories under `rust/`, each with its **own committed `Cargo.lock`**
  (no workspace).
- `rust/fedimint-client-uniffi` is the existing crate (edition 2021, version 0.11.x
  line). It has its own crate-local `.gitignore` (which uses an unanchored `target`
  among platform-specific Kotlin/Swift entries). The root `.gitignore` ignores only
  `rust/fedimint-client-uniffi/target/` — give the new crate a local `.gitignore`
  containing `/target` (do not copy the sibling's platform entries).
- `.github/workflows/rust-ci.yaml` runs two jobs (`test`: `cargo test --locked`;
  `check`: `cargo fmt --all -- --check` + `cargo clippy --locked --all-targets`) with
  `working-directory: rust/fedimint-client-uniffi`, path-filtered to that directory.
  Toolchain comes from `actions-rust-lang/setup-rust-toolchain@v1` (stable), not nix.
  **That action defaults `RUSTFLAGS` to `-D warnings`** — every warning is a hard
  error in CI, which is why section 6.2's local gates set the same flag.
- Other workflows WILL run on this PR and are not yours to change:
  `pull-request.yml` → `verify.yml` path-ignores `rust/fedimint-client-uniffi/**` but
  not `rust/fedimint-sdk/**` or `.github/workflows/**`, so the full JS Verify suite
  runs, including a repo-wide `prettier --check` (`pnpm --dir js lint`) that covers
  any Markdown/YAML this PR adds — hence the prettier gate in 6.2. Adding a file
  under `.github/workflows/` also triggers `react-native-pr.yml` (it watches
  `.github/**`). Expected noise; note as a maintainer follow-up (not this PR) that
  `rust/fedimint-sdk/**` could be added to `pull-request.yml`'s paths-ignore to
  mirror the sibling crate's exclusion.
- `nix/ffi.nix` and the react-native `ubrn` pipeline consume
  `rust/fedimint-client-uniffi` by path — reason the workspace conversion is banned.
- Upstream `fedimint-*` 0.12.0 is published on crates.io (edition 2024, carries the
  uniffi work). Not a dependency of this PR, but the docs may reference the 0.12 line
  as the implementation target.
- devimint is already wired into this repo for the JS tests via the `fedimint` flake
  input and `scripts/setup_test_shell.sh` (note its module-generation env vars —
  today's fedimint defaults modules to v2; the SDK's v1/v2 all-or-nothing rule exists
  against that backdrop). The Rust integration harness will reuse this machinery later;
  this PR only stubs the entry point.

---

## 4. Crate scaffolding

Create `rust/fedimint-sdk/` with:

```
rust/fedimint-sdk/
├── .gitignore            # exactly "/target" (see section 3 on the sibling's variant)
├── Cargo.toml
├── Cargo.lock            # committed, like the sibling crate
├── README.md             # short: what the crate is, link to issue #344, status: API skeleton
├── src/
│   ├── lib.rs
│   ├── error.rs
│   ├── types/
│   │   ├── mod.rs
│   │   ├── amount.rs     # Amount, Sats
│   │   ├── timestamp.rs  # Timestamp
│   │   ├── mnemonic.rs   # Mnemonic
│   │   ├── ids.rs        # FederationId, OperationId, GatewayId, Txid, Cursor
│   │   ├── invite.rs     # InviteCode, FederationPreview
│   │   ├── notes.rs      # Notes
│   │   ├── invoice.rs    # Bolt11Invoice
│   │   ├── address.rs    # Address
│   │   └── network.rs    # Network
│   ├── storage.rs        # Storage
│   ├── sdk.rs            # Sdk, SdkBuilder
│   ├── federation.rs     # Federation, Capabilities, BalanceUpdates
│   ├── operation.rs      # OperationState, Operation<S>, OperationUpdates<S>, AnyOperation, OperationKind
│   ├── ecash.rs          # Ecash, EcashSend, EcashSendState, EcashReceiveState
│   ├── lightning.rs      # Lightning, LnQuote, LightningRoute, LnReceive, LnSendState, LnReceiveState
│   ├── onchain.rs        # Onchain, OnchainQuote, OnchainReceive, OnchainSendState, OnchainReceiveState
│   ├── meta.rs           # Meta, ConsensusMetadata
│   ├── activity.rs       # ActivityItem, ActivityStatus, Direction, ActivityPage
│   └── recovery.rs       # cfg(feature = "experimental"): Recovery, RecoveryState
└── tests/
    └── integration.rs    # harness stub, see 6.3 — created in phase 5, NOT phase 1
```

`Cargo.toml`:

```toml
[package]
name = "fedimint-sdk"
version = "0.1.0-alpha.1"
edition = "2024"
license = "MIT"
authors = ["The Fedimint Developers"]
description = "High-level Fedimint client SDK: one ergonomic API over fedimint-client, and the single surface every binding generates from"
repository = "https://github.com/fedimint/fedimint-sdk"
publish = false   # flip when the name is reserved and the API settles

[features]
# Unstable APIs, excluded from the 0.1 stability contract. Currently: recovery
# (gated on upstream fedimint#8908 / fedimint#8934 and crash-recovery tests).
experimental = []

[dependencies]
# Intentionally empty. The skeleton defines the full public API with
# unimplemented!() bodies; fedimint-* et al. arrive with the implementation.
# No foreign error-type crates may ever appear here — the public error surface
# is crate-owned (see fedimint/fedimint#8821 for the upstream structured-error
# work this stays decoupled from).
```

`lib.rs` top matter:

```rust
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(missing_debug_implementations)]
// Skeleton-phase allowances — remove both when implementation starts. Parameters
// are deliberately named (they are rustdoc-visible API contract) but unused, and
// the private placeholder `inner` fields are never constructed or read while
// every body is unimplemented!(). CI runs with RUSTFLAGS="-D warnings"
// (section 3), so these must be in-source allows, not tolerated warnings:
#![allow(unused_variables)]
#![allow(dead_code)]
```

Phase-1 `lib.rs` is exactly this header plus a one-line placeholder crate doc
(`//! High-level Fedimint client SDK. API skeleton per fedimint-sdk#344.`) and **no
`mod` declarations** — each later phase adds its own `mod` + `pub use` lines as its
files land, so every phase's commit compiles. `#![deny(missing_docs)]` must never be
weakened to get a phase to compile.

`lib.rs` re-exports everything flat at the crate root (`pub use` from the modules;
modules themselves stay private or `#[doc(hidden)]` — bindings and users see one flat
namespace, which is also what UniFFI will see later). Crate-level rustdoc (docs phase)
carries the RFC's "what using it looks like" example, updated to the amended API
(quote-then-send, `OperationUpdates::next`, `export_mnemonic`), as a ` ```no_run `
doctest so it must _compile_ (never run — bodies are `unimplemented!()`). Because the
crate has zero dependencies there is no async runtime: the example must be an
**uncalled `async fn example() -> fedimint_sdk::Result<()> { ... }`** (optionally with
an empty `fn main() {}`), never a top-level `.await` or `#[tokio::main]` — and no
dev-dependency may be added to make a doctest run.

Derives policy:

- Handle types (`Sdk`, `Federation`, `Ecash`, `Lightning`, `Onchain`, `Meta`,
  `Operation<S>`, `AnyOperation`): `Clone` + `Debug`; contain a private
  `inner: Arc<...>`-shaped placeholder (for the skeleton, a private unit struct is
  fine, e.g. `struct SdkInner;` held as `Arc<SdkInner>` — keeps `Clone` cheap and the
  layout honest). Generic types (`Operation<S>`, `OperationUpdates<S>`) additionally
  carry `_state: PhantomData<S>`, or E0392 ("parameter `S` is never used") rejects
  them; plain `PhantomData<S>` is fine — the `OperationState` bound already
  guarantees `Send + Sync + 'static`. Document that handles are cheap clones sharing
  state, `Send + Sync` on native, and that wasm runs the same types single-threaded.
- Subscriber types (`OperationUpdates<S>`, `BalanceUpdates`): `Debug` only —
  deliberately **not** `Clone` (each is one independent cursor, D2).
- Data types (records, state enums, ids): `Debug, Clone, PartialEq, Eq` (+ `Hash` for
  ids, + `Copy` for `Amount`/`Sats`/`Timestamp`). **Exception: `Mnemonic` implements
  neither `Debug` nor `Display` (D12)** — give the type
  `#[allow(missing_debug_implementations)]` so the crate lint (a warning, hard error
  under CI's `-D warnings`) doesn't reject it, and note `SdkBuilder`, which stores
  one, therefore needs a hand-written `Debug` that redacts the field.
- All public structs that are pure data carry `#[non_exhaustive]`, as do all public
  enums (D14). Construction happens only inside the crate.

---

## 5. The public API, file by file

Signatures below are normative. Rustdoc shown as `///` sketches the contract that must
be written out in full prose (do not copy the terse comments verbatim — write real
documentation, in the register of the RFC text, stating semantics, error codes, and
invariants). `Result<T>` is the crate alias `pub type Result<T, E = Error> = core::result::Result<T, E>;`.

### 5.1 Conventions (document in crate-level rustdoc)

- Errors returned from methods mean **the call failed**. Operation outcomes (payment
  succeeded / refunded / expired) are **states**, never `Err`.
- All string-shaped types implement `Display` and `FromStr` with validating parse, so
  FFI carries them as strings without per-language parsers.
- Every public type is expressible through the **mechanical FFI adapter layer** (D6:
  one semantic model, per-target adapters — not literal type identity): plain records,
  flat data enums, objects with async methods. No tuples, no borrowed returns, no
  `impl Trait` in public signatures. The known, deliberate adaptations, to be listed
  in the crate docs: the ffi layer monomorphizes `Operation<S>` to one concrete type
  per kind; `&mut self` subscriber cursors (`OperationUpdates`, `BalanceUpdates`) get
  wrapped behind a lock to expose `&self` async `next()`; the consuming builder
  (`SdkBuilder`) is flattened to constructor arguments or `&self` setters; quotes
  passed by value (`LnQuote`, `OnchainQuote`) cross as objects consumed
  _semantically_ (reuse fails with `QuoteExpired`/`QuoteChanged`); `BTreeMap` crosses
  as the bindings' map/dict type (`BTreeMap` stays on the Rust side for deterministic
  ordering).
- One semantic model, adapter layers per target, conformance tests shared (D6).
- Native implementation will require tokio (a `fedimint-client` requirement) — a note,
  not a dependency, in this PR.
- Unknown-variant strategy (D14): all public enums are `#[non_exhaustive]`; foreign
  bindings will map unknown future variants to an explicit unknown case; Rust callers
  must write non-exhaustive matches.

### 5.2 `error.rs`

```rust
/// The one error type. Wrappers switch on `code`; humans read `message`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Error {
    /// Stable machine-readable failure category.
    pub code: ErrorCode,
    /// Human-readable context. NOT part of the stability contract: never match on it.
    pub message: String,
}

impl core::fmt::Display for Error { /* "{code:?}: {message}" — implement for real */ }
impl core::error::Error for Error {}

/// Stable machine-readable failure category. Additive-only after 1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    /// Malformed invite, invoice, notes, address, mnemonic, or cursor.
    InvalidInput,
    AlreadyJoined,
    /// Mixed v1/v2 module generations, or config the SDK refuses to operate on.
    /// Diagnostics in `message` name the conflicting modules and versions (D15).
    UnsupportedFederation,
    FederationUnreachable,
    InsufficientBalance,
    /// `forget_federation` while spendable balance remains.
    BalanceNotEmpty,
    /// `forget_federation` while non-final operations or reclaimable value remain.
    PendingOperations,
    GatewayUnavailable,
    /// Recovery still in progress for this federation.
    Recovering,
    /// The backing module is absent (config changed after the facade was obtained).
    NotSupported,
    /// A persisted operation this SDK version cannot interpret.
    UnsupportedOperation,
    /// Existing storage was initialized with a different mnemonic.
    SeedMismatch,
    /// The storage is already opened by another Sdk instance or process.
    StorageInUse,
    /// The quote passed to `send` has expired.
    QuoteExpired,
    /// Conditions changed since the quote was issued; re-quote.
    QuoteChanged,
    /// An amountless bolt11 invoice needs an explicit amount.
    AmountRequired,
    /// Address network does not match the federation's network.
    NetworkMismatch,
    /// The federation handle was closed (`close_federation` / `Sdk::shutdown`).
    FederationClosed,
    Timeout,
    Storage,
    Internal,
}

/// Crate-wide result alias.
pub type Result<T, E = Error> = core::result::Result<T, E>;
```

Rustdoc on `Error` must state: the full source chain stays inside the crate (visible
via logging/`Debug` in implementation, never via the API); some codes will grow
structured detail fields later (additively) rather than requiring message parsing.

### 5.3 `types/amount.rs`

```rust
/// Millisatoshi amount used for balances, ecash, and lightning. Checked arithmetic only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Amount(/* private */ u64);

impl Amount {
    pub const fn from_msats(msats: u64) -> Self;
    /// Errors… none: multiplication by 1000 checked — use checked ctor:
    pub const fn from_sats(sats: u64) -> Option<Self>;   // None on overflow
    pub const fn msats(self) -> u64;
    /// Whole satoshis, truncating any sub-satoshi remainder.
    pub const fn sats_floor(self) -> Sats;
    /// Exact conversion; `None` if not whole satoshis. On-chain APIs take `Sats`
    /// directly — this never silently floors (D13).
    pub const fn to_sats_exact(self) -> Option<Sats>;
    pub const fn checked_add(self, rhs: Amount) -> Option<Amount>;
    pub const fn checked_sub(self, rhs: Amount) -> Option<Amount>;
}

/// Whole-satoshi amount for on-chain operations (D13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sats(/* private */ u64);

impl Sats {
    pub const fn from_sats(sats: u64) -> Self;
    pub const fn sats(self) -> u64;
    /// `None` on msat overflow (u64::MAX msats < u64::MAX sats).
    pub const fn to_amount(self) -> Option<Amount>;
}
```

Both get `Display` (document the format you pick; suggest `"{n} msat"` / `"{n} sat"`).
Rustdoc records the binding rule: `u64` crosses wasm as `BigInt`, never `number` (D13,
D20-adjacent).

### 5.4 `types/timestamp.rs`

```rust
/// Milliseconds since the Unix epoch. The SDK's only public time type (D13):
/// `SystemTime` does not cross FFI portably.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(/* private */ u64);

impl Timestamp {
    pub const fn from_epoch_millis(millis: u64) -> Self;
    pub const fn epoch_millis(self) -> u64;
}
```

### 5.5 `types/mnemonic.rs`

```rust
/// BIP-39 seed phrase. Redacted `Debug`; zeroized on drop (contract — implementation
/// later). The SDK never persists it outside `Storage`; protecting exported copies
/// (host secure storage) is the application's responsibility (D12).
pub struct Mnemonic { /* private */ }

impl Mnemonic {
    /// Generate a fresh 12-word English mnemonic.
    pub fn generate() -> Self;
    pub fn words(&self) -> Vec<String>;
}
impl core::str::FromStr for Mnemonic { type Err = Error; /* validating parse */ }
// Clone: yes. Debug: NO ("the type won't implement Debug" — D12); annotate the
// type with #[allow(missing_debug_implementations)]. Display: NO — stringifying
// the seed must be a deliberate act (words()); document both refusals.
```

Module-level rustdoc here (or on `SdkBuilder`) records D18: derivation scheme is
upstream's `fedimint-bip39` + standard per-federation child derivation
(`get_default_client_secret`, domain-separated by federation id), version 1, giving
cross-client seed portability (fedimint-cli/multimint/Fedi); test vectors land with the
implementation.

### 5.6 `types/ids.rs`

`FederationId`, `OperationId`, `GatewayId`, `Txid`, `Cursor` — five opaque newtypes,
same shape:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FederationId { /* private */ }
impl core::fmt::Display for FederationId { ... }
impl core::str::FromStr for FederationId { type Err = Error; ... }
```

(`Cursor` is additionally documented as: opaque pagination token from `ActivityPage`,
treat as a value, never construct or interpret.)

### 5.7 `types/invite.rs`, `types/notes.rs`, `types/invoice.rs`, `types/address.rs`, `types/network.rs`

```rust
/// Federation invite code. Display/FromStr with validating parse.
pub struct InviteCode { /* private */ }   // + Display, FromStr as in 5.6

/// Everything needed to render a join screen before committing (from `Sdk::preview`).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FederationPreview {
    pub id: FederationId,
    pub name: Option<String>,
    pub network: Network,
    pub guardians: u16,
    /// Module kind names, e.g. "mint", "ln", "wallet".
    pub modules: Vec<String>,
    /// Config metadata (welcome message etc.).
    pub meta: BTreeMap<String, String>,
}

/// Out-of-band ecash notes string.
pub struct Notes { /* private */ }        // + Display, FromStr
impl Notes { pub fn value(&self) -> Amount; }

/// A bolt11 lightning invoice.
pub struct Bolt11Invoice { /* private */ }  // + Display, FromStr
impl Bolt11Invoice {
    /// `None` for amountless invoices (then `Lightning::quote` requires an amount, D1).
    pub fn amount(&self) -> Option<Amount>;
    pub fn description(&self) -> String;
    pub fn expires_at(&self) -> Timestamp;
    pub fn is_expired(&self) -> bool;
}

/// A bitcoin address. Parsed for well-formedness; network checked at quote/send
/// time against the federation (`NetworkMismatch`).
pub struct Address { /* private */ }      // + Display, FromStr

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Network { Bitcoin, Testnet, Signet, Regtest }
```

(`FederationPreview.meta` and the `Meta` facade need
`use std::collections::BTreeMap;` — the one non-obvious import in the crate; see 5.1
for how maps cross the binding layer.)

### 5.8 `storage.rs`

```rust
/// Where the SDK persists everything: federation state, operation logs, and the
/// (possibly generated) seed. Backend is target-selected behind this type
/// (rocksdb native, OPFS-backed on wasm) and is an implementation detail.
#[derive(Debug)]
pub struct Storage { /* private */ }

impl Storage {
    /// Filesystem-backed storage rooted at `path` (native targets).
    pub fn at(path: &str) -> Result<Storage>;
    /// Ephemeral storage for tests and previews.
    pub fn in_memory() -> Storage;
}
```

Rustdoc carries the D11 lifecycle contract, the D19 note (cross-process lock
delegation is a possible future additive option behind this same type; today a second
opener gets `StorageInUse`), and the D12 note that at-rest seed
encryption / host secure-storage integration is likewise a recognized future additive
design point behind this type. `path: &str` rather than `&Path` is deliberate
(FFI-portable); document that.

### 5.9 `sdk.rs`

```rust
/// The multi-federation root: one storage, one BIP-39 mnemonic, N federations.
/// Per-federation secrets derive from the one seed (see derivation docs);
/// storage is namespaced per federation internally.
#[derive(Debug, Clone)]
pub struct Sdk { /* private Arc */ }

impl Sdk {
    pub fn builder() -> SdkBuilder;

    /// Fetch and validate config for display before joining. Enforces the
    /// federation-wide single-generation rule (D15): mixed → `UnsupportedFederation`.
    pub async fn preview(&self, invite: &InviteCode) -> Result<FederationPreview>;
    /// Join and persist. Same validation as `preview`.
    pub async fn join(&self, invite: &InviteCode) -> Result<Federation>;

    /// All open federations.
    pub fn federations(&self) -> Vec<Federation>;
    pub fn federation(&self, id: &FederationId) -> Option<Federation>;

    /// Stop opening this federation automatically, retaining its data (D4).
    /// Outstanding handles for it start failing with `FederationClosed`.
    pub async fn close_federation(&self, id: &FederationId) -> Result<()>;
    /// Permanently delete local federation state. Fails with `BalanceNotEmpty`
    /// while spendable balance remains, and `PendingOperations` while non-final
    /// operations, reclaimable outgoing value, or an in-progress recovery remain (D4).
    pub async fn forget_federation(&self, id: &FederationId) -> Result<()>;

    /// The seed, for backup display. Deliberate name: this is a secret leaving
    /// the SDK (D12).
    pub fn export_mnemonic(&self) -> Mnemonic;

    /// Flush and close storage and background work. After this, every handle
    /// fails with `FederationClosed`. Required on mobile before process death (D11).
    pub async fn shutdown(&self) -> Result<()>;
}

/// Builder for [`Sdk`].
// Debug is hand-written (redacts the stored mnemonic — Mnemonic itself has no
// Debug impl, so derive would not compile anyway):
pub struct SdkBuilder { /* private */ }

impl SdkBuilder {
    pub fn storage(self, storage: Storage) -> Self;
    /// When omitted, a mnemonic is generated and durably persisted before any
    /// federation-derived state (D11). Supplying one against existing storage
    /// that holds a different seed fails `build` with `SeedMismatch`.
    pub fn mnemonic(self, mnemonic: Mnemonic) -> Self;
    /// Opens storage, reopens every previously joined federation, resumes their
    /// pending operations. A federation that fails to open is reported (D11 —
    /// exact reporting shape may be refined in review; start with failing build).
    pub async fn build(self) -> Result<Sdk>;
}
```

`recovery.rs`, `#[cfg(feature = "experimental")]` (D5 — and re-export from lib.rs under
the same cfg, with `#[doc(cfg(feature = "experimental"))]`-style documentation note in
plain rustdoc text since `doc_cfg` is nightly-only; just say it in words):

```rust
impl Sdk {
    /// EXPERIMENTAL: join and restore from backup + rescan. Not part of the 0.1
    /// stability contract; gated on upstream fedimint#8908 / fedimint#8934 and
    /// crash-at-every-checkpoint recovery tests (D5).
    #[cfg(feature = "experimental")]
    pub async fn recover(&self, invite: &InviteCode) -> Result<Recovery>;
}

#[cfg(feature = "experimental")]
#[derive(Debug)]
#[non_exhaustive]
pub struct Recovery {
    pub federation: Federation,
    pub progress: Operation<RecoveryState>,
}

#[cfg(feature = "experimental")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecoveryState {
    Running,
    Done,
    Failed { reason: String },
}
```

While recovery is running, spends and receives fail with `Recovering`; balance and
activity may be incomplete and changing — document this on `Recovery` (review's
qualification of "readable immediately").

Note: `OperationKind::Recovery` (5.11) is **not** feature-gated — persisted operations
of that kind can exist regardless of the reader's feature set.

### 5.10 `federation.rs`

```rust
/// Handle to one joined federation.
#[derive(Debug, Clone)]
pub struct Federation { /* private Arc */ }

impl Federation {
    pub fn id(&self) -> FederationId;
    pub fn name(&self) -> Option<String>;
    pub fn network(&self) -> Network;
    pub fn invite_code(&self) -> InviteCode;

    /// Spendable ecash balance.
    pub async fn balance(&self) -> Result<Amount>;
    /// Independent subscriber: current balance immediately, then every change (D2/D22).
    pub fn balance_updates(&self) -> BalanceUpdates;

    /// What this federation supports — for capability-driven UI (D7).
    pub fn capabilities(&self) -> Capabilities;
    /// `None` when the federation lacks the backing module (D7).
    pub fn ecash(&self) -> Option<Ecash>;
    pub fn lightning(&self) -> Option<Lightning>;
    pub fn onchain(&self) -> Option<Onchain>;
    /// Unconditional: config metadata always exists; the consensus side is
    /// `None`-shaped inside `Meta` when absent (D16).
    pub fn meta(&self) -> Meta;

    /// Re-attach to a pending or completed operation, e.g. after restart.
    /// Async + fallible: reads persistent state (D3). `Ok(None)` = no such
    /// operation. Storage failure = `Err(Storage)`; uninterpretable persisted
    /// operation = `Ok(Some(op))` with `OperationKind::Unknown`.
    pub async fn operation(&self, id: &OperationId) -> Result<Option<AnyOperation>>;
    /// Local, paginated, cross-module history, newest first (D10). LOCAL history:
    /// what this SDK instance recorded — not reconstructed after seed recovery.
    pub async fn activity(&self, cursor: Option<Cursor>, limit: u16) -> Result<ActivityPage>;

    /// Manual backup trigger. Automatic backups also run after changes that
    /// affect recoverability.
    pub async fn backup(&self) -> Result<()>;
}

/// Capability discovery for UI (D7). non_exhaustive like every data struct:
/// future capability fields must be additive, not breaking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Capabilities {
    pub ecash: bool,
    pub lightning: bool,
    pub onchain: bool,
}

/// Balance subscriber. Not Clone — one independent cursor per `balance_updates()` call.
#[derive(Debug)]
pub struct BalanceUpdates { /* private */ }

impl BalanceUpdates {
    /// Next balance. The first call returns the current balance immediately;
    /// later calls resolve on every change. Fails with `FederationClosed` once
    /// the federation is closed or the Sdk shut down (D4); other `Err`s are
    /// infra failures. Deliberately NOT `Option`-shaped like
    /// `OperationUpdates::next`: a balance stream has no final state, so
    /// "closed cleanly" cannot occur — closure is the terminal signal.
    pub async fn next(&mut self) -> Result<Amount>;
}
```

### 5.11 `operation.rs`

```rust
/// Progress of one operation as a typed state machine. Sealed: the set of
/// operation kinds is defined by the SDK (each maps to an FFI export later).
pub trait OperationState: sealed::Sealed + Clone + Send + Sync + 'static {
    /// Whether this state is terminal (streams end after yielding it).
    fn is_final(&self) -> bool;
}
// pub(crate) mod sealed { pub trait Sealed {} } — pub(crate), NOT private: the
// facade modules must be able to name it to write `impl Sealed for
// EcashSendState` etc. next to each enum (a fully private mod is unnameable
// from sibling modules, E0603). External crates still cannot implement it.
// RecoveryState's impls are #[cfg(feature = "experimental")].

/// Observation handle for a background operation. Operations run from the moment
/// they are created and survive restart; dropping handles or subscribers never
/// aborts them (RFC; settles the cancellable-vs-detached question).
#[derive(Debug, Clone)]
pub struct Operation<S: OperationState> { /* private */ }

impl<S: OperationState> Operation<S> {
    pub fn id(&self) -> OperationId;
    /// Current state. `Err` = infra failure reading it (D2).
    pub async fn state(&self) -> Result<S>;
    /// New INDEPENDENT subscriber: current state immediately, then every
    /// transition (D21). Multiple subscribers never steal each other's updates (D2).
    pub fn updates(&self) -> OperationUpdates<S>;
    /// Wait for the terminal state. `Err` only for infra failure — a failed
    /// payment is an Ok(final state), not an Err.
    pub async fn await_final(&self) -> Result<S>;
}

/// One subscription. Not Clone (one cursor). Dropping it (or a pending `next`)
/// cancels only this subscription, never the operation (D2).
#[derive(Debug)]
pub struct OperationUpdates<S: OperationState> { /* private */ }

impl<S: OperationState> OperationUpdates<S> {
    /// `Ok(Some(state))` per transition; `Ok(None)` exactly when a final state
    /// was already yielded and the subscription closed cleanly; `Err` = infra
    /// failure (stream may not be resumable after an Err) (D2).
    pub async fn next(&mut self) -> Result<Option<S>>;
}

/// Type-erased handle from `Federation::operation` (D3).
#[derive(Debug, Clone)]
pub struct AnyOperation { /* private */ }

impl AnyOperation {
    pub fn id(&self) -> OperationId;
    pub fn kind(&self) -> OperationKind;
    // One accessor per kind; `None` when the kind doesn't match:
    pub fn as_ecash_send(&self) -> Option<Operation<EcashSendState>>;
    pub fn as_ecash_receive(&self) -> Option<Operation<EcashReceiveState>>;
    pub fn as_ln_send(&self) -> Option<Operation<LnSendState>>;
    pub fn as_ln_receive(&self) -> Option<Operation<LnReceiveState>>;
    pub fn as_onchain_send(&self) -> Option<Operation<OnchainSendState>>;
    pub fn as_onchain_receive(&self) -> Option<Operation<OnchainReceiveState>>;
    // NOTE: no accessor for Recovery here in 0.1 (state type is experimental);
    // kind() still reports it. Revisit when recovery stabilizes.
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OperationKind {
    EcashSend, EcashReceive,
    LnSend, LnReceive,
    OnchainSend, OnchainReceive,
    Recovery,
    /// Persisted by a different SDK version or module set; observable but not
    /// interpretable by this version (D3).
    Unknown,
}
```

### 5.12 Facades: `ecash.rs`, `lightning.rs`, `onchain.rs`, `meta.rs`, `activity.rs`

```rust
// ---- ecash.rs ----
/// Chaumian ecash, backed by the mint module.
#[derive(Debug, Clone)]
pub struct Ecash { /* private */ }

impl Ecash {
    /// Create out-of-band notes worth `amount`, deducted from balance.
    /// Unredeemed notes are automatically reclaimed after a documented default
    /// period — one day in the current JS SDK (`js/shared/core` MintService
    /// `tryCancelAfter`), which the docs should state as the default, subject
    /// to confirmation at implementation time (D9). Knobs (custom reclaim
    /// period, etc.) come later as additive `send_with`.
    pub async fn send(&self, amount: Amount) -> Result<EcashSend>;
    /// Redeem out-of-band notes into balance.
    pub async fn receive(&self, notes: &Notes) -> Result<Operation<EcashReceiveState>>;
}

/// Notes to hand to the receiver + the operation tracking redemption/cancellation.
#[derive(Debug)]
#[non_exhaustive]
pub struct EcashSend {
    pub notes: Notes,
    pub operation: Operation<EcashSendState>,
}

/// Cancellation only where it is real (RFC), with request semantics (D9):
impl Operation<EcashSendState> {
    /// Ask to reclaim notes the receiver has not redeemed. Ok = request
    /// accepted; the outcome (Canceled vs Redeemed — redemption can win the
    /// race) arrives via the state machine.
    pub async fn request_cancel(&self) -> Result<()>;
}

/// Illustrative variant set — refine against fedimint-client 0.12's mint state
/// machines during review; keep flat data enums + is_final semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EcashSendState {
    Created,
    CancelRequested,
    /// Final: notes reclaimed into balance.
    Canceled,
    /// Final: receiver redeemed the notes.
    Redeemed,
    /// Final.
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EcashReceiveState {
    Created,
    Issuing,
    /// Final: notes reissued into balance.
    Done,
    /// Final.
    Failed { reason: String },
}

// ---- lightning.rs ----
/// Bolt11 pay and receive via gateways, backed by the ln module. Hides gateway
/// selection, verification, and fee quoting; verification happens before an
/// invoice is created or a payment funded (structurally removes the #296 class).
#[derive(Debug, Clone)]
pub struct Lightning { /* private */ }

impl Lightning {
    /// Executable quote (D1): binds invoice, amount override (required iff the
    /// invoice is amountless — else `AmountRequired`; forbidden mismatch with an
    /// amounted invoice — `InvalidInput`), selected+verified gateway, fee, total
    /// debit, and config context. Expires.
    pub async fn quote(&self, invoice: &Bolt11Invoice, amount: Option<Amount>) -> Result<LnQuote>;
    /// Execute exactly the quoted plan; `QuoteExpired` / `QuoteChanged` otherwise (D1).
    pub async fn send(&self, quote: LnQuote) -> Result<Operation<LnSendState>>;
    /// Issue an invoice payable into this federation.
    pub async fn receive(&self, amount: Amount, description: &str) -> Result<LnReceive>;
}

/// Opaque executable plan (D1). Accessors, no public fields: the binding
/// contract is "display these, then hand the quote back".
#[derive(Debug)]
pub struct LnQuote { /* private */ }
impl LnQuote {
    pub fn invoice_amount(&self) -> Amount;   // resolved amount incl. override
    pub fn fee(&self) -> Amount;
    /// Total balance debit (amount + fee).
    pub fn total(&self) -> Amount;
    pub fn route(&self) -> LightningRoute;
    pub fn expires_at(&self) -> Timestamp;
}

/// How a payment is (or was) routed (D8).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LightningRoute {
    /// Receiver is in the same federation; settles internally without a gateway.
    Internal,
    Gateway { gateway_id: GatewayId },
}

/// The invoice to display + the operation tracking the incoming payment.
#[derive(Debug)]
#[non_exhaustive]
pub struct LnReceive {
    pub invoice: Bolt11Invoice,
    pub operation: Operation<LnReceiveState>,
}

/// Both upstream LN state machines (internal + gateway) map into this one enum;
/// rustdoc must state the mapping (D8). Illustrative — refine in review.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LnSendState {
    Created,
    Funded,
    /// Final. Carries receipt data (D8).
    Success { preimage: String, fee: Amount, route: LightningRoute },
    /// Final: payment failed and the funds returned to balance.
    Refunded,
    /// Final.
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LnReceiveState {
    Created,
    WaitingForPayment,
    Funded,
    /// Final: amount credited to balance.
    Claimed,
    /// Final.
    Canceled { reason: String },
    /// Final: invoice expired unpaid.
    Expired,
}

// ---- onchain.rs ----
/// Peg-in and peg-out, backed by the wallet module. Whole satoshis only (D13).
#[derive(Debug, Clone)]
pub struct Onchain { /* private */ }

impl Onchain {
    /// Fresh deposit address + the operation tracking confirmation and claim.
    pub async fn receive(&self) -> Result<OnchainReceive>;
    /// Executable withdrawal quote (D1): binds address (validated against the
    /// federation network — `NetworkMismatch`), amount, fees, config context. Expires.
    pub async fn quote(&self, address: &Address, amount: Sats) -> Result<OnchainQuote>;
    /// Execute exactly the quoted withdrawal; `QuoteExpired` / `QuoteChanged` otherwise.
    pub async fn send(&self, quote: OnchainQuote) -> Result<Operation<OnchainSendState>>;
}

#[derive(Debug)]
pub struct OnchainQuote { /* private */ }
impl OnchainQuote {
    pub fn amount(&self) -> Sats;
    pub fn fee(&self) -> Sats;
    /// Total debit (amount + fee).
    pub fn total(&self) -> Sats;
    pub fn expires_at(&self) -> Timestamp;
}

/// The deposit address to display + the operation tracking incoming funds.
#[derive(Debug)]
#[non_exhaustive]
pub struct OnchainReceive {
    pub address: Address,
    pub operation: Operation<OnchainReceiveState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OnchainSendState {
    Created,
    /// Final: transaction broadcast; txid for receipts/explorers.
    Succeeded { txid: Txid },
    /// Final.
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OnchainReceiveState {
    WaitingForTransaction,
    WaitingForConfirmation { txid: Txid },
    Confirmed { txid: Txid },
    /// Final: funds claimed into balance.
    Claimed { amount: Sats },
    /// Final.
    Failed { reason: String },
}

// ---- meta.rs ----
/// Federation metadata (D16): merged convenience view over two raw sources,
/// consensus (meta module, revisioned) overriding config meta per key.
#[derive(Debug, Clone)]
pub struct Meta { /* private */ }

impl Meta {
    /// Merged view: consensus overrides config where both define a key.
    pub async fn get(&self, key: &str) -> Result<Option<String>>;
    /// Merged view, all keys.
    pub async fn all(&self) -> Result<BTreeMap<String, String>>;
    /// Raw config metadata (from the federation config; local, infallible).
    pub fn config_metadata(&self) -> BTreeMap<String, String>;
    /// Raw consensus metadata; `None` when the federation has no meta module.
    pub async fn consensus_metadata(&self) -> Result<Option<ConsensusMetadata>>;
}

/// Revisioned consensus metadata (D16). The meta module stores arbitrary bytes
/// commonly interpreted as JSON; the merged view projects top-level JSON object
/// entries to strings (document the projection as lossy).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConsensusMetadata {
    pub revision: u64,
    /// Raw value as a JSON string.
    pub value: String,
}

// ---- activity.rs ----
/// One row of LOCAL cross-module history (D10).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ActivityItem {
    pub operation_id: OperationId,
    pub kind: OperationKind,
    /// Locally recorded, NOT consensus time (D10).
    pub time: Timestamp,
    /// Principal amount (excl. fee); absent for kinds without a fixed amount.
    pub amount: Option<Amount>,
    /// Fee paid, when known; separate from `amount` (D10).
    pub fee: Option<Amount>,
    /// Absent for kinds without a direction (e.g. recovery).
    pub direction: Option<Direction>,
    pub status: ActivityStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Direction { Incoming, Outgoing }

/// Coarse on purpose; full detail lives on the operation. `Refunded`/`Canceled`
/// are first-class because transaction-list UIs need them (D10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ActivityStatus { Pending, Success, Failed, Refunded, Canceled }

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ActivityPage {
    pub items: Vec<ActivityItem>,
    /// Cursor for the next page; `None` at the end.
    pub next: Option<Cursor>,
}
```

### 5.13 The only real code in this PR

Implemented for real (trivial, dependency-free, and they make the `no_run` doctest and
unit-testable surface honest): the field-shuffling constructors/accessors on `Amount`,
`Sats`, `Timestamp` (5.3, 5.4) **including their `Display` impls** (pure formatting),
`Error`'s `Display`/`error::Error` impls, the `Result` alias, `SdkBuilder`'s
hand-written mnemonic-redacting `Debug`, and `Sdk::builder()` returning an empty
builder whose setters store nothing yet (setters may also be `unimplemented!()` — pick
one and be consistent; storing into a placeholder struct is fine). **Everything
else** — every `async fn`, every parse (`FromStr`), `Display` on the opaque
string-shaped types (ids, `InviteCode`, `Notes`, `Bolt11Invoice`, `Address`),
`Mnemonic::generate`/`words`, `Notes::value`, `Bolt11Invoice` accessors,
`Storage::at`/`in_memory`, all facade and operation methods — is `unimplemented!()`.

Add plain unit tests only for the real code above (e.g. `Amount::from_sats` overflow,
`to_sats_exact`, `Display` formats) — this also proves the CI test job works.

---

## 6. CI, gates, and harness

### 6.1 New workflow

Add `.github/workflows/rust-sdk-ci.yaml` — a **separate file**, leaving `rust-ci.yaml`
untouched. Rationale: GitHub Actions path filters are workflow-level, so matrixing the
existing workflow would make every sdk-only PR build the heavy uniffi crate (full
fedimint 0.11 dependency tree) and vice versa, and would rename the existing check
contexts ('Run tests' → 'Run tests (crate)'), silently unmatching any branch-protection
rule pinned to the old names.

Mirror `rust-ci.yaml`'s conventions exactly (triggers shape, `concurrency`,
`permissions: {}`, `actions/checkout` with `persist-credentials: false`,
`actions-rust-lang/setup-rust-toolchain@v1`, ubuntu-24.04, timeouts), with:

- `on.push` (branches: `[main]`) and `on.pull_request`, both path-filtered to
  `rust/fedimint-sdk/**` and `.github/workflows/rust-sdk-ci.yaml`.
- `working-directory: rust/fedimint-sdk` throughout.
- Do **not** override the setup action's `RUSTFLAGS` — its `-D warnings` default is
  wanted.
- `test` job: `cargo test --locked --all-features`.
- `check` job steps:
  - `cargo fmt --all -- --check`
  - `cargo clippy --locked --all-targets` (default features — keeps the
    `experimental` gate from rotting)
  - `cargo clippy --locked --all-targets --all-features`
  - Docs gate: `cargo doc --no-deps --all-features --locked` with
    `RUSTDOCFLAGS: -D warnings` — the doc contract _is_ the deliverable; broken
    intra-doc links are failures.
  - wasm-cleanliness gate:
    `cargo check --locked --target wasm32-unknown-unknown --all-features` (install
    the target via the setup action's `target:` input). The crate has no deps, so
    this is cheap — and it prevents anyone from accidentally landing a native-only
    dependency into the shared-surface crate later. (The wasm build of the
    _implementation_ will have its own story; this gate keeps the API crate portable.)

Do not touch any existing workflow (see section 3 for the other workflows that will
run on this PR anyway).

### 6.2 Local gates (run before every commit; the final run must be clean)

CI hard-errors on all warnings (section 3), so match it locally with
`RUSTFLAGS="-D warnings"` on every cargo command below (fmt excepted; it ignores it).
Install the wasm target first (`rustup target add wasm32-unknown-unknown`) — the wasm
gate is required, not optional.

```
cd rust/fedimint-sdk
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo clippy --locked --all-targets      # default features
RUSTFLAGS="-D warnings" cargo clippy --locked --all-targets --all-features
RUSTFLAGS="-D warnings" cargo test --locked --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --locked
RUSTFLAGS="-D warnings" cargo check --locked --target wasm32-unknown-unknown --all-features
# anyhow gate (see section 0) — structural, so doc prose MAY mention the rule:
! grep -q '^name = "anyhow"$' Cargo.lock            # not even a transitive dependency
! grep -rE '(^|[^_[:alnum:]])anyhow::|use anyhow' src/ tests/
```

Prettier gate (section 3: the JS Verify workflow runs `prettier --check` repo-wide on
this PR): from the repo root, run
`npx prettier --check .github/workflows/rust-sdk-ci.yaml rust/fedimint-sdk/README.md`
plus any other Markdown/YAML files the branch adds, and fix with `--write` (the exact
CI command is `pnpm --dir js install && pnpm --dir js lint`, if you prefer to run the
real thing).

Also confirm `git status` shows no accidental changes outside `rust/fedimint-sdk/` and
`.github/workflows/rust-sdk-ci.yaml`.

### 6.3 Harness stub

`tests/integration.rs`:

- One `#[test] #[ignore = "devimint harness lands with the first implemented facade"]`
  function.
- A file-level comment documenting the agreed plan: integration tests (preferred over
  unit tests per the thread) will run against devimint, reusing the repo's existing
  fedimint flake input and the `scripts/setup_test_shell.sh` pattern (including its
  module-generation env vars — devimint defaults to v2 modules now, which is exactly
  what the D15 single-generation rule must be tested against, in both an all-v1 and an
  all-v2 configuration); wired into CI only once the first facade is implemented.

Created in phase 5 (before the audit, so it gets audited too). No nix or workflow
changes for the harness in this PR.

---

## 7. Execution playbook (phases, subagents, commits)

Every subagent: `effort: max`. Suggested models per phase below. Verify each phase's
output compiles (`cargo check`) before its commit — the phase boundaries below are
drawn so that every commit compiles on its own (phase 3 is deliberately one commit:
`operation.rs`/`federation.rs` and the facade files reference each other's types, so
they cannot land separately). The `error.rs`/`types/` work of phase 2 only needs the
phase-1 scaffold, not the CI file, so its subagent can run while phase 1's CI edit is
still being reviewed.

| Phase | Work                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | Model                                                                                  | Commit                                                                |
| ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| 0     | Main agent: read issue #344 + all comments; read this plan fully; `git status` sanity; confirm section 3 facts still hold.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | Opus (main)                                                                            | —                                                                     |
| 1     | Scaffold: crate dirs, `Cargo.toml`, crate-local `.gitignore`, `lib.rs` exactly per §4 (lint header + placeholder crate doc, NO `mod` declarations), README, `Cargo.lock` (via `cargo check`), new workflow file per 6.1. Mechanical — everything is specified above.                                                                                                                                                                                                                                                                                                                                                                                                    | Sonnet                                                                                 | `feat(sdk): scaffold fedimint-sdk crate and CI`                       |
| 2     | `error.rs` + all of `types/` per 5.2–5.7 (adding their `mod`/`pub use` lines to lib.rs), incl. the real trivial impls (5.13) and their unit tests. Mechanical given the spec.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | Sonnet                                                                                 | `feat(sdk): error taxonomy and core types`                            |
| 3     | The interdependent core, ONE commit, two sequential subagents on the same tree: first `operation.rs` (sealed trait, generics, subscribers) + `storage.rs` + `sdk.rs` + `recovery.rs` + `federation.rs` — judgment-heavy: sealing pattern, cfg-gating, doc contracts D2/D3/D4/D5/D11/D21 (Opus); then the facades `ecash.rs`, `lightning.rs`, `onchain.rs`, `meta.rs`, `activity.rs` per 5.12, incl. the `Operation<EcashSendState>::request_cancel` inherent impl and the state-enum `Sealed`/`OperationState` impls (Sonnet — signatures fully specified; Opus reviews the D1/D8/D9/D10/D16 rustdoc wording). `cargo check` gates the combined output, not the halves. | Opus, then Sonnet                                                                      | `feat(sdk): Sdk, Federation, Operation model, and capability facades` |
| 4     | Docs pass: crate-level rustdoc with the amended `no_run` example (async-fn wrapper per §4); every public item documented; intra-doc links; doc build gate green.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | Sonnet (Opus review of the crate-level contract text — same rule as phase 3's rustdoc) | `docs(sdk): crate-level docs and API contract`                        |
| 5     | Harness stub per 6.3.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | Sonnet                                                                                 | `test(sdk): devimint integration harness stub`                        |
| 6     | Adversarial audit, split two ways. (a) A FRESH subagent gets the issue link and the branch diff only — not this plan — and re-derives the thread's decisions itself, reporting every place the diff violates or omits one. (b) The MAIN agent maps those findings onto D1–D22, runs all 6.2 gates itself, and checks the mechanical rules — no `anyhow`, no fedimint-\* deps, the 5.1 expressibility rules (no tuples, no borrowed returns, no `impl Trait` in public signatures; `Operation<S>` is the one sanctioned generic) — which may be handed to the subagent verbatim as a checklist, since they are not thread-derived. Fix findings, re-run both.            | Opus (both)                                                                            | fixups into prior commits or a final `fix(sdk):` commit               |
| 7     | Final full 6.2 gate run on the finished tree, push.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | Opus (main)                                                                            | —                                                                     |

Push with `git push -u origin <branch>` (retry on network errors with backoff). Do not
open a PR unless the user asks.

### Final report checklist (what the executing agent reports back)

- All 6.2 gates green (paste the command results summary).
- D1–D22 each confirmed with a file/line pointer.
- Any thread↔plan conflicts found (section 0) and how they were resolved.
- Explicit confirmation that nothing outside the two allowed paths
  (`rust/fedimint-sdk/`, `.github/workflows/rust-sdk-ci.yaml`) changed.

---

## 8. Known judgment calls baked into this plan

So review can challenge them in one place — the executing agent must not silently
deviate from these, and should surface disagreement in its report instead:

1. **Standalone crate, no workspace** — forced by `nix/ffi.nix`'s `crateDir` pin and
   the ubrn pipeline (section 3). Workspace consolidation is a future refactor.
2. **Zero dependencies in the skeleton** — even `thiserror`/`futures` are omitted;
   subscriber objects with `async next` replace `impl Stream` returns (which also
   matches D22's FFI answer). A native `futures::Stream` adapter can come later,
   additively.
3. **`experimental` cargo feature** for recovery (vs. a `#[doc(hidden)]` module or an
   `experimental` namespace) — a feature is the loudest, most conventional gate and
   keeps the stable-by-default promise (D5).
4. **`Ecash::send` keeps the plain signature** with a documented default reclaim
   period rather than a mandatory policy parameter (D9 allowed either; RFC's
   "essential surface takes no option structs" rule tips it).
5. **`Meta` is unconditional** while ecash/lightning/onchain are `Option` — config
   meta always exists; D16's raw split covers the module-less case.
6. **State-enum variant sets are marked illustrative** in rustdoc where upstream
   mapping isn't settled (they still must be plausible against fedimint-client 0.12 —
   the audit subagent should sanity-check names against upstream's public state
   machines via docs.rs, without adding a dependency).
7. **`u16` limit + opaque `Cursor`** for `activity` kept from the RFC.
8. **`Sdk::shutdown(&self)`** (not `self`) — handles are cheap clones, consuming
   `self` can't guarantee exclusivity anyway; docs define post-shutdown behavior
   (`FederationClosed`).
9. **`BalanceUpdates::next` returns `Result<Amount>`, not `Result<Option<Amount>>`** —
   a balance stream has no final state, so the `Ok(None)` = "closed cleanly" meaning
   from D2 can never occur; closure surfaces as `Err(FederationClosed)` per D4. A
   deliberate asymmetry with `OperationUpdates`.
10. **A separate `rust-sdk-ci.yaml` instead of matrixing `rust-ci.yaml`** — preserves
    the existing workflow's check-context names (branch protection) and keeps path
    filtering per-crate, at the cost of some YAML duplication.
11. **D12's secure-storage split** (exported copies = app responsibility; persisted
    seed = future design point on `Storage`) is this plan's reading of the review
    point, not verbatim thread text — flagged inside D12 itself.
