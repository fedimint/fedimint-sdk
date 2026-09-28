//! The federation handle's FFI adapters.

use std::sync::Arc;

use crate::{
    Amount, AnyOperation, BalanceUpdates, Ecash, Federation, Lightning, Onchain, OperationId,
    Result,
};

// The UniFFI view of `Federation::ecash`/`lightning`/`onchain`/`operation`, under their real names
// but different Rust identifiers so they can wrap each object in `Arc`: a UniFFI object nested in
// `Option`/`Result<Option<_>>` has to cross as `Arc<T>`.
#[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
#[cfg_attr(target_family = "wasm", uniffi::export)]
impl Federation {
    /// See [`Federation::ecash`].
    #[uniffi::method(name = "ecash")]
    pub fn ffi_ecash(&self) -> Option<Arc<Ecash>> {
        self.ecash().map(Arc::new)
    }

    /// See [`Federation::lightning`].
    #[uniffi::method(name = "lightning")]
    pub fn ffi_lightning(&self) -> Option<Arc<Lightning>> {
        self.lightning().map(Arc::new)
    }

    /// See [`Federation::onchain`].
    #[uniffi::method(name = "onchain")]
    pub fn ffi_onchain(&self) -> Option<Arc<Onchain>> {
        self.onchain().map(Arc::new)
    }

    /// See [`Federation::operation`].
    #[uniffi::method(name = "operation")]
    pub async fn ffi_operation(&self, id: &OperationId) -> Result<Option<Arc<AnyOperation>>> {
        Ok(self.operation(id).await?.map(Arc::new))
    }
}

// The UniFFI view of `BalanceUpdates::next`, under its real name but a different Rust identifier:
// the real signature takes `&mut self`, which a shared `Arc<BalanceUpdates>` can never provide, so
// this calls the same `next_shared` body through `&self` instead.
#[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
#[cfg_attr(target_family = "wasm", uniffi::export)]
impl BalanceUpdates {
    /// See [`BalanceUpdates::next`].
    #[uniffi::method(name = "next")]
    pub async fn ffi_next(&self) -> Result<Amount> {
        self.next_shared().await
    }
}
