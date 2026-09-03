//! Chaumian ecash: spending notes out of band and redeeming them.

use std::sync::Arc;

use crate::{Amount, Notes, Operation, OperationState, Result, Timestamp};

/// The ecash facade for one federation.
///
/// Obtained from [`Federation::ecash`](crate::Federation::ecash), which
/// returns `None` when the federation has no mint module.
///
/// Ecash here means *out-of-band* ecash: notes the sender takes out of
/// their balance and hands to a receiver over some channel the federation
/// knows nothing about, a chat message, a QR code, a file. The receiver
/// redeems them against the same federation. Ordinary in-federation
/// spending is not a separate concept; it is what lightning and on-chain
/// operations do with the balance.
///
/// [`Ecash::quote`] plans a send and [`Ecash::send`] executes that plan,
/// exactly as [`Lightning::quote`](crate::Lightning::quote) and
/// [`Onchain::quote`](crate::Onchain::quote) do for their kinds of value.
/// Receiving is not quoted, because it presents the caller with no
/// decision: see [`Ecash::receive`].
///
/// Every call on this facade, sending and receiving alike, is refused with
/// [`Recovering`](crate::ErrorCode::Recovering) while a recovery for the
/// federation is incomplete. A wallet whose note set was never fully
/// discovered is not safe to spend from, since a note the rescan never
/// reached can be double-spent.
#[derive(Debug, Clone)]
pub struct Ecash {
    inner: Arc<EcashInner>,
}

impl Ecash {
    /// Plans an out-of-band send and returns an executable quote for it.
    ///
    /// The value that leaves the balance is generally *more* than `amount`:
    /// a mint issues notes in fixed denominations, so the receiver ends up
    /// with the smallest value the mint can represent at or above `amount`,
    /// and assembling that value can itself cost a fee. The returned
    /// [`EcashQuote`] is that plan, frozen: it binds the requested amount,
    /// the note value that will actually be produced, the fee and the total
    /// debit. Show it, then hand it back to [`Ecash::send`], which executes
    /// exactly what was shown.
    ///
    /// `amount` is a floor rather than a promise, the least the receiver
    /// must be able to redeem. [`EcashQuote::notes_value`] is what they will
    /// actually be able to redeem, and it is the number to put in front of a
    /// user beside [`EcashQuote::fee`] and [`EcashQuote::total`].
    ///
    /// Quoting neither debits the balance nor records anything: it plans.
    /// Quotes expire; see [`EcashQuote::expires_at`].
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a zero amount,
    /// which no note can carry,
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance) when
    /// the balance cannot cover the rounded-up note value plus the fee,
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
        // Implementation notes (delete once implemented):
        // - mintv2 rounds a requested amount up to a multiple of 512 msat; that rounded
        //   value is `EcashQuote::notes_value`.
        // - When the wallet holds no combination of notes that adds up, a larger note is
        //   re-issued into smaller ones first; that self-reissue is what `EcashQuote::fee`
        //   charges for (the mint's own fee, the primary module's fee, change and dust).
        // - Bind the note inventory and federation configuration used into the quote, so
        //   a change to either invalidates it as `QuoteChanged` rather than silently
        //   re-deriving a different plan.
        unimplemented!()
    }

    /// Executes a quoted send, taking its value out of the balance as
    /// out-of-band notes.
    ///
    /// The quote is consumed: it describes one send and can fund one send.
    /// Execution follows the plan exactly, same note value, same fee, same
    /// total debit, or it does not happen:
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired) if the quote's
    /// validity window has passed,
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) if something the
    /// quote depends on moved underneath it. Both mean the same thing to a
    /// caller: quote again and re-confirm with the user.
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
    /// eventually returns to the sender's balance instead of being lost. The
    /// moment it is scheduled for is persisted as
    /// [`EcashSendDetails::reclaim_at`], so an application that restarted can
    /// still say when the notes stop being redeemable. Its outcome is
    /// reported as an operation state, like any other:
    /// [`EcashSendState::Canceled`] when the reclaim wins,
    /// [`EcashSendState::Redeemed`] when the receiver got there first.
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
        // Implementation notes (delete once implemented):
        // - Re-check the note inventory and federation configuration the quote was bound
        //   to before spending; a change to either is `QuoteChanged`, not a different debit.
        // - Write `EcashSendDetails` in the same storage transaction that creates the
        //   operation, so a process that dies right after this call still finds the notes,
        //   the amounts and the reclaim time on the next start.
        // - Schedule the automatic reclaim to fire one day after send, matching the
        //   existing JavaScript SDK's default. The exact value is subject to confirmation
        //   when this facade is implemented.
        // - Tuning the reclaim period, or constraining note selection, belongs on a later
        //   additive `quote_with`-style call rather than an options struct here, so it
        //   becomes part of the plan the user approves rather than changing an approved
        //   quote's cost after the fact.
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
    /// There is no quote on this side, because a redemption presents the
    /// caller with no decision: the notes carry the value they carry, and
    /// the reissuance fee comes out of it rather than being charged on top of
    /// it. The gross value, the fee and the net credit are all recorded in
    /// [`EcashReceiveDetails`] before this call returns, so a receipt never
    /// depends on having watched the operation.
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
        // Implementation notes (delete once implemented):
        // - Compute the reissuance fee locally from the notes and the federation's fee
        //   schedule, before submitting, so `EcashReceiveDetails` can be written in full
        //   in the same storage transaction that creates the operation.
        unimplemented!()
    }
}

