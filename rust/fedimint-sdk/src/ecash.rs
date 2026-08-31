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
    /// `Ok(())` means the *request* was accepted, not that the notes were
    /// reclaimed. Redemption and cancellation genuinely race — the receiver
    /// may be redeeming at the same moment — and only the federation
    /// decides who wins. The outcome therefore arrives where every other
    /// outcome does, as a state: [`EcashSendState::Canceled`] if the notes
    /// came back, [`EcashSendState::Redeemed`] if the receiver got them.
    /// Between the request and the outcome the operation sits in
    /// [`EcashSendState::CancelRequested`].
    ///
    /// This is the only cancellation in the crate, because it is the only
    /// place where cancelling is a real protocol action rather than an
    /// attempt to un-send money that has already moved.
    ///
    /// # Errors
    ///
    /// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation) if
    /// the operation has already reached a final state,
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn request_cancel(&self) -> Result<()> {
        unimplemented!()
    }
}

/// The lifecycle of an out-of-band ecash send.
///
/// # Relationship to the upstream state machine
///
/// Upstream `fedimint-client` models this as `SpendOOBState`, whose
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
/// [`Failed`](Self::Failed) has no upstream counterpart in that enum: it
/// covers failures the SDK observes around the state machine rather than
/// within it. This variant set is therefore provisional in that one
/// respect and will be reconciled against the mint client when this facade
/// is implemented.
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
    /// Final: the send could not be completed.
    Failed {
        /// Human-readable explanation. Diagnostic only — not a stable
        /// contract, and not something to match on.
        reason: String,
    },
}

impl crate::operation::sealed::Sealed for EcashSendState {}

impl OperationState for EcashSendState {
    fn is_final(&self) -> bool {
        unimplemented!()
    }
}

/// The lifecycle of redeeming out-of-band ecash notes.
///
/// This maps one-to-one onto upstream `fedimint-client`'s
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
        unimplemented!()
    }
}

/// Placeholder for the mint-module state this facade operates on.
#[derive(Debug)]
struct EcashInner;
