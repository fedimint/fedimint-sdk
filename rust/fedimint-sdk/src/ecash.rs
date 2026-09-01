//! Chaumian ecash: spending notes out of band and redeeming them.

use std::sync::Arc;

use crate::{Amount, Notes, Operation, OperationState, Result, Timestamp};

/// The ecash facade for one federation, backed by its mint module.
///
/// Obtained from [`Federation::ecash`](crate::Federation::ecash), which
/// returns `None` when the federation has no mint module. Like the other
/// facades it is a cheap clone over the federation's shared state.
///
/// Ecash here means *out-of-band* ecash: notes the sender takes out of
/// their balance and hands to a receiver over some channel the federation
/// knows nothing about — a chat message, a QR code, a file. The receiver
/// redeems them against the same federation. Ordinary in-federation
/// spending is not a separate concept; it is what lightning and on-chain
/// operations do with the balance.
///
/// # Sending is quoted, like every other outgoing value in this crate
///
/// [`Ecash::quote`] plans a send and [`Ecash::send`] executes that plan.
/// The indirection is not ceremony: the value a send takes out of the
/// balance is generally *more* than the amount asked for, because a mint
/// issues notes in fixed denominations and rounds a request up, and because
/// assembling the notes can itself cost a fee. Quoting is what puts the real
/// figure in front of a user before they agree to it, exactly as
/// [`Lightning::quote`](crate::Lightning::quote) and
/// [`Onchain::quote`](crate::Onchain::quote) do.
///
/// Receiving is not quoted, because it presents the caller with no decision:
/// see [`Ecash::receive`].
///
/// # Quoted terms versus realized movement
///
/// Every figure this facade records belongs to one of two halves, and they
/// are not interchangeable:
///
/// - **Quoted or expected terms** — what the executed [`EcashQuote`] fixed
///   for a send, or what the federation's fee schedule predicts for a
///   redemption. Plain (non-`Option`) values, known before the creating call
///   returns. They are what the user approved, and they describe the
///   *attempt*.
/// - **Realized movement** — what the balance actually did once the
///   operation settled: how much a reclaim gave back, what the reissue the
///   federation accepted actually charged. `Option`, absent until the
///   operation reaches a final state and then set exactly once — from the
///   transaction that was accepted, or, where none was, recording that
///   nothing moved.
///
/// The two halves exist because a federation fixes a transaction's own costs
/// — the mint's input and output fees, the change it has to make, the dust
/// it cannot represent — when that transaction is assembled and accepted. A
/// fee quote is a dry run that is then thrown away, so a figure fixed at
/// quote time is an estimate of a later event.
///
/// A send's *funding* transaction is no exception, though an earlier draft of
/// this facade said it was — that [`Ecash::send`] executes the quoted plan or
/// refuses it, so the debit really is the quoted total.
/// [`EcashQuote::total`] retracts that and sets out why: the send fee quote is
/// a non-committing inventory dry run, the spend that follows takes an amount
/// and no maximum total debit, and refusing a visibly stale quote narrows that
/// window without closing it. So the funding debit is a realized figure like
/// every other one here, recorded as
/// [`EcashSendDetails::realized_total_debited`]. And a reclaim is a *further*
/// transaction with a cost of its own that no quote covers at all.
///
/// The practical consequence, which is why this is written down rather than
/// left implicit: **a reclaimed send does not restore what it debited.** The
/// notes come back through a transaction that charges for bringing them
/// back, so what is restored is smaller than what left, and the difference
/// is a real cost — sunk, not missing. A receipt and a balance
/// reconciliation both need the two halves kept apart, so
/// [`EcashSendDetails`] and [`EcashReceiveDetails`] say, field by field,
/// which half each figure belongs to.
///
/// # The recovery lock
///
/// Every call on this facade, sending and receiving alike, is refused with
/// [`Recovering`](crate::ErrorCode::Recovering) while a recovery for the
/// federation is **incomplete**. Incomplete is not the same as "still
/// running": a recovery that stopped without finishing leaves the lock in
/// place, and only a recovery that runs to completion releases it. A wallet
/// whose note set was never fully discovered is not safe to spend from
/// either way, since a note the rescan never reached can be double-spent.
#[derive(Debug, Clone)]
pub struct Ecash {
    inner: Arc<EcashInner>,
}

impl Ecash {
    /// Plans an out-of-band send and returns an executable quote for it.
    ///
    /// Quoting is a separate step from sending because the value that leaves
    /// the balance is **not** the value the caller asked for, and the
    /// difference is the caller's money. Two things move it:
    ///
    /// - **The mint rounds up.** Notes exist in fixed denominations, so what
    ///   the receiver can redeem is the smallest value the mint can
    ///   represent at or above `amount` — mintv2 rounds a request up to a
    ///   multiple of 512 msat — and the sender is debited that larger figure
    ///   rather than the one they typed.
    /// - **Assembling the notes can cost a fee.** When the wallet holds no
    ///   combination of notes that adds up, a larger note has to be
    ///   re-issued into smaller ones first, and that self-reissue is charged
    ///   for: the mint's own fee, the primary module's fee, and whatever the
    ///   federation's configuration says about change and dust. Both
    ///   published mint generations expose a send fee quote for exactly this
    ///   reason.
    ///
    /// The returned [`EcashQuote`] is that plan, frozen: it binds the
    /// requested amount, the note value that will actually be produced, the
    /// fee, the total debit, and the note inventory and federation
    /// configuration all of those were computed against. Show it, then hand
    /// it back to [`Ecash::send`], which executes that plan or refuses it.
    ///
    /// The fee and total it names are **quoted** figures — what the send is
    /// expected to debit, and what the user approves — not a bound on what
    /// will be debited. [`EcashQuote::total`] explains where the gap comes
    /// from and why this SDK cannot close it; what the funding actually took
    /// out of the balance is reported afterwards on
    /// [`EcashSendDetails::realized_total_debited`].
    ///
    /// `amount` is a floor rather than a promise — the least the receiver
    /// must be able to redeem. [`EcashQuote::notes_value`] is what they will
    /// actually be able to redeem, and it is the number to put in front of a
    /// user beside [`EcashQuote::fee`] and [`EcashQuote::total`].
    ///
    /// Quoting neither debits the balance nor records anything: it plans.
    /// Quotes expire, and a plan that is no longer executable is refused by
    /// [`Ecash::send`] rather than silently re-derived — see
    /// [`EcashQuote::expires_at`].
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a zero amount,
    /// which no note can carry,
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance) when
    /// the balance cannot cover the rounded-up note value plus the fee —
    /// which can happen for an `amount` the balance would have covered
    /// exactly, and is itself a reason for this call to exist,
    /// [`Recovering`](crate::ErrorCode::Recovering) while a recovery for
    /// this federation is incomplete,
    /// [`NotSupported`](crate::ErrorCode::NotSupported) if the mint module
    /// disappeared from the federation's configuration after this facade
    /// was obtained,
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn quote(&self, amount: Amount) -> Result<EcashQuote> {
        unimplemented!()
    }

    /// Executes a quoted send, taking its value out of the balance as
    /// out-of-band notes.
    ///
    /// The quote is consumed: it describes one send and can fund one send.
    /// Execution follows the plan — the same note value, issued in the same
    /// denominations — or it does not happen:
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired) if the quote's
    /// validity window has passed,
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) if something the
    /// quote depends on moved underneath it (the notes it planned to spend
    /// went to another operation, the federation's fee schedule or
    /// configuration changed). Both mean the same thing to a caller: quote
    /// again and re-confirm with the user. That refusal is worth having: a
    /// user is not charged against terms they never saw.
    ///
    /// That refusal is **not** a spending ceiling, though an earlier draft of
    /// this documentation said it was. [`EcashQuote::total`] is the debit this
    /// send was quoted at, not a maximum this call is authorised against:
    /// published Fedimint offers no way to bind a total inside the funding
    /// transaction that finally commits, so the realized debit can land above
    /// the quoted one and refusing a visibly stale quote does not stop it.
    /// [`EcashQuote::total`] gives the mechanism in full and names what
    /// upstream would have to add before a ceiling could be promised.
    ///
    /// The returned [`EcashSend::notes`] are ready to hand to a receiver, and
    /// what the funding actually took out of the balance is recorded as
    /// [`EcashSendDetails::realized_total_debited`]. Until someone
    /// redeems the notes the value is in limbo: it is no longer spendable by
    /// the sender, and it is not yet the receiver's either.
    ///
    /// # Automatic reclaim
    ///
    /// Notes that go unredeemed do not vanish. The SDK schedules an
    /// automatic reclaim, so a send to someone who never opens the message
    /// eventually returns to the sender's balance instead of being lost —
    /// though not all of it. A reclaim is itself a federation transaction
    /// and is charged for, so what comes back is less than the funding took
    /// out; the restored figure and the total the send finally cost are
    /// recorded on [`EcashSendDetails`] when the reclaim settles, and are the
    /// only numbers that reconcile with the balance. The default period is **one day**, matching what the existing
    /// JavaScript SDK uses today; the exact value is subject to
    /// confirmation when this facade is implemented. The moment it is
    /// scheduled for is persisted as [`EcashSendDetails::reclaim_at`], so an
    /// application that restarted can still say when the notes stop being
    /// redeemable. Its outcome is reported through the state machine like
    /// any other: [`EcashSendState::Canceled`] when the reclaim wins,
    /// [`EcashSendState::Redeemed`] when the receiver got there first.
    ///
    /// The quote is the only argument, deliberately. Tuning the reclaim
    /// period, or constraining note selection, belongs on a later additive
    /// `quote_with`-style call, where it becomes part of the plan the user
    /// approves, rather than on an options struct here, where it could
    /// change what the approved quote costs.
    ///
    /// # Errors
    ///
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired),
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged),
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance),
    /// [`Recovering`](crate::ErrorCode::Recovering) while a recovery for
    /// this federation is incomplete,
    /// [`NotSupported`](crate::ErrorCode::NotSupported) if the mint module
    /// disappeared from the federation's configuration after this facade
    /// was obtained,
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn send(&self, quote: EcashQuote) -> Result<EcashSend> {
        unimplemented!()
    }

    /// Redeems out-of-band notes into this federation's balance.
    ///
    /// The notes are reissued as fresh notes belonging to this client,
    /// which is what makes the redemption final and unlinkable to the
    /// sender's copy. The returned operation tracks that;
    /// [`EcashReceiveState::Done`] is the point at which the value is
    /// spendable.
    ///
    /// There is deliberately no quote on this side, because a redemption
    /// presents the caller with no decision: the notes carry the value they
    /// carry, the reissuance fee comes out of it rather than being charged on
    /// top of it, and the only alternative to accepting both is not
    /// redeeming at all. Nothing is hidden by that — the gross value and the
    /// expected fee and credit are recorded in [`EcashReceiveDetails`]
    /// before this call returns, and what the accepted reissue actually
    /// charged is recorded on the same record when the federation settles it,
    /// so a receipt never depends on having watched the operation. The
    /// expected figures are an estimate and the realized ones are the
    /// movement; see [Quoted terms versus realized
    /// movement](Ecash#quoted-terms-versus-realized-movement).
    ///
    /// Redeem promptly. Notes are subject to the sender's automatic reclaim
    /// (see [`Ecash::send`]), and losing the race means the operation ends
    /// in [`EcashReceiveState::Failed`].
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) if the notes are
    /// malformed or were issued by a different federation,
    /// [`Recovering`](crate::ErrorCode::Recovering) while a recovery for
    /// this federation is incomplete,
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn receive(&self, notes: &Notes) -> Result<Operation<EcashReceiveState>> {
        unimplemented!()
    }
}

