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
/// This facade mixes the two money types, and the split is not arbitrary: a
/// value is [`Sats`](crate::Sats) when it is a figure that exists on the
/// Bitcoin chain, and [`Amount`](crate::Amount) when it is a figure that
/// exists inside the federation.
///
/// - **Whole satoshis.** The amount that arrives at a withdrawal's
///   destination ([`OnchainQuote::amount`], [`Onchain::quote`]'s `amount`
///   argument, [`OnchainSendDetails::amount`]) and the gross amount a
///   deposit transaction pays in ([`OnchainReceiveState::Claimed`],
///   [`OnchainReceiveDetails::gross_deposited`]). Bitcoin has no
///   sub-satoshi unit, so these genuinely are whole satoshis; typing them
///   as [`Amount`](crate::Amount) would invite a remainder that cannot
///   exist and force every call to decide what to do with it.
/// - **Exact millisatoshis.** Every fee, every total debit, and the net
///   amount a deposit credits to the balance
///   ([`OnchainQuote::fee`], [`OnchainQuote::total`],
///   [`OnchainSendDetails::total_debited`],
///   [`OnchainReceiveState::Claimed`],
///   [`OnchainReceiveDetails::net_credit`]). A peg-out's cost is not just
///   the chain fee for the wallet output: it also covers funding that
///   output from the primary (mint) module and the change and dust that
///   funding leaves behind, and a peg-in's cost is a federation fee taken
///   out of the deposit. Those are quoted and charged in millisatoshis, and
///   their sums are routinely not whole satoshis.
///
/// An earlier draft of this facade declared **everything** here to be whole
/// satoshis. That rule was written before upstream's fee contract was
/// checked, and it does not survive the check: both the v1 and the v2
/// `send_fee_quote` are millisatoshi-denominated, so the rule could only
/// have been honoured by rounding a fee — which understates a debit on an
/// approval screen — or by discarding part of it. The rule is therefore
/// narrowed rather than kept: whole satoshis where a value truly is whole
/// satoshis, exact millisatoshis everywhere a fee is involved.
///
/// What has not changed is that no conversion happens behind a caller's
/// back. Nothing in this facade floors, and moving between the two units is
/// always explicit — [`Sats::to_amount`](crate::Sats::to_amount) upward
/// (exact by construction, one satoshi being exactly 1000 msat) and
/// [`Amount::to_sats_exact`](crate::Amount::to_sats_exact) downward, which
/// refuses rather than truncates.
///
/// # The recovery lock applies to both directions
///
/// Every call on this facade — deposits as much as withdrawals — is refused
/// with [`Recovering`](crate::ErrorCode::Recovering) while this
/// federation's recovery is **incomplete**. "Incomplete" is the operative
/// word and it is wider than "running": an attempt that stopped short holds
/// the lock exactly as firmly as one still in progress, and only a recovery
/// that reaches completion releases it. There is no acknowledge, no
/// override, and no way to spend or receive on a partially restored wallet.
#[derive(Debug, Clone)]
pub struct Onchain {
    inner: Arc<OnchainInner>,
}

