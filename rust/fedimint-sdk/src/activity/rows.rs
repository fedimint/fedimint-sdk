//! The two tables behind an activity row: which bucket a state lands in, and which figures a
//! details record yields.

use crate::operation::Driver;
use crate::{
    ActivityStatus, Amount, DetailedOperationState, Direction, EcashReceiveDetails,
    EcashReceiveState, EcashSendDetails, EcashSendState, Error, ErrorCode, LnReceiveDetails,
    LnReceiveState, LnSendDetails, LnSendState, OnchainReceiveDetails, OnchainReceiveState,
    OnchainSendDetails, OnchainSendState, OperationDetails, OperationState, RecoveryState, Result,
    Sats,
};

/// The three numbers a row carries, as the table on [`ActivityItem`](crate::ActivityItem) fixes
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Figures {
    pub(super) amount: Option<Amount>,
    pub(super) fee: Option<Amount>,
    pub(super) direction: Option<Direction>,
}

impl Figures {
    /// No figures at all: a recovery, or a row whose outcome cannot be read.
    pub(super) const NONE: Figures = Figures {
        amount: None,
        fee: None,
        direction: None,
    };
}

/// Which bucket a state lands in, exactly as the table on
/// [`ActivityStatus`](crate::ActivityStatus) says.
pub(super) trait Bucket: OperationState {
    fn bucket(&self) -> ActivityStatus;
}

impl Bucket for EcashSendState {
    fn bucket(&self) -> ActivityStatus {
        match self {
            EcashSendState::Created | EcashSendState::CancelRequested => ActivityStatus::Pending,
            EcashSendState::Redeemed => ActivityStatus::Success,
            EcashSendState::Canceled => ActivityStatus::Canceled,
        }
    }
}

impl Bucket for EcashReceiveState {
    fn bucket(&self) -> ActivityStatus {
        match self {
            EcashReceiveState::Created | EcashReceiveState::Issuing => ActivityStatus::Pending,
            EcashReceiveState::Done => ActivityStatus::Success,
            EcashReceiveState::Failed { .. } => ActivityStatus::Failed,
        }
    }
}

impl Bucket for LnSendState {
    fn bucket(&self) -> ActivityStatus {
        match self {
            LnSendState::Created | LnSendState::Funded => ActivityStatus::Pending,
            LnSendState::Success { .. } => ActivityStatus::Success,
            LnSendState::Refunded => ActivityStatus::Refunded,
            LnSendState::Failed { .. } => ActivityStatus::Failed,
        }
    }
}

impl Bucket for LnReceiveState {
    fn bucket(&self) -> ActivityStatus {
        match self {
            LnReceiveState::Created
            | LnReceiveState::WaitingForPayment
            | LnReceiveState::Funded => ActivityStatus::Pending,
            LnReceiveState::Claimed => ActivityStatus::Success,
            LnReceiveState::Canceled { .. } => ActivityStatus::Canceled,
            // An invoice that simply lapsed unpaid is not a failure: nothing broke, the payment
            // just never arrived, so it joins the withdrawn-invoice case here.
            LnReceiveState::Expired => ActivityStatus::Canceled,
            LnReceiveState::Failed => ActivityStatus::Failed,
        }
    }
}

impl Bucket for OnchainSendState {
    fn bucket(&self) -> ActivityStatus {
        match self {
            OnchainSendState::Created => ActivityStatus::Pending,
            OnchainSendState::Succeeded { .. } => ActivityStatus::Success,
            OnchainSendState::Refunded { .. } => ActivityStatus::Refunded,
            OnchainSendState::Failed { .. } => ActivityStatus::Failed,
        }
    }
}

impl Bucket for OnchainReceiveState {
    fn bucket(&self) -> ActivityStatus {
        match self {
            OnchainReceiveState::WaitingForTransaction
            | OnchainReceiveState::WaitingForConfirmation { .. }
            | OnchainReceiveState::Confirmed { .. } => ActivityStatus::Pending,
            OnchainReceiveState::Claimed { .. } => ActivityStatus::Success,
            OnchainReceiveState::Failed { .. } => ActivityStatus::Failed,
        }
    }
}