/// A frozen, executable plan for one out-of-band ecash send.
///
/// Produced by [`Ecash::quote`] and consumed by [`Ecash::send`]. As with
/// [`LnQuote`](crate::LnQuote) and [`OnchainQuote`](crate::OnchainQuote),
/// the accessors expose exactly what a user must approve and nothing else:
/// which notes will be spent, and whether they have to be re-issued to
/// assemble the value, is the SDK's business, and the contract with a caller
/// is "display these numbers, then give the quote back" rather than "inspect
/// and reassemble the plan".
///
/// # Why the requested amount and the actual note value differ
///
/// This asymmetry is the entire reason this quote exists, and it is the
/// ordinary case rather than an edge case:
///
/// - **A mint issues notes in fixed denominations.** A request for 1234 msat
///   cannot be met exactly, so it is satisfied with notes worth *more* — the
///   smallest value the mint can represent at or above the request. mintv2
///   makes this explicit by rounding the requested value up to a multiple of
///   512 msat. The rounding is always upward, so
///   [`notes_value`](EcashQuote::notes_value) is never below
///   [`requested_amount`](EcashQuote::requested_amount).
/// - **Assembling those notes can cost a fee.** If the notes already held
///   cannot be combined into the value, some of them are re-issued to split
///   them, and the mint, the primary module, and the federation's change and
///   dust rules all charge for that.
///
/// So the debit is [`notes_value`](EcashQuote::notes_value) plus
/// [`fee`](EcashQuote::fee), and both can exceed what the user typed. An
/// interface must show [`total`](EcashQuote::total) before the user agrees,
/// because that is the number their balance is expected to move by; showing
/// them the amount they typed instead would be showing them the one figure
/// that is guaranteed not to be what they pay.
///
/// A quote is also the SDK's own record of what it committed to. The fee and
/// the resolved note value are quoted once and appear nowhere in the send's
/// progress stream, so the executed quote is what lets
/// [`EcashSendDetails`] report the terms a receipt needs, for the whole life
/// of the operation and after a restart.
///
/// Everything here is a **quoted** term: the numbers the user approved, fixed
/// when this quote was executed, and a prediction of what the send will debit
/// rather than a measurement of what it did — see the
/// [convention](Ecash#quoted-terms-versus-realized-movement) and, for the
/// reason the distinction cannot be engineered away, [`EcashQuote::total`].
/// The realized counterparts live on [`EcashSendDetails`].
///
/// What an executed quote *does* bind is the plan: the value the notes carry
/// and the denominations they are issued in. What it cannot bind is the debit.
#[derive(Debug)]
pub struct EcashQuote {
    inner: EcashQuoteInner,
}

impl EcashQuote {
    /// The amount [`Ecash::quote`] was asked for.
    ///
    /// Kept so that a confirmation screen or a receipt can show what was
    /// requested next to what will actually be issued. It is a floor, and it
    /// is **not** the figure the balance moves by; see
    /// [`EcashQuote::total`].
    pub fn requested_amount(&self) -> Amount {
        unimplemented!()
    }

    /// The value the notes will actually carry — what the receiver can
    /// redeem.
    ///
    /// At or above [`EcashQuote::requested_amount`], never below it; see the
    /// type documentation for why it is often above. This is the figure
    /// activity history reports as an ecash send's
    /// [`amount`](crate::ActivityItem::amount).
    pub fn notes_value(&self) -> Amount {
        unimplemented!()
    }

    /// The quoted cost of issuing and selecting those notes, on top of
    /// [`EcashQuote::notes_value`].
    ///
    /// Zero when the notes already held can be handed over as they are.
    /// Non-zero when they have to be re-issued to assemble the value, which
    /// is a fee the caller pays for the shape of their own note inventory
    /// rather than for anything the receiver gets.
    ///
    /// "Quoted" is doing work in that first sentence. This comes from a fee
    /// quote taken against the note inventory as it stood, and the reissue it
    /// prices is assembled later, against the inventory as it stands then; see
    /// [`EcashQuote::total`] for the mechanism. What the send actually cost is
    /// [`EcashSendDetails::realized_fee`], and it can land on either side of
    /// this number.
    pub fn fee(&self) -> Amount {
        unimplemented!()
    }

