//! On-chain Bitcoin: deposits into the federation and withdrawals out of
//! it.

use std::sync::Arc;

use crate::{Address, Operation, OperationState, Result, Sats, Timestamp, Txid};

/// The on-chain facade for one federation, backed by its wallet module.
///
/// Obtained from [`Federation::onchain`](crate::Federation::onchain), which
/// returns `None` when the federation has no wallet module.
///
/// Everything here is denominated in whole satoshis
/// ([`Sats`](crate::Sats)), not millisatoshis. Bitcoin has no sub-satoshi
/// unit, so using the millisatoshi [`Amount`](crate::Amount) type would
/// mean every on-chain call had to decide what to do with a remainder, and
/// the safe answer to that is never "silently drop it". Converting between
/// the two is always the caller's explicit choice — see
/// [`Amount::to_sats_exact`](crate::Amount::to_sats_exact).
#[derive(Debug, Clone)]
pub struct Onchain {
    inner: Arc<OnchainInner>,
}

impl Onchain {
    /// Allocates a fresh deposit address and starts watching it.
    ///
    /// Each call returns a new address, so an application never has to
    /// reuse one. The returned operation begins in
    /// [`OnchainReceiveState::WaitingForTransaction`] and follows the
    /// deposit through confirmation to the balance credit.
    ///
    /// The address is watched from the moment this returns, and the watch
    /// is persistent: a deposit that arrives while the application is
    /// closed is picked up when the SDK is next built over the same
    /// storage. Note that this is the ordinary
    /// [detached-operation](crate::Operation) behaviour, not a special
    /// case.
    ///
    /// There is no quote for deposits — the sender pays the Bitcoin
    /// network fee out of their own wallet, and the federation's peg-in
    /// terms apply to whatever arrives.
    ///
    /// # Errors
    ///
    /// [`Recovering`](crate::ErrorCode::Recovering),
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn receive(&self) -> Result<OnchainReceive> {
        unimplemented!()
    }

    /// Plans a withdrawal and returns an executable quote for it.
    ///
    /// Like its lightning counterpart, this exists because the fee is only
    /// knowable after the SDK has worked out how the federation will build
    /// and broadcast the transaction. The returned [`OnchainQuote`] binds
    /// the destination address, the amount, the fee, the total debit, and
    /// the federation configuration those were computed against, and
    /// [`Onchain::send`] executes exactly that or refuses.
    ///
    /// This is also where the address's network is checked against the
    /// federation's. A well-formed address for the wrong chain is caught
    /// here, with
    /// [`NetworkMismatch`](crate::ErrorCode::NetworkMismatch), rather than
    /// after the funds have moved — parsing an
    /// [`Address`](crate::Address) cannot do this check, because at parse
    /// time there is no federation to compare against.
    ///
    /// # Errors
    ///
    /// [`NetworkMismatch`](crate::ErrorCode::NetworkMismatch),
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for an amount the
    /// federation cannot withdraw (zero, or below its dust threshold),
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance) when
    /// the balance cannot cover amount plus fee,
    /// [`Recovering`](crate::ErrorCode::Recovering),
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn quote(&self, address: &Address, amount: Sats) -> Result<OnchainQuote> {
        unimplemented!()
    }

    /// Executes a quoted withdrawal.
    ///
    /// The quote is consumed and executed as quoted — same destination,
    /// same amount, same fee — or the call fails with
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired) if its validity
    /// window has passed, or
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) if the fee estimate
    /// or federation configuration it was built on has moved. In both
    /// cases the remedy is the same: quote again and re-confirm.
    ///
    /// The returned operation reaches
    /// [`OnchainSendState::Succeeded`] once the federation has broadcast
    /// the transaction. That is the SDK's finish line, not the chain's:
    /// confirmation of the withdrawal transaction on the Bitcoin network is
    /// the recipient's business, and the
    /// [`Txid`](crate::Txid) in that state is what an application shows or
    /// links to a block explorer.
    ///
    /// # Errors
    ///
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired),
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged),
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance),
    /// [`Recovering`](crate::ErrorCode::Recovering),
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn send(&self, quote: OnchainQuote) -> Result<Operation<OnchainSendState>> {
        unimplemented!()
    }
}

