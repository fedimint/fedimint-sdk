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
/// An activity row exists because this SDK instance recorded it while it
/// was happening, not because the federation kept a record of the account:
///
/// - Restoring a seed does not restore this history. Recovery reconstructs
///   what the federation and the backup can prove (notes, spendable
///   balance, recoverable operations), not a narrative of past activity.
///   A wallet restored on a new device has a correct balance and an empty
///   or partial activity list.
/// - Activity from another device or another client is not here. The same
///   seed used in another application produces rows in that application's
///   own storage.
/// - Forgetting a federation erases its rows along with the rest of its
///   local state.
///
/// An application that needs durable, portable history must keep its own,
/// keyed by [`ActivityItem::operation_id`].
///
/// # What the numbers mean
///
/// [`amount`](ActivityItem::amount) and [`fee`](ActivityItem::fee) are
/// defined here, once, for every kind, so that two bindings rendering the
/// same row produce the same two numbers. One rule, in three clauses, all
/// describing the terms of the transfer the operation set out to make, and
/// holding for every row whatever its outcome:
///
/// 1. `amount` is the counterparty figure of those terms: what the other
///    side was to receive (outgoing) or to send (incoming). Gross of this
///    wallet's fees, and as executed rather than as requested: an ecash
///    send reports the notes actually issued, not the amount typed.
/// 2. `fee` is what this wallet was charged on those terms: on top of the
///    counterparty figure when outgoing, out of it when incoming.
/// 3. `direction` is which way the transfer was to move value.
///
/// For a row whose [`status`](ActivityItem::status) is
/// [`Success`](ActivityStatus::Success) the terms are also what happened:
/// the counterparty received or paid `amount`, this wallet paid `fee`, and
/// the balance moved by `amount + fee` outgoing or `amount - fee` incoming.
/// No row folds a fee into `amount`, and no row reports a net figure there.
/// What the numbers mean for the other outcomes is stated under [The
/// identity describes what was
/// attempted](ActivityItem#the-identity-describes-what-was-attempted).
///
/// | kind | `amount` (the counterparty figure) | `fee` |
/// | --- | --- | --- |
/// | [`EcashSend`](OperationKind::EcashSend) | value of the notes handed over, may be rounded up from the request | cost of issuing those notes |
/// | [`EcashReceive`](OperationKind::EcashReceive) | face value of the notes presented | reissuance fee taken out of it |
/// | [`LnSend`](OperationKind::LnSend) | invoice amount, what the payee receives on success | fee bound by the executed quote |
/// | [`LnReceive`](OperationKind::LnReceive) | invoice's face value, what the payer is asked for | receive-side fee taken out of it |
/// | [`OnchainSend`](OperationKind::OnchainSend) | amount bound for the destination address | all federation-side funding costs, aggregated (peg-out, network, mint funding, change, dust) |
/// | [`OnchainReceive`](OperationKind::OnchainReceive) | gross amount that arrived on chain; `None` until a transaction is seen | all federation-side claim costs, aggregated, per [`OnchainReceiveDetails::fee`](crate::OnchainReceiveDetails::fee); `None` until the claim settles |
/// | [`Recovery`](OperationKind::Recovery) | `None`, nothing was transferred | `None` |
/// | [`Unknown`](OperationKind::Unknown) | `None`, nothing may be guessed | `None` |
///
/// ## The identity describes what was attempted
///
/// A row's numbers describe the transfer the operation set out to make. They
/// are *also* the realised balance movement exactly when
/// [`status`](ActivityItem::status) is [`Success`](ActivityStatus::Success).
/// For the other buckets:
///
/// - [`Refunded`](ActivityStatus::Refunded) and
///   [`Canceled`](ActivityStatus::Canceled): the value went out and came
///   back (a refunded payment's funding returned, a canceled send's notes
///   reclaimed), so the net movement is zero apart from a fee already
///   spent. The fields go on describing the attempt, "1000 sat, refunded" is
///   what a list needs to show, and it is the bucket, not the numbers, that
///   says the money came back.
/// - [`Failed`](ActivityStatus::Failed): the transfer neither completed nor
///   resolved into a clean return, so the balance effect is not something
///   this row can assert. Render the attempt; read the operation and the
///   balance for the rest.
/// - [`Unknown`](ActivityStatus::Unknown): `amount`, `fee` and
///   [`direction`](ActivityItem::direction) are all `None`, so there is no
///   identity to reconcile and nothing to render wrongly.
///
/// ## Requested is not actual
///
/// For an ecash send the two genuinely differ, and this row reports the
/// actual. A mint issues notes in fixed denominations, so a request for
/// 1234 msat may be satisfied by notes worth more than that, and selecting
/// them may itself cost a fee. `amount` is the value of the notes the
/// receiver can redeem, which is the only figure that reconciles with what
/// left the balance. What the caller asked for is not thrown away, it is on
/// the send's own details record, as `EcashSendDetails::requested_amount`,
/// so a receipt can show both without this row carrying a third number.
///
/// ## On-chain rows mix two denominations, exactly
///
/// An on-chain operation's chain-side figures are whole [`Sats`](crate::Sats)
/// (a transaction output cannot be a fraction of a satoshi), while this row
/// is in millisatoshi [`Amount`]s so that one list can mix kinds. The
/// conversion is [`Sats::to_amount`](crate::Sats::to_amount), which rounds
/// nothing: a satoshi is exactly 1000 msat. So an on-chain row's `amount` is
/// always a whole multiple of 1000 msat, and a caller that wants satoshis
/// back should use [`Amount::to_sats_exact`](crate::Amount::to_sats_exact)
/// rather than dividing and hoping.
///
/// Its `fee` is not: the federation-side costs of an on-chain operation are
/// quoted in millisatoshis and can carry sub-satoshi precision, so `fee`
/// here, and a deposit's net credit, may not divide by 1000. Read a fee
/// rather than deriving one from a chain-side figure.
///
/// ## When two numbers are not enough
///
/// They often are not, and that is deliberate: this is a list row, and the
/// full account of an operation lives on the operation. Resolve
/// [`operation_id`](ActivityItem::operation_id) with
/// [`Federation::operation`](crate::Federation::operation), then read
/// [`Operation::details`](crate::Operation::details) for the requested
/// amount, the gross deposit, the invoice, the destination, the route and the
/// executed quote, and [`Operation::state`](crate::Operation::state) for
/// where it stands. Two numbers and a bucket are what a list needs; a receipt
/// screen should not be built out of them.
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
    /// The counterparty figure of the terms the operation executed on: what
    /// the other side was to send or receive, gross of this wallet's fees,
    /// and as executed rather than as requested. For a
    /// [`Success`](ActivityStatus::Success) row it is also what the other
    /// side sent or received.
    ///
    /// Which figure that is for each kind, and what it deliberately is not,
    /// is fixed by the table in [What the numbers
    /// mean](ActivityItem#what-the-numbers-mean), which is the contract that
    /// stops two bindings from rendering the same row differently.
    ///
    /// `None` for a kind with no single counterparty figure: a recovery,
    /// which transfers nothing; a row this SDK cannot interpret, where any
    /// number would be invented; or an on-chain deposit before a transaction
    /// has been seen, where there is nothing yet to report. A figure that
    /// starts `None` and becomes known is written once and never changes
    /// afterwards.
    pub amount: Option<Amount>,
    /// The fee the operation's terms carry, what this wallet pays for the
    /// transfer if it succeeds, when it is known.
    ///
    /// Always a separate field from [`ActivityItem::amount`] and never folded
    /// into it: a successful outgoing row debited `amount + fee`, a
    /// successful incoming row credited `amount - fee`, and a list showing
    /// "1000 sat" wants the number the user typed or the payee received
    /// rather than a fee-inclusive total that matches neither.
    ///
    /// `None` when the kind has no fee at all, or when the fee is not knowable
    /// yet, an operation still in flight, or an on-chain deposit whose
    /// claim fee only exists once something has arrived. `Some(zero)` and
    /// `None` are different answers, and a UI should treat them so: the first
    /// says the terms carry no fee, the second that this row cannot say yet.
    pub fee: Option<Amount>,
    /// Which way the transfer was to move value: in or out.
    ///
    /// `None` for kinds that have no direction, a recovery, for example,
    /// is neither incoming nor outgoing. This is `Option` rather than a
    /// third "neither" variant on [`Direction`] so that a UI branching on
    /// direction handles the no-direction case by not drawing an arrow at
    /// all, rather than by drawing a third kind of arrow.
    ///
    /// It also selects which half of the accounting identity applies to a
    /// [`Success`](ActivityStatus::Success) row:
    /// [`Outgoing`](Direction::Outgoing) debited `amount + fee`,
    /// [`Incoming`](Direction::Incoming) credited `amount - fee`. A row with
    /// no direction has no identity to reconcile, and both figures are
    /// `None`.
    pub direction: Option<Direction>,
    /// How the operation turned out, or that it has not yet.
    pub status: ActivityStatus,
    /// Whether the operation has finished.
    ///
    /// `true` once it has reached a state it will never leave, the same
    /// predicate [`OperationState::is_final`](crate::OperationState::is_final)
    /// applies to a typed state.
    ///
    /// This is recorded independently of [`status`](ActivityItem::status)
    /// because for one bucket the two genuinely come apart. An
    /// [`Unknown`](ActivityStatus::Unknown) row's outcome cannot be
    /// interpreted, and inferring from that that it must still be running
    /// would be a fabrication: a row this SDK version cannot read may well
    /// have settled years ago. Whether an operation *finished* is a different
    /// question from *how it turned out*, and it is one this SDK can still
    /// answer for a record it cannot otherwise interpret.
    ///
    /// For every other bucket the two agree, and must:
    /// `is_final` is `false` exactly when `status` is
    /// [`Pending`](ActivityStatus::Pending). That is what makes this the one
    /// predicate a list can use to decide whether to draw a spinner, on every
    /// row, with no special case for the rows it does not understand.
    pub is_final: bool,
}

