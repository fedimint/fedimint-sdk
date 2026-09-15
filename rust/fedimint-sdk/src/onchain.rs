//! On-chain Bitcoin: deposits into the federation and withdrawals out of
//! it.

use std::collections::{HashMap, hash_map};
use std::sync::Arc;

use fedimint_client::Client;
use fedimint_client_module::ClientModuleInstance;
use fedimint_client_module::transaction::{
    FeeQuote, TRANSACTION_SUBMISSION_MODULE_INSTANCE, TxSubmissionStates, TxSubmissionStatesSM,
};
use fedimint_core::bitcoin;
use fedimint_core::config;
use fedimint_core::core::{DynInput, DynOutput, ModuleInstanceId, OperationId};
use fedimint_core::transaction::Transaction;

use crate::{
    Address, Amount, Error, ErrorCode, ErrorDetails, FederationStatus, Network, Operation,
    OperationState, Result, Sats, Timestamp, Txid,
};

mod driver;
mod v1;
mod v2;
mod wire;

pub(crate) use driver::{OnchainBackfiller, OnchainReceiveDriver, OnchainSendDriver};
/// The deposit address a walletv2 `Receive` upstream meta names, for `federation.rs`'s
/// reconciler; re-exported so it does not have to name the upstream wallet types itself.
pub(crate) use v2::deposit_address_of;
/// The only phase an on-chain record ever carries, re-exported for `federation.rs`'s erase
/// guard; see [`wire`]'s own doc for what it means.
pub(crate) use wire::PHASE_SEEN;

/// The on-chain facade for one federation, backed by its wallet module.
///
/// Obtained from [`Federation::onchain`](crate::Federation::onchain), which
/// returns `None` when the federation has no wallet module.
///
/// # Units: [`Sats`] for what moves on chain, [`Amount`] for what it costs
///
/// A value here is [`Sats`](crate::Sats) when it is a figure that exists on
/// the Bitcoin chain, and [`Amount`](crate::Amount) when it is a figure that
/// exists inside the federation.
///
/// - **Whole satoshis.** The amount that arrives at a withdrawal's
///   destination ([`OnchainQuote::amount`], [`Onchain::quote`]'s `amount`
///   argument, [`OnchainSendDetails::amount`]) and the gross amount a
///   deposit transaction pays in ([`OnchainReceiveState::Claimed`],
///   [`OnchainReceiveDetails::gross_deposited`]). Bitcoin has no sub-satoshi
///   unit, so these genuinely are whole satoshis.
/// - **Exact millisatoshis.** Every fee, every total debit, and the net
///   amount a deposit credits to the balance ([`OnchainQuote::fee`],
///   [`OnchainQuote::total`], [`OnchainSendDetails::total`],
///   [`OnchainReceiveState::Claimed`],
///   [`OnchainReceiveDetails::net_credit`]). A withdrawal's cost is more
///   than the chain fee for the destination output, and a deposit's cost is
///   a federation fee taken out of what arrives, so these sums are
///   routinely not whole satoshis.
///
/// No conversion happens behind a caller's back. Moving between the two
/// units is always explicit: [`Sats::to_amount`](crate::Sats::to_amount)
/// upward, which is exact (one satoshi is exactly 1000 msat), and
/// [`Amount::to_sats_exact`](crate::Amount::to_sats_exact) downward, which
/// refuses rather than truncates.
///
/// # The recovery lock applies to both directions
///
/// Every call on this facade, deposits as much as withdrawals, is refused
/// with [`Recovering`](crate::ErrorCode::Recovering) while this federation's
/// recovery is incomplete. An attempt that stopped short holds the lock
/// exactly as firmly as one still in progress, and only a recovery that
/// reaches completion releases it. There is no acknowledge, no override, and
/// no way to spend or receive on a partially restored wallet.
#[derive(Debug, Clone)]
pub struct Onchain {
    inner: Arc<OnchainInner>,
}

impl Onchain {
    /// Hands back a deposit address to fund, and an operation that follows
    /// whatever arrives at it.
    ///
    /// Every call allocates a fresh deposit address, never handed out before
    /// and never handed out again, and commits one durable operation for it
    /// before returning. The operation begins in
    /// [`WaitingForTransaction`](OnchainReceiveState::WaitingForTransaction)
    /// and stays there for as long as nobody pays; when an output paying the
    /// address is detected, the same operation adopts it and starts
    /// reporting it, under the same [`OperationId`](crate::OperationId).
    /// The operation's existence does not mean a deposit is under way: only
    /// a state past
    /// [`WaitingForTransaction`](OnchainReceiveState::WaitingForTransaction)
    /// means that.
    ///
    /// Two calls yield two addresses and two operations, so a per-payer
    /// address can be minted on demand. The address is watched persistently,
    /// so a deposit that arrives while the application is closed is picked
    /// up when the SDK is next built over the same storage, and the address
    /// survives a restart because it is on the operation's details record.
    ///
    /// # One address, one payer, one deposit
    ///
    /// This handle follows one deposit: the first output detected paying the
    /// address. A second output paying the same address is not reported by
    /// this operation, and this facade does not promise that the second
    /// becomes an operation of its own, appears in
    /// [activity](crate::Federation::activity), or is credited on its own
    /// schedule.
    ///
    /// Do not hand a deposit address to two people, do not show it again
    /// once it has been funded, and treat anything that does arrive twice as
    /// something to reconcile from
    /// [`Federation::balance`](crate::Federation::balance) and
    /// [activity](crate::Federation::activity) rather than as something this
    /// API tracked on the application's behalf.
    ///
    /// # An unused address never finishes
    ///
    /// [`WaitingForTransaction`](OnchainReceiveState::WaitingForTransaction)
    /// has no timeout, because a Bitcoin address has no expiry. An operation
    /// nobody pays stays non-final indefinitely, and there is no cancel,
    /// retire, or expire call for one. Do not await
    /// [`Operation::await_final`](crate::Operation::await_final) on a fresh
    /// deposit expecting it to resolve.
    ///
    /// A receive operation that has not yet seen a transaction does not
    /// count as a pending operation for
    /// [`Sdk::forget_federation`](crate::Sdk::forget_federation)'s guard, so
    /// an address that was displayed once and never funded does not block
    /// erasing the federation. Once a transaction has been seen, from
    /// [`WaitingForConfirmation`](OnchainReceiveState::WaitingForConfirmation)
    /// onwards, the operation is an ordinary pending one and the erase
    /// refuses with [`PendingOperations`](crate::ErrorCode::PendingOperations)
    /// until it reaches [`Claimed`](OnchainReceiveState::Claimed) or
    /// [`Failed`](OnchainReceiveState::Failed).
    ///
    /// # No quote
    ///
    /// There is nothing to quote for a deposit. The sender pays the Bitcoin
    /// network fee out of their own wallet, and the federation's deposit
    /// terms apply to whatever arrives; the fee those terms take is knowable
    /// only once an amount exists, and it is reported then, see
    /// [`OnchainReceiveDetails::fee`].
    ///
    /// # Errors
    ///
    /// [`Recovering`](crate::ErrorCode::Recovering) while this federation's
    /// recovery is incomplete,
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage),
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed), and
    /// [`Internal`](crate::ErrorCode::Internal) if the federation's wallet
    /// handed back an address this SDK already watches rather than a fresh
    /// one. A wallet that offers one unused address at a time does that when
    /// a second call is made before the first address has been paid; the
    /// address is not handed out again, and the operation already watching
    /// it is unaffected.
    pub async fn receive(&self) -> Result<OnchainReceive> {
        let federation = &self.inner.federation;
        // The recovery lock applies to a deposit exactly as it does to a send; see this type's
        // own doc.
        let client = federation.client(true).await?;
        match module(&client)? {
            WalletModule::V1(module) => {
                v1::receive(federation, &client, &module, Arc::new(OnchainReceiveDriver)).await
            }
            WalletModule::V2(module) => {
                v2::receive(federation, &client, &module, Arc::new(OnchainReceiveDriver)).await
            }
        }
    }

    /// Plans a withdrawal and returns an executable quote for it.
    ///
    /// Like its lightning counterpart, this exists because the cost is only
    /// knowable after the SDK has worked out how the federation will build
    /// and broadcast the transaction. The returned [`OnchainQuote`] binds
    /// the destination address, the amount, the aggregate fee, the total
    /// debit, and the federation configuration those were computed against,
    /// and [`Onchain::send`] executes exactly that or refuses.
    ///
    /// `amount` is in whole [`Sats`](crate::Sats) because it is the amount
    /// that will appear in the withdrawal transaction's output. The fee and
    /// total that come back are [`Amount`](crate::Amount)s, because they are
    /// not whole satoshis; see the [unit note](Onchain) on this facade and
    /// [`OnchainQuote::fee`].
    ///
    /// This is also where the address's network is checked against the
    /// federation's. A well-formed address for the wrong chain is caught
    /// here, with
    /// [`NetworkMismatch`](crate::ErrorCode::NetworkMismatch), rather than
    /// after the funds have moved, since parsing an
    /// [`Address`](crate::Address) cannot do this check: at parse time there
    /// is no federation to compare against.
    ///
    /// # Errors
    ///
    /// [`NetworkMismatch`](crate::ErrorCode::NetworkMismatch),
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for an amount the
    /// federation cannot withdraw (zero, or below its dust threshold),
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance) when
    /// the balance cannot cover [`OnchainQuote::total`],
    /// [`Recovering`](crate::ErrorCode::Recovering) while this federation's
    /// recovery is incomplete,
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn quote(&self, address: &Address, amount: Sats) -> Result<OnchainQuote> {
        let federation = &self.inner.federation;
        // Runs before the client is touched, so a foreign-network address fails the same way
        // on both generations and on a recovering federation alike.
        check_network(address, federation.record().network.into())?;
        let client = federation.client(true).await?;
        let available = balance_of(&client, federation.status()).await?;
        let plan = match module(&client)? {
            WalletModule::V1(module) => {
                v1::plan(federation, &client, &module, address, amount, available).await?
            }
            WalletModule::V2(module) => {
                v2::plan(federation, &client, &module, address, amount, available).await?
            }
        };
        if available < plan.total {
            return Err(insufficient(plan.total, available));
        }
        let issued = crate::db::now_millis();
        Ok(OnchainQuote {
            inner: OnchainQuoteInner {
                federation_id: federation.id,
                address: address.clone(),
                amount,
                plan,
                expires_at: Timestamp::from_epoch_millis(
                    issued.saturating_add(QUOTE_VALIDITY_MILLIS),
                ),
            },
        })
    }

    /// Executes a quoted withdrawal.
    ///
    /// The quote is consumed and executed as quoted, same destination, same
    /// amount, same fee, or the call fails with
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired) if its validity
    /// window has passed, or
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) if the fee estimate
    /// or federation configuration it was built on has moved. In both cases
    /// the remedy is the same: quote again and re-confirm.
    ///
    /// [`OnchainQuote::total`] is exactly what this call debits. A
    /// withdrawal that would now cost anything else is a
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) refusal, never a
    /// silent overspend of the difference and never a quietly smaller debit
    /// either.
    ///
    /// The returned operation reaches [`OnchainSendState::Succeeded`] once
    /// the federation has broadcast the transaction. That is the SDK's
    /// finish line, not the chain's: confirmation of the withdrawal
    /// transaction on the Bitcoin network is the recipient's business, and
    /// the [`Txid`](crate::Txid) in that state is what an application shows
    /// or links to a block explorer. The terms it was executed on stay
    /// readable, however it ends, from [`OnchainSendDetails`].
    ///
    /// # Errors
    ///
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired),
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged),
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance),
    /// [`Recovering`](crate::ErrorCode::Recovering) while this federation's
    /// recovery is incomplete,
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn send(&self, quote: OnchainQuote) -> Result<Operation<OnchainSendState>> {
        let federation = &self.inner.federation;
        let quote = quote.inner;
        ensure_executable(&quote, federation.id, crate::db::now_millis())?;
        // The guard is held across the re-check, the funding and the record write, which is
        // what `create_operation` requires of its caller.
        let client = federation.client(true).await?;
        match (module(&client)?, &quote.plan.terms) {
            (
                WalletModule::V1(module),
                Terms::V1 {
                    fees,
                    quote: quoted,
                },
            ) => {
                v1::send(
                    federation,
                    &client,
                    &module,
                    &quote,
                    *fees,
                    quoted,
                    Arc::new(OnchainSendDriver),
                )
                .await
            }
            (
                WalletModule::V2(module),
                Terms::V2 {
                    chain_fee,
                    quote: quoted,
                },
            ) => {
                v2::send(
                    federation,
                    &client,
                    &module,
                    &quote,
                    *chain_fee,
                    quoted,
                    Arc::new(OnchainSendDriver),
                )
                .await
            }
            // The federation changed wallet generation between the quote and now, which the
            // generation rule makes a different federation for every practical purpose.
            _ => Err(Error::new(
                ErrorCode::QuoteChanged,
                "this federation's wallet module changed since the quote was issued",
            )),
        }
    }

    /// Builds the facade for one federation. Handed out by `Federation::onchain`.
    pub(crate) fn new(federation: Arc<crate::federation::FederationInner>) -> Onchain {
        Onchain {
            inner: Arc::new(OnchainInner { federation }),
        }
    }
}

