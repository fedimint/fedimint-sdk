//! The monomorphising macro every facade submodule invokes, and the type-erased handle's
//! downcasts.

use std::sync::Arc;

use crate::AnyOperation;

/// Monomorphises one `(Operation<S>, OperationUpdates<S>)` pair into a UniFFI-exportable object
/// pair, invoked once per concrete state type from the facade submodule that adapts the facade
/// producing it ([`super::ecash`], [`super::lightning`], [`super::onchain`], [`super::recovery`]).
///
/// `Operation<S>` is generic and UniFFI objects cannot be, so `$op` and `$updates` name the two
/// newtypes to emit for one concrete `$state`; an optional `details: $details` adds the
/// `Operation::details` accessor, returning `$details` converted `From` the state's real details
/// type for a state that implements `DetailedOperationState` (`RecoveryState` does not, so its
/// invocation omits it). `OperationUpdates::next` takes `&mut self`, the way Rust says "one
/// subscriber, one cursor"; `$updates` holds the real subscriber behind a lock so that guarantee
/// survives crossing into a language with no borrow checker to enforce it — the same adaptation
/// this crate's top-level documentation describes for [`BalanceUpdates`](crate::BalanceUpdates).
macro_rules! ffi_operation {
    ($op:ident, $updates:ident, $state:ty $(, details: $details:ty)?) => {
        /// The UniFFI view of one `Operation<S>` instantiation: an opaque object wrapping the real
        /// handle, forwarding every method.
        #[derive(Debug, uniffi::Object)]
        pub struct $op(pub(crate) crate::Operation<$state>);

        impl From<crate::Operation<$state>> for $op {
            fn from(op: crate::Operation<$state>) -> Self {
                Self(op)
            }
        }

        #[uniffi::export(async_runtime = "tokio")]
        impl $op {
            /// See [`Operation::id`](crate::Operation::id).
            pub fn id(&self) -> crate::OperationId {
                self.0.id()
            }

            /// See [`Operation::state`](crate::Operation::state).
            pub async fn state(&self) -> crate::Result<$state> {
                self.0.state().await
            }

            /// See [`Operation::updates`](crate::Operation::updates).
            pub fn updates(&self) -> $updates {
                $updates::from(self.0.updates())
            }

            /// See [`Operation::await_final`](crate::Operation::await_final).
            pub async fn await_final(&self) -> crate::Result<$state> {
                self.0.await_final().await
            }

            $(
                /// See [`Operation::details`](crate::Operation::details).
                // Identity for most states; ecash converts into its `Arc<Notes>`-holding
                // projection.
                #[allow(clippy::useless_conversion)]
                pub async fn details(&self) -> crate::Result<$details> {
                    self.0.details().await.map(Into::into)
                }
            )?
        }

        /// The UniFFI view of one `OperationUpdates<S>` instantiation, behind a lock so `next`'s
        /// `&mut self` can cross as `&self`.
        #[derive(Debug, uniffi::Object)]
        pub struct $updates(::tokio::sync::Mutex<crate::OperationUpdates<$state>>);

        impl From<crate::OperationUpdates<$state>> for $updates {
            fn from(updates: crate::OperationUpdates<$state>) -> Self {
                Self(::tokio::sync::Mutex::new(updates))
            }
        }

        #[uniffi::export(async_runtime = "tokio")]
        impl $updates {
            /// See [`OperationUpdates::next`](crate::OperationUpdates::next).
            pub async fn next(&self) -> crate::Result<Option<$state>> {
                self.0.lock().await.next().await
            }
        }
    };
}
pub(crate) use ffi_operation;

// The UniFFI view of the seven `AnyOperation::as_*` downcasts: same names, same `None`-on-mismatch
// behaviour, but returning the monomorphised `*Operation` helpers `ffi_operation!` emits in each
// facade submodule, instead of the generic `Operation<S>` a UniFFI object cannot carry directly.
#[uniffi::export]
impl AnyOperation {
    /// See [`AnyOperation::as_ecash_send`].
    #[uniffi::method(name = "as_ecash_send")]
    pub fn ffi_as_ecash_send(&self) -> Option<Arc<super::ecash::EcashSendOperation>> {
        self.as_ecash_send().map(|op| Arc::new(op.into()))
    }

    /// See [`AnyOperation::as_ecash_receive`].
    #[uniffi::method(name = "as_ecash_receive")]
    pub fn ffi_as_ecash_receive(&self) -> Option<Arc<super::ecash::EcashReceiveOperation>> {
        self.as_ecash_receive().map(|op| Arc::new(op.into()))
    }

    /// See [`AnyOperation::as_ln_send`].
    #[uniffi::method(name = "as_ln_send")]
    pub fn ffi_as_ln_send(&self) -> Option<Arc<super::lightning::LnSendOperation>> {
        self.as_ln_send().map(|op| Arc::new(op.into()))
    }

    /// See [`AnyOperation::as_ln_receive`].
    #[uniffi::method(name = "as_ln_receive")]
    pub fn ffi_as_ln_receive(&self) -> Option<Arc<super::lightning::LnReceiveOperation>> {
        self.as_ln_receive().map(|op| Arc::new(op.into()))
    }

    /// See [`AnyOperation::as_onchain_send`].
    #[uniffi::method(name = "as_onchain_send")]
    pub fn ffi_as_onchain_send(&self) -> Option<Arc<super::onchain::OnchainSendOperation>> {
        self.as_onchain_send().map(|op| Arc::new(op.into()))
    }

    /// See [`AnyOperation::as_onchain_receive`].
    #[uniffi::method(name = "as_onchain_receive")]
    pub fn ffi_as_onchain_receive(&self) -> Option<Arc<super::onchain::OnchainReceiveOperation>> {
        self.as_onchain_receive().map(|op| Arc::new(op.into()))
    }

    /// See [`AnyOperation::as_recovery`].
    #[uniffi::method(name = "as_recovery")]
    pub fn ffi_as_recovery(&self) -> Option<Arc<super::recovery::RecoveryOperation>> {
        self.as_recovery().map(|op| Arc::new(op.into()))
    }
}
