//! Bolt11 lightning: paying invoices and getting paid.

use std::sync::Arc;

use crate::{Amount, Bolt11Invoice, GatewayId, Operation, OperationState, Result, Timestamp};

/// The lightning facade for one federation, backed by its lightning
/// module.
///
/// Obtained from [`Federation::lightning`](crate::Federation::lightning),
/// which returns `None` when the federation has no lightning module.
///
/// This facade hides the parts of fedimint lightning that applications
/// have historically had to reimplement and get wrong: choosing a gateway,
/// verifying that the chosen gateway is actually reachable and willing,
/// and quoting the fee it will charge. Verification happens *before* an
/// invoice is created or a payment is funded, so a class of failures that
/// used to appear halfway through an operation is instead an error from
/// the call that started it.
#[derive(Debug, Clone)]
pub struct Lightning {
    inner: Arc<LightningInner>,
}

impl Lightning {
    /// Plans a payment and returns an executable quote for it.
    ///
    /// Quoting is a separate step from paying because the numbers a user
    /// must approve — the fee, the total debit, which gateway will carry
    /// the payment — are only knowable after the SDK has picked and
    /// verified a route. The returned [`LnQuote`] is that plan, frozen: it
    /// binds the invoice, the resolved amount, the selected and verified
    /// gateway (or the discovery that no gateway is needed at all), the
    /// fee, the total debit, and the federation configuration those were
    /// computed against. Show it, then hand it back to [`Lightning::send`]
    /// to execute exactly what was shown. A user cannot be quoted one fee
    /// and charged another.
    ///
    /// `amount` is an override, and its rules are strict in both
    /// directions. An amountless invoice **requires** one — without it the
    /// call fails with
    /// [`AmountRequired`](crate::ErrorCode::AmountRequired) rather than
    /// guessing. An invoice that already carries an amount **forbids**
    /// one: passing `Some` there fails with
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput), because a
    /// mismatch between what the payee asked for and what the payer typed
    /// is a bug somewhere and should not be silently resolved in either
    /// direction.
    ///
    /// Quotes expire; see [`LnQuote::expires_at`].
    ///
    /// # Errors
    ///
    /// [`AmountRequired`](crate::ErrorCode::AmountRequired),
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for an amount on an
    /// already-amounted invoice or an invoice that has expired,
    /// [`GatewayUnavailable`](crate::ErrorCode::GatewayUnavailable) when no
    /// gateway can be selected and verified,
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance) when
    /// the balance cannot cover amount plus fee,
    /// [`Recovering`](crate::ErrorCode::Recovering),
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn quote(&self, invoice: &Bolt11Invoice, amount: Option<Amount>) -> Result<LnQuote> {
        unimplemented!()
    }

    /// Executes a quoted payment.
    ///
    /// The quote is consumed: it describes one payment and can fund one
    /// payment. Execution follows the plan exactly — same amount, same
    /// fee, same route — or it does not happen:
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired) if the quote's
    /// validity window has passed,
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) if something the
    /// quote depends on moved underneath it (the gateway withdrew, its fee
    /// changed, the federation configuration was updated). Both mean the
    /// same thing to a caller: quote again and re-confirm with the user.
    ///
    /// The returned operation tracks the payment from funding to preimage;
    /// a payment that fails ends in a final state, not an error from this
    /// call.
    ///
    /// # Errors
    ///
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired),
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged),
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance),
    /// [`GatewayUnavailable`](crate::ErrorCode::GatewayUnavailable),
    /// [`Recovering`](crate::ErrorCode::Recovering),
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn send(&self, quote: LnQuote) -> Result<Operation<LnSendState>> {
        unimplemented!()
    }

    /// Issues an invoice payable into this federation.
    ///
    /// A gateway is selected and verified before the invoice exists, so an
    /// invoice this call returns is one someone can actually pay. The
    /// returned operation tracks the incoming payment through to the
    /// credit landing in the balance.
    ///
    /// `description` is embedded in the invoice and shown to the payer by
    /// their wallet.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a zero amount
    /// or a description the invoice format cannot carry,
    /// [`GatewayUnavailable`](crate::ErrorCode::GatewayUnavailable),
    /// [`Recovering`](crate::ErrorCode::Recovering),
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn receive(&self, amount: Amount, description: &str) -> Result<LnReceive> {
        unimplemented!()
    }
}

