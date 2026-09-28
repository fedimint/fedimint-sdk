//! Everything that exists only to cross the UniFFI boundary.
//!
//! Every item in here exists for no reason other than crossing the boundary, and a reader of the
//! plain-Rust API never needs to see any of it. One submodule per facade, mirroring the crate's own
//! layout — [`ecash`] holds what `ecash.rs` needs, [`onchain`] what `onchain.rs` needs, and so on —
//! plus the machinery that belongs to no single facade: the [`ffi_operation!`] macro in
//! [`operation`], [`QuoteClaim`] in [`quote`], and the value-type conversions in [`types`].
//!
//! `lib.rs` declares this module behind `#[cfg(feature = "uniffi")]`, so nothing inside it repeats
//! that gate: the whole subtree is absent from the wasm and plain-Rust builds.
//!
//! # What is here
//!
//! Anything whose whole purpose is the boundary, including the `#[uniffi::export] impl` blocks that
//! adapt a real type's methods. An inherent impl may sit in any module of the defining crate, so an
//! adapter block lives beside the helper types it returns rather than beside the method it wraps:
//!
//! ```ignore
//! // in ffi/ecash.rs, next to EcashSendHandle itself
//! #[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
//! #[cfg_attr(target_family = "wasm", uniffi::export)]
//! impl Ecash {
//!     #[uniffi::method(name = "send")]
//!     pub async fn ffi_send(&self, quote: &EcashQuote) -> Result<EcashSendHandle> { /* ... */ }
//! }
//! ```
//!
//! Alongside those: the `*Handle` records and `Ffi*` projections, the monomorphised operation
//! objects, the `custom_type!` conversions, and the entry point that exists only because a builder
//! cannot cross.
//!
//! # What stays with the real type
//!
//! Two things, both because Rust gives them nowhere else to go. A derive cannot be applied from
//! another module, so the `#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]` line that marks
//! a value type as crossing stays on its definition. And where a real method is already FFI-safe,
//! it needs no adapter at all: the `#[cfg_attr(feature = "uniffi", uniffi::export)]` goes straight
//! onto the real impl block, which is real API and belongs with its type.
//!
//! An adapter block reaches private state — `QuoteClaim` fields, `next_shared` — so the facades
//! widen exactly those items to `pub(crate)` and no further.

pub(crate) mod ecash;
pub(crate) mod error;
pub(crate) mod federation;
pub(crate) mod lightning;
pub(crate) mod onchain;
pub(crate) mod operation;
pub(crate) mod quote;
pub(crate) mod recovery;
pub(crate) mod sdk;
pub(crate) mod types;

pub(crate) use operation::ffi_operation;
pub(crate) use quote::QuoteClaim;
pub use sdk::create_fedimint_sdk;
