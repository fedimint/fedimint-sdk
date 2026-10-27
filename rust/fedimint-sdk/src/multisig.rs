//! Accounts held by a group, paid out of with the approval of enough of its members.

use std::sync::Arc;

use crate::federation::FederationInner;
use crate::operation::{DetailedOperationState, OperationDetails, OperationState, kinds, sealed};
use crate::{Amount, Error, ErrorCode, Operation, Result, Timestamp};

mod account;
mod backend;
mod driver;
mod proposal;

pub use account::{
    MemberKey, MultisigAccount, MultisigAccountId, MultisigAmount, MultisigDenomination, Policy,
};
pub use backend::MultisigBackend;
#[cfg(feature = "extension-api")]
pub use backend::MultisigSweep;
pub(crate) use driver::{MultisigDepositDriver, MultisigSweepDriver, MultisigTransferDriver};
pub use proposal::{Approval, Proposal};

/// The multisig facade for one federation.
///
/// Obtained from [`Federation::multisig`](crate::Federation::multisig), which returns `None`
/// unless this build carries a module that backs it and the federation runs that module.
///
/// A multisig account belongs to a group: several members, each with a key, and a threshold of
/// how many of them it takes to pay anything out. The account is described by its [`Policy`] and
/// needs no setting up on the federation: it exists from the first deposit. The flow:
///
/// 1. Each member takes its key for the group from [`member_key`](Multisig::member_key), and the
///    members agree on a [`Policy`]: who is in, and the threshold.
/// 2. [`account`](Multisig::account) turns the policy into its [`MultisigAccount`], the same one
///    for every member.
/// 3. Anyone [`deposit`](Multisig::deposit)s into it from their ecash balance.
/// 4. A member who wants money out makes a [`Proposal`] with [`propose`](Multisig::propose). The
///    other members each [`approve`](Multisig::approve) it, and once enough have, anyone
///    [`execute`](Multisig::execute)s it with their approvals.
/// 5. An executed proposal pays the member who proposed it. That member's
///    [`sweep`](Multisig::sweep) moves what it was paid into its ecash balance.
///
/// The application moves keys, proposals and approvals between the members, over whatever channel
/// it has, and keeps the group's policy. Neither this crate nor the federation keeps a list of a
/// member's groups. The channel is never trusted with money: the federation pays out only on the
/// signatures of enough of the account's members.
///
/// Like every other facade's, a call that needs the federation fails with
/// [`FederationClosed`](crate::ErrorCode::FederationClosed) once it is closed. The two that do not
/// need it keep answering: [`account`](Multisig::account), which is computed locally, and
/// [`denominations`](Multisig::denominations).
#[derive(Clone)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct Multisig {
    inner: Arc<MultisigInner>,
}

impl core::fmt::Debug for Multisig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Multisig")
            .field("federation", &self.inner.federation.id)
            .field("module", &self.inner.backend.module_kind())
            .finish()
    }
}

// Exported as they are: every argument and return type here already crosses. The calls in the next
// block are adapted in `ffi/multisig.rs`: they take a record by reference or a list of objects, or
// return an operation.
#[cfg_attr(
    all(feature = "uniffi", not(target_family = "wasm")),
    uniffi::export(async_runtime = "tokio")
)]
#[cfg_attr(all(feature = "uniffi", target_family = "wasm"), uniffi::export)]
impl Multisig {
    /// The denominations an account can be held in on this federation.
    ///
    /// [`account`](Multisig::account) refuses a policy in any other one with
    /// [`NotSupported`](crate::ErrorCode::NotSupported).
    pub fn denominations(&self) -> Vec<MultisigDenomination> {
        self.inner.backend.denominations()
    }

