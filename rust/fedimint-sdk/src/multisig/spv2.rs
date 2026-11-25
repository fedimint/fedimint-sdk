//! The multisig facade's calls on Fedi's stability pool module (v2), which holds the accounts.
//!
//! What the stability pool adds to the facade's contract:
//!
//! - The federation has a minimum amount for deposits and payouts. A deposit below it, and a
//!   proposal to pay out less than it without emptying the account, are refused with
//!   [`InvalidInput`](crate::ErrorCode::InvalidInput).
//! - A proposal stays valid for [`PROPOSAL_LIFETIME`]. Executing it later ends the payout
//!   [`Failed`](crate::MultisigTransferState::Failed).
//! - Deposits need the federation's stability pool at version 2.1 or later, and payouts at 2.2
//!   or later. Until then they are refused with [`NotSupported`](crate::ErrorCode::NotSupported).
//! - A payout goes to the proposer's own payout account in the module, which
//!   [`sweep`] empties into the ecash balance.

use std::collections::BTreeMap;
use std::time::Duration;

use fedimint_api_client::api::FederationError;
use fedimint_client::Client;
use fedimint_client::module::{ClientModuleInstance, TransactionSubmitError};
use fedimint_core::core::OperationId as UpstreamOperationId;
use fedimint_core::module::ModuleConsensusVersion;
use fedimint_core::secp256k1::{Keypair, Message, schnorr};
use fedimint_core::util::BoxStream;
use futures::StreamExt as _;
use stability_pool_client::api::StabilityPoolApiExt as _;
use stability_pool_client::{
    OwnAccount, StabilityPoolClientModule, StabilityPoolDepositOperationState,
    StabilityPoolTransferOperationState, StabilityPoolWithdrawalOperationState,
};
use stability_pool_common::{
    Account, AccountType, AccountUnchecked, BTC_BALANCE_DEPOSIT_CONSENSUS_VERSION,
    BTC_TRANSFER_CONSENSUS_VERSION, BtcBalanceDepositMetadata, BtcTransferOutput,
    BtcTransferRequest, FiatOrAll, SignedBtcTransferRequest, StabilityPoolOutput,
    StabilityPoolOutputV2, TransferRequestId,
};

use crate::{
    Amount, Approval, Error, ErrorCode, ErrorDetails, MemberKey, MultisigAccount,
    MultisigDepositState, MultisigSweepState, MultisigTransferState, Proposal, Result,
};

/// The module kind the facade runs on: `stability_pool_common::KIND`, which is not a `const`
/// string.
pub(crate) const MODULE_KIND: &str = "multi_sig_stability_pool";

/// How long a proposal stays valid: the federation refuses to pay one out once this has passed
/// since it was made.
pub(crate) const PROPOSAL_LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// How long a sweep waits for the previous one on the same federation to finish.
///
/// Nothing locks in a payout account, so a sweep finishes as soon as the federation has accepted
/// its transactions. A sweep still running after this is reported as a `Timeout`, and no other
/// starts until it has ended.
const PREVIOUS_SWEEP_WAIT: Duration = Duration::from_secs(60);

/// The sweep last started on a federation, until it is known to have ended.
///
/// A sweep reads what the payout account holds, and one started while the previous one is still
/// claiming that would claim it again, and fail. Each federation has one of its own, so that one
/// slow to answer holds up no other.
pub(crate) type SweepLock = tokio::sync::Mutex<Option<UpstreamOperationId>>;

/// This member's key pair for `group`.
///
/// The same `group` always gives the same key, from the client's seed, and different groups give
/// unrelated keys.
pub(crate) fn member_keypair(client: &Client, group: &str) -> Result<Keypair> {
    Ok(module(client)?.derive_multispend_group_key(group.to_owned()))
}

/// This member's key for `group`.
pub(crate) fn member_key(client: &Client, group: &str) -> Result<MemberKey> {
    Ok(MemberKey::from_upstream(
        member_keypair(client, group)?.public_key(),
    ))
}