/// A frozen, executable plan for one on-chain withdrawal.
///
/// Produced by [`Onchain::quote`] and consumed by [`Onchain::send`]. The
/// accessors expose exactly what a user must approve and nothing else.
///
/// The accessors do not all speak the same unit: the destination amount is
/// whole [`Sats`](crate::Sats), and the fee and total are millisatoshi
/// [`Amount`](crate::Amount)s. See the [unit note](Onchain) on this facade
/// and [`OnchainQuote::fee`] for why.
#[derive(Debug)]
pub struct OnchainQuote {
    inner: OnchainQuoteInner,
}

impl OnchainQuote {
    /// The amount that will arrive at the destination address.
    ///
    /// Whole [`Sats`](crate::Sats), because this is the figure that becomes
    /// an output in the withdrawal transaction, and a Bitcoin output cannot
    /// hold a fraction of a satoshi. It is the same number the caller passed
    /// to [`Onchain::quote`].
    ///
    /// This is *not* what leaves the balance; see [`OnchainQuote::total`].
    pub fn amount(&self) -> Sats {
        self.inner.amount
    }

    /// The exact aggregate cost of this withdrawal, over and above
    /// [`OnchainQuote::amount`].
    ///
    /// This is every debit the withdrawal incurs beyond the destination
    /// output, summed with nothing rounded away: the chain fee for the
    /// destination output, the cost of funding it from the balance, and the
    /// change and dust that funding leaves behind.
    /// [`OnchainQuote::fee_breakdown`] names those parts individually.
    ///
    /// It is an [`Amount`](crate::Amount) rather than [`Sats`](crate::Sats)
    /// because that sum is genuinely not a whole number of satoshis; see the
    /// [unit note](Onchain) on this facade.
    ///
    /// Display it as it stands, or round it up. Never round it down, and
    /// never re-express it in satoshis with
    /// [`sats_floor`](crate::Amount::sats_floor);
    /// [`to_sats_exact`](crate::Amount::to_sats_exact) will normally return
    /// `None` here.
    pub fn fee(&self) -> Amount {
        self.inner.plan.fee
    }

    /// The total that will be debited from the balance:
    /// [`OnchainQuote::amount`] converted to millisatoshis, plus
    /// [`OnchainQuote::fee`].
    ///
    /// This is the number to show as "you will pay", and it is exact.
    ///
    /// It is also the debit execution is authorised to make, exactly, not a
    /// ceiling or a prediction: [`Onchain::send`] debits this or does not
    /// run. A withdrawal that would cost anything else by the time it
    /// executes is refused with
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged), so the user
    /// re-approves a new number instead of quietly paying a different one.
    /// This is the figure [`OnchainSendDetails::total`] records.
    pub fn total(&self) -> Amount {
        self.inner.plan.total
    }

    /// [`OnchainQuote::fee`], split into the named parts it is made of.
    ///
    /// This exists so that "why is the fee 1,234,567 msat and not a round
    /// number of sats" has an answer an application can put on screen,
    /// behind a disclosure, next to the aggregate. It re-reports the same
    /// money as [`OnchainQuote::fee`]; it is not an additional charge.
    ///
    /// The aggregate remains the figure to charge and to compare against a
    /// balance; see [`OnchainSendFeeBreakdown`] for why a caller should not
    /// re-derive it by summing.
    pub fn fee_breakdown(&self) -> OnchainSendFeeBreakdown {
        self.inner.plan.breakdown.clone()
    }

    /// When this quote stops being executable.
    ///
    /// Past this point [`Onchain::send`] fails with
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired). On-chain quotes
    /// tend to be shorter-lived than lightning ones, because the fee
    /// estimate they carry tracks a moving mempool.
    pub fn expires_at(&self) -> Timestamp {
        self.inner.expires_at
    }
}

/// What [`OnchainQuote::fee`] is made of, component by component.
///
/// Obtained from [`OnchainQuote::fee_breakdown`]. Every field is an exact
/// millisatoshi [`Amount`](crate::Amount), for the reason
/// [`OnchainQuote::fee`] gives. The components sum to [`OnchainQuote::fee`]
/// exactly, with no rounding and no residue.
///
/// # Read the aggregate; use these to explain it
///
/// A caller that needs the number to charge, to compare against a balance,
/// or to put in a receipt should read [`OnchainQuote::fee`] (or
/// [`OnchainQuote::total`]) and not sum these fields: the type is
/// `#[non_exhaustive]`, so a later version may split a component in two or
/// add one, and only the aggregate stays correct across that change. It is
/// also the figure the quote commits to and [`Onchain::send`] is authorised
/// against.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OnchainSendFeeBreakdown {
    /// What it costs to put the destination output on chain: the
    /// federation's charge for building it, including its share of the
    /// Bitcoin network fee at the feerate the quote was computed against.
    ///
    /// This is the component a user intuitively expects a withdrawal to
    /// cost, and on its own it is not the whole cost.
    pub wallet_output: Amount,
    /// What it costs to fund that output from the balance: selecting and
    /// spending the ecash that pays for the withdrawal.
    ///
    /// This is a federation-internal, millisatoshi-denominated cost with no
    /// on-chain counterpart, and it is the component most likely to make
    /// [`OnchainQuote::fee`] a non-whole number of satoshis.
    pub funding: Amount,
    /// What the change from that funding costs: reissuing the remainder as
    /// notes, plus any residue too small to be worth returning and
    /// therefore given up.
    ///
    /// Small, frequently sub-satoshi, and part of the debit.
    pub change: Amount,
}

/// The result of [`Onchain::receive`]: the address to fund, and the
/// operation tracking the deposit.
///
/// The address is here for convenience, not for safekeeping. It is also
/// persisted on the operation's details record, so an application that has
/// lost this struct, after a process restart or a screen rebuilt from an
/// operation id, reads it back with
/// [`Operation::details`](crate::Operation::details) and gets the same
/// address to display or re-encode as a QR code.
///
/// The address is fresh for this operation; see [`Onchain::receive`] for
/// what that promises, and why one address should go to one payer.
#[derive(Debug)]
#[non_exhaustive]
pub struct OnchainReceive {
    /// The deposit address to display, encode as a QR code, or hand to a
    /// sender.
    pub address: Address,
    /// Tracks the deposit from the first sight of a transaction through to
    /// the balance credit.
    ///
    /// Starts in
    /// [`WaitingForTransaction`](OnchainReceiveState::WaitingForTransaction)
    /// and stays there until an output paying the address is detected, which
    /// may be never.
    pub operation: Operation<OnchainReceiveState>,
}

/// The lifecycle of an on-chain withdrawal.
///
/// The four variants are the application-level lifecycle: accepted,
/// broadcast, did not happen with the funds safe, or did not resolve. The
/// last two are kept apart for the same reason
/// [`LnSendState`](crate::LnSendState) keeps `Refunded` and `Failed` apart:
/// whether the money is known to be safe is exactly what an application has
/// to tell the user.
///
/// The terms the withdrawal was executed on (destination, amount, fee,
/// total) are not here. They belong to what the operation is rather than to
/// where it has got to, they are the same in every state, and a receipt has
/// to be renderable for a withdrawal that failed as much as for one that
/// succeeded. They live on [`OnchainSendDetails`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OnchainSendState {
    /// The withdrawal has been accepted and the federation is assembling
    /// and signing the transaction.
    Created,
    /// Final: the federation broadcast the transaction.
    ///
    /// The funds have left the federation. Confirmation on the Bitcoin
    /// network happens afterwards and is not tracked here.
    Succeeded {
        /// The transaction id, for receipts and block explorers.
        txid: Txid,
    },
    /// Final: the withdrawal did not happen and the funds are in the
    /// spendable balance.
    ///
    /// The federation rejected the transaction that would have funded the
    /// withdrawal, so nothing was debited. Like
    /// [`LnSendState::Refunded`](crate::LnSendState::Refunded) this is a
    /// success from the SDK's point of view: the money is safe, and the user
    /// quotes again.
    Refunded {
        /// Human-readable explanation. Diagnostic only, not a stable
        /// contract, and not something to match on.
        reason: String,
    },
    /// Final: the withdrawal failed in a way that did not resolve into a
    /// clean return.
    ///
    /// The funding was accepted and no transaction came of it, so this
    /// state cannot say where the funds are. Render it as an error the user
    /// should report, and read the balance for the rest; it is not the
    /// ordinary "rejected, try again" ending, which is
    /// [`Refunded`](Self::Refunded).
    Failed {
        /// Human-readable explanation. Diagnostic only, not a stable
        /// contract, and not something to match on.
        reason: String,
    },
}