impl Bucket for RecoveryState {
    fn bucket(&self) -> ActivityStatus {
        match self {
            RecoveryState::Running => ActivityStatus::Pending,
            RecoveryState::Done => ActivityStatus::Success,
            RecoveryState::Failed { .. } => ActivityStatus::Failed,
        }
    }
}

/// The figures a details record yields, exactly as the table on
/// [`ActivityItem`](crate::ActivityItem) says.
pub(super) trait Accounted: OperationDetails {
    fn figures(&self) -> Figures;
}

impl Accounted for EcashSendDetails {
    fn figures(&self) -> Figures {
        Figures {
            amount: Some(self.notes_value),
            fee: Some(self.fee),
            direction: Some(Direction::Outgoing),
        }
    }
}

impl Accounted for EcashReceiveDetails {
    fn figures(&self) -> Figures {
        Figures {
            amount: Some(self.notes_value),
            fee: Some(self.fee),
            direction: Some(Direction::Incoming),
        }
    }
}

impl Accounted for LnSendDetails {
    fn figures(&self) -> Figures {
        Figures {
            amount: Some(self.invoice_amount),
            fee: Some(self.fee),
            direction: Some(Direction::Outgoing),
        }
    }
}

impl Accounted for LnReceiveDetails {
    fn figures(&self) -> Figures {
        Figures {
            amount: Some(self.invoice_amount),
            fee: Some(self.fee),
            direction: Some(Direction::Incoming),
        }
    }
}

impl Accounted for OnchainSendDetails {
    fn figures(&self) -> Figures {
        Figures {
            amount: self.amount.to_amount(),
            fee: Some(self.fee),
            direction: Some(Direction::Outgoing),
        }
    }
}

impl Accounted for OnchainReceiveDetails {
    fn figures(&self) -> Figures {
        Figures {
            // `gross_deposited` is `Option<Sats>`, so `to_amount`'s own `Option<Amount>` has to
            // be flattened rather than mapped into it; the brief's `.map` would leave this an
            // `Option<Option<Amount>>`, which does not typecheck against `Figures::amount`.
            amount: self.gross_deposited.and_then(Sats::to_amount),
            fee: self.fee,
            direction: Some(Direction::Incoming),
        }
    }
}

/// A state type whose rows the page walk can build: its bucket, and the figures of the record
/// its kind persists.
pub(super) trait Row: Bucket {
    /// The figures from this kind's details JSON, decoded through its driver.
    fn figures(driver: &dyn Driver<Self>, details: &str) -> Result<Figures>;
}

impl Row for EcashSendState {
    fn figures(driver: &dyn Driver<EcashSendState>, details: &str) -> Result<Figures> {
        detailed(driver, details)
    }
}

impl Row for EcashReceiveState {
    fn figures(driver: &dyn Driver<EcashReceiveState>, details: &str) -> Result<Figures> {
        detailed(driver, details)
    }
}

impl Row for LnSendState {
    fn figures(driver: &dyn Driver<LnSendState>, details: &str) -> Result<Figures> {
        detailed(driver, details)
    }
}

impl Row for LnReceiveState {
    fn figures(driver: &dyn Driver<LnReceiveState>, details: &str) -> Result<Figures> {
        detailed(driver, details)
    }
}

impl Row for OnchainSendState {
    fn figures(driver: &dyn Driver<OnchainSendState>, details: &str) -> Result<Figures> {
        detailed(driver, details)
    }
}

impl Row for OnchainReceiveState {
    fn figures(driver: &dyn Driver<OnchainReceiveState>, details: &str) -> Result<Figures> {
        detailed(driver, details)
    }
}

impl Row for RecoveryState {
    /// A recovery has no details record, so this never touches the driver: `decode_details` on
    /// its driver is `Internal` by contract, and no caller can reach it.
    fn figures(_driver: &dyn Driver<RecoveryState>, _details: &str) -> Result<Figures> {
        Ok(Figures::NONE)
    }
}