/// A frozen, executable plan for one out-of-band ecash send.
///
/// Produced by [`Ecash::quote`] and consumed by [`Ecash::send`]. As with
/// [`LnQuote`](crate::LnQuote) and [`OnchainQuote`](crate::OnchainQuote),
/// the accessors expose exactly what a user must approve: display these
/// numbers, then give the quote back.
///
/// The requested amount and the actual note value can differ, and this is
/// the ordinary case rather than an edge case: a mint issues notes in fixed
/// denominations, so a request is satisfied with notes worth at least as
/// much, never less, and assembling those notes can itself cost a fee. So
/// the debit is [`notes_value`](EcashQuote::notes_value) plus
/// [`fee`](EcashQuote::fee), and both can exceed what the user typed. Show
/// [`total`](EcashQuote::total) before the user agrees, because that is the
/// number their balance moves by.
// Implementation notes (delete once implemented):
// - mintv2 rounds a request up to a multiple of 512 msat.
// - The resolved note value and fee are quoted once and appear nowhere in the send's
//   progress stream, so the executed quote is what `EcashSendDetails` copies its terms
//   from, for the whole life of the operation and after a restart.
#[derive(Debug)]
pub struct EcashQuote {
    inner: EcashQuoteInner,
}

impl EcashQuote {
    /// The amount [`Ecash::quote`] was asked for.
    ///
    /// Kept so that a confirmation screen or a receipt can show what was
    /// requested next to what will actually be issued. It is a floor, and it
    /// is not the figure the balance moves by; see [`EcashQuote::total`].
    pub fn requested_amount(&self) -> Amount {
        unimplemented!()
    }

    /// The value the notes will actually carry, what the receiver can
    /// redeem.
    ///
    /// At or above [`EcashQuote::requested_amount`], never below it. This is
    /// the figure activity history reports as an ecash send's
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
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired). A quote can also
    /// stop being executable before this point, if notes it planned to
    /// spend are spent by another operation in the meantime; that is
    /// reported as [`QuoteChanged`](crate::ErrorCode::QuoteChanged). The
    /// remedy for both is the same: quote again and re-confirm.
    pub fn expires_at(&self) -> Timestamp {
        unimplemented!()
    }
}

