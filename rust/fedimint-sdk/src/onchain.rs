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
///   [`OnchainSendDetails::quoted_total_debited`],
///   [`OnchainSendDetails::realized_total_debited`],
///   [`OnchainReceiveState::Claimed`],
///   [`OnchainReceiveDetails::realized_net_credit`]). A peg-out's cost is
///   not just the chain fee for the wallet output: it also covers funding
///   that output from the primary (mint) module and the change and dust
///   that funding leaves behind. Nor is a peg-in's cost only the wallet
///   module's peg-in fee: claiming a deposit balances the wallet input into
///   primary-module outputs, whose fees and denomination dust reduce
///   what reaches the balance just as the peg-in fee does. Those figures
///   are quoted and charged in millisatoshis, and their sums are routinely
///   not whole satoshis.
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
/// # Quoted terms and realized outcomes
///
/// Every money figure this facade reports is one of two different kinds of
/// fact, and conflating them is how a receipt comes to assert something that
/// never happened. The records here separate them by name, and every field
/// says which half it belongs to.
///
/// - **Quoted** — everything [`OnchainQuote`] exposes, and every
///   `quoted_`-prefixed field. Fixed when [`Onchain::quote`] ran, never
///   optional (an attempt always had terms), and a description of the
///   attempt rather than of the outcome. This is what the user approved.
/// - **Realized** — every `realized_`-prefixed field. `Option`, absent until
///   the operation settles, then set exactly once from the fees the
///   federation recorded against the transaction it accepted. This is what
///   the balance actually did.
///
/// The two can differ, and neither is redundant. A withdrawal's
/// federation-side costs — mint inputs, change, denomination dust — are
/// chosen when the transaction is assembled and accepted, not when it is
/// quoted, and that gap cannot be closed from inside this SDK;
/// [`OnchainQuote::total`] sets out exactly why. So show the quoted figure
/// before the user commits, because it is the number they are approving, and
/// show the realized figure on the receipt afterwards, because it is the
/// number their balance moved by. For an operation that failed, the realized
/// figure may be zero or absent entirely.
///
/// A deposit has no quoted half at all — there is no quote for one — so
/// every money figure on [`OnchainReceiveDetails`] is realized by nature.
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
    /// # What this promises, and what it deliberately does not
    ///
    /// This is narrower than "allocate a fresh address per call", because
    /// the published wallet client underneath does not do that. What it
    /// returns is the **current highest unused** deposit address: an address
    /// stops being handed out once the scanner has observed its use, and not
    /// a moment before. Two consequences follow, and an application has to
    /// be built for both.
    ///
    /// - **Repeated calls may return the same address.** Call this twice
    ///   before anyone pays the first address and the second call can hand
    ///   back the very same address. In that window the call is idempotent
    ///   rather than merely repetitive: the same address comes back with the
    ///   same operation and the same
    ///   [`OperationId`](crate::OperationId), so two screens cannot end up
    ///   with two operations racing to describe one deposit. A different
    ///   address arrives only after the previous one has been used.
    /// - **Upstream's deposit operation appears only once an output is
    ///   detected; the handle returned here does not wait for it.** There is
    ///   no deposit for the wallet client to report until something pays the
    ///   address, so until then there is no transaction id, no amount and no
    ///   confirmation count anywhere to be had. What this call creates and
    ///   commits is the SDK's own record of the *intent* — that is what makes
    ///   the address re-readable after a restart through
    ///   [`Operation::details`](crate::Operation::details) — and it begins in
    ///   [`WaitingForTransaction`](OnchainReceiveState::WaitingForTransaction)
    ///   and stays there for as long as nobody pays. When a paying output is
    ///   detected, that record adopts the deposit and starts reporting it;
    ///   the [`OperationId`](crate::OperationId) does not change at that
    ///   moment, so an id persisted from this call stays the right one to
    ///   reattach with. What an application must not read into the
    ///   operation's existence is that a deposit is under way: only a state
    ///   past
    ///   [`WaitingForTransaction`](OnchainReceiveState::WaitingForTransaction)
    ///   says that.
    ///
    /// So an application **can** rely on this: the address is a real
    /// deposit address for this federation, derived from this instance's
    /// seed; it is watched persistently, so a deposit that arrives while the
    /// application is closed is picked up when the SDK is next built over
    /// the same storage (ordinary
    /// [detached-operation](crate::Operation) behaviour, not a special
    /// case); and the address survives a restart, because it is on the
    /// operation's details record.
    ///
    /// It **cannot** rely on this: that the address has never been shown
    /// before, that two calls yield two addresses, or that a per-payer
    /// address can be minted on demand. An application that wants one
    /// address per payer has to wait for each address to be used before
    /// asking for the next, and must be prepared for "used" to take as long
    /// as the payer takes.
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
    /// network fee out of their own wallet, and what the federation charges
    /// to bring the deposit into the balance is knowable only once there is
    /// an amount to claim and an accepted transaction that claims it. It is
    /// reported then, as a realized figure and never a quoted one — see
    /// [`OnchainReceiveDetails::realized_fee`], and note that it is the
    /// aggregate of every federation-side cost rather than the wallet
    /// module's peg-in fee alone.
    ///
    /// # The returned operation id outlives the attempts underneath it
    ///
    /// The id on the returned [`OnchainReceive`] is the SDK's own, and it is
    /// stable for the life of the deposit — from the address being displayed
    /// to the credit landing — no matter how many upstream operations the
    /// federation's wallet module runs to get there. Under the second wallet
    /// module that is more than one: a claim attempt can be aborted and the
    /// same output claimed again under a different upstream operation. See
    /// [`OnchainReceiveState`] for what that does to the state machine.
    ///
    /// The correlation that makes the id stable is built in two stages,
    /// because its two halves do not exist at the same time. When this call
    /// creates the operation, no funding output exists yet — nobody has
    /// paid — so the only key there is to persist is the address, and it is
    /// persisted to the operation id in the same storage transaction that
    /// creates the operation. The output half is bound later: when the
    /// scanner first observes an output paying that address, the
    /// implementation atomically records that outpoint against the same
    /// operation id, and from then on resolves every upstream attempt for
    /// that output back through it. Two properties follow that a caller may
    /// rely on: an operation id obtained here never stops resolving because
    /// the attempt behind it was abandoned, and the same output never
    /// surfaces as two operations. This is also why the address is a
    /// persisted field on [`OnchainReceiveDetails`] rather than a value only
    /// the returned handle knows — it is half of the correlation key, not
    /// just something to render.
    ///
    /// One funded case falls outside what the state machine can report
    /// under the second wallet module, and it is stated rather than hidden:
    /// an output too small to cover the claim's own fees is skipped by the
    /// upstream scanner, which advances past it without recording any
    /// persistent event. Nothing remains for the implementation to observe,
    /// so the operation stays in
    /// [`WaitingForTransaction`](OnchainReceiveState::WaitingForTransaction)
    /// — indistinguishable from an address nobody has paid, even though
    /// coins arrived and will not be claimed — and, like any unpaid
    /// receive, it does not block
    /// [`forget_federation`](crate::Sdk::forget_federation). Mapping this
    /// case to a terminal [`Failed`](OnchainReceiveState::Failed) needs an
    /// upstream persistent skipped-output event to key on, and that event
    /// is a named prerequisite this API documents rather than pretends to
    /// have.
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
    /// estimable after the SDK has worked out how the federation would build
    /// and broadcast the transaction. The returned [`OnchainQuote`] fixes
    /// the destination address and the amount, names the aggregate fee and
    /// total debit it computed for them, and records the federation
    /// configuration those were computed against; [`Onchain::send`] executes
    /// that plan or refuses it.
    ///
    /// The fee and total it names are **quoted** figures — what the
    /// withdrawal is expected to cost, and what the user approves — not a
    /// bound on what will be debited. [`OnchainQuote::total`] explains where
    /// the gap comes from and why this SDK cannot close it; what the
    /// withdrawal actually cost is reported afterwards on
    /// [`OnchainSendDetails::realized_total_debited`].
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
    /// the balance cannot cover the quoted [`OnchainQuote::total`],
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
    /// amount, same quoted fee — or the call fails with
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired) if its validity
    /// window has passed, or
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) if the fee estimate
    /// or federation configuration it was built on has moved. In both cases
    /// the remedy is the same: quote again and re-confirm. That refusal is
    /// worth having: a user is not charged against terms they never saw.
    ///
    /// That refusal is **not** a spending ceiling, though an earlier draft of
    /// this documentation said it was. [`OnchainQuote::total`] is the debit
    /// this withdrawal was quoted at, not a maximum this call is authorised
    /// against: published Fedimint offers no way to bind a total inside the
    /// transaction that finally commits, so the realized debit can land above
    /// the quoted one and refusing a visibly stale quote does not stop it.
    /// [`OnchainQuote::total`] gives the mechanism in full and names what
    /// upstream would have to add before a ceiling could be promised.
    ///
    /// What this call does instead is record what happened:
    /// [`OnchainSendDetails::realized_total_debited`] is the debit the
    /// balance actually took, and it is what a "you paid" line must read
    /// from. A [`QuoteChanged`](crate::ErrorCode::QuoteChanged) refusal is
    /// not the only signal a caller gets about a moving fee, and it was
    /// never sufficient as one.
    ///
    /// The returned operation reaches [`OnchainSendState::Succeeded`] once
    /// the federation has broadcast the transaction. That is the SDK's
    /// finish line, not the chain's: confirmation of the withdrawal
    /// transaction on the Bitcoin network is the recipient's business, and
    /// the [`Txid`](crate::Txid) in that state is what an application shows
    /// or links to a block explorer. The terms it was quoted on, and what it
    /// finally cost, stay readable however it ends, from
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
/// Everything here is a **quoted** figure: fixed when the quote ran, and a
/// prediction of what the withdrawal will cost rather than a measurement of
/// what it did — see the
/// [convention](Onchain#quoted-terms-and-realized-outcomes) and, for the
/// reason the distinction cannot be engineered away,
/// [`OnchainQuote::total`]. The realized counterparts live on
/// [`OnchainSendDetails`].
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
    /// This is *not* what leaves the balance: the quoted debit is
    /// [`OnchainQuote::total`], and what the balance finally paid is
    /// [`OnchainSendDetails::realized_total_debited`].
    pub fn amount(&self) -> Sats {
        unimplemented!()
    }

    /// The quoted aggregate cost of this withdrawal, over and above
    /// [`OnchainQuote::amount`].
    ///
    /// "Aggregate" is half the point: this is **every** debit the withdrawal
    /// is expected to incur beyond the destination output, summed with
    /// nothing rounded away — the chain fee for the wallet output the
    /// federation will build, the cost of funding that output from the
    /// primary (mint) module, and the change and dust that funding leaves
    /// behind. [`OnchainQuote::fee_breakdown`] names those parts
    /// individually.
    ///
    /// "Quoted" is the other half. The mint-side components are chosen when
    /// the transaction is assembled, which happens after this quote has been
    /// discarded, so this is the figure the user approves and not a
    /// measurement of what the withdrawal cost; see [`OnchainQuote::total`]
    /// for the mechanism. What it actually cost is
    /// [`OnchainSendDetails::realized_fee`], and that is the figure a
    /// receipt shows.
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

    /// The total this withdrawal is quoted at:
    /// [`OnchainQuote::amount`] converted to millisatoshis, plus
    /// [`OnchainQuote::fee`].
    ///
    /// This is the number to show as "you will pay" on an approval screen,
    /// and it is exact in the sense that matters there — the point of
    /// aggregating the fee in millisatoshis is that the figure the user says
    /// yes to does not have to be approximated.
    ///
    /// # It is an estimate, not an enforced ceiling
    ///
    /// An earlier draft of this API called this a ceiling that
    /// [`Onchain::send`] was authorised against and could not exceed. That
    /// claim is **retracted**: published Fedimint cannot enforce it, and no
    /// amount of care inside this SDK can supply the enforcement from
    /// outside. The mechanism is worth stating precisely, because its shape
    /// is what decides whether it can be worked around.
    ///
    /// - Quoting runs the primary module's input selection inside a
    ///   transaction that is explicitly **non-committable**, and then throws
    ///   that transaction away. It is a dry run by construction: nothing
    ///   about the selection it made survives it.
    /// - Executing later assembles and submits a **different**, committable
    ///   transaction, and that path takes no expected-total or maximum-total
    ///   argument. Handing it the quoted chain feerate binds the
    ///   wallet-output component; the mint input fees, the change fees and
    ///   the denomination dust are chosen at that moment and can differ from
    ///   the ones the discarded transaction implied.
    /// - Re-checking the fee immediately before submitting does not close
    ///   the gap, it only narrows the window. Between the check and the
    ///   commit the terms can still move — a time-of-check to time-of-use
    ///   race — and a check that leaves a race is not a guarantee. Saying so
    ///   is more useful than implying one.
    ///
    /// What would turn this into a real ceiling is an upstream change, not an
    /// SDK one: either an atomic maximum-total (or expected-fee) guard
    /// *inside* transaction finalization, so that assembly itself refuses to
    /// exceed a figure the caller named, or a persisted fee reservation with
    /// defined drop, expiry and restart semantics, so that the terms quoted
    /// are the terms held. Either is a prerequisite this API is documenting
    /// rather than pretending to have.
    ///
    /// # What a caller gets instead
    ///
    /// Two things, and between them they cover the honest cases.
    ///
    /// [`Onchain::send`] still refuses a quote whose terms have visibly
    /// moved, with [`QuoteChanged`](crate::ErrorCode::QuoteChanged), so a
    /// stale quote is never executed silently. That is a genuine protection
    /// against staleness; it is not a bound on the commit.
    ///
    /// And the receipt reports the truth.
    /// [`OnchainSendDetails::realized_total_debited`] is what the balance
    /// actually paid, recorded from the accepted transaction's own fees, and
    /// it is what a "you paid" line must read from. A caller that renders
    /// this quoted total after the fact will eventually render a number that
    /// is not what happened.
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
    /// It is a breakdown of the **quoted** fee, and the split is as
    /// provisional as the total it explains. The aggregate remains the figure
    /// to charge and to compare against a balance; see
    /// [`OnchainSendFeeBreakdown`] for why a caller should not re-derive it
    /// by summing.
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
/// # This explains a quote, not an outcome
///
/// These are quoted components, and they inherit everything
/// [`OnchainQuote::total`] says about quoted figures: the two mint-side
/// lines in particular are re-decided when the transaction is assembled. A
/// withdrawal's realized cost is reported as a single aggregate on
/// [`OnchainSendDetails::realized_fee`] and is deliberately not broken down
/// this way — the accepted transaction's cost is recorded as one figure, and
/// splitting it along these lines after the fact would be presenting a guess
/// as a measurement.
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
/// - The aggregate is the figure the quote is executed on, and the one
///   [`Onchain::send`] compares against when it decides whether the terms
///   have moved. A component is an explanation; it is not an authorisation,
///   and neither is the aggregate.
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
    /// Small, frequently sub-satoshi, and genuinely part of the quoted debit.
    /// It is reported rather than absorbed because a fee whose parts do not
    /// add up to the number being charged is worse than a fee with a third
    /// line in it.
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
/// No variant carries a money figure at any point, and that is upstream's
/// shape rather than a simplification of it: the terms the withdrawal was
/// quoted on — destination, amount, fee, total debit — and what it finally
/// cost are both absent from `WithdrawState`. They belong to what the
/// operation *is* rather than to where it has got to, and a receipt has to
/// be renderable for a withdrawal that failed as much as for one that
/// succeeded. They live on [`OnchainSendDetails`], quoted and realized
/// halves alike, which is therefore the only place either can be read.
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