/// [`Row::figures`] for a kind that persists a details record.
fn detailed<S>(driver: &dyn Driver<S>, details: &str) -> Result<Figures>
where
    S: DetailedOperationState,
    S::Details: Accounted,
{
    let decoded = driver.decode_details(details)?;
    match decoded.downcast::<S::Details>() {
        Ok(details) => Ok(details.figures()),
        // Unreachable through the page walk, which pairs each kind with its own driver; an
        // `Internal` rather than a panic for the same reason `Operation::details` gives.
        Err(_) => Err(Error::new(
            ErrorCode::Internal,
            "this operation's details record does not match its kind",
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use fedimint_core::core::OperationId as UpstreamOperationId;
    use fedimint_core::util::{BoxFuture, BoxStream};

    use super::*;
    use crate::db::OperationRecord;
    use crate::federation::FederationInner;
    use crate::{Address, LightningRoute, OnchainReceiveFeeBreakdown, Timestamp, Txid};

    /// A real out-of-band ecash token worth 1 satoshi, copied from `ecash.rs`'s own tests: no
    /// part of it may appear in a `Debug` output, but nothing here prints one.
    const TOKEN: &str = "AgEEKioqKgBVAf0D6AGl3T66ytG8SL2HGO7VqNodaPkTI77yhIrE-i5vju1xDzF4_UrvBHzCNOaxEnCG8zzECLOYGHgdlSFHU2DeayBfMyjkkKbZnV4lU6RVMgfIvQ==";

    /// A real regtest invoice for 100_000 msat, copied from `lightning/wire.rs`'s own tests.
    const INVOICE: &str = "lnbcrt1u1pj48ugqdq2vdhkven9v5pp5g3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zqsp5242424242424242424242424242424242424242424242424242s9qrsgqcqzys2reg4wsryjt5w8z33ugydecgfmgyvtttwa7e0yzlm803z203j9hqspa4lr6m09cd808xkw9uh4sxc8wf3w6k0gaf5zrqm7zhcxug0vqqpdkpja";

    /// A real regtest address, copied from `onchain.rs`'s own tests.
    fn an_address() -> Address {
        "bcrt1q2nfxmhd4n3c8834pj72xagvyr9gl57n5r94fsl"
            .parse()
            .expect("a valid regtest address")
    }

    /// The all-zero txid: these tests never look at its value, only carry it through a payload.
    fn a_txid() -> Txid {
        "0000000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .expect("a well-formed transaction id")
    }

    fn ecash_send_details() -> EcashSendDetails {
        EcashSendDetails {
            notes: TOKEN.parse().expect("a valid ecash token"),
            requested_amount: Amount::from_msats(750),
            notes_value: Amount::from_msats(1_000),
            fee: Amount::from_msats(50),
            total_debited: Amount::from_msats(1_050),
            reclaim_at: Timestamp::from_epoch_millis(1_700_086_400_000),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    fn ecash_receive_details() -> EcashReceiveDetails {
        EcashReceiveDetails {
            notes: TOKEN.parse().expect("a valid ecash token"),
            notes_value: Amount::from_msats(1_000),
            fee: Amount::from_msats(36),
            net_credit: Amount::from_msats(964),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    fn ln_send_details() -> LnSendDetails {
        LnSendDetails {
            invoice: INVOICE.parse().expect("a valid regtest invoice"),
            invoice_amount: Amount::from_msats(100_000),
            fee: Amount::from_msats(1_050),
            total: Amount::from_msats(101_050),
            route: LightningRoute::Internal,
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    fn ln_receive_details() -> LnReceiveDetails {
        LnReceiveDetails {
            invoice: INVOICE.parse().expect("a valid regtest invoice"),
            description: "coffee".to_owned(),
            requested_amount: Amount::from_msats(100_000),
            invoice_amount: Amount::from_msats(100_000),
            fee: Amount::from_msats(500),
            net_credit: Amount::from_msats(99_500),
            gateway_id: None,
            expires_at: Timestamp::from_epoch_millis(1_700_003_600_000),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    fn onchain_send_details() -> OnchainSendDetails {
        let amount = Sats::from_sats(25_000);
        let fee = Amount::from_msats(4_321);
        OnchainSendDetails {
            address: an_address(),
            amount,
            fee,
            total: amount
                .to_amount()
                .expect("25 000 sat is representable in msat")
                .checked_add(fee)
                .expect("no overflow at this magnitude"),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    /// A deposit before any transaction has been seen: every fillable field is still `None`.
    fn onchain_receive_details_waiting() -> OnchainReceiveDetails {
        OnchainReceiveDetails {
            address: an_address(),
            txid: None,
            gross_deposited: None,
            fee: None,
            fee_breakdown: None,
            net_credit: None,
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    /// The same deposit once its claim has settled.
    fn onchain_receive_details_claimed() -> OnchainReceiveDetails {
        let gross = Sats::from_sats(100_000);
        let fee = Amount::from_msats(1_500);
        let net_credit = gross
            .to_amount()
            .expect("100 000 sat is representable in msat")
            .checked_sub(fee)
            .expect("the fee is smaller than the deposit");
        OnchainReceiveDetails {
            txid: Some(a_txid()),
            gross_deposited: Some(gross),
            fee: Some(fee),
            fee_breakdown: Some(OnchainReceiveFeeBreakdown {
                peg_in: fee,
                network_claim: Amount::from_msats(0),
                primary_module: Amount::from_msats(0),
                dust: Amount::from_msats(0),
            }),
            net_credit: Some(net_credit),
            ..onchain_receive_details_waiting()
        }
    }

    #[test]
    fn ecash_send_state_lands_in_its_documented_bucket() {
        assert_eq!(EcashSendState::Created.bucket(), ActivityStatus::Pending);
        assert_eq!(
            EcashSendState::CancelRequested.bucket(),
            ActivityStatus::Pending
        );
        assert_eq!(EcashSendState::Redeemed.bucket(), ActivityStatus::Success);
        assert_eq!(EcashSendState::Canceled.bucket(), ActivityStatus::Canceled);

        for state in [
            EcashSendState::Created,
            EcashSendState::CancelRequested,
            EcashSendState::Redeemed,
            EcashSendState::Canceled,
        ] {
            assert_eq!(state.bucket() == ActivityStatus::Pending, !state.is_final());
        }
    }

    #[test]
    fn ecash_receive_state_lands_in_its_documented_bucket() {
        let failed = EcashReceiveState::Failed {
            reason: String::new(),
        };

        assert_eq!(EcashReceiveState::Created.bucket(), ActivityStatus::Pending);
        assert_eq!(EcashReceiveState::Issuing.bucket(), ActivityStatus::Pending);
        assert_eq!(EcashReceiveState::Done.bucket(), ActivityStatus::Success);
        assert_eq!(failed.bucket(), ActivityStatus::Failed);

        for state in [
            EcashReceiveState::Created,
            EcashReceiveState::Issuing,
            EcashReceiveState::Done,
            failed,
        ] {
            assert_eq!(state.bucket() == ActivityStatus::Pending, !state.is_final());
        }
    }

    #[test]
    fn ln_send_state_lands_in_its_documented_bucket() {
        let success = LnSendState::Success {
            preimage: "0000000000000000000000000000000000000000000000000000000000000000"
                .parse()
                .expect("a well-formed preimage"),
            fee: Amount::from_msats(1_050),
            route: LightningRoute::Internal,
        };
        let failed = LnSendState::Failed {
            reason: String::new(),
        };

        assert_eq!(LnSendState::Created.bucket(), ActivityStatus::Pending);
        assert_eq!(LnSendState::Funded.bucket(), ActivityStatus::Pending);
        assert_eq!(success.bucket(), ActivityStatus::Success);
        assert_eq!(LnSendState::Refunded.bucket(), ActivityStatus::Refunded);
        assert_eq!(failed.bucket(), ActivityStatus::Failed);

        for state in [
            LnSendState::Created,
            LnSendState::Funded,
            success,
            LnSendState::Refunded,
            failed,
        ] {
            assert_eq!(state.bucket() == ActivityStatus::Pending, !state.is_final());
        }
    }

    #[test]
    fn ln_receive_state_lands_in_its_documented_bucket() {
        let canceled = LnReceiveState::Canceled {
            reason: String::new(),
        };

        assert_eq!(LnReceiveState::Created.bucket(), ActivityStatus::Pending);
        assert_eq!(
            LnReceiveState::WaitingForPayment.bucket(),
            ActivityStatus::Pending
        );
        assert_eq!(LnReceiveState::Funded.bucket(), ActivityStatus::Pending);
        assert_eq!(LnReceiveState::Claimed.bucket(), ActivityStatus::Success);
        assert_eq!(canceled.bucket(), ActivityStatus::Canceled);
        // The one placement that is a judgement rather than a reading: an invoice that simply
        // lapsed unpaid is not a failure.
        assert_eq!(LnReceiveState::Expired.bucket(), ActivityStatus::Canceled);
        assert_eq!(LnReceiveState::Failed.bucket(), ActivityStatus::Failed);

        for state in [
            LnReceiveState::Created,
            LnReceiveState::WaitingForPayment,
            LnReceiveState::Funded,
            LnReceiveState::Claimed,
            canceled,
            LnReceiveState::Expired,
            LnReceiveState::Failed,
        ] {
            assert_eq!(state.bucket() == ActivityStatus::Pending, !state.is_final());
        }
    }

    #[test]
    fn onchain_send_state_lands_in_its_documented_bucket() {
        let succeeded = OnchainSendState::Succeeded { txid: a_txid() };
        let refunded = OnchainSendState::Refunded {
            reason: String::new(),
        };
        let failed = OnchainSendState::Failed {
            reason: String::new(),
        };

        assert_eq!(OnchainSendState::Created.bucket(), ActivityStatus::Pending);
        assert_eq!(succeeded.bucket(), ActivityStatus::Success);
        assert_eq!(refunded.bucket(), ActivityStatus::Refunded);
        assert_eq!(failed.bucket(), ActivityStatus::Failed);

        for state in [OnchainSendState::Created, succeeded, refunded, failed] {
            assert_eq!(state.bucket() == ActivityStatus::Pending, !state.is_final());
        }
    }

    #[test]
    fn onchain_receive_state_lands_in_its_documented_bucket() {
        let waiting_for_confirmation = OnchainReceiveState::WaitingForConfirmation {
            txid: a_txid(),
            gross_deposited: Sats::from_sats(100_000),
        };
        let confirmed = OnchainReceiveState::Confirmed {
            txid: a_txid(),
            gross_deposited: Sats::from_sats(100_000),
        };
        let claimed = OnchainReceiveState::Claimed {
            txid: a_txid(),
            gross_deposited: Sats::from_sats(100_000),
            net_credit: Amount::from_msats(99_998_500),
        };
        let failed = OnchainReceiveState::Failed {
            reason: String::new(),
        };

        assert_eq!(
            OnchainReceiveState::WaitingForTransaction.bucket(),
            ActivityStatus::Pending
        );
        assert_eq!(waiting_for_confirmation.bucket(), ActivityStatus::Pending);
        assert_eq!(confirmed.bucket(), ActivityStatus::Pending);
        assert_eq!(claimed.bucket(), ActivityStatus::Success);
        assert_eq!(failed.bucket(), ActivityStatus::Failed);

        for state in [
            OnchainReceiveState::WaitingForTransaction,
            waiting_for_confirmation,
            confirmed,
            claimed,
            failed,
        ] {
            assert_eq!(state.bucket() == ActivityStatus::Pending, !state.is_final());
        }
    }

    #[test]
    fn recovery_state_lands_in_its_documented_bucket() {
        let failed = RecoveryState::Failed {
            reason: String::new(),
        };

        assert_eq!(RecoveryState::Running.bucket(), ActivityStatus::Pending);
        assert_eq!(RecoveryState::Done.bucket(), ActivityStatus::Success);
        assert_eq!(failed.bucket(), ActivityStatus::Failed);

        for state in [RecoveryState::Running, RecoveryState::Done, failed] {
            assert_eq!(state.bucket() == ActivityStatus::Pending, !state.is_final());
        }
    }

    #[test]
    fn ecash_send_details_yield_notes_value_fee_and_outgoing() {
        let details = ecash_send_details();
        assert_eq!(
            details.figures(),
            Figures {
                amount: Some(details.notes_value),
                fee: Some(details.fee),
                direction: Some(Direction::Outgoing),
            }
        );
    }

    #[test]
    fn ecash_receive_details_yield_notes_value_fee_and_incoming() {
        let details = ecash_receive_details();
        assert_eq!(
            details.figures(),
            Figures {
                amount: Some(details.notes_value),
                fee: Some(details.fee),
                direction: Some(Direction::Incoming),
            }
        );
    }

    #[test]
    fn ln_send_details_yield_invoice_amount_fee_and_outgoing() {
        let details = ln_send_details();
        assert_eq!(
            details.figures(),
            Figures {
                amount: Some(details.invoice_amount),
                fee: Some(details.fee),
                direction: Some(Direction::Outgoing),
            }
        );
    }

    #[test]
    fn ln_receive_details_yield_invoice_amount_fee_and_incoming() {
        let details = ln_receive_details();
        assert_eq!(
            details.figures(),
            Figures {
                amount: Some(details.invoice_amount),
                fee: Some(details.fee),
                direction: Some(Direction::Incoming),
            }
        );
    }

    #[test]
    fn onchain_send_details_yield_a_whole_multiple_of_a_thousand_msats() {
        let details = onchain_send_details();
        let figures = details.figures();
        assert_eq!(figures.amount, Some(Amount::from_msats(25_000_000)));
        assert_eq!(
            figures.amount.and_then(Amount::to_sats_exact),
            Some(details.amount)
        );
        assert_eq!(figures.fee, Some(details.fee));
        assert_eq!(figures.direction, Some(Direction::Outgoing));
    }

    #[test]
    fn onchain_receive_details_yield_no_figures_before_a_transaction_and_the_credit_after() {
        let waiting = onchain_receive_details_waiting();
        assert_eq!(
            waiting.figures(),
            Figures {
                amount: None,
                fee: None,
                direction: Some(Direction::Incoming),
            }
        );

        let claimed = onchain_receive_details_claimed();
        assert_eq!(
            claimed.figures(),
            Figures {
                amount: claimed.gross_deposited.and_then(Sats::to_amount),
                fee: claimed.fee,
                direction: Some(Direction::Incoming),
            }
        );
    }

    /// A driver whose every method reports `Internal`. [`RecoveryState`]'s [`Row::figures`]
    /// must never call any of them: a recovery has no details record to decode.
    struct RefusingDriver;

    impl Driver<RecoveryState> for RefusingDriver {
        fn current<'a>(
            &'a self,
            _federation: &'a FederationInner,
            _id: UpstreamOperationId,
            _record: &'a OperationRecord,
        ) -> BoxFuture<'a, Result<RecoveryState>> {
            Box::pin(async { Err(Error::new(ErrorCode::Internal, "not to be called")) })
        }

        fn subscribe<'a>(
            &'a self,
            _federation: &'a FederationInner,
            _id: UpstreamOperationId,
            _record: &'a OperationRecord,
        ) -> BoxFuture<'a, Result<BoxStream<'static, Result<RecoveryState>>>> {
            Box::pin(async { Err(Error::new(ErrorCode::Internal, "not to be called")) })
        }

        fn same_state(&self, _previous: &RecoveryState, _next: &RecoveryState) -> bool {
            false
        }

        fn encode_state(&self, _state: &RecoveryState) -> Result<String> {
            Err(Error::new(ErrorCode::Internal, "not to be called"))
        }

        fn decode_state(&self, _encoded: &str) -> Result<RecoveryState> {
            Err(Error::new(ErrorCode::Internal, "not to be called"))
        }

        fn decode_details(&self, _json: &str) -> Result<Box<dyn Any + Send + Sync>> {
            Err(Error::new(ErrorCode::Internal, "not to be called"))
        }
    }

    #[test]
    fn a_recovery_yields_no_figures() {
        let figures = RecoveryState::figures(&RefusingDriver, "").expect("a recovery never fails");
        assert_eq!(figures, Figures::NONE);
    }

    #[test]
    fn figures_of_a_detailed_kind_go_through_the_driver() {
        // A hand-written `LnSendDetailsWire` (`lightning/wire.rs:79-92`), since that type is
        // `pub(super)` to `lightning` and unreachable from here.
        let json = serde_json::json!({
            "invoice": INVOICE,
            "invoice_amount_msats": 100_000,
            "fee_msats": 1_050,
            "total_msats": 101_050,
            "route": { "kind": "internal" },
            "created_at": 1_700_000_000_000u64,
        })
        .to_string();

        let figures = LnSendState::figures(&crate::lightning::LnSendDriver, &json)
            .expect("a well-formed record decodes");
        assert_eq!(
            figures,
            Figures {
                amount: Some(Amount::from_msats(100_000)),
                fee: Some(Amount::from_msats(1_050)),
                direction: Some(Direction::Outgoing),
            }
        );
    }
}
