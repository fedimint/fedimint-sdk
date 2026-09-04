//! On-chain Bitcoin: deposits into the federation and withdrawals out of
//! it.

use std::sync::Arc;

use crate::{Address, Amount, Operation, OperationState, Result, Sats, Timestamp, Txid};

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
// Implementation notes (delete once implemented):
// - Both the `v1` and `walletv2` `send_fee_quote` are millisatoshi-denominated, which is
//   why the fee and total accessors on this facade return `Amount` rather than `Sats`.
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
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn receive(&self) -> Result<OnchainReceive> {
        // Implementation notes (delete once implemented):
        // - Persist the operation, including the address, in the same storage transaction
        //   that creates it, before returning.
        // - A receive that has not yet seen a transaction must not count toward
        //   `forget_federation`'s pending-operations guard; only `WaitingForConfirmation`
        //   onward should.
        // - Reasoning about a second output paying the same address is a wallet-scanner
        //   detail this contract deliberately does not promise on.
        unimplemented!()
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
        // Implementation notes (delete once implemented):
        // - The network check must run here, before anything is committed, on both wallet
        //   module generations.
        unimplemented!()
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
        // Implementation notes (delete once implemented):
        // - Re-check every bound input of the quote (fee estimate, federation config, note
        //   selection) before funding; any drift is `QuoteChanged`, never a different debit.
        // - Write `OnchainSendDetails` in the same storage transaction that creates the
        //   operation.
        unimplemented!()
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
        unimplemented!()
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
        unimplemented!()
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
        unimplemented!()
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
        unimplemented!()
    }

    /// When this quote stops being executable.
    ///
    /// Past this point [`Onchain::send`] fails with
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired). On-chain quotes
    /// tend to be shorter-lived than lightning ones, because the fee
    /// estimate they carry tracks a moving mempool.
    pub fn expires_at(&self) -> Timestamp {
        unimplemented!()
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
// Implementation notes (delete once implemented):
//
// - The first wallet module's `WithdrawState` has `Created`, `Succeeded(Txid)` and
//   `Failed(String)`; its `Failed` means the funding transaction rejected before anything
//   left the balance, so it maps to `Refunded`, not `Failed`. Payloads become named fields.
// - The second wallet module's send machine has no separate broadcast step: its `Funding`
//   is `Created`, `Success(txid)` is `Succeeded`, `Aborted` (funding rejected, nothing
//   debited) is `Refunded`, and `Failure` (funding accepted, no transaction produced,
//   documented upstream as a programming error or a misbehaving federation) is `Failed`.
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
// Implementation notes (delete once implemented):
//
// This follows upstream `fedimint-wallet-client`'s `DepositStateV2` variant for variant,
// but not payload for payload, on the `v1` wallet module. Under `walletv2` there is no
// per-address state machine to follow: the chain-side phases here are the SDK's own
// observation of the address, and the module's claim machine lands on them as `Funding` ->
// `Confirmed`, `Success` -> `Claimed`, `Aborted` (claim stays claimable, retried under the
// same operation id) -> stays `Confirmed`.
//
// - Only the transaction half of the outpoint is carried; upstream's `v1` variants also
//   carry the vout, which nothing in this API needs.
// - Every state that knows the gross amount reports it (`WaitingForConfirmation`,
//   `Confirmed`, `Claimed`), unlike upstream `v1` which only ever names it
//   `btc_deposited`.
// - `Claimed` additionally reports a net credit this SDK computes; upstream reports only
//   the gross figure.
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
// Implementation notes (delete once implemented):
// - `txid` and `gross_deposited` are `OperationDetails` placement-rule case 3: announced by
//   `WaitingForConfirmation` and the two states after it, and dropped by `Failed`, which can
//   follow a transaction that was already seen and carries nothing but a reason.
// - `net_credit` duplicates the figure `Claimed` carries; that state is final, so nothing
//   drops it. It is here so the record alone completes a receipt without a second call to
//   `Operation::state`.
// - `fee` and `fee_breakdown` are carried by no state at any point, so this record is the
//   only place either can be read.
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
// Implementation notes (delete once implemented):
// - `peg_in` is the `v1` wallet module's flat peg-in fee, or `walletv2`'s consensus input
//   fee on the amount the transaction carries.
// - `network_claim` is zero under `v1`, which claims the full gross and charges only
//   in-transaction fees; it is nonzero only under `walletv2`, which deducts an on-chain
//   consolidation cost from the gross before the federation transaction's amount is formed.
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

/// Placeholder for the wallet-module state this facade operates on.
#[derive(Debug)]
struct OnchainInner;

/// Placeholder for a quote's frozen plan: destination, amount, the fee and
/// its components, and the configuration context they were computed
/// against.
#[derive(Debug)]
struct OnchainQuoteInner;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DetailedOperationState;

    /// The all-zero txid, which is not a real one; these tests never look at
    /// its value, only carry it through a payload.
    fn a_txid() -> Txid {
        Txid::from_raw(
            "0000000000000000000000000000000000000000000000000000000000000000".to_owned(),
        )
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
}