/// Which way a transfer moves value.
///
/// The intended direction of the operation's terms: it says which way the
/// balance moves when the operation succeeds, and is the same for a row that
/// is still pending, was refunded, or failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Direction {
    /// Value comes into this federation's balance.
    Incoming,
    /// Value leaves this federation's balance.
    Outgoing,
}

/// How an activity row turned out.
///
/// Coarse on purpose: this is the summary a list row shows, and the full
/// detail lives on the operation itself, reachable through
/// [`ActivityItem::operation_id`]. Mapping a rich set of operation states
/// down to a handful of buckets is the whole job of this type.
///
/// Every variant here is an *outcome*. Whether the operation has finished is
/// a separate axis, carried by [`ActivityItem::is_final`], and the two are
/// only redundant for the five buckets this SDK can interpret; see
/// [`Unknown`](Self::Unknown).
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
/// For a kind this SDK version understands, every non-final state maps to
/// [`Pending`](Self::Pending), so only the final ones are listed. There is no
/// unmapped state: if a state is not here, it is not final. A row whose state
/// cannot be read at all is the one case outside this table, and it is
/// [`Unknown`](Self::Unknown).
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
/// | [`OnchainSendState::Refunded`](crate::OnchainSendState::Refunded) | [`Refunded`](Self::Refunded) |
/// | [`OnchainSendState::Failed`](crate::OnchainSendState::Failed) | [`Failed`](Self::Failed) |
/// | [`OnchainReceiveState::Claimed`](crate::OnchainReceiveState::Claimed) | [`Success`](Self::Success) |
/// | [`OnchainReceiveState::Failed`](crate::OnchainReceiveState::Failed) | [`Failed`](Self::Failed) |
/// | [`RecoveryState::Done`](crate::RecoveryState::Done) | [`Success`](Self::Success) |
/// | [`RecoveryState::Failed`](crate::RecoveryState::Failed) | [`Failed`](Self::Failed) |
///
/// The one placement that is a judgement rather than a reading is
/// [`LnReceiveState::Expired`](crate::LnReceiveState::Expired). An invoice
/// that simply lapsed unpaid is not [`Failed`](Self::Failed), nothing
/// broke, so it joins the withdrawn-invoice case under
/// [`Canceled`](Self::Canceled).
///
/// # An uninterpretable row
///
/// A row whose [`kind`](crate::ActivityItem::kind) is
/// [`OperationKind::Unknown`](crate::OperationKind::Unknown) was written by a
/// version of the SDK that understood something this one does not, so its
/// outcome cannot be interpreted and must not be guessed at. Such a row
/// reports [`Unknown`](Self::Unknown), never [`Pending`](Self::Pending):
/// finality is recorded independently of the outcome, so a finished row this
/// build cannot read reports `status == Unknown` with `is_final == true`,
/// and one still running reports the same status with `is_final == false`.
/// See [`ActivityItem::is_final`].
///
/// [`amount`](crate::ActivityItem::amount),
/// [`fee`](crate::ActivityItem::fee) and
/// [`direction`](crate::ActivityItem::direction) stay `None`, so there is
/// nothing to render wrongly. What the row actually said about itself is
/// still readable: resolve [`operation_id`](crate::ActivityItem::operation_id)
/// with [`Federation::operation`](crate::Federation::operation) and read
/// [`AnyOperation::raw_kind`](crate::AnyOperation::raw_kind), so an
/// application can log or display which thing it did not understand instead
/// of only that something was unrecognised. Render it as an opaque entry,
/// not as a stalled payment and not as a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ActivityStatus {
    /// Still in flight: the operation has not reached a final state.
    ///
    /// Reported only for rows this SDK version can interpret. A row it
    /// cannot interpret reports [`Unknown`](Self::Unknown) instead, whether
    /// or not it has finished.
    Pending,
    /// Completed as intended.
    Success,
    /// Ended without completing, and without the value being known to be
    /// safe in the balance.
    Failed,
    /// Ended without completing, and the value is safe in the balance,
    /// returned after it was debited, as for a lightning payment that could
    /// not be routed, or never debited at all, as for a funding transaction
    /// the federation rejected.
    Refunded,
    /// Ended without completing, because the operation was called off or
    /// simply lapsed: reclaimed out-of-band ecash, whose notes went out and
    /// came back; a lightning receive withdrawn before it was paid; or an
    /// invoice whose expiry passed unpaid.
    ///
    /// None of these is alarming, and none of them is
    /// [`Failed`](Self::Failed): nothing went wrong, the transfer just did
    /// not happen.
    Canceled,
    /// This SDK version cannot interpret how the row turned out.
    ///
    /// Not a sixth outcome, but the absence of a reading, and it is reported
    /// in two situations:
    ///
    /// - the row's [`kind`](crate::ActivityItem::kind) is
    ///   [`OperationKind::Unknown`](crate::OperationKind::Unknown), a record
    ///   written by a build that understood a module or an operation this one
    ///   does not; and
    /// - the kind is one this build knows but the persisted final state is a
    ///   variant its state enum does not have, which a newer build can write
    ///   because every state enum here is `#[non_exhaustive]`.
    ///
    /// In both, the outcome is unreadable while the row itself is perfectly
    /// real. Guessing is not an option, and neither is
    /// [`Pending`](Self::Pending): see [the section
    /// above](ActivityStatus#an-uninterpretable-row).
    ///
    /// Whether such an operation has finished is a separate question, and one
    /// the SDK does answer: read [`ActivityItem::is_final`]. A UI should show
    /// the row, its time, and that its outcome is unknown, and should show
    /// neither an amount nor a failure.
    Unknown,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A row with the fields this module's tests care about, and defensible
    /// filler for the rest.
    fn row(
        kind: OperationKind,
        amount: Option<Amount>,
        fee: Option<Amount>,
        direction: Option<Direction>,
        status: ActivityStatus,
        is_final: bool,
    ) -> ActivityItem {
        ActivityItem {
            operation_id: "0000000000000000000000000000000000000000000000000000000000000000"
                .parse()
                .expect("a valid operation id"),
            kind,
            time: Timestamp::from_epoch_millis(1_700_000_000_000),
            amount,
            fee,
            direction,
            status,
            is_final,
        }
    }

    #[test]
    fn an_uninterpretable_row_may_already_be_final() {
        // The case the `Pending` reading got wrong: a row this build cannot
        // read, that finished long ago. Its outcome is unknown; its finality
        // is not.
        let settled = row(
            OperationKind::Unknown,
            None,
            None,
            None,
            ActivityStatus::Unknown,
            true,
        );
        assert_eq!(settled.status, ActivityStatus::Unknown);
        assert!(settled.is_final);
        assert_ne!(settled.status, ActivityStatus::Pending);

        // And the same status with the other finality, which is the point of
        // keeping the two apart.
        let running = ActivityItem {
            is_final: false,
            ..settled.clone()
        };
        assert_eq!(running.status, settled.status);
        assert_ne!(running, settled);
    }

    #[test]
    fn an_uninterpretable_row_renders_no_numbers() {
        let unknown = row(
            OperationKind::Unknown,
            None,
            None,
            None,
            ActivityStatus::Unknown,
            true,
        );
        assert_eq!(unknown.amount, None);
        assert_eq!(unknown.fee, None);
        assert_eq!(unknown.direction, None);
    }

    #[test]
    fn finality_and_pending_agree_for_interpretable_rows() {
        let pending = row(
            OperationKind::LnSend,
            Some(Amount::from_msats(1_000)),
            None,
            Some(Direction::Outgoing),
            ActivityStatus::Pending,
            false,
        );
        assert_eq!(pending.status == ActivityStatus::Pending, !pending.is_final);

        for status in [
            ActivityStatus::Success,
            ActivityStatus::Failed,
            ActivityStatus::Refunded,
            ActivityStatus::Canceled,
        ] {
            let settled = ActivityItem {
                status,
                is_final: true,
                ..pending.clone()
            };
            assert_eq!(settled.status == ActivityStatus::Pending, !settled.is_final);
        }
    }

    #[test]
    fn outgoing_rows_debit_amount_plus_fee() {
        let sent = row(
            OperationKind::LnSend,
            Some(Amount::from_msats(1_000)),
            Some(Amount::from_msats(10)),
            Some(Direction::Outgoing),
            ActivityStatus::Success,
            true,
        );
        let debited = sent
            .amount
            .and_then(|amount| sent.fee.and_then(|fee| amount.checked_add(fee)));
        assert_eq!(debited, Some(Amount::from_msats(1_010)));
    }

    #[test]
    fn incoming_rows_credit_amount_minus_fee() {
        let received = row(
            OperationKind::LnReceive,
            Some(Amount::from_msats(1_000)),
            Some(Amount::from_msats(10)),
            Some(Direction::Incoming),
            ActivityStatus::Success,
            true,
        );
        // The gross figure is what the payer paid, so the credit is smaller
        // than the row's amount, never the other way round.
        let credited = received
            .amount
            .and_then(|amount| received.fee.and_then(|fee| amount.checked_sub(fee)));
        assert_eq!(credited, Some(Amount::from_msats(990)));
        assert!(credited < received.amount);
    }

    #[test]
    fn a_free_transfer_and_an_unknown_fee_are_different_answers() {
        let free = row(
            OperationKind::LnSend,
            Some(Amount::from_msats(1_000)),
            Some(Amount::from_msats(0)),
            Some(Direction::Outgoing),
            ActivityStatus::Success,
            true,
        );
        let not_yet = ActivityItem {
            fee: None,
            ..free.clone()
        };
        assert_ne!(free.fee, not_yet.fee);
    }

    #[test]
    fn on_chain_rows_are_whole_multiples_of_a_thousand_msats() {
        let deposited = crate::Sats::from_sats(25_000);
        let deposit = row(
            OperationKind::OnchainReceive,
            deposited.to_amount(),
            crate::Sats::from_sats(300).to_amount(),
            Some(Direction::Incoming),
            ActivityStatus::Success,
            true,
        );
        assert_eq!(deposit.amount, Some(Amount::from_msats(25_000_000)));
        assert_eq!(
            deposit.amount.and_then(Amount::to_sats_exact),
            Some(deposited)
        );
        assert_eq!(
            deposit.fee.and_then(Amount::to_sats_exact),
            Some(crate::Sats::from_sats(300))
        );
    }

    #[test]
    fn a_recovery_row_has_no_transfer_to_account_for() {
        let recovery = row(
            OperationKind::Recovery,
            None,
            None,
            None,
            ActivityStatus::Success,
            true,
        );
        assert_eq!(recovery.amount, None);
        assert_eq!(recovery.fee, None);
        assert_eq!(recovery.direction, None);
    }
}