impl crate::operation::sealed::Sealed for OnchainSendState {}

impl OperationState for OnchainSendState {
    fn is_final(&self) -> bool {
        match self {
            OnchainSendState::Created => false,
            OnchainSendState::Succeeded { .. }
            | OnchainSendState::Refunded { .. }
            | OnchainSendState::Failed { .. } => true,
        }
    }
}

/// What an on-chain withdrawal *is*: the destination and the terms it was
/// executed on.
///
/// Read with [`Operation::details`](crate::Operation::details) on an
/// `Operation<OnchainSendState>`. Every field here is fixed by the executed
/// [`OnchainQuote`] and committed in the same storage transaction that
/// creates the operation, so it is readable from the first moment
/// [`Onchain::send`] returns, survives a restart, and reads the same however
/// the withdrawal ends. That last part matters: a withdrawal that failed has
/// a destination and a quoted fee just as a successful one does, and a
/// receipt that can only be produced for successes is not a receipt.
///
/// [`amount`](OnchainSendDetails::amount) is whole [`Sats`](crate::Sats),
/// since it is an output in a Bitcoin transaction, while
/// [`fee`](OnchainSendDetails::fee) and
/// [`total`](OnchainSendDetails::total) are millisatoshi
/// [`Amount`](crate::Amount)s; see the [unit note](Onchain) and
/// [`OnchainQuote::fee`].
///
/// There is no `txid` field here: the broadcast transaction id appears on
/// [`Succeeded`](OnchainSendState::Succeeded), which is final and therefore
/// stays readable from [`Operation::state`](crate::Operation::state) for the
/// rest of the operation's life.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OnchainSendDetails {
    /// The destination the withdrawal pays.
    ///
    /// The address the quote was built against and bound to, network-checked
    /// at quote time. This is what a receipt shows and what a "sent to"
    /// line reads from after a restart.
    pub address: Address,
    /// The amount bound for [`address`](OnchainSendDetails::address), in
    /// whole satoshis.
    ///
    /// The counterparty figure of the executed quote: what the recipient
    /// receives when the withdrawal is broadcast, gross of this wallet's
    /// fees. Not the debit, that is [`total`](OnchainSendDetails::total).
    pub amount: Sats,
    /// The aggregate fee as quoted, exactly.
    ///
    /// The same figure [`OnchainQuote::fee`] reported and the same one
    /// [`Onchain::send`] was authorised against. Recorded because it cannot
    /// be re-derived afterwards: the mempool it was estimated against has
    /// moved on.
    pub fee: Amount,
    /// The total the withdrawal was authorised for: `amount` converted to
    /// millisatoshis plus [`fee`](OnchainSendDetails::fee), which is
    /// [`OnchainQuote::total`].
    ///
    /// A term, not an outcome: it is what a
    /// [`Succeeded`](OnchainSendState::Succeeded) withdrawal debited, and
    /// what a [`Refunded`](OnchainSendState::Refunded) one never debited at
    /// all. The state says which; this record says how much was at stake.
    pub total: Amount,
    /// When the withdrawal was started, by this device's clock.
    ///
    /// A local reading, like [`ActivityItem::time`](crate::ActivityItem::time):
    /// the federation does not attest to it. Fine for ordering and display,
    /// not evidence of when anything happened.
    pub created_at: Timestamp,
}

impl crate::operation::sealed::Sealed for OnchainSendDetails {}

impl crate::operation::OperationDetails for OnchainSendDetails {}

impl crate::operation::DetailedOperationState for OnchainSendState {
    type Details = OnchainSendDetails;
}

/// The lifecycle of an on-chain deposit.
///
/// The five variants are the application-level lifecycle of a deposit:
/// nothing seen, seen, confirmed, credited, or could not be credited.
///
/// A deposit can stay in [`Confirmed`](Self::Confirmed) across an internal
/// retry of the claim, under the same operation id, until the claim
/// succeeds; [`Failed`](Self::Failed) is emitted only once no further claim
/// is possible, so an application never sees a still-claimable deposit
/// finalized.
///
/// # The final state is self-contained
///
/// [`Claimed`](Self::Claimed) carries the funding transaction, the gross
/// amount that arrived, and the net amount credited, and that is not
/// redundancy. A subscription yields the state an operation is in now and
/// never replays the ones before it, so an application that reattaches to a
/// deposit by id, after a restart, from an activity row, or from a
/// notification, may see [`Claimed`](Self::Claimed) as the very first state
/// it is ever shown, and it can render a full receipt from that state alone.
///
/// The one state that is deliberately not self-contained is
/// [`Failed`](Self::Failed), which carries only a diagnostic reason even
/// though a deposit can fail after its transaction was seen. That is what
/// [`OnchainReceiveDetails`] is for: the address, and the transaction and
/// gross amount once one was seen, are on the details record too, so an
/// application never needs to have observed an earlier state to describe a
/// failed deposit.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OnchainReceiveState {
    /// The address is being watched and no transaction paying it has been
    /// seen yet.
    ///
    /// A deposit can sit here indefinitely, and there is no call that ends
    /// it; see [`Onchain::receive`] for what that means for
    /// [`Operation::await_final`](crate::Operation::await_final) and for
    /// [`Sdk::forget_federation`](crate::Sdk::forget_federation).
    WaitingForTransaction,
    /// A transaction paying the address has been seen and is waiting for
    /// enough confirmations for the federation to accept it.
    WaitingForConfirmation {
        /// The funding transaction.
        txid: Txid,
        /// The gross amount that transaction paid to the address, before
        /// anything the federation charges to claim it.
        gross_deposited: Sats,
    },
    /// The transaction has the confirmations the federation requires; the
    /// deposit is being claimed into the balance.
    Confirmed {
        /// The funding transaction.
        txid: Txid,
        /// The gross amount that transaction paid to the address, before
        /// anything the federation charges to claim it.
        gross_deposited: Sats,
    },
    /// Final: the deposit is in the spendable balance.
    ///
    /// Self-contained on purpose, see the enum's own documentation. A
    /// caller holding only this state can name the transaction, what
    /// arrived, and what was credited, without having observed anything
    /// earlier.
    Claimed {
        /// The funding transaction, for receipts and block explorers.
        txid: Txid,
        /// The gross amount that arrived on chain, before anything the
        /// federation charges to claim it.
        gross_deposited: Sats,
        /// The amount actually credited to the balance: `gross_deposited`
        /// less the aggregate of every federation-side cost of claiming the
        /// deposit.
        ///
        /// [`OnchainReceiveDetails::fee`] is the aggregate it is computed
        /// from and [`OnchainReceiveDetails::fee_breakdown`] names the
        /// parts. Denominated in millisatoshis, because those fees are, so
        /// the credit need not be a whole number of satoshis. This is the
        /// number the balance moved by.
        net_credit: Amount,
    },
    /// Final: the deposit could not be claimed.
    ///
    /// Carries no transaction and no amount even when one was seen. What
    /// arrived is on [`OnchainReceiveDetails`], which is where a caller that
    /// only ever saw this state reads it; no claim settled, so that record
    /// has no fee and no credit for it either.
    Failed {
        /// Human-readable explanation. Diagnostic only, not a stable
        /// contract, and not something to match on.
        reason: String,
    },
}

impl crate::operation::sealed::Sealed for OnchainReceiveState {}

impl OperationState for OnchainReceiveState {
    fn is_final(&self) -> bool {
        match self {
            OnchainReceiveState::WaitingForTransaction
            | OnchainReceiveState::WaitingForConfirmation { .. }
            | OnchainReceiveState::Confirmed { .. } => false,
            OnchainReceiveState::Claimed { .. } | OnchainReceiveState::Failed { .. } => true,
        }
    }
}

/// What an on-chain deposit is: the address to display, and the facts about
/// the funding transaction as they become known.
///
/// Read with [`Operation::details`](crate::Operation::details) on an
/// `Operation<OnchainReceiveState>`. The record is committed in the same
/// storage transaction that creates the operation, so it is readable from
/// the moment [`Onchain::receive`] returns. No state carries the address, so
/// this record is what makes an operation id enough to rebuild a deposit
/// screen after a restart.
///
/// # Five fields fill in over time, each once and for good
///
/// [`txid`](OnchainReceiveDetails::txid) and
/// [`gross_deposited`](OnchainReceiveDetails::gross_deposited) fill in when a
/// transaction is seen, at
/// [`WaitingForConfirmation`](OnchainReceiveState::WaitingForConfirmation).
/// [`fee`](OnchainReceiveDetails::fee),
/// [`fee_breakdown`](OnchainReceiveDetails::fee_breakdown) and
/// [`net_credit`](OnchainReceiveDetails::net_credit) fill in when the claim
/// settles, at [`Claimed`](OnchainReceiveState::Claimed).
///
/// Each field goes from `None` to `Some` at most once, in the same write
/// that records the transition establishing it, and never changes to a
/// different value and never reverts. So a caller need not order this call
/// against [`Operation::state`](crate::Operation::state), and reading the
/// record twice cannot produce two contradictory receipts.
///
/// `None` means "not established", never "lost", and a field may stay `None`
/// for good: a deposit still in
/// [`WaitingForTransaction`](OnchainReceiveState::WaitingForTransaction) has
/// all five absent, and one that [`Failed`](OnchainReceiveState::Failed)
/// after its transaction was seen has the first two set and the rest `None`,
/// since no claim settled.
///
/// # The aggregate, and the arithmetic these fields satisfy
///
/// [`fee`](OnchainReceiveDetails::fee) is the aggregate of everything
/// claiming the deposit cost, not the deposit fee alone: this record's
/// identity is
/// [`gross_deposited`](OnchainReceiveDetails::gross_deposited) in
/// millisatoshis, less [`fee`](OnchainReceiveDetails::fee), equals
/// [`net_credit`](OnchainReceiveDetails::net_credit), which is the same
/// value [`Claimed`](OnchainReceiveState::Claimed) reports.
///
/// The aggregate is the figure to read;
/// [`fee_breakdown`](OnchainReceiveDetails::fee_breakdown) names its parts
/// for a screen that wants to explain the difference between what was sent
/// and what was credited rather than merely state it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OnchainReceiveDetails {
    /// The deposit address this operation watches.
    ///
    /// Fixed when the operation was created and never changes. Display it,
    /// encode it as a QR code, or hand it to a sender.
    ///
    /// Fresh for this operation: never handed out before it, and never
    /// handed out again; see [`Onchain::receive`].
    pub address: Address,
    /// The funding transaction, once one paying the address has been seen.
    ///
    /// `None` until then. Filled in when the deposit reaches
    /// [`WaitingForConfirmation`](OnchainReceiveState::WaitingForConfirmation)
    /// and never changed afterwards, including if the deposit then
    /// [`Failed`](OnchainReceiveState::Failed), which carries no transaction
    /// of its own.
    ///
    /// This tracks the first output detected at the address; see
    /// [`Onchain::receive`].
    pub txid: Option<Txid>,
    /// The gross amount that arrived on chain, before anything the
    /// federation charges to claim it.
    ///
    /// Whole [`Sats`](crate::Sats): it is the value of an output in the
    /// funding transaction. `None` until a transaction is seen, then fixed.
    ///
    /// This is the counterparty figure, what the sender sent, and it is the
    /// number to show beside the credit when a user asks why the two differ.
    pub gross_deposited: Option<Sats>,
    /// The aggregate of everything the federation charged to bring this
    /// deposit into the balance, once the claim has settled.
    ///
    /// This is the figure [`net_credit`](OnchainReceiveDetails::net_credit)
    /// is computed from; [`fee_breakdown`](OnchainReceiveDetails::fee_breakdown)
    /// names the parts.
    ///
    /// `None` until then. Millisatoshi-denominated, like every other fee in
    /// this facade.
    pub fee: Option<Amount>,
    /// [`fee`](OnchainReceiveDetails::fee), split into the named parts it is
    /// made of.
    ///
    /// `Some` exactly when the aggregate is, set in the same write, and
    /// re-reporting the same money rather than an additional charge. The
    /// aggregate stays authoritative; see [`OnchainReceiveFeeBreakdown`] for
    /// why a caller should not re-derive it by summing these.
    pub fee_breakdown: Option<OnchainReceiveFeeBreakdown>,
    /// The amount credited to the balance: `gross_deposited` in
    /// millisatoshis less [`fee`](OnchainReceiveDetails::fee), the
    /// aggregate, not a deposit fee alone.
    ///
    /// `None` until the claim completes. Equal to the
    /// [`Claimed`](OnchainReceiveState::Claimed) state's own net figure, so
    /// a receipt built from the record and one built from the state cannot
    /// disagree.
    pub net_credit: Option<Amount>,
    /// When the deposit address was allocated, by this device's clock.
    ///
    /// A local reading, like [`ActivityItem::time`](crate::ActivityItem::time).
    /// Note that this is when the *address* was handed out, not when the
    /// funding transaction arrived; a deposit may be paid days later.
    pub created_at: Timestamp,
}