/// A frozen, executable plan for one lightning payment.
///
/// Produced by [`Lightning::quote`] and consumed by [`Lightning::send`].
/// Everything a user needs to approve is readable through the accessors
/// below; nothing else is exposed, because the contract with a caller is
/// "display these numbers, then give the quote back", not "inspect and
/// reassemble the plan".
///
/// A quote is also the SDK's own record of what it committed to. Some of
/// what it holds is not recoverable later from the underlying client — the
/// fee, in particular, is quoted once and is not repeated in the payment's
/// progress stream — so the quote is what lets
/// [`LnSendState::Success`] report the fee that was actually charged.
#[derive(Debug)]
pub struct LnQuote {
    inner: LnQuoteInner,
}

impl LnQuote {
    /// The amount that will reach the payee, after applying any amount
    /// override supplied at quote time.
    pub fn invoice_amount(&self) -> Amount {
        unimplemented!()
    }

    /// The fee this payment will cost, on top of
    /// [`LnQuote::invoice_amount`].
    ///
    /// Zero for an internal payment, which needs no gateway.
    pub fn fee(&self) -> Amount {
        unimplemented!()
    }

    /// The total amount that will be debited from the balance:
    /// [`LnQuote::invoice_amount`] plus [`LnQuote::fee`].
    ///
    /// This is the number to show as "you will pay".
    pub fn total(&self) -> Amount {
        unimplemented!()
    }

    /// How this payment will be routed.
    pub fn route(&self) -> LightningRoute {
        unimplemented!()
    }

    /// When this quote stops being executable.
    ///
    /// Past this point [`Lightning::send`] fails with
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired).
    pub fn expires_at(&self) -> Timestamp {
        unimplemented!()
    }
}

/// How a lightning payment is, or was, routed.
///
/// Available from the quote before paying and from the final state
/// afterwards, so an application can both preview and receipt it. The
/// distinction is worth surfacing because it is the difference between a
/// payment that costs a fee and one that does not, and because "this stayed
/// inside the federation" is meaningful privacy information for a user.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LightningRoute {
    /// The payee holds their invoice in this same federation, so the
    /// payment settles internally without touching the lightning network
    /// and without a gateway.
    ///
    /// This corresponds exactly to upstream's `PayType::Internal` case.
    Internal,
    /// The payment leaves the federation through a lightning gateway.
    Gateway {
        /// The gateway that carries, or carried, the payment.
        gateway_id: GatewayId,
    },
}

/// The result of [`Lightning::receive`]: the invoice to show, and the
/// operation tracking payment of it.
#[derive(Debug)]
#[non_exhaustive]
pub struct LnReceive {
    /// The invoice to display, encode as a QR code, or send to the payer.
    pub invoice: Bolt11Invoice,
    /// Tracks the incoming payment through to the balance credit.
    pub operation: Operation<LnReceiveState>,
}

