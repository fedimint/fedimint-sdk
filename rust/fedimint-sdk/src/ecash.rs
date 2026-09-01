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
    /// it back to [`Ecash::send`], which executes exactly what was shown. A
    /// user cannot be quoted one debit and charged another, and no send takes
    /// value the caller was never shown.
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
    /// Execution follows the plan exactly — same note value, same fee, same
    /// total debit — or it does not happen:
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired) if the quote's
    /// validity window has passed,
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) if something the
    /// quote depends on moved underneath it (the notes it planned to spend
    /// went to another operation, the federation's fee schedule or
    /// configuration changed). Both mean the same thing to a caller: quote
    /// again and re-confirm with the user.
    ///
    /// The balance is debited by [`EcashQuote::total`] and the returned
    /// [`EcashSend::notes`] are ready to hand to a receiver. Until someone
    /// redeems them the value is in limbo: it is no longer spendable by the
    /// sender, and it is not yet the receiver's either.
    ///
    /// # Automatic reclaim
    ///
    /// Notes that go unredeemed do not vanish. The SDK schedules an
    /// automatic reclaim, so a send to someone who never opens the message
    /// eventually returns to the sender's balance instead of being lost.
    /// The default period is **one day**, matching what the existing
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
    /// redeeming at all. Nothing is hidden by that — the gross value, the
    /// fee, and the net credit are all recorded in [`EcashReceiveDetails`]
    /// before this call returns, so a receipt never depends on having
    /// watched the operation.
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
/// because that is the number their balance moves by; showing them the
/// amount they typed instead would be showing them the one figure that is
/// guaranteed not to be what they pay.
///
/// A quote is also the SDK's own record of what it committed to. The fee and
/// the resolved note value are quoted once and appear nowhere in the send's
/// progress stream, so the executed quote is what lets
/// [`EcashSendDetails`] report the terms a receipt needs, for the whole life
/// of the operation and after a restart.
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

    /// What issuing and selecting those notes will cost, on top of
    /// [`EcashQuote::notes_value`].
    ///
    /// Zero when the notes already held can be handed over as they are.
    /// Non-zero when they have to be re-issued to assemble the value, which
    /// is a fee the caller pays for the shape of their own note inventory
    /// rather than for anything the receiver gets.
    pub fn fee(&self) -> Amount {
        unimplemented!()
    }

    /// The total amount that will be debited from the balance:
    /// [`EcashQuote::notes_value`] plus [`EcashQuote::fee`].
    ///
    /// This is the number to show as "you will pay".
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
/// The notes, the amounts, and the fee the executed quote bound are all
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
    /// spendable balance.
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
/// read with [`Operation::details`](crate::Operation::details). Every field
/// is fixed when the send is created: the artifact it produced, and the terms
/// the executed [`EcashQuote`] committed to. That is case 1 of
/// [`OperationDetails`](crate::OperationDetails)'s placement rule — such
/// values live in the record and nowhere else — and it is why nothing here is
/// an `Option`: no field on this record is established by a later transition,
/// so none of them fills in after the fact.
///
/// Without this record an ecash send would be the operation least able to
/// survive a restart. [`EcashSendState`] carries no payload at all; its four
/// variants say only where the send has got to. So the notes, the amounts and
/// the fee would exist solely in the value [`Ecash::send`] returned, and an
/// application that restarted before delivering them would have debited its
/// user for bearer value it could no longer display, receipt, or even name.
///
/// # Invariants
///
/// - `total_debited == notes_value + fee`. That is what left the spendable
///   balance.
/// - `notes_value >= requested_amount`. A mint rounds a request up, never
///   down; see [`EcashQuote`] for why.
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
    /// What the caller asked [`Ecash::quote`] for.
    ///
    /// Kept so that a receipt can show what was requested beside what was
    /// actually issued. It is **not** the figure the balance moved by, and
    /// activity history deliberately does not report it — see
    /// [`ActivityItem`](crate::ActivityItem)'s note on requested versus
    /// actual.
    pub requested_amount: Amount,
    /// The value the notes actually carry, which is what the receiver can
    /// redeem.
    ///
    /// At or above [`requested_amount`](EcashSendDetails::requested_amount),
    /// because a mint issues fixed denominations and rounds a request up
    /// (mintv2 to a multiple of 512 msat). This is the figure activity
    /// history reports as an ecash send's
    /// [`amount`](crate::ActivityItem::amount).
    pub notes_value: Amount,
    /// What issuing and selecting those notes cost, on top of
    /// [`notes_value`](EcashSendDetails::notes_value).
    ///
    /// Bound by the executed quote, so it is known before the creating call
    /// returns and never fills in later. Zero where the notes already held
    /// could be handed over as they were.
    pub fee: Amount,
    /// What left the spendable balance:
    /// [`notes_value`](EcashSendDetails::notes_value) plus
    /// [`fee`](EcashSendDetails::fee).
    ///
    /// Recorded rather than left to each caller to add up. It is the number
    /// the user approved on the quote and the number a receipt shows, and
    /// storing it means no generated binding has to redo checked arithmetic
    /// on money to recover it.
    pub total_debited: Amount,
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
    /// Final: the notes were reissued and their value is spendable.
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
/// [`Operation::details`](crate::Operation::details). As with
/// [`EcashSendDetails`], every field is fixed when the redemption is created,
/// which is case 1 of [`OperationDetails`](crate::OperationDetails)'s
/// placement rule: it lives here and nowhere else, and nothing on it is an
/// `Option` because nothing on it is established by a later transition.
/// [`EcashReceiveState`] carries no amounts either — only a diagnostic reason
/// on failure — so this record is the whole of what a redemption can be
/// receipted from.
///
/// # Invariants
///
/// - `net_credit == notes_value - fee`. That is what the balance rises by
///   when the operation reaches [`EcashReceiveState::Done`]. The fee comes
///   *out of* the notes rather than being charged on top of them, which is
///   why a receive nets down where a send totals up.
///
/// # Why the fee is known before the federation answers
///
/// A redemption has no quote, and it does not need one to record its terms:
/// the notes state their own value, and the federation's fee schedule is part
/// of the configuration this client already holds, so the reissuance fee is
/// computed locally before the redemption is submitted rather than learned
/// from the federation afterwards. That is what lets all three amounts be
/// plain values here.
///
/// Should an implementation find otherwise, [`fee`](EcashReceiveDetails::fee)
/// and [`net_credit`](EcashReceiveDetails::net_credit) become `Option`
/// *together* — never one without the other. Each is derivable from the other
/// given [`notes_value`](EcashReceiveDetails::notes_value), so a record
/// reporting one as known and the other as unknown would be contradicting
/// itself.
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
    /// The gross face value redeemed, before the reissuance fee.
    ///
    /// This is the figure activity history reports as an ecash receive's
    /// [`amount`](crate::ActivityItem::amount), and it is what the sender
    /// gave up — not what this wallet gains; see
    /// [`net_credit`](EcashReceiveDetails::net_credit).
    pub notes_value: Amount,
    /// The reissuance fee, taken out of
    /// [`notes_value`](EcashReceiveDetails::notes_value) rather than charged
    /// on top of it.
    pub fee: Amount,
    /// What the balance rises by:
    /// [`notes_value`](EcashReceiveDetails::notes_value) minus
    /// [`fee`](EcashReceiveDetails::fee).
    ///
    /// The number to show as "you received". Recorded rather than derived for
    /// the same reason [`EcashSendDetails::total_debited`] is: it is the
    /// figure a receipt and a balance reconciliation both need, and no
    /// binding should have to compute it.
    pub net_credit: Amount,
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

    /// A send whose numbers are the case this facade was reworked for: 1234
    /// msat requested, satisfied by 1536 msat of notes (three 512-msat
    /// multiples, as mintv2 rounds), with a fee on top.
    fn send_details() -> EcashSendDetails {
        EcashSendDetails {
            notes: Notes::from_raw(TOKEN.to_owned()),
            requested_amount: Amount::from_msats(1_234),
            notes_value: Amount::from_msats(1_536),
            fee: Amount::from_msats(64),
            total_debited: Amount::from_msats(1_600),
            reclaim_at: Timestamp::from_epoch_millis(1_700_086_400_000),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    fn receive_details() -> EcashReceiveDetails {
        EcashReceiveDetails {
            notes: Notes::from_raw(TOKEN.to_owned()),
            notes_value: Amount::from_msats(1_536),
            fee: Amount::from_msats(36),
            net_credit: Amount::from_msats(1_500),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
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
    fn ecash_send_details_total_debited_is_notes_value_plus_fee() {
        let details = send_details();
        assert_eq!(
            details.notes_value.checked_add(details.fee),
            Some(details.total_debited)
        );
    }

    #[test]
    fn ecash_send_details_notes_value_is_never_below_the_requested_amount() {
        let details = send_details();
        assert!(details.notes_value >= details.requested_amount);
        // The whole reason `Ecash::quote` exists: the two genuinely differ,
        // and the difference is debited from the sender.
        assert_ne!(details.notes_value, details.requested_amount);
        assert!(details.total_debited > details.requested_amount);
    }

    #[test]
    fn ecash_receive_details_net_credit_is_notes_value_minus_fee() {
        let details = receive_details();
        assert_eq!(
            details.notes_value.checked_sub(details.fee),
            Some(details.net_credit)
        );
        // A receive nets down where a send totals up: the fee comes out of
        // the notes rather than being charged on top of them.
        assert!(details.net_credit < details.notes_value);
    }

    #[test]
    fn ecash_send_details_debug_redacts_the_notes_but_keeps_the_numbers() {
        let rendered = format!("{:?}", send_details());
        assert!(!rendered.contains(TOKEN), "{rendered}");
        assert!(rendered.contains("Notes(<redacted>)"), "{rendered}");
        // A details record exists to be rendered and logged, so everything
        // that is not the bearer token has to survive `Debug`.
        assert!(rendered.contains("1536"), "{rendered}");
        assert!(rendered.contains("1600"), "{rendered}");
    }

    #[test]
    fn ecash_receive_details_debug_redacts_the_notes_but_keeps_the_numbers() {
        let rendered = format!("{:?}", receive_details());
        assert!(!rendered.contains(TOKEN), "{rendered}");
        assert!(rendered.contains("Notes(<redacted>)"), "{rendered}");
        assert!(rendered.contains("1500"), "{rendered}");
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