impl crate::operation::sealed::Sealed for OnchainReceiveDetails {}

impl crate::operation::OperationDetails for OnchainReceiveDetails {}

impl crate::operation::DetailedOperationState for OnchainReceiveState {
    type Details = OnchainReceiveDetails;
}

/// What claiming a deposit cost, component by component.
///
/// Obtained from [`OnchainReceiveDetails::fee_breakdown`]. Every field is an
/// exact millisatoshi [`Amount`](crate::Amount), for the reason
/// [`OnchainQuote::fee`] gives on the withdrawal side. The components sum to
/// [`OnchainReceiveDetails::fee`] exactly, with no rounding and no residue.
///
/// Unlike [`OnchainSendFeeBreakdown`], which explains a quote, this explains
/// an outcome: the parts are what the claim was charged, not a prediction.
///
/// # Read the aggregate; use these to explain it
///
/// The type is `#[non_exhaustive]`, so a later version may split a component
/// in two or add one, and only the aggregate stays correct across that
/// change. It is also the figure
/// [`OnchainReceiveDetails::net_credit`] was actually computed from, so it is
/// the only one guaranteed to reconcile with the balance movement.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OnchainReceiveFeeBreakdown {
    /// The federation's own charge for accepting the deposit.
    ///
    /// The component a user means by "the federation's deposit fee". On its
    /// own it is not what reduced the credit.
    pub peg_in: Amount,
    /// What sweeping the deposit costs on the Bitcoin network, if anything.
    pub network_claim: Amount,
    /// What it costs to turn the deposit into spendable notes: a
    /// federation-internal, millisatoshi-denominated cost with no on-chain
    /// counterpart, and the component most likely to make the aggregate a
    /// non-whole number of satoshis.
    pub primary_module: Amount,
    /// The residue that note issuance leaves behind: value too small to be
    /// represented in the federation's denominations, and therefore given up.
    /// Small, frequently sub-satoshi, and genuinely part of why the credit is
    /// less than what arrived.
    pub dust: Amount,
}

/// The federation this facade operates on.
///
/// Held rather than a wallet-module handle, because a facade outlives the client behind it: a
/// call on a closed federation has to report `FederationClosed` rather than find nothing to talk
/// to.
#[derive(Debug)]
struct OnchainInner {
    federation: Arc<crate::federation::FederationInner>,
}

/// A quote's frozen plan: the destination, the amount, the fee and its parts, and the upstream
/// terms the fee was computed from, so that `send` can tell whether they moved.
#[derive(Debug)]
pub(super) struct OnchainQuoteInner {
    /// The federation the quote was made against. A quote is refused on any other.
    pub(super) federation_id: config::FederationId,
    pub(super) address: Address,
    pub(super) amount: Sats,
    pub(super) plan: Plan,
    pub(super) expires_at: Timestamp,
}

/// What a withdrawal will cost and how it will go, for either wallet module generation.
#[derive(Debug)]
pub(super) struct Plan {
    pub(super) breakdown: OnchainSendFeeBreakdown,
    /// The sum of `breakdown`.
    pub(super) fee: Amount,
    /// `amount` converted to millisatoshis, plus `fee`.
    pub(super) total: Amount,
    pub(super) terms: Terms,
}

/// The upstream inputs a plan was computed from. `send` recomputes the plan from the same
/// inputs read again and refuses on any difference in the total.
#[derive(Debug)]
pub(super) enum Terms {
    /// v1: the on-chain fee `get_withdraw_fees` quoted for the destination output, and the fee
    /// quote `send_fee_quote` returned for funding it.
    V1 {
        fees: fedimint_wallet_common::PegOutFees,
        quote: FeeQuote,
    },
    /// walletv2: the on-chain fee `send_fee` quoted, and the fee quote `send_fee_quote` returned.
    V2 {
        chain_fee: bitcoin::Amount,
        quote: FeeQuote,
    },
}

/// How long a quote stays executable after it is issued: shorter than lightning's 60 seconds
/// because the chain-fee estimate a plan carries tracks a moving mempool.
pub(super) const QUOTE_VALIDITY_MILLIS: u64 = 30_000;

/// The wallet module the live client has, whichever generation it is.
enum WalletModule<'a> {
    V1(ClientModuleInstance<'a, fedimint_wallet_client::WalletClientModule>),
    V2(ClientModuleInstance<'a, fedimint_walletv2_client::WalletClientModule>),
}

/// Picks the generation by asking the client, not the stored record: a facade obtained while a
/// module was present and used after the configuration dropped it is the `NotSupported` case.
fn module(client: &Client) -> Result<WalletModule<'_>> {
    if let Ok(module) = client.get_first_module::<fedimint_walletv2_client::WalletClientModule>() {
        return Ok(WalletModule::V2(module));
    }
    if let Ok(module) = client.get_first_module::<fedimint_wallet_client::WalletClientModule>() {
        return Ok(WalletModule::V1(module));
    }
    Err(Error::new(
        ErrorCode::NotSupported,
        "this federation no longer has a wallet module",
    ))
}

/// Checks a withdrawal destination's Bitcoin network against the federation's. `Onchain::quote`
/// runs this before anything is committed, on both wallet module generations.
fn check_network(address: &Address, expected: Network) -> Result<()> {
    if address.inner().is_valid_for_network(expected.to_bitcoin()) {
        return Ok(());
    }
    Err(Error::with_details(
        ErrorCode::NetworkMismatch,
        format!(
            "the address is for {} but the federation runs on {}",
            address.observed_prefix(),
            expected.as_str()
        ),
        ErrorDetails::NetworkMismatch {
            expected,
            compatible: address.compatible_networks(),
            observed_prefix: address.observed_prefix(),
        },
    ))
}

/// Refuses a withdrawal amount the federation cannot execute: zero, or below the destination's
/// dust threshold. `amount` is converted with `sats_to_bitcoin` for the comparison against
/// `dust`, which is the v1 script's `minimal_non_dust()` or the walletv2 config's `dust_limit`.
/// The refusal a plan makes before its federation round trip: a balance that does not even
/// cover `amount` on its own. The round trip cannot report a shortfall, and
/// `ErrorDetails::InsufficientBalance::required` documents that no fee is included in this early
/// figure. Runs after the amount itself was validated, so an amount the federation could never
/// withdraw is refused as invalid whatever the balance.
pub(super) fn check_covers_amount(amount: Sats, available: Amount) -> Result<()> {
    let requested = sats_to_amount(amount)?;
    if available < requested {
        return Err(insufficient(requested, available));
    }
    Ok(())
}

pub(super) fn check_amount(amount: Sats, dust: bitcoin::Amount) -> Result<()> {
    if amount.sats() == 0 {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "a withdrawal of zero satoshis is not valid",
        ));
    }
    if sats_to_bitcoin(amount) < dust {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            format!(
                "{amount} is below the destination's dust threshold of {} sat",
                dust.to_sat()
            ),
        ));
    }
    Ok(())
}

/// Refuses a quote made for another federation or past its window.
fn ensure_executable(
    quote: &OnchainQuoteInner,
    federation_id: config::FederationId,
    now_millis: u64,
) -> Result<()> {
    if quote.federation_id != federation_id {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "this quote was issued by another federation",
        ));
    }
    if now_millis > quote.expires_at.epoch_millis() {
        return Err(quote_expired(quote.expires_at, false));
    }
    Ok(())
}

/// Assembles a withdrawal's plan from the on-chain fee the destination output costs, the wallet
/// module's own fee on that output, and the fee quote for funding it, whose `output` already
/// includes that explicit fee (`fedimint-client/src/client.rs:865-935`): `change` is what is
/// left of the quote's output once the module's own fee is taken back out, plus the dust the
/// quote reports.
// `chain_fee` is `PegOutFees::amount()` on v1 and `send_fee()` on walletv2; `module_fee` is
// `peg_out_abs` on v1 and `fee_consensus.fee(output_value)` on walletv2. Both callers derive
// `wallet_output` the same way from there: `chain_fee + module_fee`.
pub(super) fn plan_of(
    chain_fee: Amount,
    module_fee: Amount,
    quote: &FeeQuote,
    amount: Sats,
    terms: Terms,
) -> Result<Plan> {
    let funding = from_upstream(quote.input.get_bitcoin());
    let output = from_upstream(quote.output.get_bitcoin());
    let dust = from_upstream(quote.dust.get_bitcoin());
    let wallet_output = add(chain_fee, module_fee)?;
    let change = output
        .checked_sub(module_fee)
        .and_then(|remainder| remainder.checked_add(dust))
        .ok_or_else(|| {
            Error::new(
                ErrorCode::Internal,
                "the fee quote's output is smaller than the wallet module's own fee",
            )
        })?;
    let breakdown = OnchainSendFeeBreakdown {
        wallet_output,
        funding,
        change,
    };
    let fee = add(add(wallet_output, funding)?, change)?;
    let total = add(sats_to_amount(amount)?, fee)?;
    Ok(Plan {
        breakdown,
        fee,
        total,
        terms,
    })
}