    /// The total this send is quoted at: [`EcashQuote::notes_value`] plus
    /// [`EcashQuote::fee`].
    ///
    /// This is the number to show as "you will pay" on an approval screen.
    ///
    /// # It is an estimate, not an enforced ceiling
    ///
    /// An earlier draft of this API called this the debit
    /// [`Ecash::send`] was authorised for and could not exceed, on the grounds
    /// that `send` executes the approved plan or refuses with
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged). That claim is
    /// **retracted**: published Fedimint cannot enforce it, and no amount of
    /// care inside this SDK can supply the enforcement from outside. The
    /// mechanism is worth stating precisely, because its shape is what decides
    /// whether it can be worked around.
    ///
    /// - **Quoting is a non-committing dry run over the note inventory.**
    ///   Both published mint generations expose a send fee quote, and it
    ///   commits to nothing: it prices a reissue against the notes held at
    ///   that instant and reserves none of them.
    /// - **The spend that follows takes an amount and nothing else.** Neither
    ///   the v1 nor the v2 out-of-band spend accepts an expected-total or
    ///   maximum-total-debit argument. The mint decides which notes to spend,
    ///   which to reissue, what change to make and what dust it cannot
    ///   represent at that moment, and charges accordingly — so the funding
    ///   debit can differ from the total quoted here.
    /// - **Re-checking the fee immediately before spending does not close the
    ///   gap, it only narrows the window.** Between the check and the commit
    ///   the inventory can still move — a time-of-check to time-of-use race —
    ///   and a check that leaves a race is not a guarantee. Saying so is more
    ///   useful than implying one.
    ///
    /// So the realized debit can land **above** this figure as well as below
    /// it. Neither direction is an error, and neither is a broken promise,
    /// because no promise of a maximum is being made.
    ///
    /// What would turn this into a real ceiling is an upstream change, not an
    /// SDK one: either an atomic maximum-total (or expected-fee) guard
    /// *inside* the spend, so that assembly itself refuses to exceed a figure
    /// the caller named, or a persisted reservation of the selected notes with
    /// defined drop, expiry and restart semantics, so that the inventory
    /// quoted against is the inventory held. Either is a prerequisite this API
    /// is documenting rather than pretending to have.
    ///
    /// # What a caller gets instead
    ///
    /// Two things, and between them they cover the honest cases.
    ///
    /// [`Ecash::send`] still refuses a quote whose terms have visibly moved —
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired) once the validity
    /// window has passed, and
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) when the notes it
    /// planned against went elsewhere or the fee schedule moved — so a stale
    /// quote is never executed silently. That is a genuine protection against
    /// staleness; it is not a bound on the commit. The plan itself *is* bound:
    /// the value the notes carry, and the denominations they are issued in,
    /// are the ones that were shown.
    ///
    /// And the receipt reports the truth.
    /// [`EcashSendDetails::realized_total_debited`] is what the balance
    /// actually paid to fund the send, and
    /// [`EcashSendDetails::realized_fee`] is what the send finally cost once
    /// any reclaim is accounted for. A caller that renders this quoted total
    /// after the fact will eventually render a number that is not what
    /// happened.
    pub fn total(&self) -> Amount {
        unimplemented!()
    }

    /// When this quote stops being executable.
    ///
    /// Past this point [`Ecash::send`] fails with
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired). Expiry is not the
    /// only way a quote can stop being executable: it is bound to the note
    /// inventory it planned against, so notes spent by another operation in
    /// the meantime invalidate it too, reported as
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged). The remedy for both
    /// is the same — quote again and re-confirm.
    pub fn expires_at(&self) -> Timestamp {
        unimplemented!()
    }
}

/// The result of [`Ecash::send`]: the notes to hand over, and the operation
/// that tracks what happens to them.
///
/// Both halves matter. The notes are what the sender transmits; the
/// operation is how the sender learns whether they were redeemed or came
/// back. Dropping the operation does not stop the reclaim timer — it keeps
/// running in the background like any other operation.
///
/// # This is a convenience, not the only copy
///
/// The notes, the amounts, and the fee the executed quote named are all
/// persisted before [`Ecash::send`] returns, and are readable afterwards
/// through [`Operation::details`](crate::Operation::details) as an
/// [`EcashSendDetails`] — from the operation id alone, in a later process,
/// with nobody having kept this struct. That is what makes an out-of-band
/// send survivable: a sender whose application dies between issuing the
/// notes and delivering them can still find them and still hand them over,
/// instead of holding value nobody can redeem until the reclaim fires.
#[derive(Debug)]
#[non_exhaustive]
pub struct EcashSend {
    /// The notes to give to the receiver. Their value is already out of the
    /// sender's spendable balance, and it is
    /// [`EcashQuote::notes_value`] — the value the mint actually issued, not
    /// the amount that was requested.
    ///
    /// The same notes are persisted as [`EcashSendDetails::notes`] and can be
    /// read back after a restart; this field is the copy the creating call
    /// hands over so that the common path needs no second lookup.
    pub notes: Notes,
    /// Tracks redemption, cancellation, and automatic reclaim.
    pub operation: Operation<EcashSendState>,
}

impl Operation<EcashSendState> {
    /// Asks for the notes back, before the receiver redeems them.
    ///
    /// # What `Ok(())` means, exactly
    ///
    /// **`Ok(())` means the cancellation intent has been committed to local
    /// storage and will survive a restart or a period offline.** That is the
    /// whole postcondition. It does not mean the federation has been
    /// contacted, that a reclaim has been attempted, or that the notes came
    /// back.
    ///
    /// This is a deliberate choice about where the boundary sits, and it is
    /// what makes the result actionable. Had the call waited on the network,
    /// it could return
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable) or
    /// [`Timeout`](crate::ErrorCode::Timeout) *after* durably recording the
    /// intent, and the caller would be left with the one answer nothing can
    /// be done with: "maybe accepted". They could not retry safely without
    /// wondering whether they were duplicating a request already in flight,
    /// and they could not report failure without possibly contradicting a
    /// reclaim that then succeeds. Committing locally first removes that
    /// state: the request is recorded, the SDK keeps trying on its own, and a
    /// device that was offline at the moment of the call still reclaims when
    /// it comes back.
    ///
    /// The outcome arrives where every other outcome does, as a state:
    /// [`EcashSendState::Canceled`] if the notes came back,
    /// [`EcashSendState::Redeemed`] if the receiver got them. Between the
    /// request and the outcome the operation sits in
    /// [`EcashSendState::CancelRequested`]. The protocol race is real —
    /// the receiver may be redeeming at this very moment and only the
    /// federation decides who wins — and it resolves through those states,
    /// not through this return value.
    ///
    /// This is the only cancellation in the crate, because it is the only
    /// place where cancelling is a real protocol action rather than an
    /// attempt to un-send money that has already moved.
    ///
    /// # Requesting a cancel on a settled send is not an error
    ///
    /// If the send has already reached a final state — the notes came back
    /// ([`EcashSendState::Canceled`]) or the receiver redeemed them
    /// ([`EcashSendState::Redeemed`]) — this returns `Ok(())` and does
    /// nothing. The postcondition the call promises already holds: no
    /// cancellation is pending, and the outcome is recorded in the state,
    /// where the caller reads it. This is the same idempotent framing
    /// [`Sdk::close_federation`](crate::Sdk::close_federation) and
    /// [`Sdk::forget_federation`](crate::Sdk::forget_federation) use.
    ///
    /// It is also unavoidable in practice: the request and the redemption
    /// race, so a caller that checks the state and then cancels can always
    /// be beaten between the two calls. Failing that race would make an
    /// ordinary, correct sequence look broken, and would tell the caller
    /// nothing that reading the state does not already tell them.
    ///
    /// # Errors
    ///
    /// Only failures that stop the intent from being recorded at all — which
    /// is why no network error appears here:
    /// [`Storage`](crate::ErrorCode::Storage) if the request cannot be
    /// committed durably, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) if the
    /// federation was closed or the SDK shut down, leaving nothing to record
    /// it against. An unreachable federation or a slow guardian is not a
    /// failure of this call: the intent is already durable and the SDK
    /// pursues it in the background.
    pub async fn request_cancel(&self) -> Result<()> {
        unimplemented!()
    }
}

