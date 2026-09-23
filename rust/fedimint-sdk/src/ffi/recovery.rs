//! The recovery facade's FFI adapters.

use std::sync::Arc;

use super::ffi_operation;
use crate::{Federation, FederationId, InviteCode, Recovery, RecoveryState, Result, Sdk};

// The UniFFI view of `Sdk::recover`/`Sdk::resume_recovery`/`Sdk::recovery_status`, under their real
// names but different Rust identifiers: `recover` and `resume_recovery` return the
// [`RecoveryHandle`] helper below instead of `Recovery`, which embeds a bare `Federation` and a
// generic `Operation<RecoveryState>`, neither of which can cross a UniFFI boundary as-is.
// `recovery_status` is already FFI-safe and is routed through the same block purely so all three
// stay together.
#[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
#[cfg_attr(target_family = "wasm", uniffi::export)]
impl Sdk {
    /// See [`Sdk::recover`].
    #[uniffi::method(name = "recover")]
    pub async fn ffi_recover(&self, invite: &InviteCode) -> Result<RecoveryHandle> {
        Ok(self.recover(invite).await?.into())
    }

    /// See [`Sdk::resume_recovery`].
    #[uniffi::method(name = "resume_recovery")]
    pub async fn ffi_resume_recovery(&self, id: &FederationId) -> Result<RecoveryHandle> {
        Ok(self.resume_recovery(id).await?.into())
    }

    /// See [`Sdk::recovery_status`].
    #[uniffi::method(name = "recovery_status")]
    pub async fn ffi_recovery_status(&self, id: &FederationId) -> Result<Option<RecoveryState>> {
        self.recovery_status(id).await
    }
}

// The UniFFI view of `Operation<RecoveryState>`: `Operation<S>` is generic and UniFFI objects
// cannot be, so `ffi_operation!` monomorphises a newtype object, forwarding every method to the
// real handle. See that macro's documentation in `ffi/operation.rs`. No `details`: a recovery has
// no fixed facts worth persisting, so `RecoveryState` does not implement `DetailedOperationState`.
ffi_operation!(RecoveryOperation, RecoveryOperationUpdates, RecoveryState);

/// The result of [`Sdk::recover`]/[`Sdk::resume_recovery`], with `progress` crossing as
/// [`RecoveryOperation`] and `federation` as an `Arc`, rather than the generic
/// `Operation<RecoveryState>` and bare [`Federation`] the real [`Recovery`] carries — a
/// `Federation` referenced from a record field crosses as `Arc<Federation>` like any other object
/// reference.
#[derive(Debug, uniffi::Record)]
pub struct RecoveryHandle {
    /// See [`Recovery::federation`].
    pub federation: Arc<Federation>,
    /// See [`Recovery::progress`].
    pub progress: Arc<RecoveryOperation>,
}

impl From<Recovery> for RecoveryHandle {
    fn from(recovery: Recovery) -> Self {
        Self {
            federation: Arc::new(recovery.federation),
            progress: Arc::new(recovery.progress.into()),
        }
    }
}