/// The fee, its named parts, and the net credit for a claimed deposit, read from what the claim
/// transaction itself minted and melted, rather than a dry run of the primary module's own
/// balancing.
///
/// A dry run cannot be right here: the mint shapes the change it mints to reach a target note
/// count per denomination, so the split, and with it the per-note fees, depends on the note
/// inventory at the moment the claim was built. By the time a claim is observed the notes it
/// minted are already in that inventory, so a fresh dry run sees a different split and reports a
/// different fee than the one actually charged.
///
/// Shared between both wallet generations, which differ only in what they pass: v1's
/// `input_amount` is the full gross deposit and `input_fee` its flat peg-in charge, with no
/// on-chain claim cost of its own (`network_claim = 0`); walletv2's `input_amount` is the gross
/// already net of the on-chain consolidation fee it deducts before forming the federation
/// transaction, `input_fee` its consensus fee on that net amount, and `network_claim` the
/// consolidation fee itself.
// `operation_id` is the claim's own upstream operation id: v1's deposit operation id, the one
// `subscribe_deposit` runs under, or walletv2's linked upstream operation id
// (`v2::Link::upstream`). `claim_mint_movement` uses it to find the submitted transaction, and
// `wallet_instance` (both callers' own `module.id`) to check that transaction funds itself from
// exactly one wallet input. A funded wallet's claim is not always single-input: mint v1
// consolidates notes above eight of one denomination in `create_final_inputs_and_outputs`
// (`$FM/modules/fedimint-mint-client/src/lib.rs:1365`) and mintv2's `rebalance` can spend
// existing notes too, so mint inputs may sit beside the wallet input that funds the claim.
pub(super) async fn claim_figures(
    client: &Client,
    operation_id: OperationId,
    wallet_instance: ModuleInstanceId,
    input_amount: Amount,
    input_fee: Amount,
    network_claim: Amount,
    gross: Sats,
) -> Result<(Amount, OnchainReceiveFeeBreakdown, Amount)> {
    let (outputs, mint_inputs) = claim_mint_movement(client, operation_id, wallet_instance).await?;
    figures_of(
        &outputs,
        &mint_inputs,
        input_amount,
        input_fee,
        network_claim,
        gross,
    )
}

/// The claim transaction's mint movement: every output it minted, and every mint input it spent
/// beside its one wallet input, each an `(amount, fee)` pair paired with what the owning mint
/// module's own fee schedule charges to move it.
///
/// Refuses, rather than silently under-reporting, a transaction that does not have the shape a
/// claim must have: exactly one input from the wallet module (`wallet_instance`) itself, any
/// number of mint inputs beside it, and at least one mint output. A claim with no mint outputs
/// read back as an empty list here would otherwise be reported as a zero credit with the whole
/// deposit folded into dust.
///
/// No public API at this pin reports what a claim actually minted and melted, so this reaches
/// into the client's own state store: `finalize_and_submit_transaction` registers a
/// transaction-submission state machine under the claim's operation id whose first state carries
/// the whole transaction, and the only way to read a state machine back out is
/// `Client::executor`, documented there as tooling for the CLI, "not meant for external use". The
/// follow-up is an upstream accessor that reports a claim's inputs and outputs directly, without
/// going through the executor at all.
async fn claim_mint_movement(
    client: &Client,
    operation_id: OperationId,
    wallet_instance: ModuleInstanceId,
) -> Result<(Vec<(Amount, Amount)>, Vec<(Amount, Amount)>)> {
    let tx = claim_transaction(client, operation_id).await?;
    let wallet_inputs = tx
        .inputs
        .iter()
        .filter(|input| input.module_instance_id() == wallet_instance)
        .count();
    check_claim_shape(wallet_inputs, tx.inputs.len(), tx.outputs.len())?;

    let mut fee_schedules: HashMap<ModuleInstanceId, MintFeeConsensus> = HashMap::new();
    let mut outputs = Vec::with_capacity(tx.outputs.len());
    for output in &tx.outputs {
        let instance_id = output.module_instance_id();
        if let hash_map::Entry::Vacant(entry) = fee_schedules.entry(instance_id) {
            entry.insert(mint_fee_consensus(client, instance_id).await?);
        }
        let amount = mint_output_amount(output, instance_id)?;
        let fee = from_upstream(fee_schedules[&instance_id].fee(to_upstream(amount)));
        outputs.push((amount, fee));
    }

    let mut mint_inputs = Vec::with_capacity(tx.inputs.len().saturating_sub(1));
    for input in &tx.inputs {
        let instance_id = input.module_instance_id();
        if instance_id == wallet_instance {
            continue;
        }
        if let hash_map::Entry::Vacant(entry) = fee_schedules.entry(instance_id) {
            entry.insert(mint_fee_consensus(client, instance_id).await?);
        }
        let amount = mint_input_amount(input, instance_id)?;
        let fee = from_upstream(fee_schedules[&instance_id].fee(to_upstream(amount)));
        mint_inputs.push((amount, fee));
    }

    Ok((outputs, mint_inputs))
}

/// Whether a claim transaction has the shape [`claim_mint_movement`] can read: exactly one input
/// from the wallet module's own instance (a claim funds itself from one wallet input, with any
/// number of mint inputs beside it, but never more than that one wallet input), and at least one
/// output to read credit from. Split out of [`claim_mint_movement`] so it is testable without a
/// client or a real transaction.
fn check_claim_shape(wallet_inputs: usize, total_inputs: usize, outputs: usize) -> Result<()> {
    if wallet_inputs != 1 {
        return Err(internal(format!(
            "a claim transaction funds itself from exactly one wallet input, found \
             {wallet_inputs} among {total_inputs} total inputs"
        )));
    }
    if outputs == 0 {
        return Err(internal(
            "a claim transaction minted no outputs, so it carries no credit to read",
        ));
    }
    Ok(())
}

/// Reads back the transaction a claim submitted, from the transaction-submission state machine
/// `finalize_and_submit_transaction` registers under the claim's operation id: `Created(tx)`
/// carries the whole transaction and stays readable, as an inactive state, once the transaction
/// is accepted.
async fn claim_transaction(client: &Client, operation_id: OperationId) -> Result<Transaction> {
    let (active, inactive) = client.executor().get_operation_states(operation_id).await;
    active
        .into_iter()
        .map(|(state, _)| state)
        .chain(inactive.into_iter().map(|(state, _)| state))
        .filter(|state| state.module_instance_id() == TRANSACTION_SUBMISSION_MODULE_INSTANCE)
        .find_map(|state| {
            let sm = state.as_any().downcast_ref::<TxSubmissionStatesSM>()?;
            match &sm.state {
                TxSubmissionStates::Created(tx) => Some(tx.clone()),
                _ => None,
            }
        })
        .ok_or_else(|| {
            internal(format!(
                "no submitted transaction found for claim {}",
                operation_id.fmt_full()
            ))
        })
}

/// One mint output's amount, whichever mint module generation minted it. `instance_id` names the
/// module instance that owns `output`, so an output this build cannot read as a mint output is
/// refused naming the module instance rather than skipped.
fn mint_output_amount(output: &DynOutput, instance_id: ModuleInstanceId) -> Result<Amount> {
    if let Some(v1) = output
        .as_any()
        .downcast_ref::<fedimint_mint_common::MintOutput>()
    {
        return v1
            .ensure_v0_ref()
            .map(|v0| from_upstream(v0.amount))
            .map_err(internal);
    }
    if let Some(v2) = output
        .as_any()
        .downcast_ref::<fedimint_mintv2_common::MintOutput>()
    {
        return v2
            .ensure_v0_ref()
            .map(|v0| from_upstream(v0.denomination.amount()))
            .map_err(internal);
    }
    Err(internal(format!(
        "a claim transaction's output belongs to module instance {instance_id}, not a mint module \
         this build can read"
    )))
}

/// One mint input's amount, whichever mint module generation melted it. `instance_id` names the
/// module instance that owns `input`, so an input that is neither the wallet input nor readable
/// as a mint input here is refused naming the module instance rather than skipped.
fn mint_input_amount(input: &DynInput, instance_id: ModuleInstanceId) -> Result<Amount> {
    if let Some(v1) = input
        .as_any()
        .downcast_ref::<fedimint_mint_common::MintInput>()
    {
        return v1
            .ensure_v0_ref()
            .map(|v0| from_upstream(v0.amount))
            .map_err(internal);
    }
    if let Some(v2) = input
        .as_any()
        .downcast_ref::<fedimint_mintv2_common::MintInput>()
    {
        return v2
            .ensure_v0_ref()
            .map(|v0| from_upstream(v0.note.denomination.amount()))
            .map_err(internal);
    }
    Err(internal(format!(
        "a claim transaction's input belongs to module instance {instance_id}, not the wallet \
         module or a mint module this build can read"
    )))
}

/// Either mint module generation's own fee schedule, read once per module instance id by
/// [`claim_mint_movement`] and applied to every note that instance's module moved, minted or
/// melted alike: both generations' `input_fee` and `output_fee` apply the very same
/// `fee_consensus.fee(amount)` regardless of direction
/// (`$FM/modules/fedimint-mint-client/src/lib.rs:1005-1022`,
/// `$FM/modules/fedimint-mintv2-client/src/lib.rs:449-461`).
enum MintFeeConsensus {
    V1(fedimint_mint_common::config::FeeConsensus),
    V2(fedimint_mintv2_common::config::FeeConsensus),
}

impl MintFeeConsensus {
    fn fee(&self, amount: fedimint_core::Amount) -> fedimint_core::Amount {
        match self {
            MintFeeConsensus::V1(fee_consensus) => fee_consensus.fee(amount),
            MintFeeConsensus::V2(fee_consensus) => fee_consensus.fee(amount),
        }
    }
}

/// The fee schedule for one mint module instance, cast out of the client's decoded config exactly
/// as `Ecash::quote` reads `MintClientConfig` for the v1 mint: both mint client crates keep the
/// type private and it is only nameable through their `-common` counterparts.
async fn mint_fee_consensus(client: &Client, id: ModuleInstanceId) -> Result<MintFeeConsensus> {
    let module_cfg = client.config().await.get_module_cfg(id).map_err(internal)?;
    if let Ok(cfg) = module_cfg.cast::<fedimint_mint_common::config::MintClientConfig>() {
        return Ok(MintFeeConsensus::V1(cfg.fee_consensus.clone()));
    }
    let cfg = module_cfg
        .cast::<fedimint_mintv2_common::config::MintClientConfig>()
        .map_err(internal)?;
    Ok(MintFeeConsensus::V2(cfg.fee_consensus.clone()))
}

