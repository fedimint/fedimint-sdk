//! One ergonomic API over `fedimint-client`, and the single surface every
//! language binding generates from.
//!
//! An application that wants a Fedimint wallet has to join federations, hold
//! ecash, pay and receive over Lightning, move value on chain, and keep the
//! user informed while all of that runs in the background. Doing that
//! directly against `fedimint-client` means assembling module clients,
//! driving their state machines, choosing and verifying gateways, and
//! inventing a persistence and reattachment story for every long-running
//! action. This crate does that work once and exposes the result as a small
//! set of handles: an [`Sdk`] built over one [`Storage`] and one
//! [`Mnemonic`], a [`Federation`] for each federation joined, a facade per
//! capability ([`Ecash`], [`Lightning`], [`Onchain`], [`Meta`]), and an
//! [`Operation`] for anything that takes longer than a single call.
//!
//! It is also the *one* place that surface is defined. The Swift, Kotlin and
//! JavaScript SDKs are meant to be generated from these types rather than
//! written against `fedimint-client` independently, so a semantic settled
//! here — that a refunded payment is a state rather than an error, that a
//! quote is executed rather than re-derived at send time — is settled for
//! every platform at once, and a fix made here is a fix everywhere.
//!
//! # Status
//!
//! This crate is currently an **API skeleton**. The types, the signatures,
//! and the contract documented throughout are real and are what review is
//! about; the bodies behind them are `unimplemented!()`, and the crate has
//! no dependencies at all. Implementation lands module by module behind this
//! surface. The example below is compiled by the test suite, and must never
//! be run.
//!
//! # A worked example
//!
//! The happy path, end to end: build an instance, look at a federation before
//! committing to it, join, spend ecash, pay an invoice through a quote,
//! follow the payment as it progresses, pick up an operation left over from a
//! previous run, and read a page of history.
//!
//! ```no_run
//! use fedimint_sdk::{
//!     Amount, Bolt11Invoice, InviteCode, LnSendState, Mnemonic, OperationId, OperationKind,
//!     Sdk, Storage,
//! };
//!
//! async fn walkthrough(
//!     data_dir: &str,
//!     phrase: &str,
//!     invite_code: &str,
//!     invoice: &str,
//!     unfinished: Option<&str>,
//! ) -> fedimint_sdk::Result<()> {
//!     // One storage, one seed, as many federations as the user joins.
//!     // Parsing the phrase validates it; leaving `.mnemonic(..)` off
//!     // entirely would use the seed already in this storage, or — if the
//!     // storage is empty — generate a fresh one, which `build` reports as
//!     // `ErrorCode::Entropy` in the rare case the platform's random source
//!     // fails.
//!     let mnemonic: Mnemonic = phrase.parse()?;
//!     let sdk = Sdk::builder()
//!         .storage(Storage::at(data_dir)?)
//!         .mnemonic(mnemonic)
//!         .build()
//!         .await?;
//!
//!     // Show the user what they are about to join, before joining it.
//!     let invite: InviteCode = invite_code.parse()?;
//!     let preview = sdk.preview(&invite).await?;
//!     println!(
//!         "{} on {:?}, {} guardians, modules {:?}",
//!         preview.name.as_deref().unwrap_or("unnamed federation"),
//!         preview.network,
//!         preview.guardians,
//!         preview.modules,
//!     );
//!     if let Some(welcome) = preview.meta.get("welcome_message") {
//!         println!("{welcome}");
//!     }
//!
//!     let federation = sdk.join(&invite).await?;
//!     println!("balance: {}", federation.balance().await?);
//!
//!     // What a federation can do is a value to branch on, never an error to
//!     // provoke: `capabilities()` to lay out a screen, the facade accessors
//!     // to actually do the work.
//!     let capabilities = federation.capabilities();
//!     println!("{capabilities:?}");
//!
//!     // Ecash: notes to hand over out of band, plus an operation that says
//!     // whether they were redeemed or came back. Quote first here too — the
//!     // mint rounds the request up to a denomination it can issue, and note
//!     // selection can cost a fee, so the debit is not the amount asked for.
//!     if let Some(ecash) = federation.ecash() {
//!         let quote = ecash.quote(Amount::from_msats(50_000)).await?;
//!         println!(
//!             "{} of notes plus {} fee ({} debited), good until {}",
//!             quote.notes_value(),
//!             quote.fee(),
//!             quote.total(),
//!             quote.expires_at(),
//!         );
//!         let sent = ecash.send(quote).await?;
//!         println!("give these to the receiver: {}", sent.notes);
//!         // Worth persisting, though not required: the notes are readable
//!         // again from `Operation::details` after a restart, and the id is
//!         // all it takes to find this send.
//!         println!("resume with {}", sent.operation.id());
//!     }
//!
//!     // Lightning: quote first, so the user sees the expected cost before
//!     // agreeing to it, and `send` refuses a quote whose terms have moved.
//!     // What finally left the balance is read from the operation's details.
//!     if let Some(lightning) = federation.lightning() {
//!         let invoice: Bolt11Invoice = invoice.parse()?;
//!         // An invoice states its own amount. One that does not cannot be
//!         // paid at all, so there is nothing to override here.
//!         let quote = lightning.quote(&invoice).await?;
//!         println!(
//!             "pay {} plus {} fee ({} total) via {:?}, good until {}",
//!             quote.invoice_amount(),
//!             quote.fee(),
//!             quote.total(),
//!             quote.route(),
//!             quote.expires_at(),
//!         );
//!
//!         // `send` takes the quote by value: one quote, one payment.
//!         let payment = lightning.send(quote).await?;
//!
//!         // The subscriber yields the current state first, then every
//!         // transition, then `None` once a final state has been seen.
//!         let mut updates = payment.updates();
//!         while let Some(state) = updates.next().await? {
//!             match state {
//!                 LnSendState::Success { preimage, fee, .. } => {
//!                     // The fee the quote bound, and therefore the fee that
//!                     // was charged.
//!                     println!("paid, fee {fee}, preimage {preimage}");
//!                 }
//!                 // Not an error: the payment did not go through, and the
//!                 // money is back in the balance.
//!                 LnSendState::Refunded => println!("refunded"),
//!                 other => println!("{other:?}"),
//!             }
//!         }
//!     }
//!
//!     // Reattaching after a restart: the operation kept running without us.
//!     if let Some(unfinished) = unfinished {
//!         let id: OperationId = unfinished.parse()?;
//!         match federation.operation(&id).await? {
//!             Some(operation) => match operation.kind() {
//!                 OperationKind::LnSend => {
//!                     if let Some(payment) = operation.as_ln_send() {
//!                         println!("still going: {:?}", payment.state().await?);
//!                     }
//!                 }
//!                 // Recorded by a version that understood something this one
//!                 // does not — still a real row, still listable.
//!                 OperationKind::Unknown => println!("an operation from another version"),
//!                 other => println!("{other:?}"),
//!             },
//!             None => println!("no operation with that id here"),
//!         }
//!     }
//!
//!     // Local history, newest first, one page at a time.
//!     let page = federation.activity(None, 20).await?;
//!     for item in &page.items {
//!         println!("{} {:?} {:?}", item.time, item.kind, item.status);
//!     }
//!     if let Some(cursor) = page.next {
//!         let _older = federation.activity(Some(cursor), 20).await?;
//!     }
//!
//!     sdk.shutdown().await
//! }
//!
//! // The walkthrough is compiled but never called: running it needs a live
//! // federation and an async runtime, and this crate deliberately brings
//! // neither of those with it.
//! fn main() {
//!     let _ = walkthrough;
//! }
//! ```
//!
//! # Conventions
//!
//! The rules below hold everywhere in this crate. They are stated once, here,
//! rather than repeated on every method that obeys them.
//!
//! ## An error means the call failed, never that the money moved badly
//!
//! This is the most important convention in the crate. `Err` is reserved for
//! the *call* going wrong: storage could not be read, no guardian answered,
//! the input did not parse, the federation was closed. What happened to a
//! payment is never reported that way. A lightning payment that could not be
//! routed and was refunded, an invoice that expired unpaid, ecash the
//! receiver redeemed before the sender tried to reclaim it — each of those is
//! a perfectly successful call that yields an ordinary **state**
//! ([`LnSendState::Refunded`], [`LnReceiveState::Expired`],
//! [`EcashSendState::Redeemed`]).
//!
//! The practical consequence is that an application renders states and logs
//! errors, and the two paths do not get confused. A `catch` block in a
//! binding is for "something is broken", not for "the payment did not go
//! through"; the second belongs on screen, phrased for the user, alongside
//! the amount that came back. It also means retry logic can be honest: a
//! failed call may be worth retrying, whereas a final state never is.
//!
//! Every error carries an [`ErrorCode`], which is the stable part to branch
//! on, and a message, which is for humans and must never be parsed. See
//! [`Error`].
//!
//! ## Everything string-shaped parses and prints
//!
//! Invite codes, invoices, addresses, ecash notes, payment preimages, every
//! id, and the activity [`Cursor`] are opaque types that implement
//! [`Display`](core::fmt::Display) and [`FromStr`](core::str::FromStr), with
//! a parse that validates rather than merely storing. Nothing in the public
//! API asks a caller to hand over a pre-parsed structure or to know a wire
//! format.
//!
//! That is what lets a binding carry all of them as plain strings — a Swift
//! `String`, a Kotlin `String`, a JavaScript string — while the validation
//! stays in one place. No language needs its own bech32 or bolt11 parser, no
//! two languages can disagree about what counts as a valid invite code, and
//! a value can be stored in a preference, passed through a deep link, or put
//! in a QR code and come back the same way it left.
//!
//! ## Operations are detached
//!
//! An operation begins running the moment the facade call that created it
//! returns. It is persisted as it goes, it resumes by itself when the SDK is
//! built again over the same storage, and it can be picked up later with
//! [`Federation::operation`] using nothing but its id. [`Operation`] and
//! [`OperationUpdates`] are observation handles, not ownership: dropping a
//! handle ends nothing at all, dropping a subscriber ends only that
//! subscription, and dropping a pending
//! [`next`](OperationUpdates::next) future ends only that one wait — the
//! subscriber stays usable and no transition is lost. None of the three ever
//! stops the work.
//!
//! This is deliberate, and it is the answer to "can I cancel it?". For most
//! operations there is nothing to cancel, because value has already moved
//! into a protocol that will resolve one way or the other; pretending
//! otherwise would only produce handles whose `drop` silently abandons money
//! in flight. Where a cancellation genuinely exists it is a named request on
//! that specific operation — [`Operation::request_cancel`] for out-of-band
//! ecash — and even then the outcome arrives as a state, because the receiver
//! may redeem the notes before the reclaim lands.
//!
//! [`Operation::updates`] hands out an independent subscriber per call: each
//! sees the current state immediately and then every transition, and no
//! subscriber can consume another's updates. It is not a replay of history —
//! subscribing to a settled operation shows the settled state and then a
//! clean close.
//!
//! ## Capability discovery is not error-driven
//!
//! Not every federation runs every module. Rather than making an application
//! find that out by attempting an operation and catching the failure,
//! [`Federation::ecash`], [`Federation::lightning`] and
//! [`Federation::onchain`] return `Option`, and
//! [`Federation::capabilities`] reports the same information as plain
//! booleans so a screen can be laid out before the user touches anything. An
//! absent capability is an ordinary value to branch on.
//!
//! [`ErrorCode::NotSupported`] still exists, but only for the narrow residual
//! case where a facade was obtained while the module was present and then
//! used after the federation's configuration dropped it.
//!
//! ## One module generation per federation
//!
//! All of a federation's modules must be of the same generation — all v1, or
//! all v2. There is no per-module override and no caller-facing opt-out: a
//! federation running a mixed set is rejected with
//! [`ErrorCode::UnsupportedFederation`], whose message names the modules that
//! conflict and the generations they declare. The rule is checked when a
//! federation is previewed, when it is joined, when it is reopened at
//! startup, and again if its configuration changes while the SDK is running,
//! and it covers every module the federation runs rather than only the ones
//! this crate exposes as facades. A configuration this SDK cannot reason
//! about completely is not one it is willing to hold funds in.
//!
//! ## Handles, threads, and the runtime
//!
//! [`Sdk`], [`Federation`], the facades, and [`Operation`] are cheap clones
//! over shared state: cloning one costs an atomic refcount bump, and every
//! clone observes the same federations, the same storage, and the same
//! background work. Pass them around freely instead of threading references
//! through an application. On native targets they are `Send + Sync`, so they
//! move between threads and tasks without ceremony; wasm compiles the very
//! same types for a single-threaded host, where those bounds cost nothing.
//! The subscribers ([`OperationUpdates`], [`BalanceUpdates`]) are the
//! exception: each is one cursor, so neither is `Clone` — call
//! [`Operation::updates`] or [`Federation::balance_updates`] again for a
//! second, independent subscription.
//!
//! The implementation will require tokio on native targets, because
//! `fedimint-client` does. That is a note about what embedding the finished
//! SDK will involve, not a dependency of this crate today — the skeleton has
//! none, and the async functions here are runtime-agnostic signatures.
//!
//! # How this maps onto the language bindings
//!
//! "One API for every platform" means **one semantic model plus mechanical,
//! per-target adapters that share conformance tests** — not literal type
//! identity across Rust, UniFFI and wasm. Chasing identity would mean
//! degrading the Rust API to the intersection of what three foreign type
//! systems can express. Instead, the model — what an operation is, when a
//! quote stops being valid, which outcomes are states — is defined once here
//! and every adapter is checked against the same tests, so the *behaviour* is
//! identical even where the spelling is not.
//!
//! The adaptations are known, deliberate, and short:
//!
//! - **Generics are monomorphised.** [`Operation`] is generic over its state
//!   type in Rust; the FFI layer emits one concrete type per operation kind
//!   instead, because neither UniFFI nor TypeScript can carry the generic.
//! - **Subscriber cursors gain a lock.** [`OperationUpdates::next`] and
//!   [`BalanceUpdates::next`] take `&mut self`, which is how Rust states
//!   "one consumer at a time". The bindings wrap each subscriber behind a
//!   lock and present an `&self` async `next()`, preserving the guarantee
//!   without requiring the host language to have a borrow checker.
//! - **The builder is flattened.** [`SdkBuilder`]'s consuming setters become
//!   constructor arguments or `&self` setters, since move semantics do not
//!   survive the crossing.
//! - **Quotes are consumed semantically.** [`Lightning::send`] and
//!   [`Onchain::send`] take a quote by value; in a binding the quote crosses
//!   as an object whose second use fails with
//!   [`ErrorCode::QuoteExpired`] rather than paying twice — that code covers
//!   a quote that is no longer valid *because it was already executed*, not
//!   only one whose validity window lapsed. The type system stops it in
//!   Rust; the runtime stops it everywhere else.
//! - **Maps cross as maps.** A [`BTreeMap`](std::collections::BTreeMap)
//!   becomes the binding's native dictionary. Rust keeps `BTreeMap` rather
//!   than `HashMap` so that iteration order is deterministic — metadata
//!   rendered in a list should not shuffle between runs.
//! - **`u64` crosses wasm as `BigInt`.** Never as a JavaScript `number`:
//!   millisatoshi amounts and timestamps exceed the 53-bit range where
//!   `number` is exact, and a silently rounded balance is a bug that would
//!   not surface until it mattered.
//!
//! Keeping that adapter layer mechanical constrains the API shape here, so
//! three rules hold throughout: **no tuples** in public signatures (they
//! become positional, unnamed records in every target — a named struct says
//! what the fields are), **no borrowed returns** (lifetimes have no
//! counterpart across a foreign-function boundary; everything returned is
//! owned or a handle), and **no `impl Trait` in public signatures** (an
//! anonymous type cannot be named, exported, or generated from). Streams
//! follow from the same rule: instead of returning `impl Stream`, this crate
//! hands out subscriber objects with an async `next()`, which is directly
//! expressible as an async pull in every target.
//!
//! # Forward compatibility
//!
//! Every public enum in this crate is `#[non_exhaustive]`. That is a
//! **Rust-only** guarantee, and the limit is worth stating plainly: for Rust
//! callers it means matches need a wildcard arm, which the compiler enforces,
//! so a variant added in a later release is not a breaking change. It does
//! **not** mean a generated Swift, Kotlin or TypeScript decoder tolerates a
//! tag it has never seen. UniFFI's generated Swift decoder throws
//! `unexpectedEnumCase` on an unknown discriminant, and no attribute on the
//! Rust side changes that. A pre-generated binding pinned to an older SDK,
//! meeting an [`ErrorCode`], a [`Network`], an [`OperationKind`] or an
//! operation-state variant added since, fails to decode it — it does not
//! quietly receive an "unknown" case.
//!
//! So forward tolerance at the boundary is not free, and there are exactly
//! two ways to have it:
//!
//! - **Regenerate the binding against the SDK version it talks to.** This is
//!   the default expectation here and the cheap answer: the binding and the
//!   SDK ship together, so no vintage gap exists to tolerate.
//! - **Hand-write an adapter for the boundary, and test it across versions.**
//!   For a fieldless enum this is cheap in principle — carry the variant's
//!   stable *name* across as a length-delimited string, so an unfamiliar one
//!   is read and skipped like any other string, and project it into the
//!   target's own enum with an explicit unknown fallback. What it costs is a
//!   per-target map that must be kept in step and a cross-version conformance
//!   suite that decodes a newer producer's output with an older consumer's
//!   adapter. Without those tests the tolerance is a claim, not a property.
//!
//! One boundary here does not rely on either, because its wire form was
//! designed for it: an error's structured detail crosses as
//! [`RawErrorDetails`] — a version, a kind string, and a length-delimited
//! opaque payload. A reader of any vintage consumes that record completely
//! and skips a payload whose kind it has never heard of, keeping it as
//! [`DetailEnvelope::Opaque`], and the typed [`ErrorDetails`] cases are
//! projected locally from what it does recognise. That is the shape to copy
//! wherever a detail must survive a version gap.
//!
//! None of this is ceremony, because persisted state outlives the version of
//! the SDK that wrote it: applications get downgraded, module sets change,
//! and a record written by a newer build is still a real record of real money.
//! This is why [`Federation::operation`] returns an operation it cannot
//! interpret as `Ok(Some(_))` with [`OperationKind::Unknown`] instead of
//! failing the lookup — an application can then list it honestly as "an
//! operation from another version", where a failure would have made it
//! invisible. Note what makes that work where the enum-level claim above does
//! not: the mapping happens *inside this crate*, onto a variant every binding
//! generated today already has, so no decoder is ever handed a tag it does
//! not know. Acting on such an operation, as opposed to observing that it
//! exists, is what [`ErrorCode::UnsupportedOperation`] reports.
//!
//! # What the binding layers must guarantee
//!
//! Three requirements belong to this design rather than to any one binding,
//! and are recorded here so they are not rediscovered per platform:
//!
//! - **The wasm entry point installs a panic hook on its very first line.**
//!   Without one, a panic inside a spawned task surfaces in the browser as a
//!   bare `unreachable` trap with no message and no location, which is
//!   effectively undebuggable. With one, it surfaces as a message and a stack
//!   position. It has to be the first statement, because a panic during
//!   initialisation is exactly the panic worth seeing.
//! - **The UniFFI response path keeps a strict no-panic discipline.** That
//!   layer is built with `panic = "abort"`, so a panic there does not unwind
//!   into a catchable error — it takes the entire host application down.
//!   Every value crossing back out is produced without unwrapping,
//!   indexing, or slicing something that could be absent.
//! - **Every in-flight operation terminates observably.** If the transport,
//!   the worker, or the runtime behind the SDK dies, everything outstanding
//!   must fail with the underlying error. An operation that is left pending
//!   for ever is worse than one that fails: a caller can retry a failure or
//!   show it, but a promise that never settles hangs a screen with no way
//!   out and no diagnostic.
//!
//! # Further reading
//!
//! The design of this API, and the review that amended it, live in
//! [fedimint-sdk#344](https://github.com/fedimint/fedimint-sdk/issues/344).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(missing_debug_implementations)]
// Skeleton-phase allowances — remove both when implementation starts. Parameters
// are deliberately named (they are rustdoc-visible API contract) but unused, and
// the private placeholder `inner` fields are never constructed or read while
// every body is unimplemented!(). CI builds this crate through
// actions-rust-lang/setup-rust-toolchain, which defaults RUSTFLAGS to
// "-D warnings", so these must be in-source allows rather than warnings
// tolerated at the command line:
#![allow(unused_variables)]
#![allow(dead_code)]

