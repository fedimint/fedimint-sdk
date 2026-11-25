//! Accounts held by a group, paid out of with the approval of enough of its members.

use std::sync::Arc;
use std::time::Duration;

use fedimint_api_client::api::FederationError;
use fedimint_client::Client;
use fedimint_client::module::{ClientModuleInstance, TransactionSubmitError};
use fedimint_core::bitcoin::hashes::Hash;
use fedimint_core::core::OperationId as UpstreamOperationId;
use fedimint_core::module::ModuleConsensusVersion;
use fedimint_core::secp256k1::{Keypair, Message};
use stability_pool_client::api::StabilityPoolApiExt as _;
use stability_pool_client::{OwnAccount, StabilityPoolClientModule};
use stability_pool_common::{
    BTC_BALANCE_DEPOSIT_CONSENSUS_VERSION, BTC_TRANSFER_CONSENSUS_VERSION,
    BtcBalanceDepositMetadata, BtcTransferOutput, BtcTransferRequest, FiatOrAll,
    StabilityPoolOutput, StabilityPoolOutputV2,
};

use crate::federation::FederationInner;
use crate::operation::{DetailedOperationState, OperationDetails, OperationState, kinds, sealed};
use crate::{Amount, Error, ErrorCode, ErrorDetails, Operation, Result, Timestamp};

mod account;
mod activity;
mod driver;
mod proposal;

pub use account::{MemberKey, MultisigAccount, MultisigAccountId, Policy};
pub(crate) use driver::{MultisigDepositDriver, MultisigSweepDriver, MultisigTransferDriver};
pub use proposal::{Approval, Proposal};

/// The module kind multisig runs on: `stability_pool_common::KIND`, which is not a `const`
/// string.
pub(crate) const MODULE_KIND: &str = "multi_sig_stability_pool";

/// How long a proposal stays valid: the federation refuses to pay one out once this has passed
/// since it was made.
const PROPOSAL_LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);

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

/// The multisig facade for one federation.
///
/// Obtained from [`Federation::multisig`](crate::Federation::multisig), which returns `None`
/// unless the federation runs Fedi's stability pool module (v2), which holds the accounts. An
/// account holds bitcoin.
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
/// 4. A member who wants money out makes a [`Proposal`] with [`propose`](Multisig::propose),
///    signed as theirs. Each member who agrees [`approve`](Multisig::approve)s it, and once enough
///    have, anyone [`execute`](Multisig::execute)s it with their approvals. The proposer's own
///    approval counts toward the threshold like any other member's, and is not implied by the
///    proposal: a proposer who agrees approves too.
/// 5. An executed proposal pays the member who proposed it. That member's
///    [`sweep`](Multisig::sweep) moves what it was paid into its ecash balance.
///
/// The application moves keys, proposals and approvals between the members, over whatever channel
/// it has, and keeps the group's policy. Neither this crate nor the federation keeps a list of a
/// member's groups. The channel is never trusted with money: the federation pays out only on the
/// signatures of enough of the account's members.
///
/// A deposit, a payout or a sweep is recorded when the call that starts it returns. If the
/// application stops before that, the federation can still carry it out, and the activity then
/// lists it as an operation of [`Unknown`](crate::OperationKind::Unknown) kind. Check the
/// account's [`balance`](Multisig::balance) before starting it again.
///
/// What the federation's stability pool adds:
///
/// - The federation has a minimum amount for deposits and payouts. A deposit below it, and a
///   proposal to pay out less than it without emptying the account, are refused with
///   [`InvalidInput`](crate::ErrorCode::InvalidInput).
/// - A proposal stays valid for a week. Executing it later ends the payout
///   [`Failed`](MultisigTransferState::Failed).
/// - Deposits need the federation's stability pool at version 2.1 or later, and payouts at 2.2
///   or later. Until then they are refused with [`NotSupported`](crate::ErrorCode::NotSupported).
/// - A payout goes to the proposer's own payout account in the stability pool, which
///   [`sweep`](Multisig::sweep) empties into the ecash balance.
///
/// Like every other facade's, a call that needs the federation fails with
/// [`FederationClosed`](crate::ErrorCode::FederationClosed) once it is closed. The one that does
/// not need it keeps answering: [`account`](Multisig::account), which is computed locally.
#[derive(Clone)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct Multisig {
    inner: Arc<MultisigInner>,
}