/// The pure arithmetic behind [`claim_figures`], factored out so it is testable without a client
/// or a real claim transaction: `outputs` is one `(amount, fee)` pair per output the claim
/// minted, `mint_inputs` one `(amount, fee)` pair per mint input it spent beside its wallet
/// input, `fee` in both already the owning mint module's `fee_consensus.fee(amount)`.
///
/// `net_credit` is the net movement through the mint modules: the outputs' amounts less the mint
/// inputs' amounts, exactly what a claim that only mints (no mint inputs) minted before, and
/// less when a mint input melted some of that back. `primary_module` is every mint fee the claim
/// paid, on outputs and mint inputs alike. `dust` is what the wallet input's net amount, plus the
/// mint inputs' amounts, leaves once the outputs and every mint fee are paid: the residue too
/// small to have been minted into a note of its own. `fee` is `gross` less `net_credit`, by
/// construction.
///
/// This function trusts its caller: `outputs` and `mint_inputs` are checked to actually be what
/// the claim moved by [`claim_mint_movement`], not here, and `input_amount`, `input_fee`,
/// `network_claim` and `gross` are trusted to already be consistent with each other, so there is
/// no reading of `outputs` or `mint_inputs` this function's own arithmetic could catch as wrong.
fn figures_of(
    outputs: &[(Amount, Amount)],
    mint_inputs: &[(Amount, Amount)],
    input_amount: Amount,
    input_fee: Amount,
    network_claim: Amount,
    gross: Sats,
) -> Result<(Amount, OnchainReceiveFeeBreakdown, Amount)> {
    let mut output_amount = Amount::from_msats(0);
    let mut output_fee = Amount::from_msats(0);
    for (amount, fee) in outputs {
        output_amount = add(output_amount, *amount)?;
        output_fee = add(output_fee, *fee)?;
    }
    let mut mint_input_amount = Amount::from_msats(0);
    let mut mint_input_fee = Amount::from_msats(0);
    for (amount, fee) in mint_inputs {
        mint_input_amount = add(mint_input_amount, *amount)?;
        mint_input_fee = add(mint_input_fee, *fee)?;
    }
    let primary_module = add(output_fee, mint_input_fee)?;
    let net_credit = output_amount
        .checked_sub(mint_input_amount)
        .ok_or_else(|| internal("a claim's mint inputs exceed its mint outputs"))?;
    let available = input_amount
        .checked_sub(input_fee)
        .ok_or_else(|| internal("the claim's peg-in fee exceeds its input amount"))?;
    let funded = add(available, mint_input_amount)?;
    let spent = add(output_amount, output_fee)?;
    let dust = funded
        .checked_sub(spent)
        .and_then(|remainder| remainder.checked_sub(mint_input_fee))
        .ok_or_else(|| internal("the claim's inputs do not cover its outputs and their fees"))?;
    let breakdown = OnchainReceiveFeeBreakdown {
        peg_in: input_fee,
        network_claim,
        primary_module,
        dust,
    };
    let fee = sats_to_amount(gross)?
        .checked_sub(net_credit)
        .ok_or_else(|| internal("the claim's net credit exceeds the deposit's gross amount"))?;
    Ok((fee, breakdown, net_credit))
}

pub(super) fn to_upstream(amount: Amount) -> fedimint_core::Amount {
    fedimint_core::Amount::from_msats(amount.msats())
}

pub(super) fn from_upstream(amount: fedimint_core::Amount) -> Amount {
    Amount::from_msats(amount.msats)
}

pub(super) fn sats_to_bitcoin(sats: Sats) -> bitcoin::Amount {
    bitcoin::Amount::from_sat(sats.sats())
}

pub(super) fn bitcoin_to_sats(amount: bitcoin::Amount) -> Sats {
    Sats::from_sats(amount.to_sat())
}

pub(super) fn sats_to_amount(sats: Sats) -> Result<Amount> {
    sats.to_amount().ok_or_else(|| {
        Error::new(
            ErrorCode::Internal,
            "the amount does not fit in a millisatoshi count",
        )
    })
}

pub(super) fn add(left: Amount, right: Amount) -> Result<Amount> {
    left.checked_add(right)
        .ok_or_else(|| Error::new(ErrorCode::Internal, "an amount overflowed"))
}

pub(super) fn quote_changed(quoted_total: Amount, current_total: Amount) -> Error {
    Error::with_details(
        ErrorCode::QuoteChanged,
        format!(
            "the withdrawal would now debit {} msat instead of the quoted {} msat",
            current_total.msats(),
            quoted_total.msats()
        ),
        ErrorDetails::QuoteTermsChanged {
            quoted_total,
            current_total,
        },
    )
}

pub(super) fn quote_expired(expires_at: Timestamp, already_executed: bool) -> Error {
    let message = if already_executed {
        "this withdrawal has already been executed or is being executed"
    } else {
        "this quote is no longer executable; quote again"
    };
    Error::with_details(
        ErrorCode::QuoteExpired,
        message,
        ErrorDetails::QuoteExpired {
            expires_at,
            already_executed,
        },
    )
}

pub(super) fn insufficient(required: Amount, available: Amount) -> Error {
    Error::with_details(
        ErrorCode::InsufficientBalance,
        format!(
            "the withdrawal needs {} msat but only {} msat is spendable",
            required.msats(),
            available.msats()
        ),
        ErrorDetails::InsufficientBalance {
            required,
            available,
        },
    )
}

pub(super) fn unreachable(cause: impl core::fmt::Display) -> Error {
    Error::new(
        ErrorCode::FederationUnreachable,
        format!("the federation did not answer: {cause}"),
    )
}

pub(super) fn internal(cause: impl core::fmt::Display) -> Error {
    Error::new(ErrorCode::Internal, cause.to_string())
}

/// An upstream subscription that could not be opened, for either generation's driver.
pub(super) fn subscribe_error(cause: impl core::fmt::Display) -> Error {
    Error::new(
        ErrorCode::Internal,
        format!("could not follow this operation upstream: {cause}"),
    )
}

/// A federation round trip took longer than `crate::sdk::CONTACT_TIMEOUT`, the same bound
/// `Sdk::download_config` and friends use, made `pub(crate)` so this facade can share it rather
/// than duplicate the value.
pub(super) fn timeout() -> Error {
    Error::new(ErrorCode::Timeout, "the federation did not answer in time")
}

/// The spendable balance, as `Federation::balance` reads it.
pub(super) async fn balance_of(client: &Client, status: FederationStatus) -> Result<Amount> {
    crate::federation::balance_of(client, status).await
}

/// What a fee-quote dry run's failure means, before either mint's answer is turned into an
/// [`Error`]. Mirrors [`crate::lightning`]'s own `FeeQuoteFailure`; kept as a separate copy here
/// because the two facades' `balance_of` differ (this one needs a [`FederationStatus`]) and
/// neither may name the other's private types.
///
/// The dry run balances the funding transaction against the real notes and fails inside the
/// primary module when they cannot cover it, on either wallet generation: v1's `send_fee_quote`
/// and walletv2's both end in the same module-agnostic `Client::fee_quote`
/// (`$FM/fedimint-client/src/client.rs:877`), so the failure is classified the same way
/// regardless of which wallet generation asked for the quote. The v1 mint (`fedimint-mint-client`)
/// reports it with the typed [`fedimint_mint_client::InsufficientBalanceError`], which already
/// carries the amounts that were short; the v2 mint (`fedimint-mintv2-client`) reports the same
/// condition as a plain-text `anyhow` context, `"Insufficient funds"`
/// (`fedimint-mintv2-client/src/lib.rs:503`), with no amounts of its own.
#[derive(Debug)]
enum FeeQuoteFailure {
    /// The v1 mint's typed error, carrying its own requested and total amounts.
    Typed { requested: Amount, total: Amount },
    /// The v2 mint's plain-text refusal, which names no amounts.
    Text,
}

/// Recognizes either mint's insufficient-balance refusal from a fee-quote failure, or reports
/// neither is a match. Pure so the mapping can be checked without a live `Client`.
fn classify_fee_quote_failure(
    short: Option<&fedimint_mint_client::InsufficientBalanceError>,
    text: &str,
) -> Option<FeeQuoteFailure> {
    if let Some(short) = short {
        return Some(FeeQuoteFailure::Typed {
            requested: from_upstream(short.requested_amount),
            total: from_upstream(short.total_amount),
        });
    }
    // The v1 mint's `InsufficientBalanceError` arrives wrapped in the client's submission error
    // when the quote is the wallet's, so its own wording is matched too, as the lightning v2
    // send does; the typed downcast above is for the unwrapped case.
    if text.contains("Insufficient funds") || text.contains("Insufficient balance") {
        return Some(FeeQuoteFailure::Text);
    }
    None
}

/// Turns a fee-quote dry run's failure into the [`Error`] it represents, for both wallet
/// generations' `plan` and send-time recheck.
///
/// `required` is the amount the failed quote was for: the withdrawal's amount plus the on-chain
/// fee when that was already quoted, since the exact funding total the quote would have reported
/// is not knowable once the quote itself failed, and `ErrorDetails::InsufficientBalance::required`
/// only promises the shortfall's rough scale, not an exact total. `context` names the quote for
/// the fallback message, when `short` is absent and `text` does not match either mint's wording
/// for "the notes on hand are short".
pub(super) async fn fee_quote_failure(
    client: &Client,
    status: FederationStatus,
    short: Option<&fedimint_mint_client::InsufficientBalanceError>,
    text: &str,
    required: Amount,
    context: &str,
) -> Error {
    match classify_fee_quote_failure(short, text) {
        Some(FeeQuoteFailure::Typed { requested, total }) => insufficient(requested, total),
        Some(FeeQuoteFailure::Text) => {
            // The v2 mint's text names no amounts, so the balance is read again here. A
            // failed read must not mask the real refusal that was already found, so it
            // falls back to zero rather than turning this into an unrelated error.
            let available = balance_of(client, status)
                .await
                .unwrap_or(Amount::from_msats(0));
            insufficient(required, available)
        }
        None => internal(format!("{context}: {text}")),
    }
}

/// This device's clock, for a details record's `created_at`.
pub(super) fn now() -> Timestamp {
    Timestamp::from_epoch_millis(crate::db::now_millis())
}

/// The deposit address a stored `ONCHAIN_RECEIVE` details record names, or `None` if it does
/// not decode as one.
///
/// Lets `FederationInner::owner_of_deposit_address` walk the operation index and compare
/// addresses without naming this facade's wire types itself.
pub(crate) fn deposit_address_of_record(details: &str) -> Option<String> {
    wire::decode_receive_wire(details)
        .ok()
        .map(|wire| wire.address)
}

#[cfg(test)]
mod tests {
    use fedimint_core::module::Amounts;