/// What an on-chain withdrawal *is*: the destination, the terms it was quoted
/// at, and what it finally cost.
///
/// Read with [`Operation::details`](crate::Operation::details) on an
/// `Operation<OnchainSendState>`. The destination, the amount and the quoted
/// half are fixed by the executed [`OnchainQuote`] and committed in the same
/// storage transaction that creates the operation, so they are readable from
/// the first moment [`Onchain::send`] returns, survive a restart, and read
/// the same however the withdrawal ends. That last part matters: a withdrawal
/// that failed has a destination and a quoted fee just as a successful one
/// does, and a receipt that can only be produced for successes is not a
/// receipt.
///
/// # Two halves: what was approved, and what happened
///
/// [`quoted_fee`](OnchainSendDetails::quoted_fee) and
/// [`quoted_total_debited`](OnchainSendDetails::quoted_total_debited) are the
/// terms the user approved.
/// [`realized_fee`](OnchainSendDetails::realized_fee) and
/// [`realized_total_debited`](OnchainSendDetails::realized_total_debited) are
/// what the balance actually did, absent until the withdrawal settles. Both
/// halves are here because the quoted pair is an estimate that
/// [`Onchain::send`] cannot bind — see [`OnchainQuote::total`] for why — and
/// a receipt that shows an estimate is not a receipt either. The
/// [convention](Onchain#quoted-terms-and-realized-outcomes) is the same in
/// every details record in this crate.
///
/// Neither realized field duplicates a state's payload: no variant of
/// [`OnchainSendState`] carries a money figure, so this record is the only
/// place either can be read and no amount of watching the operation would
/// recover them. Each goes from `None` to `Some` exactly once, in the write
/// that records the settling transition, and never changes afterwards — so
/// reading this record twice cannot produce two contradictory receipts, and a
/// caller need not order the read against
/// [`Operation::state`](crate::Operation::state).
///
/// # Why the units differ inside one record
///
/// [`amount`](OnchainSendDetails::amount) is whole
/// [`Sats`](crate::Sats) — it is an output in a Bitcoin transaction. The two
/// fees and the two totals are millisatoshi
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
    /// [`realized_total_debited`](OnchainSendDetails::realized_total_debited),
    /// or before it settles the estimate in
    /// [`quoted_total_debited`](OnchainSendDetails::quoted_total_debited).
    ///
    /// Neither quoted nor realized: this is a term of the withdrawal that
    /// [`Onchain::send`] does bind, so it needs no half.
    pub amount: Sats,
    /// **Quoted.** The aggregate fee the executed quote named, exactly.
    ///
    /// The same figure [`OnchainQuote::fee`] reported and the number the user
    /// approved: wallet output, primary (mint) funding, change and dust,
    /// added up in millisatoshis. Recorded because it appears nowhere in the
    /// withdrawal's progress stream and cannot be re-derived afterwards — the
    /// mempool it was estimated against has moved on.
    ///
    /// What the withdrawal was *expected* to cost, not a measurement of what
    /// it did and not a ceiling it was held to; compare
    /// [`realized_fee`](OnchainSendDetails::realized_fee).
    pub quoted_fee: Amount,
    /// **Quoted.** What the withdrawal was quoted to debit from the balance:
    /// [`amount`](OnchainSendDetails::amount) converted to millisatoshis plus
    /// [`quoted_fee`](OnchainSendDetails::quoted_fee).
    ///
    /// Stored rather than recomputed on read, for two reasons. It is the
    /// exact number the user approved, and a receipt should show what was
    /// approved rather than a figure reassembled from parts. And
    /// reassembling it means [`Sats::to_amount`](crate::Sats::to_amount),
    /// which is fallible, so every reader would have to handle an overflow
    /// case in order to recover a value that was already known here.
    ///
    /// It is not a ceiling — [`OnchainQuote::total`] retracts that claim and
    /// explains why it cannot be made — so it answers "what did you agree
    /// to", never "what did you pay".
    pub quoted_total_debited: Amount,
    /// **Realized.** What the withdrawal actually cost, once the federation
    /// has accepted the transaction that performed it.
    ///
    /// `None` until the withdrawal settles, then set once from the fees the
    /// federation recorded against the accepted transaction — the aggregate
    /// of all of them, on the same terms
    /// [`quoted_fee`](OnchainSendDetails::quoted_fee) aggregates its
    /// components, so the two are directly comparable figures.
    ///
    /// It can differ from the quoted fee in either direction, because the
    /// mint-side components are chosen at assembly time and the quote's own
    /// selection was discarded; [`OnchainQuote::total`] gives the mechanism.
    /// For a withdrawal that ended in [`Failed`](OnchainSendState::Failed)
    /// this may be zero, or stay `None` when nothing was ever accepted to
    /// charge for. Absent means "never settled", never "lost".
    ///
    /// Reported as one aggregate with no component split, unlike
    /// [`OnchainQuote::fee_breakdown`]: the accepted transaction's cost is
    /// recorded as a single figure, and inventing a breakdown for it would be
    /// presenting a guess as a measurement.
    pub realized_fee: Option<Amount>,
    /// **Realized.** What the balance actually paid:
    /// [`amount`](OnchainSendDetails::amount) in millisatoshis plus
    /// [`realized_fee`](OnchainSendDetails::realized_fee).
    ///
    /// The figure a "you paid" line must read from. Refusing a visibly stale
    /// quote with [`QuoteChanged`](crate::ErrorCode::QuoteChanged) happens
    /// before execution and is not a bound on the commit, so
    /// [`quoted_total_debited`](OnchainSendDetails::quoted_total_debited) is
    /// not the final word on what a withdrawal cost — this is.
    ///
    /// `None` until the withdrawal settles, and written in the same storage
    /// transaction as [`realized_fee`](OnchainSendDetails::realized_fee) so
    /// the two can never disagree. Stored rather than derived on read for the
    /// same fallible-arithmetic reason the quoted total is. Zero for a
    /// withdrawal that failed leaving nothing debited, and `None` for one
    /// that never settled at all.
    pub realized_total_debited: Option<Amount>,
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
/// `WaitingForConfirmation`. This enum differs from that in four deliberate
/// ways:
///
/// - **Only the transaction half of the outpoint is carried.** Upstream
///   identifies the funding transaction by an outpoint; what is reported
///   here is its transaction half, which is what a receipt or a
///   block-explorer link needs. The vout is dropped because nothing in this
///   API takes one.
/// - **Every state that knows the gross amount reports it.** Upstream's
///   `btc_deposited` is the amount that arrived on chain, before anything the
///   federation charges to claim it, and it is available from the moment a
///   transaction is seen. It is therefore on
///   [`WaitingForConfirmation`](Self::WaitingForConfirmation),
///   [`Confirmed`](Self::Confirmed) and [`Claimed`](Self::Claimed) alike,
///   in whole [`Sats`](crate::Sats) — an on-chain output cannot hold a
///   fraction of one.
/// - **[`Claimed`](Self::Claimed) also reports a net figure this SDK
///   computes.** The amount actually credited to the balance — what arrived,
///   less the aggregate of every federation-side cost of claiming it — is the
///   number a user sees their balance move by, and upstream never reports it.
///   That cost is *not* only the wallet module's peg-in fee, and this SDK's
///   earlier drafts said it was; see
///   [`OnchainReceiveDetails::realized_fee`]. It is an
///   [`Amount`](crate::Amount) rather than [`Sats`](crate::Sats) because
///   those fees are charged in millisatoshis and can leave the credit with
///   sub-satoshi precision; see the [unit note](Onchain).
/// - **One state has no upstream counterpart at all.**
///   [`ClaimRetrying`](Self::ClaimRetrying) exists because the second wallet
///   module can fail an individual claim attempt and then succeed on a later
///   one for the same output. See the next section.
///
/// # Two wallet modules, and why the mapping is not one table
///
/// A federation runs one of two on-chain modules, and their claim paths differ
/// in a way that changes what this enum has to be able to say. The variants
/// above are the v1 shape; the correspondence is exact there, one upstream
/// `DepositStateV2` variant to one state here, and
/// [`ClaimRetrying`](Self::ClaimRetrying) is never produced.
///
/// The second module claims a deposit by recording persistent events, and an
/// individual claim attempt may be **aborted** — after which the same output
/// can be claimed again, successfully, under a *different* underlying
/// operation. Its own receive subscription is written for exactly that: it
/// ignores an aborted attempt and keeps waiting for a success. So an abort
/// there is not the deposit failing; it is one attempt failing while the
/// deposit is still live.
///
/// Two consequences, both of which this API is shaped by.
///
/// **An abort must not map to [`Failed`](Self::Failed).** That state is final,
/// which ends the operation and closes its subscription. A caller told a
/// deposit failed, whose balance then moves by that very deposit, has been
/// told something false about their money — the one outcome this crate's
/// state machines are meant to make impossible. An aborted attempt that can
/// be retried therefore maps to [`ClaimRetrying`](Self::ClaimRetrying), which
/// is not final, and the operation stays open across as many aborts as the
/// module makes. Only an outcome the module will not retry — one that leaves
/// no path by which this output can still be claimed — reaches
/// [`Failed`](Self::Failed).
///
/// **One SDK operation spans several upstream ones.** Because each retry runs
/// under its own upstream operation id, an SDK deposit is not a view onto a
/// single upstream operation the way an SDK withdrawal is. It is a persisted
/// correlation — see [`Onchain::receive`] — keyed on the deposit address and
/// the funding output, under which every attempt for that output is the same
/// operation as far as this API is concerned. That correlation is what makes
/// [`ClaimRetrying`](Self::ClaimRetrying) → [`Claimed`](Self::Claimed) a
/// transition of *one* operation rather than the death of one and the birth of
/// another, and it is what lets a caller keep a single operation id from the
/// address it displayed to the credit it eventually saw.
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
/// amount, the realized fee and the net credit are all on the details record
/// too, and between that record and the current state an application never
/// needs to have seen an earlier one.
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
    /// A claim attempt was aborted and the deposit is still live: another
    /// attempt for the same output is expected.
    ///
    /// Reachable only under the second wallet module — see the enum's own
    /// documentation for why it exists and why an abort is not a failure.
    /// **Not final.** An operation may pass through this state any number of
    /// times, and each visit may carry a different
    /// [`last_abort`](Self::ClaimRetrying::last_abort). The terminal states
    /// remain [`Claimed`](Self::Claimed) and [`Failed`](Self::Failed); this
    /// one only says that neither has been reached yet.
    ///
    /// A caller should treat it as it treats [`Confirmed`](Self::Confirmed) —
    /// an incoming credit still in flight — and may surface
    /// `last_abort` to explain a deposit that is taking longer
    /// than expected. What it must not do is present it as an outcome: the
    /// deposit has not failed, and the amount it will credit is not yet
    /// established.
    ClaimRetrying {
        /// The funding transaction.
        txid: Txid,
        /// The gross amount that transaction paid to the address, before
        /// anything the federation charges to claim it. Unchanged by an
        /// aborted attempt — what arrived on chain arrived.
        gross_deposited: Sats,
        /// Human-readable explanation of the attempt that was aborted.
        /// Diagnostic only — not a stable contract, and not something to
        /// match on. Reports the most recent abort; earlier ones are not
        /// retained.
        last_abort: String,
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
        /// federation charged to claim it.
        gross_deposited: Sats,
        /// The amount actually credited to the balance: `gross_deposited`
        /// less the aggregate of every federation-side cost of claiming the
        /// deposit.
        ///
        /// Computed by the SDK — upstream reports only the gross figure —
        /// from the fees the federation recorded against the accepted claim
        /// transaction. Those are more than the wallet module's peg-in fee,
        /// which is why `gross_deposited` minus a peg-in fee does not
        /// reproduce this number;
        /// [`OnchainReceiveDetails::realized_fee`] is the aggregate it is
        /// computed from and
        /// [`OnchainReceiveDetails::realized_fee_breakdown`] names the parts.
        /// Denominated in millisatoshis, because those fees are, so the
        /// credit need not be a whole number of satoshis.
        ///
        /// This is the number the balance moved by, and it is the same value
        /// as [`OnchainReceiveDetails::realized_net_credit`]. Being a state
        /// this deposit actually reached, it is a realized figure by nature —
        /// a deposit has no quoted terms, because there is no quote for one.
        net_credit: Amount,
    },
    /// Final: the deposit could not be claimed, and no further attempt will
    /// be made.
    ///
    /// The second clause is the whole of this state's contract. Under the
    /// second wallet module a single aborted claim attempt is *not* this
    /// state — it is [`ClaimRetrying`](Self::ClaimRetrying) — and reaching
    /// here requires that no path remains by which the output can still be
    /// claimed. Being final, this state ends the operation and closes its
    /// subscription, so an implementation that guessed wrong here would leave
    /// a caller believing a deposit was lost that later credits their
    /// balance.
    ///
    /// Carries no transaction and no amount even when one was seen. What
    /// arrived is on [`OnchainReceiveDetails`], which is where a caller that
    /// only ever saw this state reads it — and so is what the failure cost,
    /// when it cost something. Failure comes in two shapes under the first
    /// wallet module: the claim transaction may have been rejected outright,
    /// in which case nothing was charged, or it may have been **accepted** —
    /// the peg-in spent, its fees incurred — with the primary module's note
    /// finalization failing afterwards, in which case a real fee was paid
    /// for notes that never became spendable. The state does not distinguish
    /// them; [`OnchainReceiveDetails::realized_fee`] does, by being absent
    /// for the first and recorded for the second.
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
            | OnchainReceiveState::Confirmed { .. }
            | OnchainReceiveState::ClaimRetrying { .. } => false,
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
/// # Every money figure here is realized, and none is quoted
///
/// A deposit has no quoted half, because there is no quote for one: the
/// sender pays the chain fee from their own wallet and the federation's terms
/// apply to whatever arrives. So this record has no `quoted_` fields at all,
/// and its `realized_` ones follow the same
/// [convention](Onchain#quoted-terms-and-realized-outcomes) as the withdrawal
/// side — set from the fees the federation recorded against the transaction
/// it accepted, and absent until it accepted one.
///
/// # Why five fields are optional, and what a caller can count on
///
/// A realized fact does not exist until the thing it describes has happened,
/// which is what these `Option`s mean. Against the placement rule
/// ([`OperationDetails`](crate::OperationDetails)) they fall into three
/// groups:
///
/// - [`txid`](OnchainReceiveDetails::txid) and
///   [`gross_deposited`](OnchainReceiveDetails::gross_deposited) are case 3:
///   each is announced by a state and dropped by
///   [`Failed`](OnchainReceiveState::Failed), which can follow a transaction
///   that was already seen and carries nothing but a reason. A deposit that
///   arrived and then could not be claimed is precisely the one an
///   application has to be able to describe, so the record keeps what that
///   state does not.
/// - [`realized_fee`](OnchainReceiveDetails::realized_fee) and
///   [`realized_fee_breakdown`](OnchainReceiveDetails::realized_fee_breakdown)
///   duplicate nothing at all: no state carries what the claim cost at any
///   point, so this record is the only place either can be read and no amount
///   of watching would recover them.
/// - [`realized_net_credit`](OnchainReceiveDetails::realized_net_credit) is
///   the one duplication worth arguing about, and it is deliberate rather
///   than case 3. [`Claimed`](OnchainReceiveState::Claimed) is final and
///   sticky, so by the rule's case 2 the credit could have lived on that
///   state alone. It is kept here as well because the fee it is computed
///   from lives nowhere but this record: splitting one arithmetic identity
///   across two reads would force a receipt either to re-derive the credit
///   with fallible arithmetic — which this record's own contract forbids —
///   or to pair a fee read from here with a credit read from there and order
///   the two against each other. Both copies are written in the same storage
///   transaction from the same accepted-transaction figures, so they cannot
///   disagree.
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
/// [`realized_fee`](OnchainReceiveDetails::realized_fee) is the
/// **aggregate** of everything the federation charged to bring the deposit
/// into the balance, and it is deliberately not the wallet module's peg-in
/// fee on its own. Claiming a deposit balances the wallet input into
/// primary-module outputs; the primary module's fees — on those outputs, and
/// on any existing notes it consolidates into the same transaction — and the
/// denomination dust the split leaves behind, reduce the credit exactly as
/// the peg-in fee does, and
/// upstream's own accounting reports an accepted transaction's costs as one
/// aggregate for that reason.
///
/// So the identity is:
/// [`gross_deposited`](OnchainReceiveDetails::gross_deposited) in
/// millisatoshis, less
/// [`realized_fee`](OnchainReceiveDetails::realized_fee), equals
/// [`realized_net_credit`](OnchainReceiveDetails::realized_net_credit),
/// which is the same value [`Claimed`](OnchainReceiveState::Claimed) reports.
/// It is **not** gross less a peg-in fee. An earlier draft of this record
/// documented it that way, and that subtraction does not in general equal the
/// balance movement, which is why the field was widened rather than
/// re-explained.
///
/// The aggregate is authoritative and is the figure to read;
/// [`realized_fee_breakdown`](OnchainReceiveDetails::realized_fee_breakdown)
/// names its parts, the peg-in fee among them, for a screen that wants to
/// explain the difference rather than merely state it. The fee is recorded
/// rather than left to be derived so that a receipt does not have to do
/// fallible arithmetic to name the one number a user asks about when their
/// balance moved by less than the sender sent.
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
    /// It is not necessarily an address this operation was the first to
    /// hand out; see [`Onchain::receive`].
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
    /// federation charged to claim it.
    ///
    /// Whole [`Sats`](crate::Sats): it is the value of an output in the
    /// funding transaction. `None` until a transaction is seen, then fixed.
    ///
    /// This is the counterparty figure — what the sender sent — and it is
    /// the number to show beside the credit when a user asks why the two
    /// differ. An observed on-chain fact rather than a fee, so it belongs to
    /// neither half of the quoted/realized split; it is optional only because
    /// nobody may have paid yet.
    pub gross_deposited: Option<Sats>,
    /// **Realized.** The aggregate of everything the federation charged to
    /// bring this deposit into the balance, once a claim has been accepted.
    ///
    /// Not the wallet module's peg-in fee on its own: claiming a deposit
    /// balances the wallet input into primary-module outputs, and the
    /// primary module's fees — output and input alike, if it consolidated
    /// notes along the way — and the denomination dust left over reduce the
    /// credit too.
    /// This field is the sum of all of it, which makes it the figure to read
    /// and the figure
    /// [`realized_net_credit`](OnchainReceiveDetails::realized_net_credit) is
    /// computed from;
    /// [`realized_fee_breakdown`](OnchainReceiveDetails::realized_fee_breakdown)
    /// names the parts.
    ///
    /// `None` until a claim is accepted, and absent from every state at every
    /// point, which makes this record its only home: a caller cannot recover
    /// it by watching, however carefully. Acceptance, not success, is what
    /// establishes it: a deposit that
    /// [`Failed`](OnchainReceiveState::Failed) *before* any claim
    /// transaction was accepted never establishes it — nothing was charged —
    /// but the first wallet module can accept the claim and then fail note
    /// finalization, and that deposit was charged this fee for notes that
    /// never became spendable. `Some` on such a failure is the record
    /// telling the truth about a cost with no credit to show for it.
    /// Millisatoshi-denominated, like every other fee in this facade.
    pub realized_fee: Option<Amount>,
    /// **Realized.** [`realized_fee`](OnchainReceiveDetails::realized_fee),
    /// split into the named parts it is made of.
    ///
    /// `Some` exactly when the aggregate is, set in the same write, and
    /// re-reporting the same money rather than an additional charge. It
    /// exists so that "the sender sent 100 000 sat and my balance went up by
    /// less" can be answered with the peg-in fee named separately from the
    /// mint-side costs — which is the question this record is most often read
    /// to answer, and the one the aggregate alone cannot break down.
    ///
    /// The aggregate stays authoritative; see
    /// [`OnchainReceiveFeeBreakdown`] for why a caller should not re-derive
    /// it by summing these.
    pub realized_fee_breakdown: Option<OnchainReceiveFeeBreakdown>,
    /// **Realized.** The amount credited to the balance:
    /// [`gross_deposited`](OnchainReceiveDetails::gross_deposited) in
    /// millisatoshis less
    /// [`realized_fee`](OnchainReceiveDetails::realized_fee) — the aggregate,
    /// not a peg-in fee alone.
    ///
    /// `None` until the claim completes. Equal to the
    /// [`Claimed`](OnchainReceiveState::Claimed) state's own net figure —
    /// the same value in both places, so a receipt built from the record and
    /// one built from the state cannot disagree — and an
    /// [`Amount`](crate::Amount) for the same reason it is one there: the
    /// fees deducted are millisatoshi-denominated, so the credit need not be
    /// a whole number of satoshis.
    pub realized_net_credit: Option<Amount>,
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

