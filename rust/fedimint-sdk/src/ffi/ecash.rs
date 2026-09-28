//! The ecash facade's FFI adapters.

use std::sync::Arc;

use super::ffi_operation;
use crate::{
    Amount, Ecash, EcashQuote, EcashReceiveDetails, EcashReceiveState, EcashSend, EcashSendDetails,
    EcashSendState, Notes, Result, Timestamp,
};

// The UniFFI view of `Ecash::send`/`Ecash::receive`, under their real names but different Rust
// identifiers: `send`'s real parameter is an owned `EcashQuote`, which a binding can never hand
// over — it only ever holds a shared handle — so this borrows the quote and claims it for single
// use instead, and `receive`'s real return type names the generic `Operation<EcashReceiveState>`,
// which cannot cross at all. `Ecash::quote` needs no such adapter, so it keeps the export
// attribute on the real method in `ecash.rs`.
#[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
#[cfg_attr(target_family = "wasm", uniffi::export)]
impl Ecash {
    /// See [`Ecash::send`]. Fails with
    /// [`ErrorCode::QuoteExpired`](crate::ErrorCode::QuoteExpired) if `quote` was already sent.
    #[uniffi::method(name = "send")]
    pub async fn ffi_send(&self, quote: &EcashQuote) -> Result<EcashSendHandle> {
        quote.used.claim(quote.expires_at())?;
        Ok(self.send_authorized(quote).await?.into())
    }

    /// See [`Ecash::receive`].
    #[uniffi::method(name = "receive")]
    pub async fn ffi_receive(&self, notes: &Notes) -> Result<EcashReceiveOperation> {
        Ok(self.receive(notes).await?.into())
    }
}

// The UniFFI views of `Operation<EcashSendState>` and `Operation<EcashReceiveState>`:
// `Operation<S>` is generic and UniFFI objects cannot be, so `ffi_operation!` monomorphises one
// newtype object per state, forwarding every method to the real handle. See that macro's
// documentation in `ffi/operation.rs`.
ffi_operation!(
    EcashSendOperation,
    EcashSendOperationUpdates,
    EcashSendState,
    details: FfiEcashSendDetails
);
ffi_operation!(
    EcashReceiveOperation,
    EcashReceiveOperationUpdates,
    EcashReceiveState,
    details: FfiEcashReceiveDetails
);

/// Forwards `Operation::<EcashSendState>::request_cancel`, the one inherent method that exists on
/// only this instantiation of `Operation<S>` — cancelling out-of-band ecash is the one place a
/// cancellation is a real protocol action.
#[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
#[cfg_attr(target_family = "wasm", uniffi::export)]
impl EcashSendOperation {
    /// See `Operation::<EcashSendState>::request_cancel`.
    pub async fn request_cancel(&self) -> Result<()> {
        self.0.request_cancel().await
    }
}

/// The result of [`Ecash::send`], with `operation` crossing as [`EcashSendOperation`] rather than
/// the generic `Operation<EcashSendState>` the real [`EcashSend`] carries.
#[derive(Debug, uniffi::Record)]
pub struct EcashSendHandle {
    /// See [`EcashSend::notes`].
    pub notes: Arc<Notes>,
    /// See [`EcashSend::operation`].
    pub operation: Arc<EcashSendOperation>,
}

impl From<EcashSend> for EcashSendHandle {
    fn from(send: EcashSend) -> Self {
        Self {
            notes: Arc::new(send.notes),
            operation: Arc::new(send.operation.into()),
        }
    }
}

/// The UniFFI view of [`EcashSendDetails`], with `notes` crossing as `Arc<Notes>`: [`Notes`] is a
/// UniFFI object, and an object can sit in a record only behind an `Arc`. Exported as
/// `EcashSendDetails`; the real type never crosses.
#[derive(Debug, uniffi::Record)]
#[uniffi(name = "EcashSendDetails")]
pub struct FfiEcashSendDetails {
    /// See [`EcashSendDetails::notes`].
    pub notes: Arc<Notes>,
    /// See [`EcashSendDetails::requested_amount`].
    pub requested_amount: Amount,
    /// See [`EcashSendDetails::notes_value`].
    pub notes_value: Amount,
    /// See [`EcashSendDetails::fee`].
    pub fee: Amount,
    /// See [`EcashSendDetails::total_debited`].
    pub total_debited: Amount,
    /// See [`EcashSendDetails::reclaim_at`].
    pub reclaim_at: Timestamp,
    /// See [`EcashSendDetails::created_at`].
    pub created_at: Timestamp,
}

impl From<EcashSendDetails> for FfiEcashSendDetails {
    fn from(details: EcashSendDetails) -> Self {
        // Destructured with no `..`, so a field added to `EcashSendDetails` fails to compile here
        // until this projection carries it too.
        let EcashSendDetails {
            notes,
            requested_amount,
            notes_value,
            fee,
            total_debited,
            reclaim_at,
            created_at,
        } = details;
        Self {
            notes: Arc::new(notes),
            requested_amount,
            notes_value,
            fee,
            total_debited,
            reclaim_at,
            created_at,
        }
    }
}

/// The UniFFI view of [`EcashReceiveDetails`], with `notes` crossing as `Option<Arc<Notes>>`, for
/// the same reason as the send projection above. Exported as `EcashReceiveDetails`.
#[derive(Debug, uniffi::Record)]
#[uniffi(name = "EcashReceiveDetails")]
pub struct FfiEcashReceiveDetails {
    /// See [`EcashReceiveDetails::notes`].
    pub notes: Option<Arc<Notes>>,
    /// See [`EcashReceiveDetails::notes_value`].
    pub notes_value: Amount,
    /// See [`EcashReceiveDetails::fee`].
    pub fee: Amount,
    /// See [`EcashReceiveDetails::net_credit`].
    pub net_credit: Amount,
    /// See [`EcashReceiveDetails::created_at`].
    pub created_at: Timestamp,
}

impl From<EcashReceiveDetails> for FfiEcashReceiveDetails {
    fn from(details: EcashReceiveDetails) -> Self {
        // Exhaustive for the same reason as the send projection above.
        let EcashReceiveDetails {
            notes,
            notes_value,
            fee,
            net_credit,
            created_at,
        } = details;
        Self {
            notes: notes.map(Arc::new),
            notes_value,
            fee,
            net_credit,
            created_at,
        }
    }
}