    use super::*;
    use crate::DetailedOperationState;

    /// The all-zero txid, which is not a real one; these tests never look at
    /// its value, only carry it through a payload.
    fn a_txid() -> Txid {
        "0000000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .expect("a well-formed transaction id")
    }

    /// A real regtest address, taken from `bitcoin`'s own test suite: the
    /// parse validates a checksum, so a plausible-looking string no longer
    /// works here.
    fn an_address() -> Address {
        "bcrt1q2nfxmhd4n3c8834pj72xagvyr9gl57n5r94fsl"
            .parse()
            .expect("a valid regtest address")
    }

    /// Generic over the pattern rather than over one kind, exactly as
    /// `operation.rs` does for its probe pair: this compiles only if the
    /// state type names its record and the record satisfies every bound
    /// [`crate::OperationDetails`] imposes.
    fn round_trip_details<S: DetailedOperationState>(details: S::Details) -> S::Details {
        details
    }

    #[test]
    fn onchain_send_state_created_is_not_final() {
        assert!(!OnchainSendState::Created.is_final());
    }

    #[test]
    fn onchain_send_state_succeeded_is_final() {
        assert!(OnchainSendState::Succeeded { txid: a_txid() }.is_final());
    }

    #[test]
    fn onchain_send_state_refunded_is_final() {
        assert!(
            OnchainSendState::Refunded {
                reason: String::new(),
            }
            .is_final()
        );
    }

    #[test]
    fn onchain_send_state_failed_is_final() {
        assert!(
            OnchainSendState::Failed {
                reason: String::new(),
            }
            .is_final()
        );
    }

    #[test]
    fn onchain_receive_state_waiting_for_transaction_is_not_final() {
        assert!(!OnchainReceiveState::WaitingForTransaction.is_final());
    }

    #[test]
    fn onchain_receive_state_waiting_for_confirmation_is_not_final() {
        assert!(
            !OnchainReceiveState::WaitingForConfirmation {
                txid: a_txid(),
                gross_deposited: Sats::from_sats(100_000),
            }
            .is_final()
        );
    }

    #[test]
    fn onchain_receive_state_confirmed_is_not_final() {
        assert!(
            !OnchainReceiveState::Confirmed {
                txid: a_txid(),
                gross_deposited: Sats::from_sats(100_000),
            }
            .is_final()
        );
    }

    #[test]
    fn onchain_receive_state_claimed_is_final() {
        assert!(
            OnchainReceiveState::Claimed {
                txid: a_txid(),
                gross_deposited: Sats::from_sats(100_000),
                net_credit: Amount::from_msats(99_998_500),
            }
            .is_final()
        );
    }

    #[test]
    fn onchain_receive_state_failed_is_final() {
        assert!(
            OnchainReceiveState::Failed {
                reason: String::new(),
            }
            .is_final()
        );
    }

    /// `Claimed` is self-contained: a caller holding only this state can
    /// name the transaction, the gross and the credit, with no earlier state.
    #[test]
    fn claimed_is_self_contained() {
        let state = OnchainReceiveState::Claimed {
            txid: a_txid(),
            gross_deposited: Sats::from_sats(100_000),
            net_credit: Amount::from_msats(99_998_500),
        };
        match state {
            OnchainReceiveState::Claimed {
                txid,
                gross_deposited,
                net_credit,
            } => {
                assert_eq!(txid, a_txid());
                assert_eq!(gross_deposited, Sats::from_sats(100_000));
                // A fee of 1500 msat leaves a credit that is not a whole
                // number of satoshis, which is why this field is an
                // `Amount`: as `Sats` it could only have been wrong.
                assert_eq!(net_credit, Amount::from_msats(99_998_500));
                assert_eq!(net_credit.to_sats_exact(), None);
            }
            _ => unreachable!("constructed as Claimed"),
        }
    }

    #[test]
    fn send_details_total_is_the_amount_plus_the_exact_fee() {
        let amount = Sats::from_sats(25_000);
        let fee = Amount::from_msats(1_234_567);
        let details = OnchainSendDetails {
            address: an_address(),
            amount,
            fee,
            total: amount
                .to_amount()
                .expect("25 000 sat is representable in msat")
                .checked_add(fee)
                .expect("no overflow at this magnitude"),
            created_at: Timestamp::from_epoch_millis(1),
        };
        assert_eq!(details.total, Amount::from_msats(26_234_567));
        // The reason the fee and the total are `Amount`s: neither is a whole
        // number of satoshis, so a satoshi-typed accessor would have had to
        // round the debit down.
        assert_eq!(details.fee.to_sats_exact(), None);
        assert_eq!(details.total.to_sats_exact(), None);
        // ... while what reaches the destination genuinely is whole sats.
        assert_eq!(details.amount, Sats::from_sats(25_000));
    }

    #[test]
    fn receive_details_options_fill_in_once_and_agree_with_claimed() {
        let gross = Sats::from_sats(100_000);
        let fee = Amount::from_msats(1_500);
        let net = gross
            .to_amount()
            .expect("100 000 sat is representable in msat")
            .checked_sub(fee)
            .expect("the fee is smaller than the deposit");

        let waiting = OnchainReceiveDetails {
            address: an_address(),
            txid: None,
            gross_deposited: None,
            fee: None,
            fee_breakdown: None,
            net_credit: None,
            created_at: Timestamp::from_epoch_millis(1),
        };
        // Nothing is known before a transaction is seen, and that is not a
        // failure to record anything.
        assert_eq!(waiting.txid, None);
        assert_eq!(waiting.net_credit, None);

        let claimed = OnchainReceiveDetails {
            txid: Some(a_txid()),
            gross_deposited: Some(gross),
            fee: Some(fee),
            fee_breakdown: Some(OnchainReceiveFeeBreakdown {
                peg_in: fee,
                network_claim: Amount::from_msats(0),
                primary_module: Amount::from_msats(0),
                dust: Amount::from_msats(0),
            }),
            net_credit: Some(net),
            ..waiting.clone()
        };
        // The fields that were already fixed are untouched by the fill-in.
        assert_eq!(claimed.address, waiting.address);
        assert_eq!(claimed.created_at, waiting.created_at);
        assert_ne!(claimed, waiting);

        // The record and the final state report the same money.
        let state = OnchainReceiveState::Claimed {
            txid: a_txid(),
            gross_deposited: gross,
            net_credit: net,
        };
        match state {
            OnchainReceiveState::Claimed {
                txid,
                gross_deposited,
                net_credit,
            } => {
                assert_eq!(claimed.txid, Some(txid));
                assert_eq!(claimed.gross_deposited, Some(gross_deposited));
                assert_eq!(claimed.net_credit, Some(net_credit));
            }
            _ => unreachable!("constructed as Claimed"),
        }
    }

    #[test]
    fn both_state_types_name_their_details_record() {
        let send = OnchainSendDetails {
            address: an_address(),
            amount: Sats::from_sats(1),
            fee: Amount::from_msats(1),
            total: Amount::from_msats(1_001),
            created_at: Timestamp::from_epoch_millis(0),
        };
        let receive = OnchainReceiveDetails {
            address: an_address(),
            txid: None,
            gross_deposited: None,
            fee: None,
            fee_breakdown: None,
            net_credit: None,
            created_at: Timestamp::from_epoch_millis(0),
        };
        assert_eq!(round_trip_details::<OnchainSendState>(send.clone()), send);
        assert_eq!(
            round_trip_details::<OnchainReceiveState>(receive.clone()),
            receive
        );
    }

    #[test]
    fn fee_breakdown_components_sum_to_the_aggregate() {
        let breakdown = OnchainSendFeeBreakdown {
            wallet_output: Amount::from_msats(1_200_000),
            funding: Amount::from_msats(34_000),
            change: Amount::from_msats(567),
        };
        let summed = breakdown
            .wallet_output
            .checked_add(breakdown.funding)
            .and_then(|partial| partial.checked_add(breakdown.change))
            .expect("no overflow at this magnitude");
        assert_eq!(summed, Amount::from_msats(1_234_567));
        // And the aggregate is why it is an `Amount`: the parts do not add
        // up to a whole number of satoshis.
        assert_eq!(summed.to_sats_exact(), None);
    }

    #[test]
    fn check_network_accepts_a_regtest_address_on_regtest() {
        assert!(check_network(&an_address(), Network::Regtest).is_ok());
    }

    #[test]
    fn check_network_refuses_it_on_mainnet_with_details() {
        let err = check_network(&an_address(), Network::Bitcoin).expect_err("wrong network");
        assert_eq!(err.code, ErrorCode::NetworkMismatch);
        match err.detail() {
            Some(ErrorDetails::NetworkMismatch {
                expected,
                compatible,
                observed_prefix,
            }) => {
                assert_eq!(*expected, Network::Bitcoin);
                assert_eq!(compatible, &vec![Network::Regtest]);
                assert_eq!(observed_prefix, "bcrt");
            }
            other => panic!("expected NetworkMismatch details, got {other:?}"),
        }
    }

    #[test]
    fn check_amount_refuses_zero_and_below_dust() {
        let dust = bitcoin::Amount::from_sat(546);
        assert_eq!(
            check_amount(Sats::from_sats(0), dust)
                .expect_err("zero")
                .code,
            ErrorCode::InvalidInput
        );
        assert_eq!(
            check_amount(Sats::from_sats(545), dust)
                .expect_err("below dust")
                .code,
            ErrorCode::InvalidInput
        );
    }

    #[test]
    fn check_amount_accepts_at_dust() {
        assert!(check_amount(Sats::from_sats(546), bitcoin::Amount::from_sat(546)).is_ok());
    }

    /// The hand-built quote the withdrawal fee breakdown decision spells out: `input` 34 000
    /// msat funds the transaction, `output` is the module's own 1 000 000 msat fee plus 567
    /// msat of change, `dust` is 5 msat.
    fn a_fee_quote() -> FeeQuote {
        FeeQuote {
            input: Amounts::new_bitcoin_msats(34_000),
            output: Amounts::new_bitcoin_msats(1_000_567),
            dust: Amounts::new_bitcoin_msats(5),
        }
    }

    #[test]
    fn plan_of_builds_the_withdrawal_fee_breakdown() {
        let quote = a_fee_quote();
        let amount = Sats::from_sats(500_000);
        let plan = plan_of(
            Amount::from_msats(200_000),
            Amount::from_msats(1_000_000),
            &quote,
            amount,
            Terms::V2 {
                chain_fee: bitcoin::Amount::from_sat(200),
                quote: quote.clone(),
            },
        )
        .expect("a plan");
        assert_eq!(
            plan.breakdown,
            OnchainSendFeeBreakdown {
                wallet_output: Amount::from_msats(1_200_000),
                funding: Amount::from_msats(34_000),
                change: Amount::from_msats(572),
            }
        );
        assert_eq!(plan.fee, Amount::from_msats(1_234_572));
        assert_eq!(
            plan.total,
            amount
                .to_amount()
                .expect("500 000 sat is representable in msat")
                .checked_add(plan.fee)
                .expect("no overflow at this magnitude")
        );
    }