/// A frozen, executable plan for one on-chain withdrawal.
///
/// Produced by [`Onchain::quote`] and consumed by [`Onchain::send`]. As
/// with [`LnQuote`](crate::LnQuote), the accessors expose exactly what a
/// user must approve and nothing else; the plan itself is the SDK's to
/// keep.
#[derive(Debug)]
pub struct OnchainQuote {
    inner: OnchainQuoteInner,
}

impl OnchainQuote {
    /// The amount that will arrive at the destination address.
    pub fn amount(&self) -> Sats {
        unimplemented!()
    }

    /// The fee this withdrawal will cost, on top of
    /// [`OnchainQuote::amount`].
    ///
    /// This covers the federation's peg-out fee and its estimate of the
    /// Bitcoin network fee for the transaction it will build.
    pub fn fee(&self) -> Sats {
        unimplemented!()
    }

    /// The total that will be debited from the balance:
    /// [`OnchainQuote::amount`] plus [`OnchainQuote::fee`].
    ///
    /// This is the number to show as "you will pay".
    pub fn total(&self) -> Sats {
        unimplemented!()
    }

    /// When this quote stops being executable.
    ///
    /// Past this point [`Onchain::send`] fails with
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired). On-chain quotes
    /// tend to be shorter-lived than lightning ones, because the fee
    /// estimate they carry tracks a moving mempool.
    pub fn expires_at(&self) -> Timestamp {
        unimplemented!()
    }
}

/// The result of [`Onchain::receive`]: the address to fund, and the
/// operation tracking the deposit.
#[derive(Debug)]
#[non_exhaustive]
pub struct OnchainReceive {
    /// The deposit address to display, encode as a QR code, or hand to a
    /// sender.
    pub address: Address,
    /// Tracks the deposit from the first sight of a transaction through to
    /// the balance credit.
    pub operation: Operation<OnchainReceiveState>,
}

/// The lifecycle of an on-chain withdrawal.
///
/// This maps one-to-one onto upstream `fedimint-wallet-client`'s
/// `WithdrawState` (`Created`, `Succeeded(Txid)`, `Failed(String)`); the
/// only change is that the payloads are named fields rather than positional
/// ones, so they cross a foreign-function boundary as records.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OnchainSendState {
    /// The withdrawal has been accepted and the federation is assembling
    /// and signing the transaction.
    Created,
    /// Final: the federation broadcast the transaction.
    ///
    /// The funds have left the federation. Confirmation on the Bitcoin
    /// network happens afterwards and is not tracked here.
    Succeeded {
        /// The transaction id, for receipts and block explorers.
        txid: Txid,
    },
    /// Final: the withdrawal did not happen.
    Failed {
        /// Human-readable explanation. Diagnostic only — not a stable
        /// contract, and not something to match on.
        reason: String,
    },
}

impl crate::operation::sealed::Sealed for OnchainSendState {}

impl OperationState for OnchainSendState {
    fn is_final(&self) -> bool {
        match self {
            OnchainSendState::Created => false,
            OnchainSendState::Succeeded { .. } | OnchainSendState::Failed { .. } => true,
        }
    }
}