impl Onchain {
    /// Hands back a deposit address to fund, and an operation that follows
    /// whatever arrives at it.
    ///
    /// # What this promises
    ///
    /// Every call allocates a **fresh** deposit address — derived from this
    /// instance's seed, never handed out before, and never handed out again
    /// — and commits **one durable operation** for it before returning.
    /// There is no deposit for the wallet module to report until something
    /// pays the address, so what this call creates is the SDK's own record
    /// of the intent: the operation begins in
    /// [`WaitingForTransaction`](OnchainReceiveState::WaitingForTransaction)
    /// and stays there for as long as nobody pays, and when an output paying
    /// the address is detected the same operation adopts it and starts
    /// reporting it. The [`OperationId`](crate::OperationId) does not change
    /// at that moment, so an id persisted from this call stays the right one
    /// to reattach with for the life of the deposit. What an application
    /// must not read into the operation's existence is that a deposit is
    /// under way: only a state past
    /// [`WaitingForTransaction`](OnchainReceiveState::WaitingForTransaction)
    /// says that.
    ///
    /// So an application can rely on this: two calls yield two addresses and
    /// two operations, so a per-payer address can be minted on demand; the
    /// address is watched persistently, so a deposit that arrives while the
    /// application is closed is picked up when the SDK is next built over
    /// the same storage (ordinary [detached-operation](crate::Operation)
    /// behaviour, not a special case); and the address survives a restart,
    /// because it is on the operation's details record.
    ///
    /// # Two outputs paying one address
    ///
    /// This handle follows **one** deposit: the first output detected paying
    /// the address. A second output paying the same address is not reported
    /// by this operation — its states and its details record describe the
    /// first — and this facade does not promise that the second becomes an
    /// operation of its own, appears in
    /// [activity](crate::Federation::activity), or is credited on its own
    /// schedule. Reasoning about what happens to it means reasoning about
    /// the scanner, which is exactly the upstream detail this contract
    /// refuses to promise on.
    ///
    /// The rule that follows is short: **one address, one payer, one
    /// deposit.** Do not hand a deposit address to two people, do not show
    /// it again once it has been funded, and treat anything that does arrive
    /// twice as something to reconcile from
    /// [`Federation::balance`](crate::Federation::balance) and
    /// [activity](crate::Federation::activity) rather than as something this
    /// API tracked on the application's behalf.
    ///
    /// # An unused address never finishes, and must not trap the federation
    ///
    /// [`WaitingForTransaction`](OnchainReceiveState::WaitingForTransaction)
    /// has no timeout, because a Bitcoin address has no expiry. A lightning
    /// invoice lapses and reaches
    /// [`Expired`](crate::LnReceiveState::Expired) by itself; a deposit
    /// address stays fundable indefinitely, so an operation nobody pays
    /// stays non-final indefinitely. There is no cancel, retire, or expire
    /// call for one, and this facade will not offer a "stop watching" that
    /// the wallet client cannot perform — telling an application an address
    /// is dead while funds can still arrive at it is the more dangerous of
    /// the two available lies. Do not await
    /// [`Operation::await_final`](crate::Operation::await_final) on a fresh
    /// deposit expecting it to resolve.
    ///
    /// That leaves one hazard worth closing rather than leaving to be
    /// discovered: a never-funded address must not be able to trap a
    /// federation for good.
    /// [`Sdk::forget_federation`](crate::Sdk::forget_federation) refuses
    /// while non-final operations exist, so on the plain reading a single
    /// address that was displayed once and ignored would make the
    /// destructive erase permanently unreachable — the caller cannot settle
    /// the operation, because the only thing that would settle it is a
    /// stranger deciding to send money. **A receive operation that has not
    /// yet seen a transaction therefore does not count as a pending
    /// operation for that guard.**
    ///
    /// That reads the guard's own principle rather than bending it. Every
    /// eligibility check there protects value the caller could still move if
    /// they did something else first: spend the balance down, let an
    /// operation settle, reclaim outstanding notes. A deposit still in
    /// [`WaitingForTransaction`](OnchainReceiveState::WaitingForTransaction)
    /// has received nothing, so there is no value to protect and nothing the
    /// caller could do first — the same reasoning that keeps a
    /// recovery-locked federation's provisional balance out of the
    /// zero-balance guard. Erasing such a federation forfeits nothing but
    /// the address itself, which the seed can derive again.
    ///
    /// Once a transaction *has* been seen the answer flips, and correctly:
    /// from
    /// [`WaitingForConfirmation`](OnchainReceiveState::WaitingForConfirmation)
    /// onwards there is a real credit in flight, the operation is an
    /// ordinary pending one, and the erase refuses with
    /// [`PendingOperations`](crate::ErrorCode::PendingOperations) until it
    /// reaches [`Claimed`](OnchainReceiveState::Claimed) or
    /// [`Failed`](OnchainReceiveState::Failed).
    ///
    /// # No quote
    ///
    /// There is nothing to quote for a deposit. The sender pays the Bitcoin
    /// network fee out of their own wallet, and the federation's peg-in
    /// terms apply to whatever arrives; the fee those terms take is knowable
    /// only once an amount exists, and it is reported then — see
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
    /// after the funds have moved — parsing an
    /// [`Address`](crate::Address) cannot do this check, because at parse
    /// time there is no federation to compare against.
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
        unimplemented!()
    }

    /// Executes a quoted withdrawal.
    ///
    /// The quote is consumed and executed as quoted — same destination, same
    /// amount, same fee — or the call fails with
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired) if its validity
    /// window has passed, or
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) if the fee estimate
    /// or federation configuration it was built on has moved. In both cases
    /// the remedy is the same: quote again and re-confirm.
    ///
    /// [`OnchainQuote::total`] is the ceiling this call is authorised
    /// against. Nothing beyond it can be debited: a withdrawal that would
    /// now cost more is a
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) refusal, never a
    /// silent overspend of the difference.
    ///
    /// The returned operation reaches [`OnchainSendState::Succeeded`] once
    /// the federation has broadcast the transaction. That is the SDK's
    /// finish line, not the chain's: confirmation of the withdrawal
    /// transaction on the Bitcoin network is the recipient's business, and
    /// the [`Txid`](crate::Txid) in that state is what an application shows
    /// or links to a block explorer. The terms it was executed on stay
    /// readable, however it ends, from
    /// [`OnchainSendDetails`].
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
        unimplemented!()
    }
}

