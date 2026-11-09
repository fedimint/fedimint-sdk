//! Where the multisig facade's operations fall in the activity history: which bucket each of its
//! states lands in, and which figures each of its details records yields, as the tables on
//! [`ActivityStatus`] and [`ActivityItem`](crate::ActivityItem) say.

use crate::activity::{Accounted, Bucket, Figures, Row, detailed};
use crate::operation::Driver;
use crate::{
    ActivityStatus, Direction, MultisigAmount, MultisigDepositDetails, MultisigDepositState,
    MultisigSweepDetails, MultisigSweepState, MultisigTransferDetails, MultisigTransferState,
    Result,
};

impl Bucket for MultisigDepositState {
    fn bucket(&self) -> ActivityStatus {
        match self {
            MultisigDepositState::Created | MultisigDepositState::Accepted => {
                ActivityStatus::Pending
            }
            MultisigDepositState::Done => ActivityStatus::Success,
            MultisigDepositState::Failed { .. } => ActivityStatus::Failed,
        }
    }
}

impl Bucket for MultisigTransferState {
    fn bucket(&self) -> ActivityStatus {
        match self {
            MultisigTransferState::Created => ActivityStatus::Pending,
            MultisigTransferState::Done => ActivityStatus::Success,
            MultisigTransferState::Failed { .. } => ActivityStatus::Failed,
        }
    }
}

impl Bucket for MultisigSweepState {
    fn bucket(&self) -> ActivityStatus {
        match self {
            MultisigSweepState::Created | MultisigSweepState::Accepted => ActivityStatus::Pending,
            MultisigSweepState::Done { .. } => ActivityStatus::Success,
            MultisigSweepState::Failed { .. } => ActivityStatus::Failed,
        }
    }
}

impl Accounted for MultisigDepositDetails {
    /// Outgoing: the deposit leaves this wallet's balance for the account. The fee for spending
    /// the ecash is not recorded, so it is `None` rather than a guess.
    fn figures(&self) -> Figures {
        Figures {
            amount: Some(self.amount),
            fee: None,
            direction: Some(Direction::Outgoing),
        }
    }
}

impl Accounted for MultisigTransferDetails {
    /// No direction and no fee: the payment moves value between two accounts of the federation,
    /// neither of them this wallet's balance. A fiat amount has no [`Amount`](crate::Amount) to
    /// report.
    fn figures(&self) -> Figures {
        Figures {
            amount: match &self.amount {
                MultisigAmount::Btc { amount } => Some(*amount),
                MultisigAmount::Fiat { .. } => None,
            },
            fee: None,
            direction: None,
        }
    }
}

impl Accounted for MultisigSweepDetails {
    /// Incoming: the payouts come into this wallet's balance. How much is known once the sweep is
    /// done, and the fee for issuing the ecash is not recorded, so each is `None` rather than a
    /// guess until then.
    fn figures(&self) -> Figures {
        Figures {
            amount: self.amount,
            fee: None,
            direction: Some(Direction::Incoming),
        }
    }
}

impl Row for MultisigDepositState {
    fn figures(driver: &dyn Driver<MultisigDepositState>, details: &str) -> Result<Figures> {
        detailed(driver, details)
    }
}

impl Row for MultisigTransferState {
    fn figures(driver: &dyn Driver<MultisigTransferState>, details: &str) -> Result<Figures> {
        detailed(driver, details)
    }
}

impl Row for MultisigSweepState {
    fn figures(driver: &dyn Driver<MultisigSweepState>, details: &str) -> Result<Figures> {
        detailed(driver, details)
    }
}

#[cfg(test)]
mod tests;