    /// This member's key for `group`.
    ///
    /// `group` is any name the application gives the group, the same on every member's device.
    /// The same group always gives the same key, from this seed, and different groups give
    /// unrelated keys, so that two groups cannot be linked through a shared key. Restoring the
    /// seed restores every key.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for an empty `group`,
    /// [`NotSupported`](crate::ErrorCode::NotSupported) if the federation no longer runs the
    /// module, and [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn member_key(&self, group: &str) -> Result<MemberKey> {
        check_group(group)?;
        let client = self.inner.federation.client(false).await?;
        self.inner.backend.member_key(&client, group)
    }

    /// What `account` holds, in its denomination.
    ///
    /// Read from the federation, so it can be shown to any member, and to anyone else who knows
    /// the policy. An account nothing was ever deposited into holds zero.
    ///
    /// # Errors
    ///
    /// [`NotSupported`](crate::ErrorCode::NotSupported) for an account in a denomination this
    /// federation does not hold,
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn balance(&self, account: &MultisigAccount) -> Result<MultisigAmount> {
        self.check_denomination(account.denomination())?;
        let client = self.inner.federation.client(false).await?;
        self.inner.backend.balance(&client, account).await
    }

    /// Approves `proposal`, as this member of `group`.
    ///
    /// Show the member the proposal's [`amount`](Proposal::amount) and account before calling
    /// this: an approval is a signature that lets the payment through once enough members have
    /// given theirs. The proposal is checked first, so a proposal whose contents do not match what
    /// it says it pays is refused rather than signed.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for an empty `group`, for a proposal made
    /// for another kind of module, for one that does not pay what it says, and when this member's
    /// key for `group` is not one of the account's members (a different group name, most often),
    /// [`NotSupported`](crate::ErrorCode::NotSupported), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn approve(&self, group: &str, proposal: &Proposal) -> Result<Approval> {
        check_group(group)?;
        self.check_module(proposal)?;
        let client = self.inner.federation.client(false).await?;
        let member = self.inner.backend.member_key(&client, group)?;
        if !proposal.account().members().contains(&member) {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "this member's key for that group is not in the proposal's policy",
            ));
        }
        let payload = self.inner.backend.approve(&client, group, proposal)?;
        Ok(Approval::new(proposal.digest(), member, payload))
    }

    /// Proposes paying `amount` out of `account` to this member.
    ///
    /// Nothing moves yet, and nothing is sent anywhere: hand the returned proposal to the other
    /// members to [`approve`](Multisig::approve), then [`execute`](Multisig::execute) it with
    /// their approvals. The payment goes to the member who proposed it, whoever executes it, and
    /// [`sweep`](Multisig::sweep) then moves it into that member's ecash balance.
    ///
    /// A proposal does not stay valid for ever: the federation refuses one executed too long after
    /// it was made, and the payout then ends [`Failed`](MultisigTransferState::Failed).
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a zero amount or one in a different
    /// denomination from the account's, [`NotSupported`](crate::ErrorCode::NotSupported) for an
    /// account in a denomination this federation does not hold,
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn propose(
        &self,
        account: &MultisigAccount,
        amount: MultisigAmount,
    ) -> Result<Proposal> {
        proposal::check_amount(account, &amount)?;
        self.check_denomination(account.denomination())?;
        let client = self.inner.federation.client(false).await?;
        let payload = self
            .inner
            .backend
            .propose(&client, account, &amount)
            .await?;
        Ok(Proposal::new(
            self.inner.backend.module_kind(),
            account.clone(),
            amount,
            payload,
        ))
    }
}

impl Multisig {
    /// The account `policy` describes.
    ///
    /// Computed here, without asking the federation: every member who holds the same policy gets
    /// the same account, whatever order the members are listed in.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a policy with no members, with a
    /// member listed twice, or with a threshold of zero or above the number of members, and
    /// [`NotSupported`](crate::ErrorCode::NotSupported) for a denomination this federation does
    /// not hold accounts in.
    pub fn account(&self, policy: &Policy) -> Result<MultisigAccount> {
        self.check_denomination(policy.denomination)?;
        MultisigAccount::derive(policy)
    }

