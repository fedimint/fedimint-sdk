//! The on-chain facade's FFI adapters.

use std::sync::Arc;

use super::ffi_operation;
use crate::{
    Address, Onchain, OnchainQuote, OnchainReceive, OnchainReceiveDetails, OnchainReceiveState,
    OnchainSendDetails, OnchainSendState, Result,
};

// The UniFFI view of `Onchain::receive`/`Onchain::send`, under their real names but different Rust
// identifiers: `receive`'s real return type names the generic `Operation<OnchainReceiveState>`,
// which cannot cross at all, and `send`'s real parameter is an owned `OnchainQuote`, which a
// binding can never hand over — it only ever holds a shared handle — so this borrows the quote and
// claims it for single use instead. `Onchain::quote` needs no such adapter, so it keeps the export
// attribute on the real method in `onchain.rs`.
#[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
#[cfg_attr(target_family = "wasm", uniffi::export)]
impl Onchain {
    /// See [`Onchain::receive`].
    #[uniffi::method(name = "receive")]
    pub async fn ffi_receive(&self) -> Result<OnchainReceiveHandle> {
        Ok(self.receive().await?.into())
    }

    /// See [`Onchain::send`]. Fails with
    /// [`ErrorCode::QuoteExpired`](crate::ErrorCode::QuoteExpired) if `quote` was already sent.
    #[uniffi::method(name = "send")]
    pub async fn ffi_send(&self, quote: &OnchainQuote) -> Result<OnchainSendOperation> {
        quote.used.claim(quote.expires_at())?;
        Ok(self.send_authorized(quote).await?.into())
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

/// The result of [`Onchain::receive`], with `operation` crossing as [`OnchainReceiveOperation`]
/// rather than the generic `Operation<OnchainReceiveState>` the real [`OnchainReceive`] carries.
#[derive(Debug, uniffi::Record)]
pub struct OnchainReceiveHandle {
    /// See [`OnchainReceive::address`].
    pub address: Address,
    /// See [`OnchainReceive::operation`].
    pub operation: Arc<OnchainReceiveOperation>,
}

impl From<OnchainReceive> for OnchainReceiveHandle {
    fn from(receive: OnchainReceive) -> Self {
        Self {
            address: receive.address,
            operation: Arc::new(receive.operation.into()),
        }
    }
}