/// The lifecycle of an out-of-band ecash send.
///
/// # Relationship to the upstream state machine
///
/// Upstream `fedimint-mint-client` models this as `SpendOOBState`, whose
/// variants are `Created`, `UserCanceledProcessing`, `UserCanceledSuccess`,
/// `UserCanceledFailure`, `Success`, and `Refunded`. Two of those names
/// mean the opposite of what they suggest when read in isolation, because
/// they are named from the point of view of the *cancellation attempt*
/// rather than the send: upstream `Success` means the automatic reclaim
/// **failed**, i.e. the receiver redeemed the notes, and upstream
/// `Refunded` means the reclaim **succeeded**, i.e. the notes returned to
/// the sender.
///
/// This enum is named from the point of view of the send, and collapses the
/// upstream set accordingly:
///
/// | upstream `SpendOOBState`                   | here                          |
/// | ------------------------------------------ | ----------------------------- |
/// | `Created`                                  | [`Created`](Self::Created)    |
/// | `UserCanceledProcessing`                   | [`CancelRequested`](Self::CancelRequested) |
/// | `UserCanceledSuccess`, `Refunded`          | [`Canceled`](Self::Canceled)  |
/// | `UserCanceledFailure`, `Success`           | [`Redeemed`](Self::Redeemed)  |
///
/// The two pairs collapse because the distinction upstream draws inside
/// each — whether the notes came back because the user asked or because the
/// timer fired, and whether the receiver won against an explicit cancel or
/// against no cancel at all — is a distinction about *why*, not about what
/// happened to the money. An application asking "did my notes come back?"
/// needs the second question answered, and gets one variant per answer.
///
/// The mapping is total: every upstream variant lands somewhere here, and
/// there is no variant here without an upstream counterpart.
///
/// # There is no failure state, and that is the point
///
/// An ecash send has exactly two terminal outcomes — the notes came back
/// ([`Canceled`](Self::Canceled)) or the receiver got them
/// ([`Redeemed`](Self::Redeemed)) — because those are the only two things
/// that can happen to the money. Upstream's `SpendOOBState` has no state for
/// a failed send either: its `UserCanceledFailure` names a failed
/// *cancellation*, which is precisely the receiver having redeemed, and maps
/// to [`Redeemed`](Self::Redeemed) above.
///
/// Infrastructure failure does not become a third outcome. If storage cannot
/// be read, no guardian answers, or the federation handle is closed, that is
/// a failure of the *observation*, and it surfaces exactly where the crate's
/// central convention says it does: as `Err` from
/// [`Operation::state`](crate::Operation::state),
/// [`Operation::await_final`](crate::Operation::await_final), or
/// [`OperationUpdates::next`](crate::OperationUpdates::next). The send
/// itself keeps running, unaffected by the fact that nobody could see it.
///
/// Recording such a failure as a terminal state would be a lie about money,
/// not just about naming. Bearer notes that are out in the world can still
/// be redeemed by a receiver, and can still be reclaimed by the sender's
/// pending reclaim, long after some call failed to observe them. A state
/// declaring the operation over would tell an application the value is
/// settled when it is not, and — because
/// [`Sdk::forget_federation`](crate::Sdk::forget_federation) refuses while
/// reclaimable outgoing value remains — could let a federation's local state
/// be deleted while notes it could still have reclaimed were outstanding.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EcashSendState {
    /// The notes have been issued and handed to the caller. The value has
    /// left the spendable balance; nobody has redeemed or reclaimed it
    /// yet.
    Created,
    /// A reclaim has been requested — either by
    /// [`request_cancel`](Operation::request_cancel) or by the automatic
    /// reclaim timer — and is being processed. Not final: the request may
    /// still lose to a redemption.
    CancelRequested,
    /// Final: the notes were reclaimed and their value is back in the
    /// spendable balance — most of it. The reclaim is a federation
    /// transaction of its own and is charged for, so what returns is less
    /// than what the send debited; the exact figures are
    /// [`EcashSendDetails::realized_total_debited`],
    /// [`EcashSendDetails::restored_amount`] and
    /// [`EcashSendDetails::realized_fee`], and this state carries no payload
    /// of its own to disagree with them.
    Canceled,
    /// Final: the receiver redeemed the notes. The value is theirs; a
    /// cancellation request, if one was made, lost the race.
    Redeemed,
}

impl crate::operation::sealed::Sealed for EcashSendState {}

impl OperationState for EcashSendState {
    fn is_final(&self) -> bool {
        match self {
            EcashSendState::Created | EcashSendState::CancelRequested => false,
            EcashSendState::Canceled | EcashSendState::Redeemed => true,
        }
    }
}

