//! The on-chain facade's FFI adapters.

use std::sync::Arc;

use super::ffi_operation;
use crate::{
    Onchain, OnchainDeposits, OnchainQuote, OnchainReceiveDetails, OnchainReceiveState,
    OnchainSendDetails, OnchainSendState, Result,
};

// The UniFFI view of `Onchain::send`, under its real name but a different Rust identifier: the
// real parameter is an owned `OnchainQuote`, which a binding can never hand over (it only ever
// holds a shared handle), so this borrows the quote and claims it for single use instead.
// `Onchain::receive`, `Onchain::deposits` and `Onchain::quote` need no such adapter, so they keep
// the export attribute on the real methods in `onchain.rs`.
#[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
#[cfg_attr(target_family = "wasm", uniffi::export)]
impl Onchain {
    /// See [`Onchain::send`]. Fails with
    /// [`ErrorCode::QuoteExpired`](crate::ErrorCode::QuoteExpired) if `quote` was already sent.
    #[uniffi::method(name = "send")]
    pub async fn ffi_send(&self, quote: &OnchainQuote) -> Result<OnchainSendOperation> {
        quote.used.claim(quote.expires_at())?;
        Ok(self.send_authorized(quote).await?.into())
    }
}

// The UniFFI view of `OnchainDeposits::next`, under its real name but a different Rust identifier:
// the real signature takes `&mut self`, which a shared `Arc<OnchainDeposits>` can never provide,
// and returns the generic `Operation<OnchainReceiveState>`, which cannot cross at all. This calls
// the same `next_shared` body through `&self` and hands the operation over as
// `OnchainReceiveOperation`.
#[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
#[cfg_attr(target_family = "wasm", uniffi::export)]
impl OnchainDeposits {
    /// See [`OnchainDeposits::next`].
    #[uniffi::method(name = "next")]
    pub async fn ffi_next(&self) -> Result<Arc<OnchainReceiveOperation>> {
        Ok(Arc::new(self.next_shared().await?.into()))
    }
}

// The UniFFI views of `Operation<OnchainSendState>` and `Operation<OnchainReceiveState>`:
// `Operation<S>` is generic and UniFFI objects cannot be, so `ffi_operation!` monomorphises one
// newtype object per state, forwarding every method to the real handle. See that macro's
// documentation in `ffi/operation.rs`.
ffi_operation!(
    OnchainSendOperation,
    OnchainSendOperationUpdates,
    OnchainSendState,
    details: OnchainSendDetails
);
ffi_operation!(
    OnchainReceiveOperation,
    OnchainReceiveOperationUpdates,
    OnchainReceiveState,
    details: OnchainReceiveDetails
);
