//! Local, cross-module history of what a federation has been used for.

use crate::{Amount, Cursor, OperationId, OperationKind, Timestamp};

/// One row of a federation's activity history.
///
/// Read through [`Federation::activity`](crate::Federation::activity),
/// which returns them newest first. The point of this type is to let an
/// application render a single transaction list across ecash, lightning,
/// and on-chain activity without querying each facade separately and
/// merging the results itself.
///
/// # This is local history, not complete history
///
/// An activity row exists because *this SDK instance recorded it* while it
/// was happening. That has consequences worth being explicit about, because
/// the alternative reading — that this is the federation's record of the
/// account — is wrong and would be a bad thing to build a UI on:
///
/// - **Restoring a seed does not restore this history.** Recovery
///   reconstructs what the federation and the backup can prove — notes,
///   spendable balance, recoverable operations — not a narrative of past
///   activity. A wallet restored on a new device has a correct balance and
///   an empty or partial activity list, and that is not a bug.
/// - **Activity from another device or another client is not here.** The
///   same seed used in another application produces rows in *that*
///   application's storage.
/// - **Forgetting a federation erases its rows** along with the rest of its
///   local state.
///
/// An application that needs durable, portable history must keep its own,
/// keyed by [`ActivityItem::operation_id`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ActivityItem {
    /// The operation this row describes.
    ///
    /// Pass it to [`Federation::operation`](crate::Federation::operation)
    /// to reattach to the operation itself and read its full, typed state.
    pub operation_id: OperationId,
    /// What kind of operation it is, for labelling and grouping.
    ///
    /// May be [`OperationKind::Unknown`](crate::OperationKind::Unknown) for
    /// a row recorded by a version of the SDK that understood something
    /// this one does not.
    pub kind: OperationKind,
    /// When this SDK instance recorded the activity.
    ///
    /// This is a **local** clock reading, not a consensus timestamp: the
    /// federation does not attest to it, it comes from the device that
    /// happened to record the row, and a device with a wrong clock produces
    /// wrong times here. It is suitable for ordering and displaying a
    /// user's own history and unsuitable as evidence of when anything
    /// actually happened.
    pub time: Timestamp,
    /// The principal amount, **excluding** any fee.
    ///
    /// `None` for kinds that have no single fixed amount — an on-chain
    /// deposit before anything has arrived, for instance. Keeping the
    /// principal separate from [`ActivityItem::fee`] means a list can show
    /// "1000 sat" as the amount of a payment rather than a fee-inclusive
    /// total that matches neither what the user typed nor what the payee
    /// received.
    pub amount: Option<Amount>,
    /// The fee paid, when it is known.
    ///
    /// `None` when the kind has no fee, or when the fee is not known yet
    /// (an operation still in flight). Always a separate field from
    /// [`ActivityItem::amount`], never folded into it.
    pub fee: Option<Amount>,
    /// Whether value moved in or out.
    ///
    /// `None` for kinds that have no direction — a recovery, for example,
    /// is neither incoming nor outgoing. This is `Option` rather than a
    /// third "neither" variant on [`Direction`] so that a UI branching on
    /// direction handles the no-direction case by not drawing an arrow at
    /// all, rather than by drawing a third kind of arrow.
    pub direction: Option<Direction>,
    /// How the operation turned out, or that it has not yet.
    pub status: ActivityStatus,
}

/// Which way value moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Direction {
    /// Value came into this federation's balance.
    Incoming,
    /// Value left this federation's balance.
    Outgoing,
}