impl core::fmt::Debug for Multisig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Multisig")
            .field("federation", &self.inner.federation.id)
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
    /// This member's key for `group`.
    ///
    /// `group` is any name the application gives the group, the same on every member's device.
    /// The same group always gives the same key, from this seed, and different groups give
    /// unrelated keys, so that two groups cannot be linked through a shared key. Restoring the
    /// seed restores every key.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for an empty `group`, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn member_key(&self, group: &str) -> Result<MemberKey> {
        check_group(group)?;
        let client = self.inner.federation.client(false).await?;
        let key = module(&client)?.derive_multispend_group_key(group.to_owned());
        Ok(MemberKey::from_upstream(key.public_key()))
    }

    /// What `account` holds.
    ///
    /// Read from the federation, so it can be shown to any member, and to anyone else who knows
    /// the policy. An account nothing was ever deposited into holds zero.
    ///
    /// # Errors
    ///
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn balance(&self, account: &MultisigAccount) -> Result<Amount> {
        let client = self.inner.federation.client(false).await?;
        let sync = module(&client)?
            .api
            .account_sync(account.module_account()?.id())
            .await
            .map_err(unreachable)?;
        let held = sync.staged_balance + sync.locked_balance + sync.idle_balance;
        Ok(Amount::from_msats(held.msats))
    }

    /// Approves `proposal`, as this member of `group`.
    ///
    /// Show the member the proposal's [`proposer`](Proposal::proposer), who the payment goes to,
    /// its [`amount`](Proposal::amount) and its account before calling this: an approval is a
    /// signature that lets the payment through once enough members have given theirs. Another
    /// member can hand out a proposal of their own in place of the one expected, and the proposer
    /// is what tells them apart. The proposal is checked first, so a proposal whose contents do
    /// not match what it says it pays is refused rather than signed.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for an empty `group`, for a proposal made
    /// on another federation, and when this member's key for `group` is not one of the account's
    /// members (a different group name, most often), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn approve(&self, group: &str, proposal: &Proposal) -> Result<Approval> {
        check_group(group)?;
        self.check_origin(proposal)?;
        let client = self.inner.federation.client(false).await?;
        let key = self.member_keypair_of(&client, group, &proposal.account())?;
        let signature = proposal::sign_request(&key, proposal.request());
        Ok(Approval::new(
            proposal.digest(),
            MemberKey::from_upstream(key.public_key()),
            signature,
        ))
    }

    /// Proposes paying `amount` out of `account` to this member of `group`.
    ///
    /// Nothing moves yet, and nothing is sent anywhere: hand the returned proposal to the other
    /// members to [`approve`](Multisig::approve), then [`execute`](Multisig::execute) it with
    /// their approvals. The proposal is signed with this member's key for `group`, and names this
    /// member as its [`proposer`](Proposal::proposer). The payment goes to the proposer, whoever
    /// executes it, and [`sweep`](Multisig::sweep) then moves it into the proposer's ecash
    /// balance.
    ///
    /// A proposal does not stay valid for ever: the federation refuses one executed too long after
    /// it was made, and the payout then ends [`Failed`](MultisigTransferState::Failed).
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for an empty `group`, for a zero amount,
    /// for one below the federation's minimum that does not empty the account, and when this
    /// member's key for `group` is not one of the account's members (a different group name, most
    /// often), [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn propose(
        &self,
        group: &str,
        account: &MultisigAccount,
        amount: Amount,
    ) -> Result<Proposal> {
        check_group(group)?;
        if amount.msats() == 0 {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "cannot pay out nothing",
            ));
        }
        let client = self.inner.federation.client(false).await?;
        let key = self.member_keypair_of(&client, group, account)?;
        let module = module(&client)?;
        let from = account.module_account()?;
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
        // The payout goes to the proposer's own payout account, which `sweep` empties.
        let to = module.own_account(OwnAccount::MultispendBtcPayout).id();
        let request =
            BtcTransferRequest::new(nonce(), from, amount, to, Vec::new(), valid_until_cycle)
                .map_err(|err| invalid(format!("the payout cannot be proposed: {err}")))?;
        let federation = self.inner.federation.id;
        let proposer = MemberKey::from_upstream(key.public_key());
        let digest = Proposal::digest_of(federation, proposer, &request);
        let signature = key.sign_schnorr(Message::from_digest(digest.to_byte_array()));
        // Checked here as the parse checks it, so a request the module builds in a shape the
        // parse refuses fails the member who proposes rather than every member asked to approve.
        Proposal::signed(federation, proposer, request, signature).map_err(|err| {
            Error::new(
                ErrorCode::Internal,
                format!("the proposal does not check out: {}", err.message),
            )
        })
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
    /// member listed twice, or with a threshold of zero or above the number of members.
    pub fn account(&self, policy: &Policy) -> Result<MultisigAccount> {
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
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a zero amount or one below the
    /// federation's minimum, [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance),
    /// [`NotSupported`](crate::ErrorCode::NotSupported) when the federation cannot take deposits
    /// yet,
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
        let federation = &self.inner.federation;
        federation.ensure_open()?;
        let client = federation.client(true).await?;
        let module = module(&client)?;
        let to = account.module_account()?;
        let msats = fedimint_core::Amount::from_msats(amount.msats());
        let minimum = module.cfg.min_allowed_seek;
        if msats < minimum {
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
        let id = module
            .deposit_to_btc_balance(to.id(), msats, BtcBalanceDepositMetadata(Vec::new()), ())
            .await;
        let id = match id {
            Ok(id) => id,
            Err(err) => {
                return Err(submit_error(&client, err.as_ref(), Some(msats), "the deposit").await);
            }
        };
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
                MODULE_KIND,
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
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a proposal made on another federation,
    /// an approval of a different proposal, from someone who is not a member, or whose signature
    /// is not the member's over the proposal, and fewer approvals than the account's threshold,
    /// [`NotSupported`](crate::ErrorCode::NotSupported) when the federation cannot pay out of
    /// multisig accounts yet,
    /// [`Recovering`](crate::ErrorCode::Recovering) while a recovery for this federation is
    /// incomplete, [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Storage`](crate::ErrorCode::Storage) and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn execute(
        &self,
        proposal: &Proposal,
        approvals: &[Approval],
    ) -> Result<Operation<MultisigTransferState>> {
        self.check_origin(proposal)?;
        let approvals = proposal::approvals_for(proposal, approvals)?;
        // Checked before the federation is asked anything, so a bad approval is named as one.
        let signed_request = proposal::signed_request(proposal, &approvals)?;
        let federation = &self.inner.federation;
        federation.ensure_open()?;
        let client = federation.client(true).await?;
        let module = module(&client)?;
        require_version(
            &module,
            BTC_TRANSFER_CONSENSUS_VERSION,
            "payouts out of multisig accounts",
        )
        .await?;
        let output =
            StabilityPoolOutput::V2(StabilityPoolOutputV2::BtcTransfer(BtcTransferOutput {
                signed_request,
            }));
        let id = match module
            .submit_transfer(output, OwnAccount::MultispendBtcPayout, ())
            .await
        {
            Ok(id) => id,
            Err(err) => return Err(submit_error(&client, err.as_ref(), None, "the payout").await),
        };
        let details = MultisigTransferDetails {
            account_id: proposal.account_id(),
            amount: proposal.amount(),
            created_at: Timestamp::from_epoch_millis(crate::db::now_millis()),
        };
        federation
            .create_operation(
                id,
                kinds::MULTISIG_TRANSFER,
                MODULE_KIND,
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
    /// The sweep takes whatever is waiting when the federation processes it, a payout that lands
    /// in the meantime included, so how much it moved is known once it is
    /// [`Done`](MultisigSweepState::Done).
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
        // Held until this sweep has started or found nothing to start. A call dropped on the way
        // leaves the previous sweep recorded, for the next call to wait for.
        let mut pending = federation.multisig_sweep().lock().await;
        if let Some(previous) = *pending {
            match fedimint_core::runtime::timeout(
                PREVIOUS_SWEEP_WAIT,
                driver::sweep_finished(&client, previous),
            )
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
        let module = module(&client)?;
        let payout = module.own_account(OwnAccount::MultispendBtcPayout);
        let sync = module
            .api
            .account_sync(payout.id())
            .await
            .map_err(unreachable)?;
        if sync.staged_balance + sync.idle_balance == fedimint_core::Amount::ZERO {
            return Ok(None);
        }
        // Nothing locks in a payout account, so unlocking everything frees what is staged when
        // the federation runs the unlock, and the withdrawal that follows claims it along with any
        // idle balance. How much that was, the sweep's `Done` state says. Idle balance alone is
        // what an unlock leaves when its withdrawal never completed.
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
            Err(err) => return Err(submit_error(&client, err.as_ref(), None, "the sweep").await),
        };
        *pending = Some(id);
        drop(pending);
        let details = MultisigSweepDetails {
            amount: None,
            created_at: Timestamp::from_epoch_millis(crate::db::now_millis()),
        };
        federation
            .create_operation(
                id,
                kinds::MULTISIG_SWEEP,
                MODULE_KIND,
                &driver::SweepDetailsWire::from(&details),
                Arc::new(MultisigSweepDriver::default()),
            )
            .await
            .map(Some)
    }

    /// Builds the facade. Handed out by `Federation::multisig`.
    pub(crate) fn new(federation: Arc<FederationInner>) -> Multisig {
        Multisig {
            inner: Arc::new(MultisigInner { federation }),
        }
    }

    /// This member's key pair for `group`, whose public key has to be one of `account`'s members.
    fn member_keypair_of(
        &self,
        client: &Client,
        group: &str,
        account: &MultisigAccount,
    ) -> Result<Keypair> {
        // The same group always gives the same key, from the client's seed, and different groups
        // give unrelated keys.
        let key = module(client)?.derive_multispend_group_key(group.to_owned());
        if !account
            .members()
            .contains(&MemberKey::from_upstream(key.public_key()))
        {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "this member's key for that group is not one of the account's members",
            ));
        }
        Ok(key)
    }

    /// Refuses a proposal made on another federation.
    fn check_origin(&self, proposal: &Proposal) -> Result<()> {
        if proposal.federation().inner() != self.inner.federation.id {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "the proposal was made on another federation",
            ));
        }
        Ok(())
    }
}