/// A frozen, executable plan for one on-chain withdrawal.
///
/// Produced by [`Onchain::quote`] and consumed by [`Onchain::send`]. As with
/// [`LnQuote`](crate::LnQuote), the accessors expose exactly what a user
/// must approve and nothing else; the plan itself is the SDK's to keep.
///
/// The accessors deliberately do not all speak the same unit — the
/// destination amount is whole [`Sats`](crate::Sats) and the fee and total
/// are millisatoshi [`Amount`](crate::Amount)s. That asymmetry reads as an
/// inconsistency until it is explained, so it is explained twice: on
/// [`Onchain`] as a rule, and on [`OnchainQuote::fee`] as the reason.
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
    /// "Aggregate" is the whole point: this is **every** debit the
    /// withdrawal incurs beyond the destination output, summed with nothing
    /// rounded away — the chain fee for the wallet output the federation
    /// will build, the cost of funding that output from the primary (mint)
    /// module, and the change and dust that funding leaves behind.
    /// [`OnchainQuote::fee_breakdown`] names those parts individually.
    ///
    /// It is an [`Amount`](crate::Amount) rather than
    /// [`Sats`](crate::Sats) because that sum is genuinely not a whole
    /// number of satoshis. Upstream's fee quote is millisatoshi-denominated
    /// on both module generations, and the mint-side components in
    /// particular carry sub-satoshi precision. A satoshi-typed fee could
    /// only be produced by rounding, and rounding a fee **down** on an
    /// approval screen understates what the user is about to pay, which is
    /// the one direction a money figure must never be wrong in.
    ///
    /// Display it as it stands, or round it up. Never round it down, and
    /// never re-express it in satoshis with
    /// [`sats_floor`](crate::Amount::sats_floor);
    /// [`to_sats_exact`](crate::Amount::to_sats_exact) will normally return
    /// `None` here, and that is the type system reporting a real fact rather
    /// than an inconvenience to work around.
    pub fn fee(&self) -> Amount {
        unimplemented!()
    }

    /// The total that will be debited from the balance:
    /// [`OnchainQuote::amount`] converted to millisatoshis, plus
    /// [`OnchainQuote::fee`].
    ///
    /// This is the number to show as "you will pay", and it is exact — the
    /// point of aggregating the fee in millisatoshis is that this figure
    /// does not have to be approximated.
    ///
    /// It is also a **ceiling**, not merely a prediction:
    /// [`Onchain::send`] is authorised for this and no more. A withdrawal
    /// that would cost more than this by the time it executes is refused
    /// with [`QuoteChanged`](crate::ErrorCode::QuoteChanged), so the user
    /// re-approves a new number instead of quietly paying it. That is what
    /// makes it safe to render as a commitment rather than an estimate.
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
/// [`OnchainQuote::fee`] gives: these are federation-side figures and
/// several of them are not whole satoshis. Together they account for the
/// aggregate exactly — the SDK's own invariant is that the components sum to
/// [`OnchainQuote::fee`], with no rounding and no residue.
///
/// # Read the aggregate; use these to explain it
///
/// A caller that needs the number to charge, to compare against a balance,
/// or to put in a receipt should read [`OnchainQuote::fee`] (or
/// [`OnchainQuote::total`]) and not sum these fields. Two reasons:
///
/// - The type is `#[non_exhaustive]`, so a later version may split a
///   component in two or name one that did not exist. The aggregate stays
///   correct across that change; a caller that had hard-coded the sum of
///   the fields it knew about would quietly start understating the fee.
/// - The aggregate is the figure the quote commits to and
///   [`Onchain::send`] is authorised against. Nothing else is.
///
/// So: aggregate for arithmetic, breakdown for explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OnchainSendFeeBreakdown {
    /// What it costs to put the destination output on chain: the
    /// federation's charge for the wallet output it will build, including
    /// its share of the Bitcoin network fee at the feerate the quote was
    /// computed against.
    ///
    /// This is the component a user intuitively expects a withdrawal to
    /// cost, and on its own it is not the whole cost — which is why the
    /// other two fields exist rather than being folded silently into this
    /// one.
    pub wallet_output: Amount,
    /// What it costs to fund that output from the primary (mint) module:
    /// selecting and spending the ecash inputs that pay for the peg-out.
    ///
    /// This is a federation-internal, millisatoshi-denominated cost with no
    /// on-chain counterpart, and it is the component most likely to make
    /// [`OnchainQuote::fee`] a non-whole number of satoshis.
    pub funding: Amount,
    /// What the change from that funding costs: reissuing the remainder as
    /// notes, plus any residue too small to be worth returning and
    /// therefore given up.
    ///
    /// Small, frequently sub-satoshi, and genuinely part of the debit. It is
    /// reported rather than absorbed because a fee that does not reconcile
    /// with the balance movement is worse than a fee with a third line in
    /// it.
    pub change: Amount,
}

