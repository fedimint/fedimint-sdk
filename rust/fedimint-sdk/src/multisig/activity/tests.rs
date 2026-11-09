use super::*;
use crate::{Amount, Timestamp};

#[test]
fn multisig_states_land_in_their_documented_buckets() {
    let failed = || String::from("refused");
    assert_eq!(
        MultisigDepositState::Accepted.bucket(),
        ActivityStatus::Pending
    );
    assert_eq!(MultisigDepositState::Done.bucket(), ActivityStatus::Success);
    assert_eq!(
        MultisigDepositState::Failed { reason: failed() }.bucket(),
        ActivityStatus::Failed
    );
    assert_eq!(
        MultisigTransferState::Created.bucket(),
        ActivityStatus::Pending
    );
    assert_eq!(
        MultisigTransferState::Done.bucket(),
        ActivityStatus::Success
    );
    assert_eq!(
        MultisigTransferState::Failed { reason: failed() }.bucket(),
        ActivityStatus::Failed
    );
    assert_eq!(
        MultisigSweepState::Accepted.bucket(),
        ActivityStatus::Pending
    );
    assert_eq!(
        MultisigSweepState::Done {
            amount: Amount::from_msats(1)
        }
        .bucket(),
        ActivityStatus::Success
    );
    assert_eq!(
        MultisigSweepState::Failed { reason: failed() }.bucket(),
        ActivityStatus::Failed
    );
}

#[test]
fn multisig_details_yield_what_moved_this_balance_and_which_way() {
    let account_id = "ab".repeat(32).parse().expect("a valid id");
    let created_at = Timestamp::from_epoch_millis(1_700_000_000_000);

    let deposit = MultisigDepositDetails {
        account_id,
        amount: Amount::from_msats(10_000),
        created_at,
    };
    assert_eq!(
        deposit.figures(),
        Figures {
            amount: Some(Amount::from_msats(10_000)),
            fee: None,
            direction: Some(Direction::Outgoing),
        }
    );

    // A payout moves value between two accounts, neither of them this balance.
    let btc = MultisigTransferDetails {
        account_id,
        amount: MultisigAmount::Btc {
            amount: Amount::from_msats(4_000),
        },
        created_at,
    };
    assert_eq!(
        btc.figures(),
        Figures {
            amount: Some(Amount::from_msats(4_000)),
            fee: None,
            direction: None,
        }
    );
    let fiat = MultisigTransferDetails {
        amount: MultisigAmount::Fiat {
            minor_units: 100,
            currency: "USD".to_owned(),
        },
        ..btc
    };
    assert_eq!(fiat.figures(), Figures::NONE);

    let sweep = MultisigSweepDetails {
        amount: Some(Amount::from_msats(4_000)),
        created_at,
    };
    assert_eq!(
        sweep.figures(),
        Figures {
            amount: Some(Amount::from_msats(4_000)),
            fee: None,
            direction: Some(Direction::Incoming),
        }
    );
    let running = MultisigSweepDetails {
        amount: None,
        ..sweep
    };
    assert_eq!(running.figures().amount, None);
}