/// The module's account for `account`: a bitcoin account whose keys are the policy's members and
/// whose threshold is the policy's.
pub(crate) fn module_account(account: &MultisigAccount) -> Result<Account> {
    Account::try_from(AccountUnchecked {
        acc_type: AccountType::BtcDepositor,
        pub_keys: account.members().iter().map(MemberKey::upstream).collect(),
        threshold: u64::from(account.threshold()),
    })
    // The facade only hands out accounts whose policy it has checked, so this is a policy the
    // module does not accept although the facade does.
    .map_err(|err| internal(format!("the stability pool refuses the policy: {err}")))
}

/// What `account` holds. Zero for an account the federation has never seen.
pub(crate) async fn balance(client: &Client, account: &MultisigAccount) -> Result<Amount> {
    let module = module(client)?;
    let account = module_account(account)?;
    let sync = module
        .api
        .account_sync(account.id())
        .await
        .map_err(unreachable)?;
    let held = sync.staged_balance + sync.locked_balance + sync.idle_balance;
    Ok(Amount::from_msats(held.msats))
}

/// Starts moving `amount`, which is not zero, from the ecash balance into `account`.
pub(crate) async fn deposit(
    client: &Client,
    account: &MultisigAccount,
    amount: Amount,
) -> Result<UpstreamOperationId> {
    let module = module(client)?;
    let account = module_account(account)?;
    let amount = fedimint_core::Amount::from_msats(amount.msats());
    let minimum = module.cfg.min_allowed_seek;
    if amount < minimum {
        return Err(invalid(format!(
            "the federation takes deposits into multisig accounts of {minimum} or more"
        )));
    }
    require_version(
        &module,
        BTC_BALANCE_DEPOSIT_CONSENSUS_VERSION,
        "deposits into multisig accounts",
    )
    .await?;
    let deposit = module
        .deposit_to_btc_balance(
            account.id(),
            amount,
            BtcBalanceDepositMetadata(Vec::new()),
            (),
        )
        .await;
    match deposit {
        Ok(id) => Ok(id),
        Err(err) => Err(submit_error(client, err.as_ref(), Some(amount), "the deposit").await),
    }
}

/// The module's request to pay `amount`, which is not zero, out of `account` to this member's
/// payout account.
pub(crate) async fn payout_request(
    client: &Client,
    account: &MultisigAccount,
    amount: Amount,
) -> Result<BtcTransferRequest> {
    let module = module(client)?;
    let from = module_account(account)?;
    let amount = fedimint_core::Amount::from_msats(amount.msats());
    let sync = module
        .api
        .account_sync(from.id())
        .await
        .map_err(unreachable)?;
    // The module would refuse to pay this out, and leave what remains stuck below it.
    let minimum = module.cfg.min_allowed_seek;
    if amount < minimum && amount != sync.staged_balance {
        return Err(invalid(format!(
            "the federation pays out of multisig accounts {minimum} or more, or all they hold"
        )));
    }
    let valid_until_cycle = sync.current_cycle.idx
        + PROPOSAL_LIFETIME
            .as_secs()
            .div_ceil(module.cfg.cycle_duration.as_secs().max(1));
    let to = module.own_account(OwnAccount::MultispendBtcPayout).id();
    BtcTransferRequest::new(nonce(), from, amount, to, Vec::new(), valid_until_cycle)
        .map_err(|err| invalid(format!("the payout cannot be proposed: {err}")))
}

/// A random nonce for a payout request, so that two requests for the same payout differ.
fn nonce() -> u64 {
    let id = UpstreamOperationId::new_random();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&id.0[..8]);
    u64::from_le_bytes(bytes)
}

/// `key`'s signature over `request`, as the module checks a member's approval.
pub(crate) fn sign_request(key: &Keypair, request: &BtcTransferRequest) -> schnorr::Signature {
    key.sign_schnorr(Message::from(&TransferRequestId::from(request)))
}

