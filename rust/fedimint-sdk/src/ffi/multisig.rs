//! The multisig facade's FFI adapters.

use std::sync::Arc;

use super::ffi_operation;
use crate::{
    Amount, AnyOperation, Approval, Federation, MemberKey, Multisig, MultisigAccount,
    MultisigAccountId, MultisigDepositDetails, MultisigDepositState, MultisigSweepDetails,
    MultisigSweepState, MultisigTransferDetails, MultisigTransferState, Policy, Proposal, Result,
};

uniffi::custom_type!(MemberKey, String, {
    lower: |key| key.to_string(),
    try_lift: |s| s.parse::<MemberKey>().map_err(Into::into),
});

uniffi::custom_type!(MultisigAccountId, String, {
    lower: |id| id.to_string(),
    try_lift: |s| s.parse::<MultisigAccountId>().map_err(Into::into),
});

// The UniFFI view of `Federation::multisig`, under its real name but a different Rust identifier
// so it can wrap the facade in `Arc`, as `ffi/federation.rs` does for the other facades. In a
// block of its own because the facade is behind the `multisig` feature.
#[uniffi::export]
impl Federation {
    /// See [`Federation::multisig`].
    #[uniffi::method(name = "multisig")]
    pub fn ffi_multisig(&self) -> Option<Arc<Multisig>> {
        self.multisig().map(Arc::new)
    }
}

// The downcasts to the three multisig operations, as `ffi/operation.rs` has them for the other
// kinds.
#[uniffi::export]
impl AnyOperation {
    /// See [`AnyOperation::as_multisig_deposit`].
    #[uniffi::method(name = "as_multisig_deposit")]
    pub fn ffi_as_multisig_deposit(&self) -> Option<Arc<MultisigDepositOperation>> {
        self.as_multisig_deposit().map(|op| Arc::new(op.into()))
    }

    /// See [`AnyOperation::as_multisig_transfer`].
    #[uniffi::method(name = "as_multisig_transfer")]
    pub fn ffi_as_multisig_transfer(&self) -> Option<Arc<MultisigTransferOperation>> {
        self.as_multisig_transfer().map(|op| Arc::new(op.into()))
    }

    /// See [`AnyOperation::as_multisig_sweep`].
    #[uniffi::method(name = "as_multisig_sweep")]
    pub fn ffi_as_multisig_sweep(&self) -> Option<Arc<MultisigSweepOperation>> {
        self.as_multisig_sweep().map(|op| Arc::new(op.into()))
    }
}

// The UniFFI view of `Multisig::account`/`deposit`/`execute`/`sweep`, under their real names but
// different Rust identifiers: `account` takes its policy, a record, by reference, which a binding
// hands over by value; `execute` takes a slice of approvals, which crosses as a list of shared
// handles; and the other three return the generic `Operation<S>`, which cannot cross at all. The
// rest of the facade needs no adapter and keeps the export attribute in `multisig.rs`.
#[cfg_attr(not(target_family = "wasm"), uniffi::export(async_runtime = "tokio"))]
#[cfg_attr(target_family = "wasm", uniffi::export)]
impl Multisig {
    /// See [`Multisig::account`].
    #[uniffi::method(name = "account")]
    pub fn ffi_account(&self, policy: Policy) -> Result<MultisigAccount> {
        self.account(&policy)
    }

    /// See [`Multisig::deposit`].
    #[uniffi::method(name = "deposit")]
    pub async fn ffi_deposit(
        &self,
        account: &MultisigAccount,
        amount: Amount,
    ) -> Result<MultisigDepositOperation> {
        Ok(self.deposit(account, amount).await?.into())
    }

    /// See [`Multisig::execute`].
    #[uniffi::method(name = "execute")]
    pub async fn ffi_execute(
        &self,
        proposal: &Proposal,
        approvals: Vec<Arc<Approval>>,
    ) -> Result<MultisigTransferOperation> {
        let approvals: Vec<Approval> = approvals
            .iter()
            .map(|approval| (**approval).clone())
            .collect();
        Ok(self.execute(proposal, &approvals).await?.into())
    }

    /// See [`Multisig::sweep`].
    #[uniffi::method(name = "sweep")]
    pub async fn ffi_sweep(&self) -> Result<Option<Arc<MultisigSweepOperation>>> {
        Ok(self.sweep().await?.map(|op| Arc::new(op.into())))
    }
}

// The UniFFI views of the three multisig `Operation<S>`s: `Operation<S>` is generic and UniFFI
// objects cannot be, so `ffi_operation!` monomorphises one newtype object per state, forwarding
// every method to the real handle. See that macro's documentation in `ffi/operation.rs`.
ffi_operation!(
    MultisigDepositOperation,
    MultisigDepositOperationUpdates,
    MultisigDepositState,
    details: MultisigDepositDetails
);
ffi_operation!(
    MultisigTransferOperation,
    MultisigTransferOperationUpdates,
    MultisigTransferState,
    details: MultisigTransferDetails
);
ffi_operation!(
    MultisigSweepOperation,
    MultisigSweepOperationUpdates,
    MultisigSweepState,
    details: MultisigSweepDetails
);
