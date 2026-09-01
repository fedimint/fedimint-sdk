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
///
/// # What the numbers mean
///
/// [`amount`](ActivityItem::amount) and [`fee`](ActivityItem::fee) are
/// defined here, once, for every kind — because two bindings rendering the
/// same row have to produce the same two numbers, and "the principal,
/// excluding fee" does not make that true. It does not say whether an
/// incoming row is what the payer sent or what landed in the balance, and it
/// does not say whether an ecash send reports what the user asked for or what
/// the mint actually handed out. Those are different numbers.
///
/// One rule, in four clauses:
///
/// 1. **`amount` is the counterparty figure** — what the other side received
///    (outgoing) or sent (incoming). Gross of this wallet's fees, and
///    *actual* rather than requested.
/// 2. **`fee` is what this wallet was charged** for that transfer: on top of
///    the counterparty figure when outgoing, out of it when incoming.
/// 3. **The balance therefore moves by `amount + fee` outgoing, and
///    `amount - fee` incoming** — for the transfer as it was *attempted*.
/// 4. **Both numbers are the quoted or expected terms, not the realized
///    movement.** They are what the operation was executed on and what the
///    user approved. What the balance finally did — how much a refund or a
///    reclaim actually gave back, what an accepted transaction actually
///    charged — is recorded on the operation's own details record, not here.
///    The one exception is an on-chain deposit, whose costs no quote or fee
///    schedule predicts up front: its row reports the observed gross and the
///    realized claim fee, per the table below.
///
/// A receive row is gross, then: the payer paid `amount`, and the credit
/// expected to land is `amount - fee`. A send row's `amount` is what the
/// payee got, and the debit was `amount + fee`. No row folds a fee into
/// `amount`, and no row reports a net figure there.
///
/// | kind | `amount` — what the counterparty sent or received | `fee` — quoted or expected, unless said otherwise |
/// | --- | --- | --- |
/// | [`EcashSend`](OperationKind::EcashSend) | the value of the notes actually handed over, which the mint may have rounded **up** from the amount requested | what the quote fixed for issuing those notes: the quoted cost of a send the receiver redeems, and only part of the quoted cost of one that is reclaimed. The realized cost is on the operation's record |
/// | [`EcashReceive`](OperationKind::EcashReceive) | the face value of the notes redeemed | the reissuance fee expected from the fee schedule, taken out of that value rather than added to it |
/// | [`LnSend`](OperationKind::LnSend) | the invoice amount that reached the payee | the fee the executed quote quoted: what the payment was funded with, which is not necessarily what the payment finally cost |
/// | [`LnReceive`](OperationKind::LnReceive) | the invoice's face value: what the payer paid | the receive-side fee, taken out of it |
/// | [`OnchainSend`](OperationKind::OnchainSend) | the amount arriving at the destination address | every federation-side cost of funding the withdrawal, aggregated as quoted — peg-out and network fees plus mint funding, change and dust |
/// | [`OnchainReceive`](OperationKind::OnchainReceive) | the gross amount that arrived on chain, before anything the federation charged to claim it; `None` until a transaction is seen | every federation-side cost of claiming the deposit, aggregated as realized — the peg-in fee plus the primary module's output fees and denomination dust, per [`OnchainReceiveDetails::realized_fee`](crate::OnchainReceiveDetails::realized_fee); nothing predicts these costs up front, so `None` until a claim is accepted |
/// | [`Recovery`](OperationKind::Recovery) | `None` — nothing was transferred | `None` |
/// | [`Unknown`](OperationKind::Unknown) | `None` — nothing may be guessed | `None` |
///
/// ## The identity describes the attempt, and a return costs money
///
/// A row's numbers describe the transfer the operation set out to make, on
/// the terms it was quoted — every kind but an on-chain deposit, whose
/// numbers are the realized ones and are the one pair that does reconcile.
/// [`Success`](ActivityStatus::Success) on
/// [`status`](ActivityItem::status) says that transfer completed as intended
/// — not that these two numbers are what the balance did. A quote is not a
/// ceiling, so even a successful send's realized debit can come out either
/// side of `amount + fee`, and the *realized* movement is read from the
/// operation's own details record in every case, success included. So this
/// row is the right thing to render in every bucket and the wrong thing to
/// reconcile with in any of them:
///
/// - [`Refunded`](ActivityStatus::Refunded) and
///   [`Canceled`](ActivityStatus::Canceled): the value left the balance and
///   came back — but not all of it, and the shortfall is not this row's
///   `fee`. Giving it back is itself a federation transaction. Reclaiming
///   out-of-band notes, and refunding a lightning payment that could not be
///   routed, both assemble a further transaction that pays the mint's input
///   and output fees and loses whatever the denominations it reissues cannot
///   represent. So **the amount restored is less than the amount debited**,
///   and the difference — this row's `fee`, which was already spent, plus
///   whatever the return itself cost — is gone for good. It is *sunk*, not
///   missing: nothing is unaccounted for, and no third number on this row
///   would recover it, because the figure that reconciles is the one the
///   operation recorded when it settled. Resolve
///   [`operation_id`](ActivityItem::operation_id) and read
///   [`Operation::details`](crate::Operation::details) for what actually came
///   back and what the attempt finally cost; a receipt for a refunded or
///   canceled transfer has to be built from those.
///
///   What this row asserts, and all it asserts, is the attempt: "1000 sat,
///   refunded" is what a list needs to show, and it is the bucket, not the
///   numbers, that says the money came back.
///
///   Not every row in these two buckets moved value at all. An invoice that
///   lapsed unpaid, or a receive withdrawn before anyone paid it, is
///   [`Canceled`](ActivityStatus::Canceled) with a balance that never moved
///   in either direction — the row describes an attempt the counterparty
///   never took up. The operation's details record is what separates that
///   case from a reclaim numerically; this row does not.
/// - [`Failed`](ActivityStatus::Failed): the transfer neither completed nor
///   resolved into a clean return, so the balance effect is not something
///   this row can assert. Render the attempt; read the operation and the
///   balance for the rest.
/// - [`Unknown`](ActivityStatus::Unknown): `amount`, `fee` and
///   [`direction`](ActivityItem::direction) are all `None`, so there is no
///   identity to reconcile and nothing to render wrongly.
///
/// A UI that must show a single truthful figure for a settled row therefore
/// reads the operation's own realized figures, whatever the bucket —
/// including a [`Success`](ActivityStatus::Success). `amount ± fee` is what
/// was attempted and what the user approved, which is what a list wants and
/// — the deposit row excepted — never a statement of what the balance did.
///
/// ## Requested is not actual
///
/// For an ecash send the two genuinely differ, and this row reports the
/// actual. A mint issues notes in fixed denominations, so a request for
/// 1234 msat may be satisfied by notes worth more than that, and selecting
/// them may itself cost a fee. `amount` is the value of the notes the
/// receiver can redeem, and it is the only figure that, with `fee`, accounts
/// for what left the balance; the requested figure accounts for nothing. What
/// the caller asked for is not thrown away — it is on the send's own details
/// record, as `EcashSendDetails::requested_amount` — so a receipt can show
/// both without this row carrying a third number.
///
/// ## On-chain rows mix two denominations, exactly
///
/// An on-chain operation's chain-side figures are whole
/// [`Sats`](crate::Sats) — a transaction output cannot be a fraction of a
/// satoshi — while this row is in millisatoshi [`Amount`]s so that one list
/// can mix kinds. The conversion is
/// [`Sats::to_amount`](crate::Sats::to_amount), which rounds nothing: a
/// satoshi is exactly 1000 msat. So an on-chain row's `amount` is always a
/// whole multiple of 1000 msat, and a caller that wants satoshis back should
/// use [`Amount::to_sats_exact`](crate::Amount::to_sats_exact) rather than
/// dividing and hoping.
///
/// Its `fee` is not. The federation-side costs of an on-chain operation are
/// quoted in millisatoshis and can carry sub-satoshi precision, so `fee`
/// here — and a deposit's net credit — may not divide by 1000. That is why
/// the on-chain quote reports its amount in `Sats` but its fee and total in
/// `Amount`, and it is the reason to read a fee rather than derive one.
///
/// ## When two numbers are not enough
///
/// They often are not, and that is deliberate: this is a list row, and the
/// full account of an operation lives on the operation. Resolve
/// [`operation_id`](ActivityItem::operation_id) with
/// [`Federation::operation`](crate::Federation::operation), then read
/// [`Operation::details`](crate::Operation::details) for the requested
/// amount, the gross deposit, the invoice, the destination, the route, the
/// executed quote, and the realized figures a settled operation records —
/// what a reclaim or a refund actually restored, and what the operation
/// finally cost — and [`Operation::state`](crate::Operation::state) for where
/// it stands. Two numbers and a bucket are what a list needs; a receipt
/// screen should not be built out of them.
///
/// This row stays at two numbers on purpose. A realized figure exists only
/// once an operation has settled, so carrying one here would add fields that
/// are `None` for every pending row and duplicate, for the settled ones, a
/// value the details record already holds and holds more precisely — a
/// restored amount, a realized fee and an aggregate net cost do not compress
/// into one number without lying about one of the endings. The row says what
/// was attempted; the operation says what happened. An on-chain deposit,
/// with nothing attempted to report, is the one row built from the happened
/// figures.
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
    /// What the counterparty sent or received: gross of this wallet's fees,
    /// and actual rather than requested.
    ///
    /// Which figure that is for each kind, and what it deliberately is not,
    /// is fixed by the table in [What the numbers
    /// mean](ActivityItem#what-the-numbers-mean) — that table is the
    /// contract, and it is what stops two bindings from rendering the same
    /// row differently.
    ///
    /// Like [`fee`](ActivityItem::fee), it describes the transfer as
    /// attempted — observed, for an on-chain deposit: it is what the
    /// counterparty was to receive or did send, not a statement that the
    /// value stayed with them. A
    /// [`Refunded`](ActivityStatus::Refunded) or
    /// [`Canceled`](ActivityStatus::Canceled) row reports the attempt and the
    /// bucket says the value came back.
    ///
    /// `None` for a kind with no single counterparty figure: a recovery,
    /// which transfers nothing; a row this SDK cannot interpret, where any
    /// number would be invented; or an on-chain deposit before a transaction
    /// has been seen, where there is nothing yet to report. A figure that
    /// starts `None` and becomes known is written once and never changes
    /// afterwards.
    pub amount: Option<Amount>,
    /// The fee this wallet was quoted for the transfer, when it is known.
    /// For an on-chain deposit, whose costs no quote or fee schedule
    /// predicts, it is the realized claim fee instead. Which figure it is
    /// for each kind is fixed by the table in [What the numbers
    /// mean](ActivityItem#what-the-numbers-mean).
    ///
    /// Always a separate field from [`ActivityItem::amount`] and never folded
    /// into it: an outgoing row debited `amount + fee`, an incoming row was to
    /// credit `amount - fee`, and a list showing "1000 sat" wants the
    /// number the user typed or the payee received rather than a
    /// fee-inclusive total that matches neither.
    ///
    /// For every kind but that deposit it is the fee for the transfer as
    /// attempted, and not necessarily what the operation finally cost. A
    /// refunded or reclaimed transfer paid this
    /// *and then* paid to have its value returned; the aggregate is on the
    /// operation's details record, per [What the numbers
    /// mean](ActivityItem#what-the-numbers-mean).
    ///
    /// `None` when the kind has no fee at all, or when the fee is not knowable
    /// yet — an operation still in flight, or an on-chain deposit whose claim
    /// costs only exist once a claim has been accepted. `Some(zero)` and
    /// `None` are different answers, and a UI should treat them so: the first
    /// says the transfer was free, the second that this row cannot say yet.
    pub fee: Option<Amount>,
    /// Whether value moved in or out.
    ///
    /// `None` for kinds that have no direction — a recovery, for example,
    /// is neither incoming nor outgoing. This is `Option` rather than a
    /// third "neither" variant on [`Direction`] so that a UI branching on
    /// direction handles the no-direction case by not drawing an arrow at
    /// all, rather than by drawing a third kind of arrow.
    ///
    /// It also selects which half of the accounting identity applies:
    /// [`Outgoing`](Direction::Outgoing) debited `amount + fee`,
    /// [`Incoming`](Direction::Incoming) was to credit `amount - fee` — in
    /// both cases for the transfer as attempted, and never a claim about what
    /// the balance finally did, which the operation's details record holds
    /// even for a [`Success`](ActivityStatus::Success) row. A row with no
    /// direction has no identity to reconcile, and both figures are `None`.
    pub direction: Option<Direction>,
    /// How the operation turned out, or that it has not yet.
    pub status: ActivityStatus,
    /// Whether the operation has finished.
    ///
    /// `true` once it has reached a state it will never leave — the same
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
/// [`ActivityItem::operation_id`]. Mapping a rich state machine down to a
/// handful of buckets is the whole job of this type.
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
/// # An uninterpretable row
///
/// A row whose [`kind`](crate::ActivityItem::kind) is
/// [`OperationKind::Unknown`](crate::OperationKind::Unknown) was written by a
/// version of the SDK that understood something this one does not, so its
/// outcome cannot be interpreted and must not be guessed at. Such a row
/// reports [`Unknown`](Self::Unknown).
///
/// It does **not** report [`Pending`](Self::Pending). An earlier revision of
/// this documentation said it did, on the reasoning that "this SDK cannot
/// tell that it finished" is the honest answer; it is not, for two reasons.
/// It is a claim about the money — a payment that settled long ago rendered
/// for ever as one still in flight — and it is not even true: finality is
/// recorded independently of the outcome, so the SDK can tell, and
/// [`ActivityItem::is_final`] says so. A finished row this build cannot read
/// therefore reports `status == Unknown` with `is_final == true`, and one that
/// is genuinely still running reports the same status with
/// `is_final == false`. The two cases are distinguishable, which is the whole
/// point.
///
/// [`amount`](crate::ActivityItem::amount),
/// [`fee`](crate::ActivityItem::fee) and
/// [`direction`](crate::ActivityItem::direction) stay `None`: the fields are
/// absent precisely so there is nothing to render wrongly. What the row
/// actually said about itself is still readable — resolve
/// [`operation_id`](crate::ActivityItem::operation_id) with
/// [`Federation::operation`](crate::Federation::operation) and read
/// [`AnyOperation::raw_kind`](crate::AnyOperation::raw_kind) — so an
/// application can log or display *which* thing it did not understand instead
/// of only that something was unrecognised. Render it as an opaque entry:
/// not as a stalled payment, and not as a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ActivityStatus {
    /// Still in flight — the operation has not reached a final state.
    ///
    /// Reported only for rows this SDK version can interpret. A row it
    /// cannot interpret reports [`Unknown`](Self::Unknown) instead, whether
    /// or not it has finished.
    Pending,
    /// Completed as intended.
    Success,
    /// Ended without completing, and without the value being returned by a
    /// refund or a cancellation.
    Failed,
    /// Ended without completing, with the value returned to the balance —
    /// a lightning payment that could not be routed, for example.
    ///
    /// Returned, not restored in full: the refund is a further federation
    /// transaction with a cost of its own, so the balance ends a little below
    /// where it started. See [The identity describes the
    /// attempt](ActivityItem#the-identity-describes-the-attempt-and-a-return-costs-money).
    Refunded,
    /// Ended without the transfer happening, because it was called off or
    /// simply lapsed — reclaimed out-of-band ecash, a lightning receive
    /// withdrawn before it was paid, or an invoice whose expiry passed
    /// unpaid.
    ///
    /// "The transfer did not happen" is not the same as "nothing moved". A
    /// reclaim of out-of-band ecash debited the balance when the notes were
    /// issued and restores less than that when they come back, because
    /// reclaiming is a transaction too; an invoice that lapsed unpaid, by
    /// contrast, never moved anything at all. Both are this bucket, and the
    /// operation's details record is what distinguishes them numerically.
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
    ///   [`OperationKind::Unknown`](crate::OperationKind::Unknown) — a record
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
    /// the SDK does answer — read [`ActivityItem::is_final`]. A UI should show
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
            operation_id: OperationId::from_raw("op".to_owned()),
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
        // than the row's amount — never the other way round.
        let credited = received
            .amount
            .and_then(|amount| received.fee.and_then(|fee| amount.checked_sub(fee)));
        assert_eq!(credited, Some(Amount::from_msats(990)));
        assert!(credited < received.amount);
    }

    /// The reclaimed-send case the accounting section was rewritten for. The
    /// row's two numbers reconcile with the debit that funded the attempt and
    /// with nothing else; what the balance ended up doing is on the
    /// operation's own record.
    #[test]
    fn a_canceled_rows_numbers_describe_the_attempt_not_the_movement() {
        let reclaimed = row(
            OperationKind::EcashSend,
            Some(Amount::from_msats(1_536)),
            Some(Amount::from_msats(64)),
            Some(Direction::Outgoing),
            ActivityStatus::Canceled,
            true,
        );
        // The same operation as its details record has it: 1600 msat was the
        // quoted debit, 1604 actually left the balance when the notes were
        // issued, and 1408 came back once the reclaim's own transaction had
        // been paid for.
        let details = crate::EcashSendDetails {
            notes: crate::Notes::from_raw("notes".to_owned()),
            requested_amount: Amount::from_msats(1_234),
            notes_value: Amount::from_msats(1_536),
            quoted_fee: Amount::from_msats(64),
            quoted_total_debited: Amount::from_msats(1_600),
            realized_total_debited: Some(Amount::from_msats(1_604)),
            restored_amount: Some(Amount::from_msats(1_408)),
            realized_fee: Some(Amount::from_msats(196)),
            reclaim_at: Timestamp::from_epoch_millis(1_700_086_400_000),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        };

        // `amount + fee` still means something for a canceled row: it is the
        // debit the attempt was funded with.
        let debited = reclaimed
            .amount
            .and_then(|amount| reclaimed.fee.and_then(|fee| amount.checked_add(fee)));
        assert_eq!(debited, Some(details.quoted_total_debited));

        // It is not the movement. Reading "canceled" as "the debit came back"
        // overstates the balance by what the reclaim cost, and this row
        // carries no number that corrects it — the record does.
        let restored = details.restored_amount.expect("the reclaim settled");
        let realized_debit = details
            .realized_total_debited
            .expect("the funding transaction was accepted");
        assert!(restored < realized_debit);
        assert_eq!(realized_debit.checked_sub(restored), details.realized_fee);
        // The sunk cost is strictly more than the fee this row reports.
        assert!(details.realized_fee > reclaimed.fee);
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