/// What an out-of-band ecash send *is*, as opposed to where it has got to.
///
/// The persisted record for an [`Operation<EcashSendState>`](crate::Operation),
/// read with [`Operation::details`](crate::Operation::details). It has two
/// halves, in the sense of [Quoted terms versus realized
/// movement](Ecash#quoted-terms-versus-realized-movement), and every field
/// below says which one it belongs to.
///
/// Without this record an ecash send would be the operation least able to
/// survive a restart. [`EcashSendState`] carries no payload at all; its four
/// variants say only where the send has got to. So the notes, the amounts and
/// the fees would exist solely in the value [`Ecash::send`] returned, and an
/// application that restarted before delivering them would have debited its
/// user for bearer value it could no longer display, receipt, or even name.
///
/// # The quoted half
///
/// [`requested_amount`](EcashSendDetails::requested_amount),
/// [`notes_value`](EcashSendDetails::notes_value),
/// [`quoted_fee`](EcashSendDetails::quoted_fee) and
/// [`quoted_total_debited`](EcashSendDetails::quoted_total_debited) are fixed
/// when the send is created, from the terms the executed [`EcashQuote`] named,
/// along with [`notes`](EcashSendDetails::notes),
/// [`reclaim_at`](EcashSendDetails::reclaim_at) and
/// [`created_at`](EcashSendDetails::created_at). That is case 1 of
/// [`OperationDetails`](crate::OperationDetails)'s placement rule — such
/// values live in the record and nowhere else — which is why none of them is
/// an `Option`.
///
/// An earlier revision of this record kept the debit as a single plain
/// `total_debited`, on the reasoning that it was quoted and realized at once:
/// [`Ecash::send`] executes the approved plan or refuses with
/// [`QuoteChanged`](crate::ErrorCode::QuoteChanged), so the funding debit had
/// to be the quoted total. That reasoning does not hold on published
/// Fedimint, and [`EcashQuote::total`] says why — the send fee quote reserves
/// nothing and the spend takes no maximum. So the quoted total stays here as
/// the approved estimate, and what the funding actually took is a realized
/// figure like every other realized figure in this crate.
///
/// # The realized half
///
/// [`realized_total_debited`](EcashSendDetails::realized_total_debited),
/// [`restored_amount`](EcashSendDetails::restored_amount) and
/// [`realized_fee`](EcashSendDetails::realized_fee) are the outcome, and they
/// are `Option` because they do not exist yet before the transactions that
/// establish them are accepted. None of them is announced by a state —
/// [`EcashSendState`] carries no payload — so this record is their only
/// possible home, and they take the shape the placement rule's case 3
/// prescribes for a fact established at a transition: absent, then written
/// once, in the same write that records the transition establishing them, and
/// never revised. There is no duplication to justify, only the record.
///
/// They exist for two reasons. The funding debit is chosen by the transaction
/// the mint assembles, not by the quote that priced it. And a reclaim is not
/// the send running backwards: the notes return through a further federation
/// transaction, which pays the primary module's input and output fees and
/// loses whatever the mint cannot represent in the denominations it reissues.
/// So a [`Canceled`](EcashSendState::Canceled) send restores *less* than it
/// debited, and without these fields the record of a reclaimed send could not
/// be reconciled against the balance at all: it would assert a debit that had
/// been partly given back, with no figure anywhere for how much.
///
/// # Invariants
///
/// The quoted half, at all times:
///
/// - `quoted_total_debited == notes_value + quoted_fee`. That is what the send
///   was quoted to take out of the spendable balance, and what the user
///   approved.
/// - `notes_value >= requested_amount`. A mint rounds a request up, never
///   down; see [`EcashQuote`] for why.
///
/// `realized_total_debited` fills in when the funding transaction is accepted.
/// It may be above or below `quoted_total_debited`, in either direction and by
/// either party's doing; that gap is the whole reason the two are separate
/// fields, and no invariant here bounds one by the other.
///
/// `restored_amount` and `realized_fee` fill in together at settlement — both
/// `None` or both `Some`, never one without the other, because each is the
/// complement of the other against a debit that is by then known. Whenever
/// they are present `realized_total_debited` is present too. Once all three
/// are:
///
/// - `realized_total_debited == restored_amount + realized_fee + delivered`,
///   where `delivered` is [`notes_value`](EcashSendDetails::notes_value) for a
///   [`Redeemed`](EcashSendState::Redeemed) send and zero for a
///   [`Canceled`](EcashSendState::Canceled) one. One equation, both endings:
///   what left the balance either reached the receiver, came back, or was
///   spent getting there. The left-hand side is the realized debit and not the
///   quoted one — substituting the quoted total is exactly the error this
///   record was reshaped to stop.
/// - [`Redeemed`](EcashSendState::Redeemed): `restored_amount == 0` and
///   `realized_fee == realized_total_debited - notes_value`. Nothing came
///   back, so the send cost whatever the funding transaction charged — which
///   is what `quoted_fee` estimated and need not equal. The receiver's own
///   reissue is charged to the receiver.
/// - [`Canceled`](EcashSendState::Canceled): `realized_fee ==
///   realized_total_debited - restored_amount`, and therefore
///   `restored_amount <= notes_value <= realized_total_debited` and
///   `realized_fee >= realized_total_debited - notes_value` — never below the
///   funding fee that is already sunk, because the reclaim adds its own cost
///   on top of it. These are stated as bounds rather than strict comparisons
///   only so that a federation charging nothing cannot violate the record's
///   own contract; under any non-zero fee schedule, which is the ordinary
///   case, a reclaim strictly restores less than the send debited.
///
/// A caller reconciling a receipt against the balance should read
/// `realized_total_debited` as the movement at creation and `restored_amount`
/// as the movement at settlement. `realized_fee` is the number to show as
/// "this cost you", for either ending.
///
/// `Debug` is derived rather than written by hand, deliberately: [`Notes`]
/// redacts its own `Debug`, so a derive keeps the bearer token out of every
/// log line, tracing span and assertion message that renders this record —
/// and keeps doing so without this type having to remember to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EcashSendDetails {
    /// The notes handed to the caller: the same value as
    /// [`EcashSend::notes`].
    ///
    /// Kept here because it is the artifact the whole operation exists to
    /// produce, and no state carries it. This record therefore holds
    /// spendable value for as long as the notes are unredeemed, which is what
    /// makes it useful — it is the caller's own bearer artifact, not a
    /// secret they did not already have.
    pub notes: Notes,
    /// Quoted: what the caller asked [`Ecash::quote`] for.
    ///
    /// Kept so that a receipt can show what was requested beside what was
    /// actually issued. It is **not** the figure the balance moved by, and
    /// activity history deliberately does not report it — see
    /// [`ActivityItem`](crate::ActivityItem)'s note on requested versus
    /// actual.
    pub requested_amount: Amount,
    /// Quoted: the value the notes actually carry, which is what the receiver
    /// can redeem if they get there before the reclaim does.
    ///
    /// At or above [`requested_amount`](EcashSendDetails::requested_amount),
    /// because a mint issues fixed denominations and rounds a request up
    /// (mintv2 to a multiple of 512 msat). This is the figure activity
    /// history reports as an ecash send's
    /// [`amount`](crate::ActivityItem::amount).
    pub notes_value: Amount,
    /// Quoted: what issuing and selecting those notes cost, on top of
    /// [`notes_value`](EcashSendDetails::notes_value).
    ///
    /// Bound by the executed quote, so it is known before the creating call
    /// returns and never fills in later. Zero where the notes already held
    /// could be handed over as they were.
    ///
    /// Named for the half it belongs to. It is what the *funding* transaction
    /// was quoted at, which estimates the whole cost of a send the receiver
    /// redeems and only the first instalment of the cost of one that is
    /// reclaimed — the rest is in
    /// [`realized_fee`](EcashSendDetails::realized_fee), which is also where
    /// the funding transaction's own charge is finally reported.
    pub quoted_fee: Amount,
    /// Quoted: what the send was quoted to take out of the spendable balance —
    /// [`notes_value`](EcashSendDetails::notes_value) plus
    /// [`quoted_fee`](EcashSendDetails::quoted_fee).
    ///
    /// The number the user said yes to. An earlier revision of this record
    /// called it quoted and realized at once, and kept it as the only debit
    /// figure, on the grounds that [`Ecash::send`] either follows the approved
    /// plan or refuses it with
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged). That is **retracted**:
    /// [`EcashQuote::total`] gives the mechanism — the send fee quote reserves
    /// nothing and the spend takes no maximum total debit — so what the funding
    /// took is
    /// [`realized_total_debited`](EcashSendDetails::realized_total_debited),
    /// and this field answers "what did you agree to", never "what did you
    /// pay".
    ///
    /// It is **not** what the send finally cost either. A reclaimed send gives
    /// part of the debit back, at a further cost of its own; read
    /// [`restored_amount`](EcashSendDetails::restored_amount) and
    /// [`realized_fee`](EcashSendDetails::realized_fee) for that.
    ///
    /// Recorded rather than left to each caller to add up, so that no
    /// generated binding has to redo checked arithmetic on money to recover
    /// the number a receipt shows.
    pub quoted_total_debited: Amount,
    /// Realized: what actually left the spendable balance to fund this send,
    /// as the accepted funding transaction charged it.
    ///
    /// `None` until that transaction is accepted, then written once and never
    /// revised. It can differ from
    /// [`quoted_total_debited`](EcashSendDetails::quoted_total_debited) in
    /// **either** direction: the mint decides which notes to spend, which to
    /// reissue, what change to make and what dust it cannot represent when it
    /// assembles the transaction, and it takes no maximum from this SDK.
    /// [`EcashQuote::total`] sets out why that gap cannot be closed from here.
    ///
    /// This is the figure a "you paid" line must read from at creation, and
    /// the figure the record's reconciliation identity is written against — not
    /// [`quoted_total_debited`](EcashSendDetails::quoted_total_debited).
    ///
    /// Zero and absent are different answers, as everywhere else in this crate:
    /// `Some(0)` would say the send was funded without moving anything, absent
    /// says no funding transaction has been accepted yet.
    pub realized_total_debited: Option<Amount>,
    /// Realized: what a reclaim actually put back into the spendable
    /// balance.
    ///
    /// `None` until the send reaches a final state, then written once and
    /// never revised:
    ///
    /// - [`Canceled`](EcashSendState::Canceled) — the value the reclaim
    ///   restored. Less than
    ///   [`realized_total_debited`](EcashSendDetails::realized_total_debited),
    ///   because the notes come back through a federation transaction that
    ///   charges the primary module's fees and drops what the reissued
    ///   denominations cannot represent.
    /// - [`Redeemed`](EcashSendState::Redeemed) — zero. The receiver has the
    ///   value; nothing came back.
    ///
    /// Zero and absent are different answers, as everywhere else in this
    /// crate: zero says the send settled and gave nothing back, absent says
    /// it has not settled and the question has no answer yet. A caller must
    /// not read absence as zero, and must not assume a reclaim restores what
    /// the send debited — that assumption is precisely what this field exists
    /// to correct.
    pub restored_amount: Option<Amount>,
    /// Realized: what the send finally cost, aggregated over every
    /// transaction it took.
    ///
    /// `None` until the send reaches a final state, written once alongside
    /// [`restored_amount`](EcashSendDetails::restored_amount) — the two
    /// always fill in together — and never revised. It is the number to show
    /// as "this cost you", for either ending:
    ///
    /// - [`Redeemed`](EcashSendState::Redeemed) —
    ///   [`realized_total_debited`](EcashSendDetails::realized_total_debited)
    ///   less [`notes_value`](EcashSendDetails::notes_value): what the funding
    ///   transaction charged, and nothing else, since the receiver pays for
    ///   their own reissue. It is what
    ///   [`quoted_fee`](EcashSendDetails::quoted_fee) estimated and can be
    ///   above or below it — an earlier revision asserted the two were equal
    ///   here, which published Fedimint does not guarantee.
    /// - [`Canceled`](EcashSendState::Canceled) —
    ///   [`realized_total_debited`](EcashSendDetails::realized_total_debited)
    ///   less [`restored_amount`](EcashSendDetails::restored_amount): the
    ///   funding fee, which is sunk, plus what the reclaim itself cost. Never
    ///   below that funding fee.
    ///
    /// This is a cost, not a discrepancy. A user who reclaims their own
    /// unredeemed notes is out this much money for good, and a receipt that
    /// showed only the quoted fee would understate it.
    pub realized_fee: Option<Amount>,
    /// When the automatic reclaim is scheduled for.
    ///
    /// Fixed when the send is created and never rewritten, so this is when
    /// the reclaim was *due* rather than when anything happened: a send that
    /// settles early — the receiver redeems, or
    /// [`request_cancel`](Operation::request_cancel) wins — keeps the
    /// schedule it was created with, and the outcome is read from the state.
    /// Before this moment a receiver can redeem freely; from it the reclaim
    /// is under way, and a receiver who has not redeemed is racing it.
    pub reclaim_at: Timestamp,
    /// When the send was created and the balance debited.
    ///
    /// A local clock reading, like [`ActivityItem::time`](crate::ActivityItem::time)
    /// and with the same caveat: the federation does not attest to it, and a
    /// device with a wrong clock records a wrong time here. Good for
    /// ordering and display, not evidence of when anything happened.
    pub created_at: Timestamp,
}