/// The result of [`Onchain::receive`]: the address to fund, and the
/// operation tracking the deposit.
///
/// The address is here for convenience, not for safekeeping. It is also
/// persisted on the operation's details record, so an application that has
/// lost this struct — a process restart, a screen rebuilt from an operation
/// id — reads it back with
/// [`Operation::details`](crate::Operation::details) and gets the same
/// address to display or re-encode as a QR code. That is the point of
/// [`OnchainReceiveDetails::address`]: nothing about a deposit needs to be
/// kept by the caller in order to be recoverable.
///
/// Read [`Onchain::receive`] before assuming this address is new. It may be
/// one an earlier call already handed out, and it may be handed out again
/// until it has been used.
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
/// This maps one-to-one onto upstream `fedimint-wallet-client`'s
/// `WithdrawState` (`Created`, `Succeeded(Txid)`, `Failed(String)`); the
/// only change is that the payloads are named fields rather than positional
/// ones, so they cross a foreign-function boundary as records.
///
/// The terms the withdrawal was executed on — destination, amount, fee,
/// total debit — are not here. They belong to what the operation *is*
/// rather than to where it has got to, they are the same in every state,
/// and a receipt has to be renderable for a withdrawal that failed as much
/// as for one that succeeded. They live on [`OnchainSendDetails`].
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
    /// Final: the withdrawal did not happen.
    Failed {
        /// Human-readable explanation. Diagnostic only — not a stable
        /// contract, and not something to match on.
        reason: String,
    },
}

impl crate::operation::sealed::Sealed for OnchainSendState {}