    /// Moves `amount` from the ecash balance into `account`.
    ///
    /// Anyone can deposit into an account, member or not. The deposit is final once
    /// [`Done`](MultisigDepositState::Done): from then on only the account's members, enough of
    /// them, can move the money again.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a zero amount,
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance),
    /// [`NotSupported`](crate::ErrorCode::NotSupported) for an account in a denomination this
    /// federation does not hold, and when the federation cannot take deposits yet,
    /// [`Recovering`](crate::ErrorCode::Recovering) while a recovery for this federation is
    /// incomplete, [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Storage`](crate::ErrorCode::Storage) and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn deposit(
        &self,
        account: &MultisigAccount,
        amount: Amount,
    ) -> Result<Operation<MultisigDepositState>> {
        if amount.msats() == 0 {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "cannot deposit nothing",
            ));
        }
        self.check_denomination(account.denomination())?;
        let federation = &self.inner.federation;
        federation.ensure_open()?;
        let client = federation.client(true).await?;
        let id = self.inner.backend.deposit(&client, account, amount).await?;
        let details = MultisigDepositDetails {
            account_id: account.id(),
            amount,
            created_at: Timestamp::from_epoch_millis(crate::db::now_millis()),
        };
        // Written while `client` is still held, as `Driver::subscribe` asks of every facade.
        federation
            .create_operation(
                id,
                kinds::MULTISIG_DEPOSIT,
                &self.inner.backend.module_kind(),
                &driver::DepositDetailsWire::from(&details),
                Arc::new(MultisigDepositDriver::default()),
            )
            .await
    }

    /// Pays out `proposal`, with `approvals` from enough of its account's members.
    ///
    /// Anyone holding the proposal and the approvals can execute it, member or not; the payment
    /// goes to the member who proposed it either way. A member's approval passed twice counts
    /// once. The operation reaches [`Done`](MultisigTransferState::Done) when the federation has
    /// paid out, and [`Failed`](MultisigTransferState::Failed) when it refuses to: a signature it
    /// does not accept, an account that no longer holds enough, or a proposal that has expired.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a proposal made for another kind of
    /// module, an approval of a different proposal or from someone who is not a member, and fewer
    /// approvals than the account's threshold, [`NotSupported`](crate::ErrorCode::NotSupported)
    /// when the federation cannot pay out of multisig accounts yet,
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Storage`](crate::ErrorCode::Storage) and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn execute(
        &self,
        proposal: &Proposal,
        approvals: &[Approval],
    ) -> Result<Operation<MultisigTransferState>> {
        self.check_module(proposal)?;
        let approvals = proposal::approvals_for(proposal, approvals)?;
        let federation = &self.inner.federation;
        federation.ensure_open()?;
        let client = federation.client(false).await?;
        let id = self
            .inner
            .backend
            .execute(&client, proposal, &approvals)
            .await?;
        let details = MultisigTransferDetails {
            account_id: proposal.account_id(),
            amount: proposal.amount(),
            created_at: Timestamp::from_epoch_millis(crate::db::now_millis()),
        };
        federation
            .create_operation(
                id,
                kinds::MULTISIG_TRANSFER,
                &self.inner.backend.module_kind(),
                &driver::TransferDetailsWire::from(&details),
                Arc::new(MultisigTransferDriver::default()),
            )
            .await
    }

    /// Moves everything multisig accounts paid out to this member into its ecash balance, or
    /// returns `None` when nothing is waiting.
    ///
    /// What [`execute`](Multisig::execute) pays out waits for this, whoever executed it: call it
    /// once a payout this member proposed is [`Done`](MultisigTransferState::Done), and when the
    /// application starts, to collect any payout that finished while it was not running.
    ///
    /// # Errors
    ///
    /// [`Recovering`](crate::ErrorCode::Recovering) while a recovery for this federation is
    /// incomplete, [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Storage`](crate::ErrorCode::Storage) and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn sweep(&self) -> Result<Option<Operation<MultisigSweepState>>> {
        let federation = &self.inner.federation;
        federation.ensure_open()?;
        let client = federation.client(true).await?;
        let Some(sweep) = self.inner.backend.sweep(&client).await? else {
            return Ok(None);
        };
        let details = MultisigSweepDetails {
            amount: sweep.amount,
            created_at: Timestamp::from_epoch_millis(crate::db::now_millis()),
        };
        federation
            .create_operation(
                sweep.operation,
                kinds::MULTISIG_SWEEP,
                &self.inner.backend.module_kind(),
                &driver::SweepDetailsWire::from(&details),
                Arc::new(MultisigSweepDriver::default()),
            )
            .await
            .map(Some)
    }

    /// Builds the facade over `backend`. Handed out by `Federation::multisig`.
    pub(crate) fn new(
        federation: Arc<FederationInner>,
        backend: Arc<dyn MultisigBackend>,
    ) -> Multisig {
        Multisig {
            inner: Arc::new(MultisigInner {
                federation,
                backend,
            }),
        }
    }

    /// Refuses a denomination the backend does not hold accounts in.
    fn check_denomination(&self, denomination: MultisigDenomination) -> Result<()> {
        if self.denominations().contains(&denomination) {
            return Ok(());
        }
        Err(Error::new(
            ErrorCode::NotSupported,
            format!("this federation holds no multisig accounts in {denomination:?}"),
        ))
    }

    /// Refuses a proposal another kind of backend made, which this one cannot read.
    fn check_module(&self, proposal: &Proposal) -> Result<()> {
        let module = self.inner.backend.module_kind();
        if proposal.module() == module {
            return Ok(());
        }
        Err(Error::new(
            ErrorCode::InvalidInput,
            format!(
                "the proposal was made for the {} module, and this federation's multisig runs on \
                 {module}",
                proposal.module()
            ),
        ))
    }
}