/// The lifecycle of an outgoing lightning payment.
///
/// # Relationship to the upstream state machines
///
/// Upstream does not have one outgoing-payment state machine; it has
/// three, and this enum unifies them:
///
/// - **`LnPayState`** (v1 lightning, gateway-routed):  `Created`,
///   `Canceled`, `Funded { block_height }`, `WaitingForRefund { error_reason }`,
///   `AwaitingChange`, `Success { preimage }`, `Refunded { gateway_error }`,
///   `UnexpectedError { error_message }`.
/// - **`InternalPayState`** (v1 lightning, payee in the same federation,
///   selected by `PayType::Internal`): `Funding`, `Preimage(..)`,
///   `RefundSuccess { .. }`, `RefundError { .. }`, `FundingFailed { .. }`,
///   `UnexpectedError(..)`.
/// - **`SendOperationState`** (lnv2): `Funding`, `Funded`, `Success`,
///   `Refunding`, `Refunded`, `Failure`.
///
/// Which of the first two applies is decided upstream by `PayType`, and it
/// is exactly the distinction [`LightningRoute`] exposes — a caller
/// following an internal payment and a caller following a gateway payment
/// are watching structurally different upstream machines today. Collapsing
/// them here is the point: an application should not have to write two
/// payment screens because the payee happened to be in the same
/// federation, and it should not have to be rewritten when lnv2 replaces
/// v1 underneath.
///
/// The collapse maps funding-in-progress states onto
/// [`Created`](Self::Created) and [`Funded`](Self::Funded), all the
/// preimage-obtained states onto [`Success`](Self::Success), all the
/// refund-completed states onto [`Refunded`](Self::Refunded), and the
/// error and refund-failure states onto [`Failed`](Self::Failed). Because
/// that is a judgement about which upstream distinctions matter to an
/// application rather than a one-to-one mapping, this variant set is
/// provisional and will be reconciled against the lightning client when
/// this facade is implemented.
///
/// # An obligation this enum places on the implementation
///
/// [`Success`](Self::Success) carries the fee and the route. **Neither is
/// available from the v1 upstream progress stream.** Upstream reports the
/// fee exactly once, synchronously, as the `fee` field of the
/// `OutgoingLightningPayment` returned when the payment is initiated, and
/// it does not put the gateway id into the pay-state stream at all. So the
/// SDK must capture both from the quote it executed and carry them forward
/// itself, persisting them alongside the operation so they survive a
/// restart. That is a real obligation on whoever implements this facade,
/// and it is precisely why [`LnQuote`] is an executable object rather than
/// a set of numbers to display and discard.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LnSendState {
    /// The payment has been accepted and is being funded.
    Created,
    /// The payment is funded and in flight — handed to the gateway, or
    /// committed internally.
    Funded,
    /// Final: the payee was paid and the preimage proves it.
    Success {
        /// The payment preimage, hex-encoded. This is the receipt: it
        /// proves to anyone holding the invoice that it was paid.
        preimage: String,
        /// The fee actually charged, carried forward from the executed
        /// quote.
        fee: Amount,
        /// How the payment was routed, carried forward from the executed
        /// quote.
        route: LightningRoute,
    },
    /// Final: the payment did not go through and the funds are back in the
    /// spendable balance.
    ///
    /// This is the ordinary failure of a lightning payment — no route, the
    /// payee went away, the gateway gave up — and it is a success from the
    /// SDK's point of view in that the money is safe.
    Refunded,
    /// Final: the payment failed in a way that did not resolve into a
    /// clean refund.
    Failed {
        /// Human-readable explanation. Diagnostic only — not a stable
        /// contract, and not something to match on.
        reason: String,
    },
}

impl crate::operation::sealed::Sealed for LnSendState {}

impl OperationState for LnSendState {
    fn is_final(&self) -> bool {
        match self {
            LnSendState::Created | LnSendState::Funded => false,
            LnSendState::Success { .. } | LnSendState::Refunded | LnSendState::Failed { .. } => {
                true
            }
        }
    }
}