impl crate::operation::sealed::Sealed for EcashSendDetails {}

impl crate::operation::OperationDetails for EcashSendDetails {}

impl crate::operation::DetailedOperationState for EcashSendState {
    type Details = EcashSendDetails;
}

/// The lifecycle of redeeming out-of-band ecash notes.
///
/// This maps one-to-one onto upstream `fedimint-mint-client`'s
/// `ReissueExternalNotesState` (`Created`, `Issuing`, `Done`,
/// `Failed(String)`); there is no collapsing or renaming here beyond
/// carrying the failure reason as a named field so it crosses a
/// foreign-function boundary as a record rather than a positional tuple.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EcashReceiveState {
    /// The redemption has been accepted locally and is about to be
    /// submitted to the federation.
    Created,
    /// The federation is reissuing the notes to this client.
    Issuing,
    /// Final: the notes were reissued and their value is spendable. What the
    /// reissue actually charged, and what the balance therefore rose by, are
    /// recorded on [`EcashReceiveDetails`] as this state is written.
    Done,
    /// Final: the notes could not be redeemed — most often because they
    /// were already spent or had been reclaimed by the sender.
    Failed {
        /// Human-readable explanation. Diagnostic only — not a stable
        /// contract, and not something to match on.
        reason: String,
    },
}

impl crate::operation::sealed::Sealed for EcashReceiveState {}

impl OperationState for EcashReceiveState {
    fn is_final(&self) -> bool {
        match self {
            EcashReceiveState::Created | EcashReceiveState::Issuing => false,
            EcashReceiveState::Done | EcashReceiveState::Failed { .. } => true,
        }
    }
}

/// What an ecash redemption *is*, as opposed to where it has got to.
///
/// The persisted record for an
/// [`Operation<EcashReceiveState>`](crate::Operation), read with
/// [`Operation::details`](crate::Operation::details). Like
/// [`EcashSendDetails`] it has two halves — see [Quoted terms versus realized
/// movement](Ecash#quoted-terms-versus-realized-movement) — and every field
/// says which one it belongs to. [`EcashReceiveState`] carries no amounts at
/// all, only a diagnostic reason on failure, so this record is the whole of
/// what a redemption can be receipted from.
///
/// # Expected, not quoted, and an estimate either way
///
/// There is no quote on this side ([`Ecash::receive`] explains why: a
/// redemption offers the caller no decision), so the up-front figures here
/// are *expected* rather than quoted. They are computed locally, before the
/// reissue is submitted, from the notes' own face value and the fee schedule
/// in the configuration this client already holds — which is what lets a
/// pending redemption be rendered at all, and what a "you will receive"
/// screen shows.
///
/// An earlier revision of this record presented those two figures as plain
/// fact, on the reasoning that a locally computable fee is a known fee. It is
/// not. A federation fixes a transaction's own costs when it assembles and
/// accepts that transaction: the primary module's input and output fees, the
/// denominations it chooses to reissue into, the change it has to make and
/// the dust it cannot represent. The local computation predicts those; it
/// does not decide them. Presenting a prediction as the credit that landed
/// would make this record assert something false of every redemption that
/// settled on different terms, and of every one that
/// [`Failed`](EcashReceiveState::Failed) and credited nothing at all.
///
/// So the fee and the credit appear twice, as an expected pair fixed at
/// creation and a realized pair written when the operation settles. The cost
/// is two extra fields; what it buys is a record that is true at every point
/// in a redemption's life instead of only at the end of a successful one.
///
/// # Invariants
///
/// - `expected_net_credit == notes_value - expected_fee`. The estimate is
///   internally consistent: the fee comes *out of* the notes rather than
///   being charged on top of them, which is why a receive nets down where a
///   send totals up.
/// - The realized pair fills in *together* — both `None` or both `Some`,
///   never one without the other. They are written in the same transition, and
///   a record calling one of them known while the other is unknown would be
///   contradicting itself about whether the redemption has settled.
/// - For a [`Done`](EcashReceiveState::Done) redemption,
///   `realized_net_credit == notes_value - realized_fee`, and that credit is
///   what the balance actually rose by. It may be smaller or larger than
///   `expected_net_credit`; that difference is the whole reason both pairs
///   are here.
/// - For a [`Failed`](EcashReceiveState::Failed) one, both realized figures
///   are zero. Nothing was reissued, so the notes' value never entered the
///   balance and a rejected transaction is not charged for. The identity
///   above does not apply, because there was no transfer for it to describe —
///   two zeros say "nothing moved", which is exactly what a receipt for a
///   failed redemption has to be able to say.
///
/// `Debug` is derived, for the reason given on [`EcashSendDetails`]: [`Notes`]
/// redacts itself, and a derive inherits that.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EcashReceiveDetails {
    /// The notes this redemption consumed — the ones handed to
    /// [`Ecash::receive`].
    ///
    /// Kept because no state carries them and a redemption that has to be
    /// looked up by id must still be able to say which notes it was about: to
    /// receipt a success, to diagnose an [`EcashReceiveState::Failed`] that
    /// lost the race against the sender's reclaim, or to recognise a second
    /// submission of the same notes. While the redemption is pending these
    /// are still bearer value, which is the other reason [`Notes`] redacts
    /// its own `Debug`.
    pub notes: Notes,
    /// The gross face value redeemed, before any reissuance fee.
    ///
    /// A fact rather than an estimate, and the one amount here that needs no
    /// half: the notes state their own value, so this is fixed when the
    /// redemption is created and cannot be revised by anything the federation
    /// decides later.
    ///
    /// This is the figure activity history reports as an ecash receive's
    /// [`amount`](crate::ActivityItem::amount), and it is what the sender
    /// gave up — not what this wallet gains; see
    /// [`realized_net_credit`](EcashReceiveDetails::realized_net_credit).
    pub notes_value: Amount,
    /// Expected: the reissuance fee predicted from the federation's fee
    /// schedule before the reissue is submitted, taken out of
    /// [`notes_value`](EcashReceiveDetails::notes_value) rather than charged
    /// on top of it.
    ///
    /// Known before [`Ecash::receive`] returns, which is what makes a pending
    /// redemption renderable. An estimate of what the accepted transaction
    /// will charge, not a statement of what it did charge — that is
    /// [`realized_fee`](EcashReceiveDetails::realized_fee).
    pub expected_fee: Amount,
    /// Expected: what the balance should rise by —
    /// [`notes_value`](EcashReceiveDetails::notes_value) minus
    /// [`expected_fee`](EcashReceiveDetails::expected_fee).
    ///
    /// The number to show as "you will receive" while the redemption is in
    /// flight. Recorded rather than derived for the same reason
    /// [`EcashSendDetails::quoted_total_debited`] is: it is a figure a receipt
    /// needs, and no binding should have to do fallible arithmetic on money to
    /// recover it.
    pub expected_net_credit: Amount,
    /// Realized: what the accepted reissue actually charged.
    ///
    /// `None` until the redemption reaches a final state, then written once,
    /// in the same write that records that transition, and never revised.
    /// This is the placement rule's case 3 shape with nothing to duplicate:
    /// [`EcashReceiveState`] carries no amounts, so no amount of watching
    /// would recover this and the record is its only home.
    ///
    /// Zero for a [`Failed`](EcashReceiveState::Failed) redemption, because a
    /// transaction the federation rejected is not charged for — should an
    /// implementation find a failure that did cost something, this field
    /// reports that cost. The rule is "what actually moved", never "zero on
    /// failure".
    pub realized_fee: Option<Amount>,
    /// Realized: what the balance actually rose by.
    ///
    /// `None` until the redemption settles, and present exactly when
    /// [`realized_fee`](EcashReceiveDetails::realized_fee) is. This is the
    /// number to show as "you received", and the only figure here that
    /// reconciles with the balance:
    ///
    /// - [`Done`](EcashReceiveState::Done) —
    ///   [`notes_value`](EcashReceiveDetails::notes_value) less
    ///   [`realized_fee`](EcashReceiveDetails::realized_fee), which may sit a
    ///   little above or below
    ///   [`expected_net_credit`](EcashReceiveDetails::expected_net_credit).
    /// - [`Failed`](EcashReceiveState::Failed) — zero. The notes were not
    ///   reissued, so their value never entered the balance; this is not
    ///   `notes_value` minus a zero fee.
    pub realized_net_credit: Option<Amount>,
    /// When the redemption was created.
    ///
    /// A local clock reading, with the same caveat as
    /// [`EcashSendDetails::created_at`].
    pub created_at: Timestamp,
}