/// The lifecycle of an on-chain deposit.
///
/// This follows upstream `fedimint-wallet-client`'s `DepositStateV2` variant
/// for variant, but not payload for payload. Upstream's variants are
/// `WaitingForTransaction`,
/// `WaitingForConfirmation { btc_deposited, btc_out_point }`,
/// `Confirmed { btc_deposited, btc_out_point }`,
/// `Claimed { btc_deposited, btc_out_point }`, and `Failed(String)` — note
/// that all three of the middle variants carry the same pair, not just
/// `WaitingForConfirmation`. This enum differs from that in two deliberate
/// ways:
///
/// - **Only the transaction half of the outpoint is carried, and only while
///   it is actionable.** Upstream identifies the funding transaction by an
///   outpoint; the [`Txid`](crate::Txid) reported by
///   [`WaitingForConfirmation`](Self::WaitingForConfirmation) and
///   [`Confirmed`](Self::Confirmed) is its transaction half, which is what
///   a receipt or a block-explorer link needs. The vout is dropped because
///   nothing in this API takes one. [`Claimed`](Self::Claimed) deliberately
///   carries no outpoint at all: once the value is in the balance the
///   deposit is an ordinary credit, and an application that wants the
///   transaction has already seen it in the two preceding states.
/// - **`Claimed.amount` is a net figure this SDK computes, not upstream's
///   gross one.** Upstream's `btc_deposited` is the amount that arrived on
///   chain, before the federation's peg-in fee. What
///   [`Claimed`](Self::Claimed) reports is the amount actually credited to
///   the balance — deposited less that fee — because that is the number a
///   user sees their balance move by. Upstream never reports that figure,
///   so the SDK derives it; an application needing the gross amount must
///   record it from an earlier state.
///
/// Amounts are reported as whole [`Sats`](crate::Sats) throughout, like the
/// rest of this facade.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OnchainReceiveState {
    /// The address is being watched and no transaction paying it has been
    /// seen yet. A deposit can sit here indefinitely — until someone
    /// sends, there is nothing to report.
    WaitingForTransaction,
    /// A transaction paying the address has been seen and is waiting for
    /// enough confirmations for the federation to accept it.
    WaitingForConfirmation {
        /// The funding transaction.
        txid: Txid,
    },
    /// The transaction has the confirmations the federation requires; the
    /// deposit is being claimed into the balance.
    Confirmed {
        /// The funding transaction.
        txid: Txid,
    },
    /// Final: the deposit is in the spendable balance.
    Claimed {
        /// The amount credited, which is the deposited amount less the
        /// federation's peg-in fee.
        ///
        /// This is computed by the SDK. It is **not** upstream's
        /// `btc_deposited`, which is the gross amount that arrived on chain
        /// with no fee deducted.
        amount: Sats,
    },
    /// Final: the deposit could not be claimed.
    Failed {
        /// Human-readable explanation. Diagnostic only — not a stable
        /// contract, and not something to match on.
        reason: String,
    },
}

impl crate::operation::sealed::Sealed for OnchainReceiveState {}

impl OperationState for OnchainReceiveState {
    fn is_final(&self) -> bool {
        match self {
            OnchainReceiveState::WaitingForTransaction
            | OnchainReceiveState::WaitingForConfirmation { .. }
            | OnchainReceiveState::Confirmed { .. } => false,
            OnchainReceiveState::Claimed { .. } | OnchainReceiveState::Failed { .. } => true,
        }
    }
}

/// Placeholder for the wallet-module state this facade operates on.
#[derive(Debug)]
struct OnchainInner;

/// Placeholder for a quote's frozen plan: destination, amount, fee, and
/// the configuration context they were computed against. Held by value
/// rather than behind an `Arc`, because a quote is owned by one caller and
/// consumed once, never shared.
#[derive(Debug)]
struct OnchainQuoteInner;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onchain_send_state_created_is_not_final() {
        assert!(!OnchainSendState::Created.is_final());
    }

    // `OnchainSendState::Succeeded` carries a `Txid`, which cannot be
    // constructed from this module: its field is private to
    // `crate::types::ids` and its only public constructor,
    // `FromStr::from_str`, is `unimplemented!()`. So this variant cannot be
    // built in a test without panicking.

    #[test]
    fn onchain_send_state_failed_is_final() {
        assert!(
            OnchainSendState::Failed {
                reason: String::new(),
            }
            .is_final()
        );
    }

    #[test]
    fn onchain_receive_state_waiting_for_transaction_is_not_final() {
        assert!(!OnchainReceiveState::WaitingForTransaction.is_final());
    }

    // `OnchainReceiveState::WaitingForConfirmation` and `::Confirmed` both
    // carry a `Txid`, which cannot be constructed from this module for the
    // same reason as `OnchainSendState::Succeeded` above.

    #[test]
    fn onchain_receive_state_claimed_is_final() {
        assert!(
            OnchainReceiveState::Claimed {
                amount: Sats::from_sats(0),
            }
            .is_final()
        );
    }

    #[test]
    fn onchain_receive_state_failed_is_final() {
        assert!(
            OnchainReceiveState::Failed {
                reason: String::new(),
            }
            .is_final()
        );
    }
}