/// `proposal`'s request signed with `approvals`, each filed under the index of its member's key
/// in the module's account.
pub(crate) fn signed_request(
    proposal: &Proposal,
    approvals: &[Approval],
) -> Result<SignedBtcTransferRequest> {
    let request = proposal.request();
    let mut signatures = BTreeMap::new();
    for approval in approvals {
        let key = approval.member().upstream();
        let index = request
            .from()
            .pub_keys()
            .position(|member| *member == key)
            .ok_or_else(|| invalid("an approval is not from one of the account's members"))?;
        signatures.insert(index as u64, approval.signature());
    }
    SignedBtcTransferRequest::new(request.clone(), signatures)
        .map_err(|err| invalid(format!("the approvals do not sign the payout: {err}")))
}

/// Starts paying out `proposal` with `approvals`, which the facade has checked are for it, from
/// distinct members, and enough of them.
pub(crate) async fn execute(
    client: &Client,
    proposal: &Proposal,
    approvals: &[Approval],
) -> Result<UpstreamOperationId> {
    let module = module(client)?;
    let signed_request = signed_request(proposal, approvals)?;
    require_version(
        &module,
        BTC_TRANSFER_CONSENSUS_VERSION,
        "payouts out of multisig accounts",
    )
    .await?;
    let output = StabilityPoolOutput::V2(StabilityPoolOutputV2::BtcTransfer(BtcTransferOutput {
        signed_request,
    }));
    let transfer = module
        .submit_transfer(output, OwnAccount::MultispendBtcPayout, ())
        .await;
    match transfer {
        Ok(id) => Ok(id),
        Err(err) => Err(submit_error(client, err.as_ref(), None, "the payout").await),
    }
}

/// Starts moving everything paid out to this member into the ecash balance, or `None` when
/// nothing is waiting. `lock` is the federation's [`SweepLock`].
pub(crate) async fn sweep(
    client: &Client,
    lock: &SweepLock,
) -> Result<Option<UpstreamOperationId>> {
    // Held until this sweep has started or found nothing to start. A call dropped on the way
    // leaves the previous sweep recorded, for the next call to wait for.
    let mut pending = lock.lock().await;
    if let Some(previous) = *pending {
        match fedimint_core::runtime::timeout(PREVIOUS_SWEEP_WAIT, sweep_finished(client, previous))
            .await
        {
            Ok(Ok(())) => *pending = None,
            Ok(Err(err)) => return Err(err),
            Err(_) => {
                return Err(Error::new(
                    ErrorCode::Timeout,
                    "the previous sweep on this federation has not finished yet",
                ));
            }
        }
    }
    let module = module(client)?;
    let payout = module.own_account(OwnAccount::MultispendBtcPayout);
    let sync = module
        .api
        .account_sync(payout.id())
        .await
        .map_err(unreachable)?;
    if sync.staged_balance + sync.idle_balance == fedimint_core::Amount::ZERO {
        return Ok(None);
    }
    // Nothing locks in a payout account, so unlocking everything frees what is staged when the
    // federation runs the unlock, and the withdrawal that follows claims it along with any idle
    // balance. How much that was, the sweep's `Done` state says. Idle balance alone is what an
    // unlock leaves when its withdrawal never completed.
    let started = if sync.staged_balance != fedimint_core::Amount::ZERO {
        module
            .withdraw(OwnAccount::MultispendBtcPayout, FiatOrAll::All, ())
            .await
    } else {
        module
            .withdraw_idle_balance(OwnAccount::MultispendBtcPayout, sync.idle_balance, ())
            .await
    };
    let (id, _) = match started {
        Ok(started) => started,
        Err(err) => return Err(submit_error(client, err.as_ref(), None, "the sweep").await),
    };
    *pending = Some(id);
    Ok(Some(id))
}

