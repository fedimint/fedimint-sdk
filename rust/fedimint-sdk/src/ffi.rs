//! Shared UniFFI machinery, and nothing else.
//!
//! Every type this crate exports to UniFFI lives beside the real type it
//! adapts (`ecash.rs`, `lightning.rs`, `onchain.rs`, `recovery.rs`,
//! `federation.rs`, `sdk.rs`), so a reader never has to leave a facade's own
//! file to see its FFI shape. The exceptions are genuinely repeated
//! machinery: `Operation<S>` is monomorphised into a UniFFI object once per
//! concrete state type, and hand-writing that seven times over would be
//! pure repetition, and the three quote types share one single-use guard.
//! This module holds the `ffi_operation!` macro, invoked from each facade
//! file so the *generated code* still lands next to the real type, and
//! [`QuoteClaim`]. The whole module is `#[cfg(feature = "uniffi")]` gated.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::{Error, ErrorCode, ErrorDetails, Result, Timestamp};

/// The single-use guard a quote object carries across the UniFFI boundary.
///
/// In plain Rust `send` takes its quote by value, so a quote is consumed by
/// the first attempt whatever that attempt's outcome, and a second attempt
/// does not compile. A binding only ever holds a shared `Arc` to the quote,
/// so the quote holds this instead and each facade's UniFFI `send` claims it
/// before doing anything else. The claim is never released, for parity with
/// the by-value API: a failed attempt may already have moved funds, so the
/// only safe retry is a fresh quote.
#[derive(Debug, Default)]
pub(crate) struct QuoteClaim(AtomicBool);

impl QuoteClaim {
    /// Claims the quote for one `send` attempt.
    ///
    /// # Errors
    ///
    /// [`QuoteExpired`](ErrorCode::QuoteExpired) with
    /// `already_executed: true` if an earlier attempt already claimed it,
    /// the sub-case [`ErrorDetails::QuoteExpired`] reserves for a binding
    /// reusing a quote object. The message says the quote was already
    /// submitted, not that anything was paid: the earlier attempt may have
    /// failed.
    pub(crate) fn claim(&self, expires_at: Timestamp) -> Result<()> {
        if self.0.swap(true, Ordering::SeqCst) {
            return Err(Error::with_details(
                ErrorCode::QuoteExpired,
                "this quote was already submitted to send; quote again",
                ErrorDetails::QuoteExpired {
                    expires_at,
                    already_executed: true,
                },
            ));
        }
        Ok(())
    }
}

/// Monomorphises one `(Operation<S>, OperationUpdates<S>)` pair into a
/// UniFFI-exportable object pair, invoked once per concrete state type next
/// to the facade that produces it (`ecash.rs`, `lightning.rs`,
/// `onchain.rs`, `recovery.rs`) so the generated code lives beside the real
/// type it wraps rather than in one shared file.
///
/// `Operation<S>` is generic and UniFFI objects cannot be, so `$op` and
/// `$updates` name the two newtypes to emit for one concrete `$state`; an
/// optional `details: $details` adds the [`Operation::details`] accessor,
/// returning `$details` converted `From` the state's real details type
/// for a state that implements [`DetailedOperationState`]
/// ([`RecoveryState`] does not, so its invocation omits it).
/// [`OperationUpdates::next`] takes `&mut self`, the way Rust says "one
/// subscriber, one cursor"; `$updates` holds the real subscriber behind a
/// lock so that guarantee survives crossing into a language with no borrow
/// checker to enforce it — the same adaptation this crate's top-level
/// documentation describes for [`BalanceUpdates`](crate::BalanceUpdates).
macro_rules! ffi_operation {
    ($op:ident, $updates:ident, $state:ty $(, details: $details:ty)?) => {
        /// The UniFFI view of one `Operation<S>` instantiation: an opaque
        /// object wrapping the real handle, forwarding every method.
        #[derive(Debug, uniffi::Object)]
        pub struct $op(crate::Operation<$state>);

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
                // Identity for most states; ecash converts into its
                // `Arc<Notes>`-holding projection.
                #[allow(clippy::useless_conversion)]
                pub async fn details(&self) -> crate::Result<$details> {
                    self.0.details().await.map(Into::into)
                }
            )?
        }

        /// The UniFFI view of one `OperationUpdates<S>` instantiation,
        /// behind a lock so `next`'s `&mut self` can cross as `&self`.
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