/// What [`OnchainReceiveDetails::realized_fee`] is made of, component by
/// component.
///
/// Obtained from [`OnchainReceiveDetails::realized_fee_breakdown`]. Every
/// field is an exact millisatoshi [`Amount`](crate::Amount), for the reason
/// [`OnchainQuote::fee`] gives on the withdrawal side: these are
/// federation-side figures and several of them are not whole satoshis.
/// Together they account for the aggregate exactly — the SDK's own invariant
/// is that the components sum to
/// [`OnchainReceiveDetails::realized_fee`], with no rounding and no residue.
///
/// Unlike [`OnchainSendFeeBreakdown`], which explains a quote, this explains
/// an outcome: every field is read from the claim transaction the federation
/// accepted, so the parts are measurements rather than predictions.
///
/// # Read the aggregate; use these to explain it
///
/// The same rule, for the same two reasons. The type is
/// `#[non_exhaustive]`, so a later version may split a component in two or
/// name one that did not exist, and a caller that had hard-coded the sum of
/// the fields it knew about would quietly start understating what the deposit
/// cost. And the aggregate is the figure
/// [`OnchainReceiveDetails::realized_net_credit`] was actually computed from,
/// so it is the only one guaranteed to reconcile with the balance movement.
///
/// So: aggregate for arithmetic, breakdown for explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OnchainReceiveFeeBreakdown {
    /// The wallet module's own charge for accepting the peg-in.
    ///
    /// The component a user means by "the federation's deposit fee", and the
    /// only one an earlier draft of this record reported. On its own it is
    /// not what reduced the credit, which is exactly why the other two fields
    /// exist rather than being folded silently into this one.
    pub peg_in: Amount,
    /// What it costs to turn the peg-in into spendable notes: everything the
    /// primary (mint) module charged on the transaction that balances the
    /// wallet input into notes — the output fees on the notes issued, and
    /// the input fees on any existing notes the module chose to spend into
    /// the same transaction while it was at it (the first mint module
    /// consolidates small denominations opportunistically, the second may
    /// rebalance; both ride along on the claim).
    ///
    /// One component covering both directions rather than an input/output
    /// split, for the same reason
    /// [`LnFeeBreakdown::primary_module`](crate::LnFeeBreakdown::primary_module)
    /// is: the split depends on which notes the module happened to select,
    /// which is not information a receipt can promise. A
    /// federation-internal, millisatoshi-denominated cost with no on-chain
    /// counterpart. Claiming a deposit cannot avoid it, because there is no
    /// way to receive a peg-in without issuing the notes it becomes, and it
    /// is the component most likely to make the aggregate a non-whole number
    /// of satoshis.
    pub primary_module: Amount,
    /// The residue that note issuance leaves behind: value too small to be
    /// represented in the federation's denominations, and therefore given up.
    ///
    /// Small, frequently sub-satoshi, and genuinely part of why the credit is
    /// less than what arrived. Reported rather than absorbed because a credit
    /// that does not reconcile with the balance movement is worse than one
    /// with a third line in it.
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

    /// A withdrawal record as it reads the moment [`Onchain::send`] returns:
    /// the quoted half committed, the realized half not yet established.
    fn a_quoted_send(amount: Sats, quoted_fee: Amount) -> OnchainSendDetails {
        OnchainSendDetails {
            address: an_address(),
            amount,
            quoted_fee,
            quoted_total_debited: amount
                .to_amount()
                .expect("the test amounts are representable in msat")
                .checked_add(quoted_fee)
                .expect("no overflow at this magnitude"),
            realized_fee: None,
            realized_total_debited: None,
            created_at: Timestamp::from_epoch_millis(1),
        }
    }

    /// The same record once the federation has accepted the transaction and
    /// charged `realized_fee` for it — which need not be what was quoted.
    fn settled(details: &OnchainSendDetails, realized_fee: Amount) -> OnchainSendDetails {
        OnchainSendDetails {
            realized_fee: Some(realized_fee),
            realized_total_debited: Some(
                details
                    .amount
                    .to_amount()
                    .expect("the test amounts are representable in msat")
                    .checked_add(realized_fee)
                    .expect("no overflow at this magnitude"),
            ),
            ..details.clone()
        }
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

    /// The non-finality that keeps a retried deposit observable: under the
    /// second wallet module an aborted claim attempt can be followed by a
    /// successful one for the same output, so this state must not end the
    /// operation.
    #[test]
    fn onchain_receive_state_claim_retrying_is_not_final() {
        assert!(
            !OnchainReceiveState::ClaimRetrying {
                txid: a_txid(),
                gross_deposited: Sats::from_sats(100_000),
                last_abort: String::new(),
            }
            .is_final()
        );
    }

    /// An abort leaves what arrived on chain untouched, so a caller can still
    /// render the deposit while it is being retried.
    #[test]
    fn claim_retrying_keeps_the_gross_and_the_transaction() {
        let state = OnchainReceiveState::ClaimRetrying {
            txid: a_txid(),
            gross_deposited: Sats::from_sats(100_000),
            last_abort: "peer rejected the claim transaction".to_owned(),
        };
        match state {
            OnchainReceiveState::ClaimRetrying {
                txid,
                gross_deposited,
                last_abort,
            } => {
                assert_eq!(txid, a_txid());
                assert_eq!(gross_deposited, Sats::from_sats(100_000));
                assert!(!last_abort.is_empty());
            }
            other => panic!("expected ClaimRetrying, got {other:?}"),
        }
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
                // An aggregate fee of 1500 msat leaves a credit that is not
                // a whole number of satoshis, which is why this field is an
                // `Amount`: as `Sats` it could only have been wrong.
                assert_eq!(net_credit, Amount::from_msats(99_998_500));
                assert_eq!(net_credit.to_sats_exact(), None);
            }
            _ => unreachable!("constructed as Claimed"),
        }
    }

    #[test]
    fn send_details_quoted_total_is_the_amount_plus_the_quoted_fee() {
        let details = a_quoted_send(Sats::from_sats(25_000), Amount::from_msats(1_234_567));
        assert_eq!(details.quoted_total_debited, Amount::from_msats(26_234_567));
        // The reason the fees and totals are `Amount`s: neither is a whole
        // number of satoshis, so a satoshi-typed field would have had to
        // round the debit down.
        assert_eq!(details.quoted_fee.to_sats_exact(), None);
        assert_eq!(details.quoted_total_debited.to_sats_exact(), None);
        // ... while what reaches the destination genuinely is whole sats.
        assert_eq!(details.amount, Sats::from_sats(25_000));
    }

    /// The quoted half is committed when `send` returns; the realized half
    /// does not exist until the federation has accepted a transaction, and
    /// absent means "not settled", not "free".
    #[test]
    fn send_details_realized_is_absent_until_the_withdrawal_settles() {
        let details = a_quoted_send(Sats::from_sats(25_000), Amount::from_msats(1_234_567));
        assert_eq!(details.realized_fee, None);
        assert_eq!(details.realized_total_debited, None);
        assert_eq!(details.quoted_fee, Amount::from_msats(1_234_567));
    }

    /// The invariant that the retraction of the ceiling claim rests on: the
    /// debit that actually lands can exceed the one that was quoted, because
    /// the mint inputs, change and dust are re-decided when the transaction
    /// is assembled. The record has to be able to state both figures.
    #[test]
    fn send_details_realized_may_exceed_quoted() {
        let quoted = a_quoted_send(Sats::from_sats(25_000), Amount::from_msats(1_234_567));
        let realized = settled(&quoted, Amount::from_msats(1_400_000));

        assert!(realized.realized_fee > Some(realized.quoted_fee));
        assert!(realized.realized_total_debited > Some(realized.quoted_total_debited));
        assert_eq!(
            realized.realized_total_debited,
            Some(Amount::from_msats(26_400_000))
        );
        // Settlement does not revise the quoted half: it is still exactly
        // what the user approved, which is what makes the pair worth keeping.
        assert_eq!(realized.quoted_fee, quoted.quoted_fee);
        assert_eq!(realized.quoted_total_debited, quoted.quoted_total_debited);
    }

    /// And it can land below the quote, or at zero for a withdrawal that
    /// failed leaving nothing debited.
    #[test]
    fn send_details_realized_may_be_below_quoted_or_zero() {
        let quoted = a_quoted_send(Sats::from_sats(25_000), Amount::from_msats(1_234_567));

        let cheaper = settled(&quoted, Amount::from_msats(900_000));
        assert!(cheaper.realized_fee < Some(cheaper.quoted_fee));
        assert!(cheaper.realized_total_debited < Some(cheaper.quoted_total_debited));

        let failed = OnchainSendDetails {
            realized_fee: Some(Amount::from_msats(0)),
            realized_total_debited: Some(Amount::from_msats(0)),
            ..quoted.clone()
        };
        assert_eq!(failed.realized_total_debited, Some(Amount::from_msats(0)));
        // A failed withdrawal still has terms to show on a receipt.
        assert_eq!(failed.quoted_total_debited, quoted.quoted_total_debited);
        assert_eq!(failed.address, quoted.address);
    }

    /// The deposit fee this SDK records, component by component. The peg-in
    /// fee is deliberately only part of it.
    fn a_receive_breakdown() -> OnchainReceiveFeeBreakdown {
        OnchainReceiveFeeBreakdown {
            peg_in: Amount::from_msats(1_000),
            primary_module: Amount::from_msats(400),
            dust: Amount::from_msats(100),
        }
    }

    fn aggregate_of(breakdown: &OnchainReceiveFeeBreakdown) -> Amount {
        breakdown
            .peg_in
            .checked_add(breakdown.primary_module)
            .and_then(|partial| partial.checked_add(breakdown.dust))
            .expect("no overflow at this magnitude")
    }

    #[test]
    fn receive_details_options_fill_in_once_and_agree_with_claimed() {
        let gross = Sats::from_sats(100_000);
        let breakdown = a_receive_breakdown();
        let fee = aggregate_of(&breakdown);
        let net = gross
            .to_amount()
            .expect("100 000 sat is representable in msat")
            .checked_sub(fee)
            .expect("the fee is smaller than the deposit");

        let waiting = OnchainReceiveDetails {
            address: an_address(),
            txid: None,
            gross_deposited: None,
            realized_fee: None,
            realized_fee_breakdown: None,
            realized_net_credit: None,
            created_at: Timestamp::from_epoch_millis(1),
        };
        // Nothing is known before a transaction is seen, and that is not a
        // failure to record anything.
        assert_eq!(waiting.txid, None);
        assert_eq!(waiting.realized_fee, None);
        assert_eq!(waiting.realized_net_credit, None);

        let claimed = OnchainReceiveDetails {
            txid: Some(a_txid()),
            gross_deposited: Some(gross),
            realized_fee: Some(fee),
            realized_fee_breakdown: Some(breakdown),
            realized_net_credit: Some(net),
            ..waiting.clone()
        };
        // The fields that were already fixed are untouched by the fill-in.
        assert_eq!(claimed.address, waiting.address);
        assert_eq!(claimed.created_at, waiting.created_at);
        assert_ne!(claimed, waiting);
        // The breakdown appears exactly when the aggregate does.
        assert_eq!(
            claimed.realized_fee.is_some(),
            claimed.realized_fee_breakdown.is_some()
        );

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
                assert_eq!(claimed.realized_net_credit, Some(net_credit));
            }
            _ => unreachable!("constructed as Claimed"),
        }
    }

    /// The accounting fix: what reduces a deposit's credit is the aggregate
    /// of every federation-side cost, so subtracting the peg-in fee alone
    /// does not reproduce the balance movement — it overstates the credit by
    /// the mint-side costs.
    #[test]
    fn receive_details_net_credit_comes_from_the_aggregate_not_the_peg_in_fee() {
        let gross = Sats::from_sats(100_000);
        let gross_msats = gross
            .to_amount()
            .expect("100 000 sat is representable in msat");
        let breakdown = a_receive_breakdown();
        let aggregate = aggregate_of(&breakdown);
        assert_eq!(aggregate, Amount::from_msats(1_500));
        assert!(aggregate > breakdown.peg_in);

        let net = gross_msats
            .checked_sub(aggregate)
            .expect("the fee is smaller than the deposit");
        let claimed = OnchainReceiveDetails {
            address: an_address(),
            txid: Some(a_txid()),
            gross_deposited: Some(gross),
            realized_fee: Some(aggregate),
            realized_fee_breakdown: Some(breakdown.clone()),
            realized_net_credit: Some(net),
            created_at: Timestamp::from_epoch_millis(1),
        };

        // The identity this record documents.
        assert_eq!(
            claimed.realized_net_credit,
            Some(Amount::from_msats(99_998_500))
        );
        // And the identity it explicitly does not: `gross - peg_in` is a
        // different, larger number, which is the whole reason the field is
        // the aggregate.
        let peg_in_only = gross_msats
            .checked_sub(breakdown.peg_in)
            .expect("the peg-in fee is smaller than the deposit");
        assert_ne!(Some(peg_in_only), claimed.realized_net_credit);
        assert!(Some(peg_in_only) > claimed.realized_net_credit);
        // The credit is not a whole number of satoshis, which is why it is an
        // `Amount`.
        assert_eq!(net.to_sats_exact(), None);
    }

    /// A deposit that failed after its transaction was seen: the on-chain
    /// facts survive on the record, and no fee is invented for a claim that
    /// was never accepted.
    #[test]
    fn receive_details_failed_deposit_keeps_the_facts_and_records_no_fee() {
        let details = OnchainReceiveDetails {
            address: an_address(),
            txid: Some(a_txid()),
            gross_deposited: Some(Sats::from_sats(100_000)),
            realized_fee: None,
            realized_fee_breakdown: None,
            realized_net_credit: None,
            created_at: Timestamp::from_epoch_millis(1),
        };
        assert_eq!(details.txid, Some(a_txid()));
        assert_eq!(details.gross_deposited, Some(Sats::from_sats(100_000)));
        assert_eq!(details.realized_fee, None);
        assert_eq!(details.realized_net_credit, None);
    }

    #[test]
    fn receive_fee_breakdown_components_sum_to_the_aggregate() {
        let breakdown = OnchainReceiveFeeBreakdown {
            peg_in: Amount::from_msats(1_000_000),
            primary_module: Amount::from_msats(234_000),
            dust: Amount::from_msats(567),
        };
        assert_eq!(aggregate_of(&breakdown), Amount::from_msats(1_234_567));
        // As on the withdrawal side, the parts do not add up to a whole
        // number of satoshis.
        assert_eq!(aggregate_of(&breakdown).to_sats_exact(), None);
    }

    #[test]
    fn both_state_types_name_their_details_record() {
        let send = a_quoted_send(Sats::from_sats(1), Amount::from_msats(1));
        let receive = OnchainReceiveDetails {
            address: an_address(),
            txid: None,
            gross_deposited: None,
            realized_fee: None,
            realized_fee_breakdown: None,
            realized_net_credit: None,
            created_at: Timestamp::from_epoch_millis(0),
        };
        assert_eq!(round_trip_details::<OnchainSendState>(send.clone()), send);
        assert_eq!(
            round_trip_details::<OnchainReceiveState>(receive.clone()),
            receive
        );
    }

    #[test]
    fn send_fee_breakdown_components_sum_to_the_aggregate() {
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