impl crate::operation::sealed::Sealed for EcashReceiveDetails {}

impl crate::operation::OperationDetails for EcashReceiveDetails {}

impl crate::operation::DetailedOperationState for EcashReceiveState {
    type Details = EcashReceiveDetails;
}

/// Placeholder for the mint-module state this facade operates on.
#[derive(Debug)]
struct EcashInner;

/// Placeholder for a quote's frozen plan: the requested amount, the notes
/// selected to satisfy it and the denominations they will be issued in, the
/// fee, and the note inventory and configuration context all of those were
/// computed against. Held by value rather than behind an `Arc`, because a
/// quote is owned by one caller and consumed once, never shared.
#[derive(Debug)]
struct EcashQuoteInner;

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for a real bearer token. No part of this string may appear
    /// in the `Debug` output of a record that carries it.
    const TOKEN: &str = "notes-secret-bearer-value-0123456789";

    /// Nothing, as a sum of money. Zero and absent are different answers
    /// throughout this record, and the tests below have to be able to say the
    /// first one.
    const NOTHING: Amount = Amount::from_msats(0);

    /// A send whose numbers are the case this facade was reworked for: 1234
    /// msat requested, satisfied by 1536 msat of notes (three 512-msat
    /// multiples, as mintv2 rounds), with a fee on top.
    ///
    /// The funding transaction has been accepted — the notes are out in the
    /// world, so it must have been — and it charged 68 msat where the quote
    /// said 64, which is the gap [`EcashQuote::total`] retracts the ceiling
    /// claim over. The settlement pair does not exist yet: nobody has redeemed
    /// the notes and no reclaim has run.
    fn send_details() -> EcashSendDetails {
        EcashSendDetails {
            notes: Notes::from_raw(TOKEN.to_owned()),
            requested_amount: Amount::from_msats(1_234),
            notes_value: Amount::from_msats(1_536),
            quoted_fee: Amount::from_msats(64),
            quoted_total_debited: Amount::from_msats(1_600),
            realized_total_debited: Some(Amount::from_msats(1_604)),
            restored_amount: None,
            realized_fee: None,
            reclaim_at: Timestamp::from_epoch_millis(1_700_086_400_000),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    /// The same send after the receiver redeemed it: nothing came back, so the
    /// cost is what the funding transaction charged — 68 msat, not the 64 the
    /// quote named.
    fn redeemed_send_details() -> EcashSendDetails {
        EcashSendDetails {
            restored_amount: Some(NOTHING),
            realized_fee: Some(Amount::from_msats(68)),
            ..send_details()
        }
    }

    /// The same send after the reclaim won. The reclaim is a transaction of
    /// its own: 128 msat of it went on the primary module's fees and the dust
    /// the reissued denominations could not represent, so 1408 of the 1536
    /// msat of notes came back and the send cost 196 msat all told.
    fn canceled_send_details() -> EcashSendDetails {
        EcashSendDetails {
            restored_amount: Some(Amount::from_msats(1_408)),
            realized_fee: Some(Amount::from_msats(196)),
            ..send_details()
        }
    }

    /// A redemption in flight: the expected pair is computed from the fee
    /// schedule, and nothing is realized yet.
    fn receive_details() -> EcashReceiveDetails {
        EcashReceiveDetails {
            notes: Notes::from_raw(TOKEN.to_owned()),
            notes_value: Amount::from_msats(1_536),
            expected_fee: Amount::from_msats(36),
            expected_net_credit: Amount::from_msats(1_500),
            realized_fee: None,
            realized_net_credit: None,
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    /// The same redemption, accepted on terms the local estimate did not
    /// predict exactly: the federation charged 52 rather than 36 msat.
    fn done_receive_details() -> EcashReceiveDetails {
        EcashReceiveDetails {
            realized_fee: Some(Amount::from_msats(52)),
            realized_net_credit: Some(Amount::from_msats(1_484)),
            ..receive_details()
        }
    }

    /// The same redemption, rejected: nothing was reissued, so nothing was
    /// charged and nothing landed.
    fn failed_receive_details() -> EcashReceiveDetails {
        EcashReceiveDetails {
            realized_fee: Some(NOTHING),
            realized_net_credit: Some(NOTHING),
            ..receive_details()
        }
    }

    /// Generic over the pattern rather than over one kind, like the probe in
    /// [`crate::operation`]'s tests: this compiles only if the state type
    /// names its record and that record satisfies every bound
    /// [`crate::OperationDetails`] imposes.
    fn round_trip_details<S: crate::operation::DetailedOperationState>(
        details: S::Details,
    ) -> S::Details {
        details
    }

    #[test]
    fn ecash_send_state_names_its_details_record() {
        let details = send_details();
        assert_eq!(
            round_trip_details::<EcashSendState>(details.clone()),
            details
        );
    }

    #[test]
    fn ecash_receive_state_names_its_details_record() {
        let details = receive_details();
        assert_eq!(
            round_trip_details::<EcashReceiveState>(details.clone()),
            details
        );
    }

    #[test]
    fn ecash_send_details_quoted_total_debited_is_notes_value_plus_the_quoted_fee() {
        let details = send_details();
        assert_eq!(
            details.notes_value.checked_add(details.quoted_fee),
            Some(details.quoted_total_debited)
        );
    }

    #[test]
    fn ecash_send_details_notes_value_is_never_below_the_requested_amount() {
        let details = send_details();
        assert!(details.notes_value >= details.requested_amount);
        // The whole reason `Ecash::quote` exists: the two genuinely differ,
        // and the difference is debited from the sender.
        assert_ne!(details.notes_value, details.requested_amount);
        assert!(details.quoted_total_debited > details.requested_amount);
    }

    /// The assertion the retraction of the ceiling claim rests on: the funding
    /// debit that actually lands can exceed the one that was quoted, because
    /// the send fee quote reserves nothing and the spend takes no maximum. An
    /// earlier revision kept a single plain `total_debited` and asserted the
    /// quoted total *was* the movement, which published 0.12 cannot enforce.
    #[test]
    fn ecash_send_details_realized_debit_may_exceed_the_quoted_total() {
        let details = send_details();
        assert!(details.realized_total_debited > Some(details.quoted_total_debited));
        // Neither is redundant, and settlement does not revise the quoted half.
        assert_eq!(
            canceled_send_details().quoted_total_debited,
            details.quoted_total_debited
        );
    }

    /// And it can land below the quote, or stay absent while nothing has been
    /// accepted to charge for.
    #[test]
    fn ecash_send_details_realized_debit_may_be_below_the_quoted_total_or_absent() {
        let cheaper = EcashSendDetails {
            realized_total_debited: Some(Amount::from_msats(1_580)),
            ..send_details()
        };
        assert!(cheaper.realized_total_debited < Some(cheaper.quoted_total_debited));

        let unfunded = EcashSendDetails {
            realized_total_debited: None,
            ..send_details()
        };
        assert_eq!(unfunded.realized_total_debited, None);
        // A send with no established debit still has terms to show.
        assert_eq!(
            unfunded.quoted_total_debited,
            send_details().quoted_total_debited
        );
    }

    #[test]
    fn ecash_send_details_settlement_figures_are_absent_until_the_send_settles() {
        // While the notes are out in the world the settlement pair does not
        // exist: the reclaim has not run, and the receiver has not redeemed.
        // The funding debit is already known, because the notes exist.
        let unsettled = send_details();
        assert_eq!(unsettled.restored_amount, None);
        assert_eq!(unsettled.realized_fee, None);
        assert!(unsettled.realized_total_debited.is_some());

        // The pair fills in together, never one without the other, and never
        // without the debit they are complements of.
        for settled in [redeemed_send_details(), canceled_send_details()] {
            assert_eq!(
                settled.restored_amount.is_some(),
                settled.realized_fee.is_some()
            );
            assert!(settled.restored_amount.is_some());
            assert!(settled.realized_total_debited.is_some());
        }
    }

    #[test]
    fn ecash_send_details_a_reclaim_restores_less_than_was_debited() {
        let details = canceled_send_details();
        let restored = details.restored_amount.expect("a canceled send settled");
        let debited = details
            .realized_total_debited
            .expect("the funding transaction was accepted");

        // The finding this field exists for: a reclaim is a transaction of its
        // own, so the value that comes back is smaller than the value that
        // left. Anything reading the debit as "and then it all came back" is
        // wrong about the balance.
        assert!(restored < debited);
        // Smaller than the notes themselves, too: the shortfall is the
        // reclaim's own cost, not just the funding fee being kept.
        assert!(restored < details.notes_value);
    }

    #[test]
    fn ecash_send_details_a_canceled_sends_realized_fee_is_the_debit_less_what_came_back() {
        let details = canceled_send_details();
        let restored = details.restored_amount.expect("a canceled send settled");
        let debited = details
            .realized_total_debited
            .expect("the funding transaction was accepted");

        // What it cost is exactly what did not come back — measured against the
        // realized debit, not the quoted one.
        assert_eq!(debited.checked_sub(restored), details.realized_fee);
        // The funding fee is sunk and the reclaim adds its own cost on top, so
        // the realized figure is never below what the funding actually charged
        // — and, wherever the reclaim cost anything at all, is strictly above
        // it.
        let realized = details.realized_fee.expect("a canceled send settled");
        let funding = debited
            .checked_sub(details.notes_value)
            .expect("the debit covers the notes");
        assert!(realized > funding);
    }

    #[test]
    fn ecash_send_details_a_redeemed_send_restores_nothing_and_costs_what_funding_charged() {
        let details = redeemed_send_details();
        let debited = details
            .realized_total_debited
            .expect("the funding transaction was accepted");

        // Zero, not absent: the send settled, and what it gave back was
        // nothing at all.
        assert_eq!(details.restored_amount, Some(NOTHING));
        assert_ne!(details.restored_amount, None);
        // The receiver pays for their own reissue, so the sender's cost is what
        // the funding transaction charged — which is *not* the quoted fee. An
        // earlier revision asserted the two were equal.
        assert_eq!(
            details.realized_fee,
            debited.checked_sub(details.notes_value)
        );
        assert_ne!(details.realized_fee, Some(details.quoted_fee));
    }

    #[test]
    fn ecash_send_details_both_endings_account_for_every_millisatoshi_debited() {
        // One equation, both endings: what left the balance either reached the
        // receiver, came back, or was spent getting there. The left-hand side
        // is the realized debit, never the quoted total.
        let redeemed = redeemed_send_details();
        let delivered = redeemed.notes_value;
        assert_eq!(
            redeemed
                .restored_amount
                .and_then(|restored| restored.checked_add(redeemed.realized_fee?))
                .and_then(|accounted| accounted.checked_add(delivered)),
            redeemed.realized_total_debited
        );

        // Nothing was delivered on a reclaim, so the same sum has two terms.
        let canceled = canceled_send_details();
        assert_eq!(
            canceled
                .restored_amount
                .and_then(|restored| restored.checked_add(canceled.realized_fee?)),
            canceled.realized_total_debited
        );
    }

    #[test]
    fn ecash_receive_details_expected_net_credit_is_notes_value_minus_the_expected_fee() {
        let details = receive_details();
        assert_eq!(
            details.notes_value.checked_sub(details.expected_fee),
            Some(details.expected_net_credit)
        );
        // A receive nets down where a send totals up: the fee comes out of
        // the notes rather than being charged on top of them.
        assert!(details.expected_net_credit < details.notes_value);
    }

    #[test]
    fn ecash_receive_details_realized_figures_are_absent_until_the_reissue_settles() {
        let pending = receive_details();
        assert_eq!(pending.realized_fee, None);
        assert_eq!(pending.realized_net_credit, None);

        for settled in [done_receive_details(), failed_receive_details()] {
            assert_eq!(
                settled.realized_fee.is_some(),
                settled.realized_net_credit.is_some(),
            );
            assert!(settled.realized_fee.is_some());
        }
    }

    #[test]
    fn ecash_receive_details_an_accepted_reissue_may_cost_more_than_predicted() {
        let details = done_receive_details();

        assert_eq!(
            details.notes_value.checked_sub(
                details
                    .realized_fee
                    .expect("an accepted reissue recorded its fee")
            ),
            details.realized_net_credit
        );
        // The reason both pairs exist: the federation fixed the reissue's
        // costs when it accepted the transaction, and the local estimate did
        // not predict them exactly.
        assert_ne!(details.realized_fee, Some(details.expected_fee));
        assert!(details.realized_net_credit < Some(details.expected_net_credit));
    }

    #[test]
    fn ecash_receive_details_a_failed_reissue_moved_nothing() {
        let details = failed_receive_details();

        // Nothing was reissued, so the notes' value never entered the balance
        // and no transaction was charged for. Explicitly not `notes_value`
        // minus a zero fee.
        assert_eq!(details.realized_net_credit, Some(NOTHING));
        assert_eq!(details.realized_fee, Some(NOTHING));
        assert_ne!(details.realized_net_credit, Some(details.notes_value));
        // And zero is not absence: the redemption settled, and this is the
        // answer.
        assert_ne!(details.realized_net_credit, None);
    }

    #[test]
    fn ecash_send_details_debug_redacts_the_notes_but_keeps_the_numbers() {
        let rendered = format!("{:?}", canceled_send_details());
        assert!(!rendered.contains(TOKEN), "{rendered}");
        assert!(rendered.contains("Notes(<redacted>)"), "{rendered}");
        // A details record exists to be rendered and logged, so everything
        // that is not the bearer token has to survive `Debug` — the realized
        // figures a reclaimed send is receipted from included.
        assert!(rendered.contains("1536"), "{rendered}");
        assert!(rendered.contains("1600"), "{rendered}");
        assert!(rendered.contains("1604"), "{rendered}");
        assert!(rendered.contains("1408"), "{rendered}");
        assert!(rendered.contains("196"), "{rendered}");
    }

    #[test]
    fn ecash_receive_details_debug_redacts_the_notes_but_keeps_the_numbers() {
        let rendered = format!("{:?}", done_receive_details());
        assert!(!rendered.contains(TOKEN), "{rendered}");
        assert!(rendered.contains("Notes(<redacted>)"), "{rendered}");
        assert!(rendered.contains("1500"), "{rendered}");
        assert!(rendered.contains("1484"), "{rendered}");
    }

    #[test]
    fn ecash_send_state_created_is_not_final() {
        assert!(!EcashSendState::Created.is_final());
    }

    #[test]
    fn ecash_send_state_cancel_requested_is_not_final() {
        assert!(!EcashSendState::CancelRequested.is_final());
    }

    #[test]
    fn ecash_send_state_canceled_is_final() {
        assert!(EcashSendState::Canceled.is_final());
    }

    #[test]
    fn ecash_send_state_redeemed_is_final() {
        assert!(EcashSendState::Redeemed.is_final());
    }

    #[test]
    fn ecash_receive_state_created_is_not_final() {
        assert!(!EcashReceiveState::Created.is_final());
    }

    #[test]
    fn ecash_receive_state_issuing_is_not_final() {
        assert!(!EcashReceiveState::Issuing.is_final());
    }

    #[test]
    fn ecash_receive_state_done_is_final() {
        assert!(EcashReceiveState::Done.is_final());
    }

    #[test]
    fn ecash_receive_state_failed_is_final() {
        assert!(
            EcashReceiveState::Failed {
                reason: String::new(),
            }
            .is_final()
        );
    }
}