/// How an activity row turned out.
///
/// Coarse on purpose: this is the summary a list row shows, and the full
/// detail lives on the operation itself, reachable through
/// [`ActivityItem::operation_id`]. Mapping a rich state machine down to
/// five buckets is the whole job of this type.
///
/// [`Refunded`](Self::Refunded) and [`Canceled`](Self::Canceled) are
/// first-class rather than being folded into
/// [`Failed`](Self::Failed) because transaction lists need them: "your
/// payment failed and the money is back" and "you took your ecash back" are
/// outcomes users understand and expect to see distinguished, and both are
/// ordinary rather than alarming.
///
/// # Which operation state lands in which bucket
///
/// Every non-final state maps to [`Pending`](Self::Pending), so only the
/// final ones are listed. There is no unmapped state: if a state is not
/// here, it is not final.
///
/// | operation state | bucket |
/// | --- | --- |
/// | [`EcashSendState::Redeemed`](crate::EcashSendState::Redeemed) | [`Success`](Self::Success) |
/// | [`EcashSendState::Canceled`](crate::EcashSendState::Canceled) | [`Canceled`](Self::Canceled) |
/// | [`EcashReceiveState::Done`](crate::EcashReceiveState::Done) | [`Success`](Self::Success) |
/// | [`EcashReceiveState::Failed`](crate::EcashReceiveState::Failed) | [`Failed`](Self::Failed) |
/// | [`LnSendState::Success`](crate::LnSendState::Success) | [`Success`](Self::Success) |
/// | [`LnSendState::Refunded`](crate::LnSendState::Refunded) | [`Refunded`](Self::Refunded) |
/// | [`LnSendState::Failed`](crate::LnSendState::Failed) | [`Failed`](Self::Failed) |
/// | [`LnReceiveState::Claimed`](crate::LnReceiveState::Claimed) | [`Success`](Self::Success) |
/// | [`LnReceiveState::Canceled`](crate::LnReceiveState::Canceled) | [`Canceled`](Self::Canceled) |
/// | [`LnReceiveState::Expired`](crate::LnReceiveState::Expired) | [`Canceled`](Self::Canceled) |
/// | [`LnReceiveState::Failed`](crate::LnReceiveState::Failed) | [`Failed`](Self::Failed) |
/// | [`OnchainSendState::Succeeded`](crate::OnchainSendState::Succeeded) | [`Success`](Self::Success) |
/// | [`OnchainSendState::Failed`](crate::OnchainSendState::Failed) | [`Failed`](Self::Failed) |
/// | [`OnchainReceiveState::Claimed`](crate::OnchainReceiveState::Claimed) | [`Success`](Self::Success) |
/// | [`OnchainReceiveState::Failed`](crate::OnchainReceiveState::Failed) | [`Failed`](Self::Failed) |
/// | `RecoveryState::Done` (experimental) | [`Success`](Self::Success) |
/// | `RecoveryState::Failed` (experimental) | [`Failed`](Self::Failed) |
///
/// The one placement that is a judgement rather than a reading is
/// [`LnReceiveState::Expired`](crate::LnReceiveState::Expired). An invoice
/// that simply lapsed unpaid is not [`Failed`](Self::Failed) — nothing
/// broke, and that variant exists precisely because lapsing unpaid is the
/// commonest way a receive ends and is not worth alarming a user about — so
/// it joins the withdrawn-invoice case under
/// [`Canceled`](Self::Canceled).
///
/// # An `Unknown` row
///
/// A row whose
/// [`kind`](crate::ActivityItem::kind) is
/// [`OperationKind::Unknown`](crate::OperationKind::Unknown) was written by
/// a version of the SDK that understood something this one does not, so its
/// state cannot be interpreted and must not be guessed at. Such a row
/// reports [`Pending`](Self::Pending) for
/// [`status`](crate::ActivityItem::status) — the honest answer is "this SDK
/// cannot tell that it finished" — and `None` for
/// [`amount`](crate::ActivityItem::amount),
/// [`fee`](crate::ActivityItem::fee) and
/// [`direction`](crate::ActivityItem::direction). An application should
/// render it as an opaque entry rather than as a stalled payment; the fields
/// are absent precisely so it has nothing to render wrongly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ActivityStatus {
    /// Still in flight — the operation has not reached a final state. Also
    /// what an uninterpretable
    /// [`OperationKind::Unknown`](crate::OperationKind::Unknown) row
    /// reports, since this SDK version cannot tell that it finished.
    Pending,
    /// Completed as intended.
    Success,
    /// Ended without completing, and without the value being returned by a
    /// refund or a cancellation.
    Failed,
    /// Ended without completing, with the value returned to the balance —
    /// a lightning payment that could not be routed, for example.
    Refunded,
    /// Ended without the value moving, because it was called off or simply
    /// lapsed — reclaimed out-of-band ecash, a lightning receive withdrawn
    /// before it was paid, or an invoice whose expiry passed unpaid.
    ///
    /// None of these is alarming, and none of them is
    /// [`Failed`](Self::Failed): nothing went wrong, the transfer just did
    /// not happen.
    Canceled,
}

/// One page of activity history.
///
/// Returned by [`Federation::activity`](crate::Federation::activity). Pages
/// run newest first; carry [`ActivityPage::next`] into the following call
/// to continue.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ActivityPage {
    /// The rows in this page, newest first. May contain fewer items than
    /// the requested limit, including none at all.
    pub items: Vec<ActivityItem>,
    /// The cursor for the following page, or `None` when this page is the
    /// last one.
    ///
    /// Treat it as an opaque value: pass it back unchanged, or persist and
    /// reload it, but never construct or interpret one. See
    /// [`Cursor`](crate::Cursor).
    pub next: Option<Cursor>,
}