/// The result of [`Ecash::send`]: the notes to hand over, and the operation
/// that tracks what happens to them.
///
/// Both halves matter. The notes are what the sender transmits; the
/// operation is how the sender learns whether they were redeemed or came
/// back. Dropping the operation does not stop the reclaim timer, it keeps
/// running in the background like any other operation.
///
/// Everything here is also persisted before [`Ecash::send`] returns, and
/// readable afterwards through
/// [`Operation::details`](crate::Operation::details) as an
/// [`EcashSendDetails`], from the operation id alone, in a later process,
/// with nobody having kept this struct. That is what makes an out-of-band
/// send survivable: a sender whose application dies between issuing the
/// notes and delivering them can still find them and still hand them over,
/// instead of holding value nobody can redeem until the reclaim fires.
#[derive(Debug)]
#[non_exhaustive]
pub struct EcashSend {
    /// The notes to give to the receiver. Their value is already out of the
    /// sender's spendable balance, and it is [`EcashQuote::notes_value`],
    /// the value the mint actually issued, not the amount that was
    /// requested.
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
    /// `Ok(())` means the cancellation intent has been committed to local
    /// storage and will survive a restart or a period offline. It does not
    /// mean the federation has been contacted, that a reclaim has been
    /// attempted, or that the notes came back: the SDK pursues the request
    /// in the background from here, so a device offline at the moment of
    /// the call still reclaims once it comes back online.
    ///
    /// The outcome arrives where every other outcome does, as a state:
    /// [`EcashSendState::Canceled`] if the notes came back,
    /// [`EcashSendState::Redeemed`] if the receiver got them first. Between
    /// the request and the outcome the operation sits in
    /// [`EcashSendState::CancelRequested`]. The receiver may be redeeming at
    /// this very moment, and only the federation decides who wins that race.
    ///
    /// Calling this on a send that already reached a final state
    /// ([`EcashSendState::Canceled`] or [`EcashSendState::Redeemed`]) is not
    /// an error: it returns `Ok(())` and does nothing, since no cancellation
    /// is pending and the outcome is already recorded in the state.
    ///
    /// # Errors
    ///
    /// Only failures that stop the intent from being recorded at all:
    /// [`Storage`](crate::ErrorCode::Storage) if the request cannot be
    /// committed durably, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) if the
    /// federation was closed or the SDK shut down, leaving nothing to record
    /// it against. An unreachable federation or a slow guardian is not a
    /// failure of this call: the intent is already durable and the SDK
    /// pursues it in the background.
    pub async fn request_cancel(&self) -> Result<()> {
        // Implementation notes (delete once implemented):
        // - The boundary is deliberate: waiting on the network here would let this call
        //   return `FederationUnreachable` or `Timeout` after the intent was already
        //   durable, leaving the caller unable to tell whether a retry would duplicate a
        //   request already in flight.
        // - This is the only cancellation in the crate, because it is the only place
        //   where cancelling is a real protocol action rather than an attempt to un-send
        //   money that has already moved.
        unimplemented!()
    }
}

