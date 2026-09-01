//! Chaumian ecash: spending notes out of band and redeeming them.

use std::sync::Arc;

use crate::{Amount, Notes, Operation, OperationState, Result};

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
#[derive(Debug, Clone)]
pub struct Ecash {
    inner: Arc<EcashInner>,
}

impl Ecash {
    /// Takes `amount` out of the balance as out-of-band notes.
    ///
    /// The balance is debited immediately and the returned
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
    /// confirmation when this facade is implemented. Its outcome is
    /// reported through the state machine like any other:
    /// [`EcashSendState::Canceled`] when the reclaim wins,
    /// [`EcashSendState::Redeemed`] when the receiver got there first.
    ///
    /// The signature stays deliberately minimal — an amount and nothing
    /// else. Tuning the reclaim period, or selecting notes differently, is
    /// a later additive `send_with`-style call rather than an options
    /// struct on the common path.
    ///
    /// # Errors
    ///
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance),
    /// [`Recovering`](crate::ErrorCode::Recovering) while a recovery is in
    /// progress,
    /// [`NotSupported`](crate::ErrorCode::NotSupported) if the mint module
    /// disappeared from the federation's configuration after this facade
    /// was obtained,
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn send(&self, amount: Amount) -> Result<EcashSend> {
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
    /// Redeem promptly. Notes are subject to the sender's automatic reclaim
    /// (see [`Ecash::send`]), and losing the race means the operation ends
    /// in [`EcashReceiveState::Failed`].
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) if the notes are
    /// malformed or were issued by a different federation,
    /// [`Recovering`](crate::ErrorCode::Recovering),
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn receive(&self, notes: &Notes) -> Result<Operation<EcashReceiveState>> {
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
#[derive(Debug)]
#[non_exhaustive]
pub struct EcashSend {
    /// The notes to give to the receiver. Their value is already out of
    /// the sender's spendable balance.
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

/// Placeholder for the mint-module state this facade operates on.
#[derive(Debug)]
struct EcashInner;

#[cfg(test)]
mod tests {
    use super::*;

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