/// Refuses an empty group name, which would give every unnamed group the same key.
fn check_group(group: &str) -> Result<()> {
    if group.is_empty() {
        return Err(Error::new(ErrorCode::InvalidInput, "a group needs a name"));
    }
    Ok(())
}

/// The federation and the backend a [`Multisig`] facade operates on.
///
/// The federation rather than a module handle, because a facade outlives the client behind it: a
/// call on a closed federation has to report `FederationClosed` rather than find nothing to talk
/// to.
struct MultisigInner {
    federation: Arc<FederationInner>,
    backend: Arc<dyn MultisigBackend>,
}

/// The progress of a [`Multisig::deposit`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[non_exhaustive]
pub enum MultisigDepositState {
    /// The deposit has been submitted to the federation.
    Created,
    /// The federation accepted the deposit: the account holds it. The change from the ecash this
    /// paid with is still being issued.
    Accepted,
    /// Final: the account holds the deposit, and the change is back in the ecash balance.
    Done,
    /// Final: the deposit did not go through.
    Failed {
        /// Human-readable explanation. Diagnostic only, not a stable contract, and not something
        /// to match on.
        reason: String,
    },
}

impl sealed::Sealed for MultisigDepositState {}

impl OperationState for MultisigDepositState {
    fn is_final(&self) -> bool {
        match self {
            MultisigDepositState::Created | MultisigDepositState::Accepted => false,
            MultisigDepositState::Done | MultisigDepositState::Failed { .. } => true,
        }
    }
}

/// The progress of a [`Multisig::execute`]: a payment out of a multisig account to the member who
/// proposed it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[non_exhaustive]
pub enum MultisigTransferState {
    /// The payment has been submitted to the federation.
    Created,
    /// Final: the federation paid it. The proposer collects it with
    /// [`Multisig::sweep`].
    Done,
    /// Final: the federation refused it.
    Failed {
        /// Human-readable explanation. Diagnostic only, not a stable contract, and not something
        /// to match on.
        reason: String,
    },
}

impl sealed::Sealed for MultisigTransferState {}

impl OperationState for MultisigTransferState {
    fn is_final(&self) -> bool {
        match self {
            MultisigTransferState::Created => false,
            MultisigTransferState::Done | MultisigTransferState::Failed { .. } => true,
        }
    }
}

/// The progress of a [`Multisig::sweep`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[non_exhaustive]
pub enum MultisigSweepState {
    /// The sweep has been submitted to the federation.
    Created,
    /// The federation accepted it, and is issuing the ecash.
    Accepted,
    /// Final: the swept value is in the ecash balance.
    Done,
    /// Final: the sweep did not go through, and what was waiting is still waiting.
    Failed {
        /// Human-readable explanation. Diagnostic only, not a stable contract, and not something
        /// to match on.
        reason: String,
    },
}

impl sealed::Sealed for MultisigSweepState {}

impl OperationState for MultisigSweepState {
    fn is_final(&self) -> bool {
        match self {
            MultisigSweepState::Created | MultisigSweepState::Accepted => false,
            MultisigSweepState::Done | MultisigSweepState::Failed { .. } => true,
        }
    }
}

/// What a [`Multisig::deposit`] is, as opposed to where it has got to.
///
/// Every field is fixed when the deposit is made.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[non_exhaustive]
pub struct MultisigDepositDetails {
    /// The account deposited into.
    pub account_id: MultisigAccountId,
    /// How much went into the account. The fee for spending the ecash it came from is on top.
    pub amount: Amount,
    /// When the deposit was made, by this device's clock.
    pub created_at: Timestamp,
}

impl sealed::Sealed for MultisigDepositDetails {}

impl OperationDetails for MultisigDepositDetails {}

impl DetailedOperationState for MultisigDepositState {
    type Details = MultisigDepositDetails;
}

/// What a [`Multisig::execute`] is, as opposed to where it has got to.
///
/// Every field is fixed when the proposal is executed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[non_exhaustive]
pub struct MultisigTransferDetails {
    /// The account paid out of.
    pub account_id: MultisigAccountId,
    /// How much the proposal pays, in the account's denomination.
    pub amount: MultisigAmount,
    /// When the proposal was executed, by this device's clock.
    pub created_at: Timestamp,
}