/// The lifecycle of an out-of-band ecash send.
///
/// An ecash send has exactly two terminal outcomes: the notes came back
/// ([`Canceled`](Self::Canceled)) or the receiver got them
/// ([`Redeemed`](Self::Redeemed)), because those are the only two things
/// that can happen to the money. There is no failure state: if storage
/// cannot be read, no guardian answers, or the federation handle is closed,
/// that is a failure to *observe* the send, reported as `Err` from
/// [`Operation::state`](crate::Operation::state),
/// [`Operation::await_final`](crate::Operation::await_final) or
/// [`OperationUpdates::next`](crate::OperationUpdates::next), not a state
/// of the send itself. The send keeps running, unaffected by the fact that
/// nobody could see it: bearer notes out in the world can still be redeemed
/// or reclaimed long after some call failed to observe them. See
/// [`Sdk::forget_federation`](crate::Sdk::forget_federation), which refuses
/// while reclaimable outgoing value remains.
// Implementation notes (delete once implemented):
//
// Upstream `fedimint-mint-client` models this as `SpendOOBState`: `Created`,
// `UserCanceledProcessing`, `UserCanceledSuccess`, `UserCanceledFailure`, `Success`,
// `Refunded`. Two of those names mean the opposite of what they suggest read in
// isolation, since they are named from the point of view of the cancellation attempt
// rather than the send: `Success` means the automatic reclaim failed (the receiver
// redeemed), `Refunded` means the reclaim succeeded (the notes returned).
//
// | upstream `SpendOOBState`          | here                                        |
// | ---------------------------------- | ------------------------------------------- |
// | `Created`                          | `Created`                                    |
// | `UserCanceledProcessing`           | `CancelRequested`                            |
// | `UserCanceledSuccess`, `Refunded`  | `Canceled`                                   |
// | `UserCanceledFailure`, `Success`   | `Redeemed`                                   |
//
// The mapping is total. The two pairs collapse because upstream's internal
// distinction (asked for vs. timer fired; won against an explicit cancel vs. no
// cancel at all) is about why, not about what happened to the money.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EcashSendState {
    /// The notes have been issued and handed to the caller. The value has
    /// left the spendable balance; nobody has redeemed or reclaimed it
    /// yet.
    Created,
    /// A reclaim has been requested, either by
    /// [`request_cancel`](Operation::request_cancel) or by the automatic
    /// reclaim timer, and is being processed. Not final: the request may
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
/// is fixed when the send is created and never changes afterwards, so an
/// application that restarted before delivering the notes can still display,
/// receipt or hand them over, from the operation id alone.
///
/// # Invariants
///
/// - `total_debited == notes_value + fee`. That is what left the spendable
///   balance.
/// - `notes_value >= requested_amount`. A mint rounds a request up, never
///   down; see [`EcashQuote`] for why.
///
/// `Debug` output redacts the notes, as [`Notes`] itself does.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EcashSendDetails {
    /// The notes handed to the caller: the same value as
    /// [`EcashSend::notes`].
    ///
    /// Kept here because it is the artifact the whole operation exists to
    /// produce, and no state carries it. This record therefore holds
    /// spendable value for as long as the notes are unredeemed: it is the
    /// caller's own bearer artifact, not a secret they did not already have.
    pub notes: Notes,
    /// What the caller asked [`Ecash::quote`] for.
    ///
    /// Kept so that a receipt can show what was requested beside what was
    /// actually issued. It is not the figure the balance moved by, and
    /// activity history deliberately does not report it; see
    /// [`ActivityItem`](crate::ActivityItem)'s note on requested versus
    /// actual.
    pub requested_amount: Amount,
    /// The value the notes actually carry, which is what the receiver can
    /// redeem.
    ///
    /// At or above [`requested_amount`](EcashSendDetails::requested_amount),
    /// because a mint issues fixed denominations and rounds a request up to
    /// one it can represent. This is the figure activity
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
    /// The number the user approved on the quote and the number a receipt
    /// shows.
    pub total_debited: Amount,
    /// When the automatic reclaim is scheduled for.
    ///
    /// Fixed when the send is created and never rewritten, so this is when
    /// the reclaim was *due* rather than when anything happened: a send that
    /// settles early, the receiver redeems, or
    /// [`request_cancel`](Operation::request_cancel) wins, keeps the
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
// Implementation notes (delete once implemented):
// - Maps one-to-one onto upstream `fedimint-mint-client`'s `ReissueExternalNotesState`
//   (`Created`, `Issuing`, `Done`, `Failed(String)`); the only change is carrying the
//   failure reason as a named field rather than a positional tuple.
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
    /// Final: the notes could not be redeemed, most often because they
    /// were already spent or had been reclaimed by the sender.
    Failed {
        /// Human-readable explanation. Diagnostic only, not a stable
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
/// [`Operation::details`](crate::Operation::details). Every field is fixed
/// when the redemption is created and never changes afterwards.
/// [`EcashReceiveState`] carries no amounts, only a diagnostic reason on
/// failure, so this record is the whole of what a redemption can be
/// receipted from. The fee is known and recorded before the federation
/// answers: the notes state their own value, and the federation's fee
/// schedule is part of the configuration this client already holds.
///
/// # Invariants
///
/// - `net_credit == notes_value - fee`. That is what the balance rises by
///   when the operation reaches [`EcashReceiveState::Done`]. The fee comes
///   out of the notes rather than being charged on top of them, which is
///   why a receive nets down where a send totals up.
///
/// `Debug` output redacts the notes, as [`Notes`] itself does.
// Implementation notes (delete once implemented):
// - Should the fee turn out not to be knowable locally after all, `fee` and `net_credit`
//   become `Option` together, never one without the other: each is derivable from the
//   other given `notes_value`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EcashReceiveDetails {
    /// The notes this redemption consumed, the ones handed to
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
    /// gave up, not what this wallet gains; see
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
    /// The number to show as "you received".
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

    /// A send whose request is rounded up: 1234 msat requested, satisfied by
    /// 1536 msat of notes (three 512-msat denominations), with a fee on top.
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