/// The states of the deposit `id`, current first, ending after the first final one.
pub(crate) async fn deposit_updates(
    client: &Client,
    id: UpstreamOperationId,
) -> Result<BoxStream<'static, Result<MultisigDepositState>>> {
    let updates = module(client)?
        .subscribe_deposit_operation(id)
        .await
        .map_err(|err| {
            internal(format!(
                "cannot follow the deposit: {}",
                chain(err.as_ref())
            ))
        })?;
    // `Box::pin` rather than `boxed`: a stream is not `Send` on wasm.
    Ok(Box::pin(
        updates.into_stream().map(|state| Ok(deposit_state(state))),
    ))
}

/// The states of the payout `id`, current first, ending after the first final one.
pub(crate) async fn transfer_updates(
    client: &Client,
    id: UpstreamOperationId,
) -> Result<BoxStream<'static, Result<MultisigTransferState>>> {
    let updates = module(client)?
        .subscribe_transfer_operation(id)
        .await
        .map_err(|err| internal(format!("cannot follow the payout: {}", chain(err.as_ref()))))?;
    Ok(Box::pin(
        updates.into_stream().map(|state| Ok(transfer_state(state))),
    ))
}

/// The states of the sweep `id`, current first, ending after the first final one.
pub(crate) async fn sweep_updates(
    client: &Client,
    id: UpstreamOperationId,
) -> Result<BoxStream<'static, Result<MultisigSweepState>>> {
    let updates = withdrawal_updates(client, id).await?;
    Ok(Box::pin(updates.map(|state| Ok(sweep_state(state)))))
}

/// The stability pool module of `client`.
fn module(client: &Client) -> Result<ClientModuleInstance<'_, StabilityPoolClientModule>> {
    client
        .get_first_module::<StabilityPoolClientModule>()
        .map_err(|err| internal(format!("the stability pool module is not running: {err}")))
}

/// Refuses what the federation's stability pool cannot do yet, `what` saying what that is.
async fn require_version(
    module: &ClientModuleInstance<'_, StabilityPoolClientModule>,
    version: ModuleConsensusVersion,
    what: &str,
) -> Result<()> {
    let active = module
        .api
        .module_consensus_version()
        .await
        .map_err(unreachable)?;
    if active < version {
        return Err(Error::new(
            ErrorCode::NotSupported,
            format!(
                "the federation's stability pool takes {what} from version {version}, and runs \
                 {active}"
            ),
        ));
    }
    Ok(())
}

/// The states of the sweep `id`: a withdrawal that unlocks first, or one of idle balance only.
async fn withdrawal_updates(
    client: &Client,
    id: UpstreamOperationId,
) -> Result<BoxStream<'static, StabilityPoolWithdrawalOperationState>> {
    let module = module(client)?;
    let updates = match module.subscribe_withdraw(id).await {
        Ok(updates) => updates,
        Err(_) => module
            .subscribe_withdraw_idle_balance(id)
            .await
            .map_err(|err| internal(format!("cannot follow the sweep: {}", chain(err.as_ref()))))?,
    };
    Ok(updates.into_stream())
}

/// Waits for the sweep `id` to end, however it ends, or the refusal that says why that cannot be
/// known.
async fn sweep_finished(client: &Client, id: UpstreamOperationId) -> Result<()> {
    let mut updates = withdrawal_updates(client, id).await?;
    while let Some(state) = updates.next().await {
        if matches!(
            sweep_state(state),
            MultisigSweepState::Done { .. } | MultisigSweepState::Failed { .. }
        ) {
            return Ok(());
        }
    }
    Err(internal(
        "the previous sweep on this federation stopped reporting before it ended",
    ))
}

fn deposit_state(state: StabilityPoolDepositOperationState) -> MultisigDepositState {
    match state {
        StabilityPoolDepositOperationState::Initiated => MultisigDepositState::Created,
        StabilityPoolDepositOperationState::TxAccepted => MultisigDepositState::Accepted,
        StabilityPoolDepositOperationState::Success => MultisigDepositState::Done,
        StabilityPoolDepositOperationState::TxRejected(reason)
        | StabilityPoolDepositOperationState::PrimaryOutputError(reason) => {
            MultisigDepositState::Failed { reason }
        }
    }
}