mod activity;
mod ecash;
mod error;
mod federation;
mod lightning;
mod meta;
mod onchain;
mod operation;
mod recovery;
mod sdk;
mod storage;
mod types;

pub use activity::{ActivityItem, ActivityPage, ActivityStatus, Direction};
pub use ecash::{
    Ecash, EcashQuote, EcashReceiveDetails, EcashReceiveState, EcashSend, EcashSendDetails,
    EcashSendState,
};
pub use error::{
    DetailEnvelope, Diagnostic, Error, ErrorCode, ErrorDetails, ModuleGeneration, RawErrorDetails,
    Result,
};
pub use federation::{BalanceUpdates, Capabilities, Federation};
pub use lightning::{
    Lightning, LightningRoute, LnFeeBreakdown, LnQuote, LnReceive, LnReceiveDetails,
    LnReceiveState, LnSendDetails, LnSendState,
};
pub use meta::{ConsensusMetadata, Meta};
pub use onchain::{
    Onchain, OnchainQuote, OnchainReceive, OnchainReceiveDetails, OnchainReceiveFeeBreakdown,
    OnchainReceiveState, OnchainSendDetails, OnchainSendFeeBreakdown, OnchainSendState,
};
pub use operation::{
    AnyOperation, DetailedOperationState, Operation, OperationDetails, OperationKind,
    OperationState, OperationSupport, OperationUpdates, RawOperationKind,
};
pub use recovery::{Recovery, RecoveryState};
pub use sdk::{FederationInfo, FederationStatus, FederationStatusUpdates, Sdk, SdkBuilder};
pub use storage::Storage;
pub use types::{
    Address, Amount, Bolt11Invoice, Cursor, FederationId, FederationPreview, GatewayId, InviteCode,
    Mnemonic, Network, Notes, OperationId, Preimage, Sats, Timestamp, Txid,
};