/// The lifecycle of an incoming lightning payment.
///
/// # Relationship to the upstream state machine
///
/// Upstream v1's `LnReceiveState` is `Created`,
/// `WaitingForPayment { invoice, timeout }`, `Canceled { reason }`,
/// `Funded`, `AwaitingFunds`, `Claimed`. This enum tracks it closely: the
/// invoice and timeout that upstream attaches to `WaitingForPayment` are
/// already known to the caller from [`LnReceive::invoice`], so
/// [`WaitingForPayment`](Self::WaitingForPayment) carries no payload, and
/// upstream's `AwaitingFunds` is folded into [`Funded`](Self::Funded) —
/// both mean "paid, settling".
///
/// [`Expired`](Self::Expired) is the one addition. An invoice that simply
/// lapses unpaid is the most common way a receive ends, and it is not a
/// failure worth alarming a user about; v1 upstream has no dedicated
/// variant for it and reports it as a `Canceled` with an expiry reason,
/// while lnv2 does have an explicit expired state. Splitting it out here
/// means an application can render "this invoice expired" without parsing
/// a reason string, and it aligns the SDK with where upstream is going.
///
/// Because that split is a judgement rather than a one-to-one mapping,
/// this variant set is provisional and will be reconciled against the
/// lightning client when this facade is implemented.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LnReceiveState {
    /// The invoice is being created and registered with the gateway.
    Created,
    /// The invoice exists and nobody has paid it yet.
    WaitingForPayment,
    /// Someone paid; the funds are being settled into the federation.
    Funded,
    /// Final: the amount is in the spendable balance.
    Claimed,
    /// Final: the receive was cancelled before it was paid — for example
    /// because the gateway withdrew the offer.
    Canceled {
        /// Human-readable explanation. Diagnostic only — not a stable
        /// contract, and not something to match on.
        reason: String,
    },
    /// Final: the invoice's expiry passed without it being paid.
    Expired,
}

impl crate::operation::sealed::Sealed for LnReceiveState {}

impl OperationState for LnReceiveState {
    fn is_final(&self) -> bool {
        match self {
            LnReceiveState::Created
            | LnReceiveState::WaitingForPayment
            | LnReceiveState::Funded => false,
            LnReceiveState::Claimed | LnReceiveState::Canceled { .. } | LnReceiveState::Expired => {
                true
            }
        }
    }
}

/// Placeholder for the lightning-module state this facade operates on.
#[derive(Debug)]
struct LightningInner;

/// Placeholder for a quote's frozen plan: invoice, resolved amount,
/// verified gateway, fee, and the configuration context they were computed
/// against. Held by value rather than behind an `Arc`, because a quote is
/// owned by one caller and consumed once, never shared.
#[derive(Debug)]
struct LnQuoteInner;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ln_send_state_created_is_not_final() {
        assert!(!LnSendState::Created.is_final());
    }

    #[test]
    fn ln_send_state_funded_is_not_final() {
        assert!(!LnSendState::Funded.is_final());
    }

    #[test]
    fn ln_send_state_success_is_final() {
        assert!(
            LnSendState::Success {
                preimage: String::new(),
                fee: Amount::from_msats(0),
                route: LightningRoute::Internal,
            }
            .is_final()
        );
    }

    #[test]
    fn ln_send_state_refunded_is_final() {
        assert!(LnSendState::Refunded.is_final());
    }

    #[test]
    fn ln_send_state_failed_is_final() {
        assert!(
            LnSendState::Failed {
                reason: String::new(),
            }
            .is_final()
        );
    }

    #[test]
    fn ln_receive_state_created_is_not_final() {
        assert!(!LnReceiveState::Created.is_final());
    }

    #[test]
    fn ln_receive_state_waiting_for_payment_is_not_final() {
        assert!(!LnReceiveState::WaitingForPayment.is_final());
    }

    #[test]
    fn ln_receive_state_funded_is_not_final() {
        assert!(!LnReceiveState::Funded.is_final());
    }

    #[test]
    fn ln_receive_state_claimed_is_final() {
        assert!(LnReceiveState::Claimed.is_final());
    }

    #[test]
    fn ln_receive_state_canceled_is_final() {
        assert!(
            LnReceiveState::Canceled {
                reason: String::new(),
            }
            .is_final()
        );
    }

    #[test]
    fn ln_receive_state_expired_is_final() {
        assert!(LnReceiveState::Expired.is_final());
    }
}