    #[test]
    fn plan_of_with_output_below_the_module_fee_is_internal() {
        let quote = FeeQuote {
            input: Amounts::ZERO,
            output: Amounts::new_bitcoin_msats(10),
            dust: Amounts::ZERO,
        };
        let err = plan_of(
            Amount::from_msats(0),
            Amount::from_msats(25),
            &quote,
            Sats::from_sats(1),
            Terms::V2 {
                chain_fee: bitcoin::Amount::ZERO,
                quote: quote.clone(),
            },
        )
        .expect_err("inconsistent");
        assert_eq!(err.code, ErrorCode::Internal);
    }

    fn a_quote(expires_at: u64) -> OnchainQuoteInner {
        let quote = a_fee_quote();
        OnchainQuoteInner {
            federation_id: fedimint_core::config::FederationId::dummy(),
            address: an_address(),
            amount: Sats::from_sats(500_000),
            plan: plan_of(
                Amount::from_msats(200_000),
                Amount::from_msats(1_000_000),
                &quote,
                Sats::from_sats(500_000),
                Terms::V2 {
                    chain_fee: bitcoin::Amount::from_sat(200),
                    quote: quote.clone(),
                },
            )
            .expect("a plan"),
            expires_at: Timestamp::from_epoch_millis(expires_at),
        }
    }

    #[test]
    fn a_quote_is_executable_until_it_expires_and_only_on_its_federation() {
        let quote = a_quote(1_000);
        let id = fedimint_core::config::FederationId::dummy();
        assert!(ensure_executable(&quote, id, 999).is_ok());
        assert!(ensure_executable(&quote, id, 1_000).is_ok());
        let err = ensure_executable(&quote, id, 1_001).expect_err("expired");
        assert_eq!(err.code, ErrorCode::QuoteExpired);
        match err.detail() {
            Some(ErrorDetails::QuoteExpired {
                expires_at,
                already_executed,
            }) => {
                assert_eq!(*expires_at, Timestamp::from_epoch_millis(1_000));
                assert!(!already_executed);
            }
            other => panic!("expected QuoteExpired details, got {other:?}"),
        }
        let other = fedimint_core::config::FederationId(
            fedimint_core::bitcoin::hashes::Hash::hash(b"another federation"),
        );
        assert_eq!(
            ensure_executable(&quote, other, 0)
                .expect_err("wrong federation")
                .code,
            ErrorCode::InvalidInput
        );
    }

    #[test]
    fn quote_accessors_read_the_frozen_plan() {
        let quote = OnchainQuote { inner: a_quote(5) };
        assert_eq!(quote.amount(), Sats::from_sats(500_000));
        assert_eq!(quote.fee(), Amount::from_msats(1_234_572));
        assert_eq!(quote.total(), Amount::from_msats(501_234_572));
        assert_eq!(quote.expires_at(), Timestamp::from_epoch_millis(5));
        assert_eq!(
            quote.fee_breakdown().wallet_output,
            Amount::from_msats(1_200_000)
        );
    }

    #[test]
    fn the_error_helpers_carry_their_details() {
        match quote_changed(Amount::from_msats(10), Amount::from_msats(12)).detail() {
            Some(ErrorDetails::QuoteTermsChanged {
                quoted_total,
                current_total,
            }) => {
                assert_eq!(*quoted_total, Amount::from_msats(10));
                assert_eq!(*current_total, Amount::from_msats(12));
            }
            other => panic!("expected QuoteTermsChanged, got {other:?}"),
        }
        match insufficient(Amount::from_msats(10), Amount::from_msats(3)).detail() {
            Some(ErrorDetails::InsufficientBalance {
                required,
                available,
            }) => {
                assert_eq!(*required, Amount::from_msats(10));
                assert_eq!(*available, Amount::from_msats(3));
            }
            other => panic!("expected InsufficientBalance, got {other:?}"),
        }
        assert_eq!(
            quote_expired(Timestamp::from_epoch_millis(1), true).code,
            ErrorCode::QuoteExpired
        );
        assert_eq!(unreachable("down").code, ErrorCode::FederationUnreachable);
        assert_eq!(internal("oops").code, ErrorCode::Internal);
        assert_eq!(subscribe_error("closed").code, ErrorCode::Internal);
        assert_eq!(timeout().code, ErrorCode::Timeout);
    }

    #[test]
    fn fee_quote_failure_is_classified_before_either_mint_is_asked() {
        // The v1 mint's typed error wins even when the accompanying text also happens to
        // mention the v2 mint's wording; the typed case is unambiguous and checked first.
        let typed = fedimint_mint_client::InsufficientBalanceError {
            requested_amount: fedimint_core::Amount::from_msats(10),
            total_amount: fedimint_core::Amount::from_msats(3),
        };
        match classify_fee_quote_failure(Some(&typed), "Insufficient funds") {
            Some(FeeQuoteFailure::Typed { requested, total }) => {
                assert_eq!(requested, Amount::from_msats(10));
                assert_eq!(total, Amount::from_msats(3));
            }
            other => panic!("expected the typed case, got {other:?}"),
        }
        // The v2 mint's plain-text refusal, with no typed error at all.
        assert!(matches!(
            classify_fee_quote_failure(None, "Insufficient funds"),
            Some(FeeQuoteFailure::Text)
        ));
        // The v1 mint's wording, wrapped by the client's submission error so no typed error
        // survives the downcast.
        assert!(matches!(
            classify_fee_quote_failure(
                None,
                "primary module: Insufficient balance: requested 1 sat but only 0 sat available"
            ),
            Some(FeeQuoteFailure::Text)
        ));
        // Neither mint's wording: not this crate's problem to interpret.
        assert!(classify_fee_quote_failure(None, "the federation timed out").is_none());
    }

    /// The hand-built claim the defect fix's own worked example uses: a 100 000 000 msat input,
    /// a 1 000 000 msat peg-in fee, and three change notes the mint shaped to hit its target note
    /// count, 67 108 864 / 31 000 000 / 890 000 msat, each charged the same 100 msat fee. Dust is
    /// whatever the notes plus their fees left of the input after the peg-in fee: 99 000 000
    /// available less 98 999 164 actually minted, 836 msat.
    #[test]
    fn figures_of_builds_the_claim_fee_breakdown_from_its_own_outputs() {
        let outputs = [
            (Amount::from_msats(67_108_864), Amount::from_msats(100)),
            (Amount::from_msats(31_000_000), Amount::from_msats(100)),
            (Amount::from_msats(890_000), Amount::from_msats(100)),
        ];
        let (fee, breakdown, net_credit) = figures_of(
            &outputs,
            &[],
            Amount::from_msats(100_000_000),
            Amount::from_msats(1_000_000),
            Amount::from_msats(0),
            Sats::from_sats(100_000),
        )
        .expect("a claim");
        assert_eq!(net_credit, Amount::from_msats(98_998_864));
        assert_eq!(
            breakdown,
            OnchainReceiveFeeBreakdown {
                peg_in: Amount::from_msats(1_000_000),
                network_claim: Amount::from_msats(0),
                primary_module: Amount::from_msats(300),
                dust: Amount::from_msats(836),
            }
        );
        assert_eq!(fee, Amount::from_msats(1_001_136));
    }

    #[test]
    fn figures_of_refuses_a_claim_that_minted_more_than_its_input_allows() {
        let outputs = [(Amount::from_msats(200_000_000), Amount::from_msats(0))];
        let err = figures_of(
            &outputs,
            &[],
            Amount::from_msats(0),
            Amount::from_msats(0),
            Amount::from_msats(0),
            Sats::from_sats(100_000),
        )
        .expect_err("the notes minted exceed the input amount");
        assert_eq!(err.code, ErrorCode::Internal);
    }

    /// A consolidating claim on a funded wallet: one 10 000 msat output (fee 10) minted beside
    /// two mint inputs it melted, 3 000 and 2 000 msat (fee 5 each), on top of an 8 000 msat
    /// wallet input charged a 100 msat peg-in fee. `net_credit` is the output net of what was
    /// melted back (10 000 − 5 000 = 5 000), `primary_module` every mint fee paid regardless of
    /// direction (10 + 5 + 5 = 20), and `dust` what the wallet input's net amount plus the melted
    /// notes leaves once the output and its fee, and the melted notes' own fees, are paid:
    /// (8 000 − 100) + 5 000 − (10 000 + 10) − 10 = 2 880 msat.
    #[test]
    fn figures_of_reads_the_net_movement_of_a_claim_with_mint_inputs() {
        let outputs = [(Amount::from_msats(10_000), Amount::from_msats(10))];
        let mint_inputs = [
            (Amount::from_msats(3_000), Amount::from_msats(5)),
            (Amount::from_msats(2_000), Amount::from_msats(5)),
        ];
        let (fee, breakdown, net_credit) = figures_of(
            &outputs,
            &mint_inputs,
            Amount::from_msats(8_000),
            Amount::from_msats(100),
            Amount::from_msats(0),
            Sats::from_sats(8),
        )
        .expect("a consolidating claim");
        assert_eq!(net_credit, Amount::from_msats(5_000));
        assert_eq!(
            breakdown,
            OnchainReceiveFeeBreakdown {
                peg_in: Amount::from_msats(100),
                network_claim: Amount::from_msats(0),
                primary_module: Amount::from_msats(20),
                dust: Amount::from_msats(2_880),
            }
        );
        assert_eq!(fee, Amount::from_msats(3_000));
    }

    /// A claim transaction with no outputs would otherwise be read as a zero credit with the
    /// whole deposit folded into dust; `check_claim_shape` refuses it before any output is read.
    #[test]
    fn check_claim_shape_refuses_a_claim_with_no_outputs() {
        let err = check_claim_shape(1, 1, 0).expect_err("a claim with no outputs");
        assert_eq!(err.code, ErrorCode::Internal);
    }

    /// A funded wallet's claim consolidating notes: one wallet input plus mint inputs beside it
    /// is a valid shape, not just the single-input case.
    #[test]
    fn check_claim_shape_accepts_one_wallet_input_plus_mint_inputs() {
        check_claim_shape(1, 3, 1).expect("one wallet input plus two mint inputs");
    }

    #[test]
    fn check_claim_shape_refuses_a_claim_with_no_wallet_input() {
        let err = check_claim_shape(0, 2, 1).expect_err("a claim with no wallet input");
        assert_eq!(err.code, ErrorCode::Internal);
    }

    #[test]
    fn check_claim_shape_refuses_a_claim_with_two_wallet_inputs() {
        let err = check_claim_shape(2, 2, 1).expect_err("a claim with two wallet inputs");
        assert_eq!(err.code, ErrorCode::Internal);
    }
}