fn transfer_state(state: StabilityPoolTransferOperationState) -> MultisigTransferState {
    match state {
        StabilityPoolTransferOperationState::Initiated => MultisigTransferState::Created,
        StabilityPoolTransferOperationState::Success => MultisigTransferState::Done,
        StabilityPoolTransferOperationState::TxRejected(reason) => {
            MultisigTransferState::Failed { reason }
        }
    }
}

fn sweep_state(state: StabilityPoolWithdrawalOperationState) -> MultisigSweepState {
    match state {
        StabilityPoolWithdrawalOperationState::Initiated
        | StabilityPoolWithdrawalOperationState::UnlockTxAccepted
        | StabilityPoolWithdrawalOperationState::WithdrawalInitiated(_) => {
            MultisigSweepState::Created
        }
        StabilityPoolWithdrawalOperationState::WithdrawalTxAccepted(_) => {
            MultisigSweepState::Accepted
        }
        StabilityPoolWithdrawalOperationState::Success(amount) => MultisigSweepState::Done {
            amount: Amount::from_msats(amount.msats),
        },
        StabilityPoolWithdrawalOperationState::UnlockTxRejected(reason)
        | StabilityPoolWithdrawalOperationState::UnlockProcessingError(reason)
        | StabilityPoolWithdrawalOperationState::WithdrawalTxRejected(reason)
        | StabilityPoolWithdrawalOperationState::PrimaryOutputError(reason) => {
            MultisigSweepState::Failed { reason }
        }
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidInput, message)
}

fn internal(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::Internal, message)
}

fn unreachable(err: impl core::fmt::Display) -> Error {
    Error::new(
        ErrorCode::FederationUnreachable,
        format!("the federation did not answer: {err}"),
    )
}

/// `err` and every error under it, outermost first: what the module's errors are made of.
fn causes<'a>(
    err: &'a (dyn std::error::Error + 'static),
) -> impl Iterator<Item = &'a (dyn std::error::Error + 'static)> {
    std::iter::successors(Some(err), |err| err.source())
}

/// `err` with every error under it, as one message.
fn chain(err: &(dyn std::error::Error + 'static)) -> String {
    causes(err)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}

/// What a transaction the module could not submit is reported as, `what` naming it.
///
/// A transaction that needs more ecash than the balance holds is refused for that. When it was to
/// spend `required`, the balance is read again for the figures.
async fn submit_error(
    client: &Client,
    err: &(dyn std::error::Error + Send + Sync + 'static),
    required: Option<fedimint_core::Amount>,
    what: &str,
) -> Error {
    if let Some(err) = causes(err).find_map(|err| err.downcast_ref::<FederationError>()) {
        return unreachable(err);
    }
    let short = causes(err).any(|err| {
        err.downcast_ref::<TransactionSubmitError>()
            .is_some_and(TransactionSubmitError::is_insufficient_funds)
    });
    if !short {
        return internal(format!("{what} could not be submitted: {}", chain(err)));
    }
    let message = format!("the ecash balance cannot fund {what}");
    let Some(required) = required else {
        return Error::new(ErrorCode::InsufficientBalance, message);
    };
    match client.get_balance_for_btc().await {
        Ok(available) => Error::with_details(
            ErrorCode::InsufficientBalance,
            message,
            ErrorDetails::InsufficientBalance {
                required: Amount::from_msats(required.msats),
                available: Amount::from_msats(available.msats),
            },
        ),
        Err(_) => Error::new(ErrorCode::InsufficientBalance, message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_module_kind_is_the_stability_pools() {
        assert_eq!(MODULE_KIND, stability_pool_common::KIND.as_str());
    }
}