/// Refuses an empty group name, which would give every unnamed group the same key.
fn check_group(group: &str) -> Result<()> {
    if group.is_empty() {
        return Err(Error::new(ErrorCode::InvalidInput, "a group needs a name"));
    }
    Ok(())
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

/// A random nonce for a payout request, so that two requests for the same payout differ.
fn nonce() -> u64 {
    let id = UpstreamOperationId::new_random();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&id.0[..8]);
    u64::from_le_bytes(bytes)
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

/// The federation a [`Multisig`] facade operates on.
///
/// The federation rather than a module handle, because a facade outlives the client behind it: a
/// call on a closed federation has to report `FederationClosed` rather than find nothing to talk
/// to.
struct MultisigInner {
    federation: Arc<FederationInner>,
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
    Done {
        /// How much the sweep moved into the ecash balance, before the fee for issuing it.
        amount: Amount,
    },
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
            MultisigSweepState::Done { .. } | MultisigSweepState::Failed { .. } => true,
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
    /// How much the proposal pays.
    pub amount: Amount,
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
/// Every field but [`amount`](MultisigSweepDetails::amount) is fixed when the sweep starts.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[non_exhaustive]
pub struct MultisigSweepDetails {
    /// How much the sweep moved into the ecash balance, before the fee for issuing it: the amount
    /// its [`Done`](MultisigSweepState::Done) state carries.
    ///
    /// `None` until then, and for a sweep that failed.
    pub amount: Option<Amount>,
    /// When the sweep started, by this device's clock.
    pub created_at: Timestamp,
}

impl sealed::Sealed for MultisigSweepDetails {}

impl OperationDetails for MultisigSweepDetails {}

impl DetailedOperationState for MultisigSweepState {
    type Details = MultisigSweepDetails;
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use fedimint_core::bitcoin::hashes::sha256;
    use fedimint_core::config;

    use super::*;
    use crate::db::{federation_namespace, in_memory_root};
    use crate::multisig::account::{test_keypairs, test_keys};
    use crate::multisig::proposal::{test_proposal, test_proposal_on};

    /// The facade over a federation with no client, open or closed.
    fn multisig(open: bool) -> Multisig {
        let federation =
            FederationInner::detached(federation_namespace(&in_memory_root(), [1u8; 32]), open);
        Multisig::new(federation)
    }

    fn account() -> MultisigAccount {
        let [a, b, c] = test_keys();
        MultisigAccount::derive(&Policy::new(vec![a, b, c], 2)).expect("a valid policy")
    }

    /// `pair`'s approval of `proposal`, signed as the module checks it.
    fn approval(pair: &fedimint_core::secp256k1::Keypair, proposal: &Proposal) -> Approval {
        Approval::new(
            proposal.digest(),
            MemberKey::from_upstream(pair.public_key()),
            proposal::sign_request(pair, proposal.request()),
        )
    }

    #[test]
    fn the_module_kind_is_the_stability_pools() {
        assert_eq!(MODULE_KIND, stability_pool_common::KIND.as_str());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn nothing_is_deposited_or_proposed_for_nothing() {
        let multisig = multisig(true);
        let account = account();

        let err = multisig
            .deposit(&account, Amount::from_msats(0))
            .await
            .expect_err("a zero deposit");
        assert_eq!(err.code, ErrorCode::InvalidInput);

        let err = multisig
            .propose("group", &account, Amount::from_msats(0))
            .await
            .expect_err("a zero payout");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_group_needs_a_name() {
        let multisig = multisig(true);
        let account = account();

        let err = multisig.member_key("").await.expect_err("no name");
        assert_eq!(err.code, ErrorCode::InvalidInput);
        let err = multisig
            .propose("", &account, Amount::from_msats(1_000))
            .await
            .expect_err("no name");
        assert_eq!(err.code, ErrorCode::InvalidInput);
        let err = multisig
            .approve("", &test_proposal(5_000))
            .await
            .expect_err("no name");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_proposal_short_of_approvals_or_with_a_forged_one_is_refused_up_front() {
        let multisig = multisig(true);
        let [a, b, c] = test_keypairs();
        let ours = test_proposal(5_000);

        let err = multisig
            .execute(&ours, &[approval(&a, &ours)])
            .await
            .expect_err("one approval of the two needed");
        assert_eq!(err.code, ErrorCode::InvalidInput);

        // b's approval, signed by c: a member and a proposal that match, and a signature that
        // does not. Refused before the closed-off federation is asked anything.
        let forged = Approval::new(
            ours.digest(),
            MemberKey::from_upstream(b.public_key()),
            proposal::sign_request(&c, ours.request()),
        );
        let err = multisig
            .execute(&ours, &[approval(&a, &ours), forged])
            .await
            .expect_err("b did not sign it");
        assert_eq!(err.code, ErrorCode::InvalidInput);
        assert!(err.message.contains("do not sign"), "{}", err.message);

        // b's signature over another payout.
        let other = test_proposal(6_000);
        let elsewhere = Approval::new(
            ours.digest(),
            MemberKey::from_upstream(b.public_key()),
            proposal::sign_request(&b, other.request()),
        );
        let err = multisig
            .execute(&ours, &[approval(&a, &ours), elsewhere])
            .await
            .expect_err("b signed another payout");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_proposal_made_on_another_federation_is_refused_up_front() {
        let multisig = multisig(true);
        let [a, b, _] = test_keypairs();
        let elsewhere = test_proposal_on(
            config::FederationId(sha256::Hash::hash(b"another federation")),
            5_000,
        );

        let err = multisig
            .approve("group", &elsewhere)
            .await
            .expect_err("made on another federation");
        assert_eq!(err.code, ErrorCode::InvalidInput);
        let err = multisig
            .execute(
                &elsewhere,
                &[approval(&a, &elsewhere), approval(&b, &elsewhere)],
            )
            .await
            .expect_err("made on another federation");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_recovering_federation_moves_no_money() {
        let multisig = multisig(true);
        multisig
            .inner
            .federation
            .set_status(crate::FederationStatus::Recovering);
        let [a, b, _] = test_keypairs();
        let ours = test_proposal(5_000);

        let err = multisig
            .execute(&ours, &[approval(&a, &ours), approval(&b, &ours)])
            .await
            .expect_err("recovering");
        assert_eq!(err.code, ErrorCode::Recovering);
        let err = multisig
            .deposit(&account(), Amount::from_msats(1_000))
            .await
            .expect_err("recovering");
        assert_eq!(err.code, ErrorCode::Recovering);
        let err = multisig.sweep().await.expect_err("recovering");
        assert_eq!(err.code, ErrorCode::Recovering);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_federation_that_is_not_running_answers_closed() {
        let multisig = multisig(false);
        let [a, b, c] = test_keys();
        let account = multisig
            .account(&Policy::new(vec![a, b, c], 2))
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