impl sealed::Sealed for MultisigTransferDetails {}

impl OperationDetails for MultisigTransferDetails {}

impl DetailedOperationState for MultisigTransferState {
    type Details = MultisigTransferDetails;
}

/// What a [`Multisig::sweep`] is, as opposed to where it has got to.
///
/// Every field is fixed when the sweep starts.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[non_exhaustive]
pub struct MultisigSweepDetails {
    /// How much the sweep moves into the ecash balance, before the fee for issuing it.
    pub amount: Amount,
    /// When the sweep started, by this device's clock.
    pub created_at: Timestamp,
}

impl sealed::Sealed for MultisigSweepDetails {}

impl OperationDetails for MultisigSweepDetails {}

impl DetailedOperationState for MultisigSweepState {
    type Details = MultisigSweepDetails;
}

/// A backend for tests, which must never be reached: every refusal the facade owes is made before
/// it asks the backend anything, and a federation without a client stops a call before that.
#[cfg(test)]
pub(crate) struct UnreachableBackend;

#[cfg(test)]
impl UnreachableBackend {
    /// The module kind it claims to run on.
    pub(crate) const MODULE: &'static str = "test_multisig";

    fn reached() -> Error {
        Error::new(ErrorCode::Internal, "the backend was reached")
    }
}

#[cfg(test)]
impl MultisigBackend for UnreachableBackend {
    fn module_kind(&self) -> String {
        UnreachableBackend::MODULE.to_owned()
    }

    fn denominations(&self) -> Vec<MultisigDenomination> {
        vec![MultisigDenomination::Btc]
    }

    fn member_key(&self, _client: &fedimint_client::Client, _group: &str) -> Result<MemberKey> {
        Err(UnreachableBackend::reached())
    }

