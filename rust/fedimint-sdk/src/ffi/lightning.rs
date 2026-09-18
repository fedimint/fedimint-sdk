//! The lightning facade's FFI adapters.

use std::sync::Arc;

use super::ffi_operation;
use crate::{
    Amount, Bolt11Invoice, Lightning, LnQuote, LnReceive, LnReceiveDetails, LnReceiveState,
    LnSendDetails, LnSendState, Result,
};

// The UniFFI view of `Lightning::send`/`Lightning::receive`, under their real names but different
// Rust identifiers: `send`'s real parameter is an owned `LnQuote`, which a binding can never hand
// over — it only ever holds a shared handle — so this borrows the quote and claims it for single
// use instead, and `receive`'s real return type names the generic `Operation<LnReceiveState>`,
// which cannot cross at all. `Lightning::quote` needs no such adapter, so it keeps the export
// attribute on the real method in `lightning.rs`.
#[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
#[cfg_attr(target_family = "wasm", uniffi::export)]
impl Lightning {
    /// See [`Lightning::send`]. Fails with
    /// [`ErrorCode::QuoteExpired`](crate::ErrorCode::QuoteExpired) if `quote` was already sent.
    #[uniffi::method(name = "send")]
    pub async fn ffi_send(&self, quote: &LnQuote) -> Result<LnSendOperation> {
        quote.used.claim(quote.expires_at())?;
        Ok(self.send_authorized(quote).await?.into())
    }

    /// See [`Lightning::receive`].
    #[uniffi::method(name = "receive")]
    pub async fn ffi_receive(&self, amount: Amount, description: &str) -> Result<LnReceiveHandle> {
        Ok(self.receive(amount, description).await?.into())
    }
}

// The UniFFI views of `Operation<LnSendState>` and `Operation<LnReceiveState>`: `Operation<S>` is
// generic and UniFFI objects cannot be, so `ffi_operation!` monomorphises one newtype object per
// state, forwarding every method to the real handle. See that macro's documentation in
// `ffi/operation.rs`.
ffi_operation!(
    LnSendOperation,
    LnSendOperationUpdates,
    LnSendState,
    details: LnSendDetails
);
ffi_operation!(
    LnReceiveOperation,
    LnReceiveOperationUpdates,
    LnReceiveState,
    details: LnReceiveDetails
);

/// The result of [`Lightning::receive`], with `operation` crossing as [`LnReceiveOperation`] rather
/// than the generic `Operation<LnReceiveState>` the real [`LnReceive`] carries.
#[derive(Debug, uniffi::Record)]
pub struct LnReceiveHandle {
    /// See [`LnReceive::invoice`].
    pub invoice: Bolt11Invoice,
    /// See [`LnReceive::operation`].
    pub operation: Arc<LnReceiveOperation>,
}

impl From<LnReceive> for LnReceiveHandle {
    fn from(receive: LnReceive) -> Self {
        Self {
            invoice: receive.invoice,
            operation: Arc::new(receive.operation.into()),
        }
    }
}