impl OperationState for OnchainSendState {
    fn is_final(&self) -> bool {
        match self {
            OnchainSendState::Created => false,
            OnchainSendState::Succeeded { .. } | OnchainSendState::Failed { .. } => true,
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
/// # Why the units differ inside one record
///
/// [`amount`](OnchainSendDetails::amount) is whole
/// [`Sats`](crate::Sats) — it is an output in a Bitcoin transaction.
/// [`fee`](OnchainSendDetails::fee) and
/// [`total_debited`](OnchainSendDetails::total_debited) are millisatoshi
/// [`Amount`](crate::Amount)s — they are federation-side figures that are
/// not whole satoshis. See the [unit note](Onchain) and
/// [`OnchainQuote::fee`].
///
/// # Why there is no `txid` here
///
/// Because there is exactly one state that has one, and it is final. A
/// broadcast transaction id appears on
/// [`Succeeded`](OnchainSendState::Succeeded), and a final state is sticky:
/// it never transitions again, so
/// [`Operation::state`](crate::Operation::state) returns it for the rest of
/// time and the id on it is already as durable as a record field would be.
/// Copying it here would duplicate a value that cannot be missed — the
/// placement rule's case 2, which reserves duplication
/// ([`OperationDetails`](crate::OperationDetails), case 3) for values a
/// *later* state drops. Nothing about a withdrawal drops the txid, because
/// nothing follows the state that carries it.
///
/// That is the opposite of a deposit, where a transaction is seen well
/// before the operation ends and
/// [`Failed`](OnchainReceiveState::Failed) can follow it carrying nothing —
/// which is exactly why [`OnchainReceiveDetails::txid`] exists.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OnchainSendDetails {
    /// The destination the withdrawal pays.
    ///
    /// The address the quote was built against and bound to, network-checked
    /// at quote time. This is what a receipt shows and what a "sent to"
    /// line reads from after a restart.
    pub address: Address,
    /// The amount arriving at [`address`](OnchainSendDetails::address), in
    /// whole satoshis.
    ///
    /// The counterparty figure: what the recipient receives, gross of this
    /// wallet's fees. Not what left the balance — that is
    /// [`total_debited`](OnchainSendDetails::total_debited).
    pub amount: Sats,
    /// The aggregate fee as quoted, exactly.
    ///
    /// The same figure [`OnchainQuote::fee`] reported and the same one
    /// [`Onchain::send`] was authorised against: wallet output, primary
    /// (mint) funding, change and dust, added up in millisatoshis. Recorded
    /// because it appears nowhere in the withdrawal's progress stream and
    /// cannot be re-derived afterwards — the mempool it was estimated
    /// against has moved on.
    pub fee: Amount,
    /// What the withdrawal debits from the balance: `amount` converted to
    /// millisatoshis plus [`fee`](OnchainSendDetails::fee).
    ///
    /// Stored rather than recomputed on read, for two reasons. It is the
    /// exact number the user approved, and a receipt should show what was
    /// approved rather than a figure reassembled from parts. And
    /// reassembling it means [`Sats::to_amount`](crate::Sats::to_amount),
    /// which is fallible, so every reader would have to handle an overflow
    /// case in order to recover a value that was already known here.
    pub total_debited: Amount,
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
/// This follows upstream `fedimint-wallet-client`'s `DepositStateV2` variant
/// for variant, but not payload for payload. Upstream's variants are
/// `WaitingForTransaction`,
/// `WaitingForConfirmation { btc_deposited, btc_out_point }`,
/// `Confirmed { btc_deposited, btc_out_point }`,
/// `Claimed { btc_deposited, btc_out_point }`, and `Failed(String)` — note
/// that all three of the middle variants carry the same pair, not just
/// `WaitingForConfirmation`. This enum differs from that in three deliberate
/// ways:
///
/// - **Only the transaction half of the outpoint is carried.** Upstream
///   identifies the funding transaction by an outpoint; what is reported
///   here is its transaction half, which is what a receipt or a
///   block-explorer link needs. The vout is dropped because nothing in this
///   API takes one.
/// - **Every state that knows the gross amount reports it.** Upstream's
///   `btc_deposited` is the amount that arrived on chain, before anything
///   the federation charges to claim it, and it is available from the moment a
///   transaction is seen. It is therefore on
///   [`WaitingForConfirmation`](Self::WaitingForConfirmation),
///   [`Confirmed`](Self::Confirmed) and [`Claimed`](Self::Claimed) alike,
///   in whole [`Sats`](crate::Sats) — an on-chain output cannot hold a
///   fraction of one.
/// - **[`Claimed`](Self::Claimed) also reports a net figure this SDK
///   computes.** The amount actually credited to the balance — deposited
///   less the aggregate claim fee — is the number a user sees their balance
///   move by,
///   and upstream never reports it. It is an
///   [`Amount`](crate::Amount) rather than [`Sats`](crate::Sats) because
///   the fee is charged in millisatoshis and can leave the credit with
///   sub-satoshi precision; see the [unit note](Onchain).
///
/// # The final state is self-contained
///
/// [`Claimed`](Self::Claimed) carries the funding transaction, the gross
/// amount that arrived, and the net amount credited, and that is not
/// redundancy. A subscription yields the state an operation is in *now* and
/// never replays the ones before it, so an application that reattaches to a
/// deposit by id — after a restart, from an activity row, from a
/// notification — may see [`Claimed`](Self::Claimed) as the very first state
/// it is ever shown. If the final state named only the credit, that
/// application could not render a receipt at all: it never saw the txid, and
/// the gross amount was nowhere to be found. It now can, from the current
/// state alone.
///
/// The one state that is deliberately not self-contained is
/// [`Failed`](Self::Failed), which carries only a diagnostic reason even
/// though a deposit can fail after its transaction was seen. That is what
/// [`OnchainReceiveDetails`] is for: the address, the transaction, the gross
/// amount, the fee and the net credit are all on the details record too, and
/// between that record and the current state an application never needs to
/// have seen an earlier one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OnchainReceiveState {
    /// The address is being watched and no transaction paying it has been
    /// seen yet.
    ///
    /// A deposit can sit here indefinitely — until someone sends, there is
    /// nothing to report — and there is no call that ends it; see
    /// [`Onchain::receive`] for what that means for
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
    /// Self-contained on purpose — see the enum's own documentation. A
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
        /// Computed by the SDK — upstream reports only the gross figure —
        /// from the fees the federation charged on the claim. Those are more
        /// than the wallet module's peg-in fee, which is why `gross_deposited`
        /// minus a peg-in fee does not reproduce this number;
        /// [`OnchainReceiveDetails::fee`] is the aggregate it is computed from
        /// and [`OnchainReceiveDetails::fee_breakdown`] names the parts.
        /// Denominated in millisatoshis, because those fees are, so the credit
        /// need not be a whole number of satoshis. This is the number the
        /// balance moved by.
        net_credit: Amount,
    },
    /// Final: the deposit could not be claimed.
    ///
    /// Carries no transaction and no amount even when one was seen. What
    /// arrived, and what it was going to cost, are on
    /// [`OnchainReceiveDetails`], which is where a caller that only ever
    /// saw this state reads them.
    Failed {
        /// Human-readable explanation. Diagnostic only — not a stable
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

/// What an on-chain deposit *is*: the address to display, and the facts
/// about the funding transaction as they become known.
///
/// Read with [`Operation::details`](crate::Operation::details) on an
/// `Operation<OnchainReceiveState>`. The record is committed in the same
/// storage transaction that creates the operation, so it is readable from
/// the moment [`Onchain::receive`] returns.
///
/// # The address is the fix this record exists for
///
/// A deposit's one indispensable artifact is the address, and no state
/// carries it. Before this record, an application that lost the
/// [`OnchainReceive`] it was handed — a process restart, a screen rebuilt
/// from an operation id — had no way to show the user where to send: the
/// state stream cannot supply it, because the address was never a state, and
/// a subscription is not a replay. Now
/// [`address`](OnchainReceiveDetails::address) is a persisted field, and an
/// operation id is genuinely enough to re-render the QR code.
///
/// # Why four fields are optional, and what a caller can count on
///
/// All four are the placement rule's case 3
/// ([`OperationDetails`](crate::OperationDetails)): a fact that comes into
/// existence at a transition and is *not* carried by every state after it,
/// so the state announces it and this record keeps it. It takes that shape
/// twice over here, for two different reasons:
///
/// - [`txid`](OnchainReceiveDetails::txid),
///   [`gross_deposited`](OnchainReceiveDetails::gross_deposited) and
///   [`net_credit`](OnchainReceiveDetails::net_credit) are each announced by
///   a state and dropped by [`Failed`](OnchainReceiveState::Failed), which
///   can follow a transaction that was already seen and carries nothing but
///   a reason. A deposit that arrived and then could not be claimed is
///   precisely the one an application has to be able to describe, so the
///   record keeps what that state does not.
/// - [`fee`](OnchainReceiveDetails::fee) and
///   [`fee_breakdown`](OnchainReceiveDetails::fee_breakdown) duplicate
///   nothing at all: no state carries what the claim cost at any point, so
///   this record is the only place either can be read and no amount of
///   watching would recover them.
///
/// The guarantee on all five is the same, and it is what makes them safe to
/// read at any time: each goes from `None` to `Some` exactly once, in the
/// same write that records the transition establishing it, and never changes
/// to a different value and never reverts. So a caller need not order this
/// call against [`Operation::state`](crate::Operation::state), and reading
/// the record twice cannot produce two contradictory receipts.
///
/// `None` means "not established yet", never "lost". A deposit still in
/// [`WaitingForTransaction`](OnchainReceiveState::WaitingForTransaction) has
/// all five absent, which is simply the truth: nobody has paid.
///
/// # The aggregate, and the arithmetic these fields satisfy
///
/// [`fee`](OnchainReceiveDetails::fee) is the **aggregate** of everything
/// claiming the deposit cost, and it is deliberately not the wallet module's
/// peg-in fee on its own. Claiming a deposit balances the wallet input into
/// primary-module outputs; the primary module's fees on those outputs, and
/// the denomination dust the split leaves behind, reduce the credit exactly
/// as the peg-in fee does. So the identity is:
/// [`gross_deposited`](OnchainReceiveDetails::gross_deposited) in
/// millisatoshis, less [`fee`](OnchainReceiveDetails::fee), equals
/// [`net_credit`](OnchainReceiveDetails::net_credit), which is the same
/// value [`Claimed`](OnchainReceiveState::Claimed) reports. It is **not**
/// gross less a peg-in fee; that subtraction does not in general equal the
/// balance movement, which is why the field is the aggregate.
///
/// The aggregate is authoritative and is the figure to read;
/// [`fee_breakdown`](OnchainReceiveDetails::fee_breakdown) names its parts,
/// the peg-in fee among them, for a screen that wants to explain the
/// difference rather than merely state it. The fee is recorded rather than
/// left to be derived so that a receipt does not have to do fallible
/// arithmetic to name the one number a user asks about when their balance
/// moved by less than the sender sent.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OnchainReceiveDetails {
    /// The deposit address this operation watches.
    ///
    /// Fixed when the operation was created and never changes. Display it,
    /// encode it as a QR code, hand it to a sender — this is the field that
    /// makes an operation id sufficient to rebuild a deposit screen after a
    /// restart.
    ///
    /// Fresh for this operation: never handed out before it, and never
    /// handed out again; see [`Onchain::receive`].
    pub address: Address,
    /// The funding transaction, once one paying the address has been seen.
    ///
    /// `None` until then. Filled in when the deposit reaches
    /// [`WaitingForConfirmation`](OnchainReceiveState::WaitingForConfirmation)
    /// and never changed afterwards — including if the deposit then
    /// [`Failed`](OnchainReceiveState::Failed), which is the case this field
    /// exists for: that state carries no transaction, and a deposit that
    /// arrived and could not be claimed is precisely the one an application
    /// needs to be able to name.
    ///
    /// This tracks the **first** output detected at the address. It does not
    /// become a second transaction if a second one pays the same address;
    /// see [`Onchain::receive`].
    pub txid: Option<Txid>,
    /// The gross amount that arrived on chain, before anything the
    /// federation charges to claim it.
    ///
    /// Whole [`Sats`](crate::Sats): it is the value of an output in the
    /// funding transaction. `None` until a transaction is seen, then fixed.
    ///
    /// This is the counterparty figure — what the sender sent — and it is
    /// the number to show beside the credit when a user asks why the two
    /// differ.
    pub gross_deposited: Option<Sats>,
    /// The aggregate of everything the federation charged to bring this
    /// deposit into the balance, once the claim has settled.
    ///
    /// Not the wallet module's peg-in fee on its own: claiming a deposit
    /// balances the wallet input into primary-module outputs, and the primary
    /// module's fees and the denomination dust the split leaves behind reduce
    /// the credit exactly as the peg-in fee does. This field is the sum of all
    /// of it, which makes it the figure to read and the figure
    /// [`net_credit`](OnchainReceiveDetails::net_credit) is computed from;
    /// [`fee_breakdown`](OnchainReceiveDetails::fee_breakdown) names the
    /// parts.
    ///
    /// `None` until then, and absent from every state at every point, which
    /// makes this record its only home: a caller cannot recover it by
    /// watching, however carefully. Millisatoshi-denominated, like every
    /// other fee in this facade.
    pub fee: Option<Amount>,
    /// [`fee`](OnchainReceiveDetails::fee), split into the named parts it is
    /// made of.
    ///
    /// `Some` exactly when the aggregate is, set in the same write, and
    /// re-reporting the same money rather than an additional charge. It
    /// exists so that "the sender sent 100 000 sat and my balance went up by
    /// less" can be answered with the peg-in fee named separately from the
    /// mint-side costs. The aggregate stays authoritative; see
    /// [`OnchainReceiveFeeBreakdown`] for why a caller should not re-derive
    /// it by summing these.
    pub fee_breakdown: Option<OnchainReceiveFeeBreakdown>,
    /// The amount credited to the balance: `gross_deposited` in
    /// millisatoshis less [`fee`](OnchainReceiveDetails::fee) — the
    /// aggregate, not a peg-in fee alone.
    ///
    /// `None` until the claim completes. Equal to the
    /// [`Claimed`](OnchainReceiveState::Claimed) state's own net figure —
    /// the same value in both places, so a receipt built from the record and
    /// one built from the state cannot disagree — and an
    /// [`Amount`](crate::Amount) for the same reason it is one there: the
    /// fee deducted is millisatoshi-denominated, so the credit need not be a
    /// whole number of satoshis.
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
/// [`OnchainQuote::fee`] gives on the withdrawal side: these are
/// federation-side figures and several of them are not whole satoshis.
/// Together they account for the aggregate exactly — the SDK's own invariant
/// is that the components sum to [`OnchainReceiveDetails::fee`], with no
/// rounding and no residue.
///
/// Unlike [`OnchainSendFeeBreakdown`], which explains a quote, this explains
/// an outcome: the parts are what the claim was charged, not a prediction.
///
/// # Read the aggregate; use these to explain it
///
/// The type is `#[non_exhaustive]`, so a later version may split a component
/// in two or name one that did not exist, and a caller that had hard-coded
/// the sum of the fields it knew about would quietly start understating what
/// the deposit cost. And the aggregate is the figure
/// [`OnchainReceiveDetails::net_credit`] was actually computed from, so it is
/// the only one guaranteed to reconcile with the balance movement.
///
/// So: aggregate for arithmetic, breakdown for explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OnchainReceiveFeeBreakdown {
    /// The wallet module's own charge for accepting the peg-in: the first
    /// wallet module's flat peg-in fee, or the second's consensus input fee
    /// on the amount the transaction carries.
    ///
    /// The component a user means by "the federation's deposit fee". On its
    /// own it is not what reduced the credit, which is exactly why the other
    /// fields exist rather than being folded silently into this one.
    pub peg_in: Amount,
    /// What sweeping the deposit costs on the Bitcoin network: the on-chain
    /// consolidation cost the second wallet module deducts from the gross
    /// before the federation transaction's amount is formed. Zero under the
    /// first wallet module, which claims the full gross and charges only
    /// in-transaction fees.
    pub network_claim: Amount,
    /// What it costs to turn the peg-in into spendable notes: everything the
    /// primary (mint) module charged on the transaction that balances the
    /// wallet input into notes. A federation-internal,
    /// millisatoshi-denominated cost with no on-chain counterpart, and the
    /// component most likely to make the aggregate a non-whole number of
    /// satoshis.
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
/// against. Held by value rather than behind an `Arc`, because a quote is
/// owned by one caller and consumed once, never shared.
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

    fn an_address() -> Address {
        Address::from_raw("bcrt1qexampleexampleexampleexampleexampleex".to_owned())
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

    /// The whole point of item 1: a caller holding only `Claimed` can name
    /// the transaction, the gross and the credit, with no earlier state.
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
            total_debited: amount
                .to_amount()
                .expect("25 000 sat is representable in msat")
                .checked_add(fee)
                .expect("no overflow at this magnitude"),
            created_at: Timestamp::from_epoch_millis(1),
        };
        assert_eq!(details.total_debited, Amount::from_msats(26_234_567));
        // The reason the fee and the total are `Amount`s: neither is a whole
        // number of satoshis, so a satoshi-typed accessor would have had to
        // round the debit down.
        assert_eq!(details.fee.to_sats_exact(), None);
        assert_eq!(details.total_debited.to_sats_exact(), None);
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
            total_debited: Amount::from_msats(1_001),
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