    fn balance<'a>(
        &'a self,
        _client: &'a fedimint_client::Client,
        _account: &'a MultisigAccount,
    ) -> fedimint_core::util::BoxFuture<'a, Result<MultisigAmount>> {
        Box::pin(async { Err(UnreachableBackend::reached()) })
    }

    fn deposit<'a>(
        &'a self,
        _client: &'a fedimint_client::Client,
        _account: &'a MultisigAccount,
        _amount: Amount,
    ) -> fedimint_core::util::BoxFuture<'a, Result<fedimint_core::core::OperationId>> {
        Box::pin(async { Err(UnreachableBackend::reached()) })
    }

    fn propose<'a>(
        &'a self,
        _client: &'a fedimint_client::Client,
        _account: &'a MultisigAccount,
        _amount: &'a MultisigAmount,
    ) -> fedimint_core::util::BoxFuture<'a, Result<Vec<u8>>> {
        Box::pin(async { Err(UnreachableBackend::reached()) })
    }

    fn approve(
        &self,
        _client: &fedimint_client::Client,
        _group: &str,
        _proposal: &Proposal,
    ) -> Result<Vec<u8>> {
        Err(UnreachableBackend::reached())
    }

    fn execute<'a>(
        &'a self,
        _client: &'a fedimint_client::Client,
        _proposal: &'a Proposal,
        _approvals: &'a [Approval],
    ) -> fedimint_core::util::BoxFuture<'a, Result<fedimint_core::core::OperationId>> {
        Box::pin(async { Err(UnreachableBackend::reached()) })
    }

    fn sweep<'a>(
        &'a self,
        _client: &'a fedimint_client::Client,
    ) -> fedimint_core::util::BoxFuture<'a, Result<Option<backend::MultisigSweep>>> {
        Box::pin(async { Err(UnreachableBackend::reached()) })
    }

    fn deposit_updates<'a>(
        &'a self,
        _client: &'a fedimint_client::Client,
        _id: fedimint_core::core::OperationId,
    ) -> fedimint_core::util::BoxFuture<
        'a,
        Result<fedimint_core::util::BoxStream<'static, Result<MultisigDepositState>>>,
    > {
        Box::pin(async { Err(UnreachableBackend::reached()) })
    }

    fn transfer_updates<'a>(
        &'a self,
        _client: &'a fedimint_client::Client,
        _id: fedimint_core::core::OperationId,
    ) -> fedimint_core::util::BoxFuture<
        'a,
        Result<fedimint_core::util::BoxStream<'static, Result<MultisigTransferState>>>,
    > {
        Box::pin(async { Err(UnreachableBackend::reached()) })
    }

    fn sweep_updates<'a>(
        &'a self,
        _client: &'a fedimint_client::Client,
        _id: fedimint_core::core::OperationId,
    ) -> fedimint_core::util::BoxFuture<
        'a,
        Result<fedimint_core::util::BoxStream<'static, Result<MultisigSweepState>>>,
    > {
        Box::pin(async { Err(UnreachableBackend::reached()) })
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use crate::db::{federation_namespace, in_memory_root};
    use crate::multisig::account::test_keys;

    /// The facade over a federation with no client, open or closed.
    fn multisig(open: bool) -> Multisig {
        let federation =
            FederationInner::detached(federation_namespace(&in_memory_root(), [1u8; 32]), open);
        Multisig::new(federation, Arc::new(UnreachableBackend))
    }

    fn policy(denomination: MultisigDenomination) -> Policy {
        let [a, b, c] = test_keys();
        Policy::new(vec![a, b, c], 2, denomination)
    }

    fn proposal(module: &str, account: &MultisigAccount) -> Proposal {
        Proposal::new(
            module.to_owned(),
            account.clone(),
            MultisigAmount::Btc {
                amount: Amount::from_msats(1_000),
            },
            Vec::new(),
        )
    }

    #[test]
    fn an_account_is_held_only_in_a_denomination_the_backend_holds() {
        let multisig = multisig(true);
        multisig
            .account(&policy(MultisigDenomination::Btc))
            .expect("bitcoin is held");
        let err = multisig
            .account(&policy(MultisigDenomination::Fiat))
            .expect_err("fiat is not");
        assert_eq!(err.code, ErrorCode::NotSupported);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn nothing_is_deposited_or_proposed_for_nothing_or_in_another_denomination() {
        let multisig = multisig(true);
        let account = multisig
            .account(&policy(MultisigDenomination::Btc))
            .expect("a valid policy");

        let err = multisig
            .deposit(&account, Amount::from_msats(0))
            .await
            .expect_err("a zero deposit");
        assert_eq!(err.code, ErrorCode::InvalidInput);

        let err = multisig
            .propose(
                &account,
                MultisigAmount::Btc {
                    amount: Amount::from_msats(0),
                },
            )
            .await
            .expect_err("a zero payout");
        assert_eq!(err.code, ErrorCode::InvalidInput);

        let err = multisig
            .propose(
                &account,
                MultisigAmount::Fiat {
                    minor_units: 100,
                    currency: "USD".to_owned(),
                },
            )
            .await
            .expect_err("fiat out of a bitcoin account");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_group_needs_a_name() {
        let multisig = multisig(true);
        let account = multisig
            .account(&policy(MultisigDenomination::Btc))
            .expect("a valid policy");

        let err = multisig.member_key("").await.expect_err("no name");
        assert_eq!(err.code, ErrorCode::InvalidInput);
        let err = multisig
            .approve("", &proposal(UnreachableBackend::MODULE, &account))
            .await
            .expect_err("no name");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_proposal_for_another_module_or_short_of_approvals_is_refused_up_front() {
        let multisig = multisig(true);
        let account = multisig
            .account(&policy(MultisigDenomination::Btc))
            .expect("a valid policy");

        let elsewhere = proposal("another_module", &account);
        let err = multisig
            .approve("group", &elsewhere)
            .await
            .expect_err("not this backend's");
        assert_eq!(err.code, ErrorCode::InvalidInput);
        let err = multisig
            .execute(&elsewhere, &[])
            .await
            .expect_err("not this backend's");
        assert_eq!(err.code, ErrorCode::InvalidInput);

        let [a, ..] = test_keys();
        let ours = proposal(UnreachableBackend::MODULE, &account);
        let one = Approval::new(ours.digest(), a, Vec::new());
        let err = multisig
            .execute(&ours, &[one])
            .await
            .expect_err("one approval of the two needed");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_federation_that_is_not_running_answers_closed() {
        let multisig = multisig(false);
        let account = multisig
            .account(&policy(MultisigDenomination::Btc))
            .expect("a valid policy, read without the federation");

        let err = multisig.member_key("group").await.expect_err("no client");
        assert_eq!(err.code, ErrorCode::FederationClosed);
        let err = multisig
            .deposit(&account, Amount::from_msats(1_000))
            .await
            .expect_err("no client");
        assert_eq!(err.code, ErrorCode::FederationClosed);
        let err = multisig.sweep().await.expect_err("no client");
        assert_eq!(err.code, ErrorCode::FederationClosed);
    }
}
