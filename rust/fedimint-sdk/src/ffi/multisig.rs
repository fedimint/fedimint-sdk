//! The multisig facade's FFI adapters.

use std::sync::Arc;

use super::ffi_operation;
use crate::{
    Amount, Approval, Multisig, MultisigAccount, MultisigDepositDetails, MultisigDepositState,
    MultisigSweepDetails, MultisigSweepState, MultisigTransferDetails, MultisigTransferState,
    Policy, Proposal, Result,
};

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
