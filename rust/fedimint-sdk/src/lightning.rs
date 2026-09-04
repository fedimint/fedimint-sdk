//! Bolt11 lightning: paying invoices and getting paid.

use std::sync::Arc;

use crate::{
    Amount, Bolt11Invoice, GatewayId, Operation, OperationState, Preimage, Result, Timestamp,
};

/// The lightning facade for one federation.
///
/// Obtained from [`Federation::lightning`](crate::Federation::lightning),
/// which returns `None` when the federation has no lightning module.
///
/// Gateway selection, gateway verification and fee quoting happen inside the
/// facade, before an invoice is created or a payment is funded. A gateway
/// problem is therefore an error from the call that started the operation,
/// not a failure halfway through it.
#[derive(Debug, Clone)]
pub struct Lightning {
    inner: Arc<LightningInner>,
}

impl Lightning {
    /// Plans a payment and returns an executable quote for it.
    ///
    /// The returned [`LnQuote`] is the frozen plan for paying `invoice`: the
    /// amount the invoice names, the route, the aggregate fee and the total
    /// debit. Show those numbers to the user, then pass the quote to
    /// [`Lightning::send`], which executes exactly what was shown.
    ///
    /// The amount is always the invoice's own. An invoice that names no
    /// amount cannot be paid through fedimint and is refused here with
    /// [`AmountlessInvoice`](crate::ErrorCode::AmountlessInvoice); check
    /// [`Bolt11Invoice::amount`](crate::Bolt11Invoice::amount) first to show
    /// the user a better message than a failed quote.
    ///
    /// The invoice's network is checked here against
    /// [`Federation::network`](crate::Federation::network). The comparison is
    /// by BOLT11 currency class, which is all an invoice can express: a `tb`
    /// invoice is compatible with a federation on testnet3 or testnet4 alike.
    /// A mismatch fails with
    /// [`NetworkMismatch`](crate::ErrorCode::NetworkMismatch), whose
    /// [`ErrorDetails::NetworkMismatch`](crate::ErrorDetails::NetworkMismatch)
    /// names the federation's network, every network the invoice could have
    /// been for and the currency prefix that was seen.
    ///
    /// Quotes expire; see [`LnQuote::expires_at`].
    ///
    /// # Errors
    ///
    /// [`AmountlessInvoice`](crate::ErrorCode::AmountlessInvoice) for an
    /// invoice that names no amount,
    /// [`NetworkMismatch`](crate::ErrorCode::NetworkMismatch) for an invoice
    /// denominated for another network,
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for an invoice that
    /// has already expired,
    /// [`GatewayUnavailable`](crate::ErrorCode::GatewayUnavailable) when no
    /// gateway can be selected and verified,
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance) when
    /// the balance cannot cover [`LnQuote::total`],
    /// [`Recovering`](crate::ErrorCode::Recovering) while the federation's
    /// recovery is incomplete,
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn quote(&self, invoice: &Bolt11Invoice) -> Result<LnQuote> {
        // Implementation notes (delete once implemented):
        // - Amountless invoices are rejected by both the v1 and the lnv2 payment paths and
        //   upstream considers supporting them unsafe, so no amount parameter is offered.
        // - The network check lives here so it runs before anything is committed and on both
        //   module generations; lnv2's own `WrongCurrency` failure would only surface
        //   mid-payment on one of them.
        // - Bind the note selection into the quote: dust depends on which notes are spent, and
        //   binding it is what makes `LnQuote::total` exact rather than a ceiling.
        unimplemented!()
    }

    /// Executes a quoted payment.
    ///
    /// The quote is consumed. Execution follows it exactly, same amount, same
    /// fee, same route, or does not happen:
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired) if the quote's
    /// validity window has passed,
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) if something the
    /// quote depends on moved underneath it, such as the gateway withdrawing
    /// or changing its fee. Both mean the same thing to a caller: quote again
    /// and re-confirm with the user.
    ///
    /// The returned operation tracks the payment from funding to preimage. A
    /// payment that fails ends in a final state, not in an error from this
    /// call.
    ///
    /// The terms executed on are persisted as [`LnSendDetails`] before this
    /// call returns, so the invoice, the amounts, the fee and the route stay
    /// readable from [`Operation::details`](crate::Operation::details) after a
    /// restart and however the payment ends.
    ///
    /// # Errors
    ///
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired),
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged),
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance),
    /// [`GatewayUnavailable`](crate::ErrorCode::GatewayUnavailable),
    /// [`Recovering`](crate::ErrorCode::Recovering) while the federation's
    /// recovery is incomplete,
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn send(&self, quote: LnQuote) -> Result<Operation<LnSendState>> {
        // Implementation notes (delete once implemented):
        // - Re-check every bound input of the quote (gateway, its fee, federation config,
        //   note selection) before funding; any drift is `QuoteChanged`, never a different
        //   debit.
        // - Write `LnSendDetails` in the same storage transaction that creates the operation.
        //   The v1 progress stream reports neither the fee nor the gateway id, so the record
        //   is the only source for them on a refunded or failed payment.
        unimplemented!()
    }

    /// Issues an invoice payable into this federation.
    ///
    /// A gateway is selected and verified before the invoice exists, so an
    /// invoice this call returns is one someone can actually pay. The
    /// returned operation tracks the incoming payment through to the credit
    /// landing in the balance.
    ///
    /// `description` is embedded in the invoice and shown to the payer by
    /// their wallet.
    ///
    /// `amount` is what the payer is asked for. The invoice's face value is
    /// exactly this amount and the receive-side fee is taken out of it, so
    /// the credit that lands is slightly smaller. [`LnReceiveDetails`]
    /// records all three numbers.
    ///
    /// The invoice and its terms are persisted as [`LnReceiveDetails`] before
    /// this call returns, so the QR code can be re-displayed and the expiry
    /// counted down after a restart from nothing but the operation's id.
    ///
    /// # Errors
    ///
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a zero amount
    /// or a description the invoice format cannot carry,
    /// [`GatewayUnavailable`](crate::ErrorCode::GatewayUnavailable),
    /// [`Recovering`](crate::ErrorCode::Recovering) while the federation's
    /// recovery is incomplete,
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn receive(&self, amount: Amount, description: &str) -> Result<LnReceive> {
        // Implementation notes (delete once implemented):
        // - Write `LnReceiveDetails` in the same storage transaction that creates the
        //   operation.
        // - Record the phase the receive reaches durably (see the notes on `LnReceiveState`):
        //   the terminal v1 event alone does not say whether a payment was ever confirmed.
        unimplemented!()
    }
}

/// A frozen, executable plan for one lightning payment.
///
/// Produced by [`Lightning::quote`] and consumed by [`Lightning::send`].
/// Everything a user needs to approve is readable through the accessors
/// below. The numbers shown are the numbers charged: a quote is executed
/// exactly or not at all.
#[derive(Debug)]
pub struct LnQuote {
    inner: LnQuoteInner,
}

impl LnQuote {
    /// The invoice's amount: what will reach the payee.
    pub fn invoice_amount(&self) -> Amount {
        unimplemented!()
    }

    /// The aggregate fee this payment will cost, on top of
    /// [`LnQuote::invoice_amount`].
    ///
    /// Every debit that funding the payment incurs is in this one number:
    /// the gateway's charge, the federation's own transaction fees and any
    /// value too small for a note denomination to represent. It is not zero
    /// on an internal route: no gateway means no gateway fee, but the
    /// federation transaction still has costs.
    ///
    /// [`LnQuote::fee_breakdown`] itemises this same number. This accessor
    /// is authoritative and the breakdown sums to it exactly.
    pub fn fee(&self) -> Amount {
        unimplemented!()
    }

    /// The parts [`LnQuote::fee`] is made of, for an approval screen that
    /// itemises them.
    pub fn fee_breakdown(&self) -> LnFeeBreakdown {
        unimplemented!()
    }

    /// The whole debit this payment will make against the balance:
    /// [`LnQuote::invoice_amount`] plus [`LnQuote::fee`].
    ///
    /// This is the number to show as "you will pay", and it is exact.
    /// [`Lightning::send`] debits this much or fails with
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged), whose
    /// [`ErrorDetails::QuoteTermsChanged`](crate::ErrorDetails::QuoteTermsChanged)
    /// names this total and the one the payment would now cost. The same
    /// figure is what [`LnSendDetails::total`] records.
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

/// The parts [`LnQuote::fee`] is made of.
///
/// Obtained from [`LnQuote::fee_breakdown`], for an approval screen that
/// would rather say "1,050 msat of fees, of which 1,000 is the gateway's and
/// 50 the federation's" than show one unexplained lump.
///
/// The components sum to [`LnQuote::fee`] exactly. Take the total from that
/// accessor rather than adding these up, so the number on screen stays the
/// number the quote committed to even if a later release itemises the fee
/// more finely.
///
/// Any component may be zero. On [`LightningRoute::Internal`] the gateway
/// component always is. Zero components are reported as zero rather than
/// omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LnFeeBreakdown {
    /// The gateway's own charge for carrying the payment out to the lightning
    /// network. Zero on [`LightningRoute::Internal`].
    pub gateway: Amount,
    /// The lightning module's fee on the output that funds the payment.
    pub lightning_module: Amount,
    /// The primary module's fees for assembling the funding transaction: what
    /// it charges on the ecash inputs spent and on the change reissued.
    pub primary_module: Amount,
    /// Value lost to denominations: the part of the change too small for any
    /// note denomination to represent, which is therefore never reissued.
    ///
    /// Nobody charges it, but it leaves the balance and does not come back,
    /// so it belongs in the number a user approves.
    pub dust: Amount,
}

/// How a lightning payment is, or was, routed.
///
/// Available from the quote before paying and from the final state
/// afterwards. The distinction matters to a user: an internal payment pays
/// no gateway, and "this stayed inside the federation" is meaningful privacy
/// information.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LightningRoute {
    /// The payee holds their invoice in this same federation, so the
    /// payment settles internally without touching the lightning network
    /// and without a gateway.
    Internal,
    /// The payment leaves the federation through a lightning gateway.
    Gateway {
        /// The gateway that carries, or carried, the payment.
        gateway_id: GatewayId,
    },
}

/// The result of [`Lightning::receive`]: the invoice to show, and the
/// operation tracking payment of it.
///
/// Everything on this value is also persisted as [`LnReceiveDetails`], so an
/// application that dropped it, or that is running again after a restart,
/// re-reads the invoice from
/// [`Operation::details`](crate::Operation::details) with nothing but the
/// operation's id.
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
/// One lifecycle covers both internally settled and gateway-routed
/// payments, so an application needs one payment screen.
///
/// The final states are drawn by what happened to the money.
/// [`Success`](Self::Success) means the payee was paid;
/// [`Refunded`](Self::Refunded) means the funds are safe in the balance,
/// whether returned or never debited; [`Failed`](Self::Failed) means the
/// payment did not resolve into either. A payment has no cancellation:
/// once sent it runs to one of those endings.
// Implementation notes (delete once implemented):
//
// This unifies three upstream machines: v1 `LnPayState` (gateway-routed), v1
// `InternalPayState` (selected by `PayType::Internal`) and lnv2 `SendOperationState`.
//
// - Funding-in-progress states map to `Created`/`Funded`; every preimage-obtained state to
//   `Success`; everything that ends with the funds spendable again to `Refunded`; a refund
//   that itself failed, or an unresolved error, to `Failed`.
// - v1 `LnPayState::Canceled`: called off before the gateway took it, nothing debited,
//   so `Refunded`.
// - `InternalPayState::FundingFailed`, and lnv2 `Failure` straight after `Funding`: the
//   federation rejected the funding transaction, nothing debited, so `Refunded`. lnv2 uses
//   the same `Failure` variant for a failed refund, so key on whether `Funded` was reached
//   and persist that phase.
// - lnv2 `Failure` after `Refunding`: neither paid nor back, so `Failed`.
// - lnv2 `Refunding` is in progress, not final: map to `Funded`, then `Refunded` when it
//   lands.
// - Normalise the preimage: v1 reports hex, lnv2 raw bytes.
// - The v1 progress stream carries neither the fee nor the gateway id. Both come from the
//   executed quote and are persisted in `LnSendDetails`; `Success` is filled from there.
//
// The variant set is provisional until reconciled against the lightning client.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LnSendState {
    /// The payment has been accepted and is being funded.
    Created,
    /// The payment is funded and in flight: handed to the gateway, or
    /// committed internally.
    Funded,
    /// Final: the payee was paid and the [`Preimage`] proves it.
    Success {
        /// The payment preimage. It proves to anyone holding the invoice
        /// that it was paid.
        preimage: Preimage,
        /// The aggregate fee charged, as quoted by [`LnQuote::fee`].
        ///
        /// Also recorded as [`LnSendDetails::fee`], which stays readable for
        /// a payment that was refunded or failed.
        fee: Amount,
        /// How the payment was routed.
        ///
        /// Also recorded as [`LnSendDetails::route`].
        route: LightningRoute,
    },
    /// Final: the payment did not go through and the funds are in the
    /// spendable balance, returned or never debited.
    ///
    /// This is the ordinary failure of a lightning payment: no route, the
    /// payee went away, the gateway gave up, or the funding was rejected
    /// before anything left. The money is safe.
    Refunded,
    /// Final: the payment failed in a way that did not resolve into a clean
    /// refund.
    Failed {
        /// Human-readable explanation. Diagnostic only, not a stable
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

/// The terms an outgoing lightning payment was executed on.
///
/// Read with [`Operation::details`](crate::Operation::details). Persisted
/// when [`Lightning::send`] creates the operation and never changed, so a
/// payment picked up after a restart, or one that was refunded or failed,
/// still has an invoice, amounts, a fee and a route to show.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LnSendDetails {
    /// The invoice this payment pays.
    ///
    /// The payee, the payment hash, the description and the expiry all read
    /// back off it.
    pub invoice: Bolt11Invoice,
    /// The invoice's own amount: what the payee receives when the payment
    /// succeeds.
    pub invoice_amount: Amount,
    /// The aggregate fee the executed quote committed to, [`LnQuote::fee`].
    pub fee: Amount,
    /// The total the payment was authorised for, [`LnQuote::total`]: equal to
    /// [`invoice_amount`](LnSendDetails::invoice_amount) plus
    /// [`fee`](LnSendDetails::fee).
    ///
    /// This is a term, not an outcome. On [`LnSendState::Success`] it is what
    /// was debited; on [`LnSendState::Refunded`] it is what was at stake.
    pub total: Amount,
    /// How the payment is routed, [`LnQuote::route`].
    pub route: LightningRoute,
    /// When the payment was started.
    ///
    /// The timestamp to sort and label a history row by. It is the moment
    /// the payment was committed, not the moment it settled.
    pub created_at: Timestamp,
}

impl crate::operation::sealed::Sealed for LnSendDetails {}

impl crate::operation::OperationDetails for LnSendDetails {}

impl crate::operation::DetailedOperationState for LnSendState {
    type Details = LnSendDetails;
}

/// The lifecycle of an incoming lightning payment.
///
/// The invoice to show and the expiry to count down to are in
/// [`LnReceiveDetails`], not in any state, so a receive screen can be rebuilt
/// from the operation's id alone.
///
/// Three endings other than [`Claimed`](Self::Claimed) are told apart:
/// [`Expired`](Self::Expired), the invoice lapsed unpaid;
/// [`Canceled`](Self::Canceled), the receive was called off before anything
/// was funded; and [`Failed`](Self::Failed), a payment got past "nobody paid"
/// and still produced no credit. Only the last warrants alarming a user.
// Implementation notes (delete once implemented):
//
// v1 `LnReceiveState` is `Created`, `WaitingForPayment { invoice, timeout }`,
// `Canceled { reason }`, `Funded`, `AwaitingFunds`, `Claimed`. `AwaitingFunds` folds into
// `Funded`. The cancellation reason is a typed `LightningReceiveError`; nothing is parsed.
//
// | upstream v1                   | phase reached      | here                          |
// | ----------------------------- | ------------------ | ----------------------------- |
// | `Canceled { Timeout }`        | any                | `Expired`                     |
// | `Canceled { ClaimRejected }`  | any                | `Funded`, then reclaim        |
// | `Canceled { InvalidPreimage }`| any                | `Failed`                      |
// | `Canceled { Rejected }`       | before `Funded`    | `Canceled`                    |
// | `Canceled { Rejected }`       | at or after `Funded` | `Failed`                    |
//
// - `ClaimRejected` and `InvalidPreimage` presuppose a funded contract and arrive before
//   upstream's own `Funded` (which is only emitted once the claim is accepted), so the phase
//   must not be consulted for them. `InvalidPreimage` unwinds the payment: `Failed`.
//   `ClaimRejected` is not final: move to `Funded` and drive the client's reclaim
//   (`reclaim_ln_receive`) under the same operation id until `Claimed`, or `Failed` once no
//   further claim is possible.
// - `Rejected` is emitted both for the invoice-registration transaction being refused and
//   for the claim's primary outputs failing after a confirmed payment. Persist whether the
//   receive ever reached `Funded`; after a restart that is the only way to tell them apart.
// - lnv2 `ReceiveOperationState` has explicit pending/claiming/claimed/expired states that
//   map directly. Its `Failure` after an accepted claim is `Failed`; a rejected but still
//   claimable claim stays `Funded` and reclaims, as for v1.
//
// The variant set is provisional until reconciled against the lightning client.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LnReceiveState {
    /// The invoice is being created and registered with the gateway.
    Created,
    /// The invoice exists and nobody has paid it yet.
    ///
    /// The invoice and its expiry are in [`LnReceiveDetails`].
    WaitingForPayment,
    /// Someone paid; the funds are being settled into the federation.
    ///
    /// A receive stays here while the SDK retries a claim the federation
    /// rejected. It only becomes [`Failed`](Self::Failed) once no further
    /// claim is possible.
    Funded,
    /// Final: the amount is in the spendable balance.
    ///
    /// The amount that landed is [`LnReceiveDetails::net_credit`], the
    /// invoice's face value less the receive-side fee.
    Claimed,
    /// Final: the receive was called off before anything was funded, for
    /// example because the gateway withdrew the offer.
    Canceled {
        /// Human-readable explanation. Diagnostic only, not a stable
        /// contract, and not something to match on.
        reason: String,
    },
    /// Final: the invoice's expiry passed without it being paid.
    Expired,
    /// Final: a payment got past "nobody paid" and no ecash was issued for
    /// it.
    ///
    /// The amount is not in the balance and will not arrive by waiting.
    /// Either a confirmed payment did not become spendable notes and the
    /// payer is out of pocket, or the protocol went wrong on a funded
    /// contract and the payment was unwound before anyone was paid. The
    /// application cannot tell the two apart from this state. Render it as
    /// an error the user should report, not as an expired invoice.
    Failed,
}

impl crate::operation::sealed::Sealed for LnReceiveState {}

impl OperationState for LnReceiveState {
    fn is_final(&self) -> bool {
        match self {
            LnReceiveState::Created
            | LnReceiveState::WaitingForPayment
            | LnReceiveState::Funded => false,
            LnReceiveState::Claimed
            | LnReceiveState::Canceled { .. }
            | LnReceiveState::Expired
            | LnReceiveState::Failed => true,
        }
    }
}

/// The invoice an incoming lightning payment was issued for, and its terms.
///
/// Read with [`Operation::details`](crate::Operation::details). Persisted
/// when [`Lightning::receive`] creates the operation and never changed, so a
/// receive screen can re-display the same QR code and resume the same
/// countdown after a restart from the operation's id alone.
///
/// # Which amount is which
///
/// The fee is deducted from the invoice, not added on top of it. The
/// invoice's face value is exactly what [`Lightning::receive`] was asked for,
/// and the receive-side fee comes out of it:
///
/// ```text
/// invoice_amount == requested_amount == net_credit + fee
/// ```
///
/// All three amounts are recorded so a caller can render "you asked for X,
/// the payer pays Y, you receive Z" without doing the arithmetic.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LnReceiveDetails {
    /// The invoice that was issued, the same value [`LnReceive::invoice`]
    /// returned.
    pub invoice: Bolt11Invoice,
    /// The description embedded in the invoice, as it was passed to
    /// [`Lightning::receive`].
    ///
    /// Kept separately because an invoice may carry only a hash of its
    /// description, in which case
    /// [`Bolt11Invoice::description`](crate::Bolt11Invoice::description) has
    /// nothing to return.
    pub description: String,
    /// The amount asked of [`Lightning::receive`].
    pub requested_amount: Amount,
    /// The invoice's face value: what the payer is asked to pay.
    ///
    /// Equal to [`requested_amount`](LnReceiveDetails::requested_amount), and
    /// to [`net_credit`](LnReceiveDetails::net_credit) plus
    /// [`fee`](LnReceiveDetails::fee).
    pub invoice_amount: Amount,
    /// The receive-side fee: the gateway's charge for taking the payment in,
    /// plus what the federation charges to issue the ecash for it.
    ///
    /// This is the whole difference between what the payer pays and what
    /// lands; no other deduction appears later. It can be zero but usually is
    /// not, since issuing the notes is itself a federation transaction.
    pub fee: Amount,
    /// What lands in the spendable balance:
    /// [`invoice_amount`](LnReceiveDetails::invoice_amount) minus
    /// [`fee`](LnReceiveDetails::fee).
    ///
    /// The number to show as "you will receive".
    pub net_credit: Amount,
    /// The gateway that agreed to take the payment in, if there was one.
    ///
    /// `None` means no gateway took part, not that the gateway is unknown.
    pub gateway_id: Option<GatewayId>,
    /// When the invoice stops being payable.
    ///
    /// The countdown to render beside the QR code, and the moment after which
    /// [`LnReceiveState::Expired`] is the ending to expect.
    pub expires_at: Timestamp,
    /// When the receive was started.
    ///
    /// The timestamp to sort and label a history row by.
    pub created_at: Timestamp,
}

impl crate::operation::sealed::Sealed for LnReceiveDetails {}

impl crate::operation::OperationDetails for LnReceiveDetails {}

impl crate::operation::DetailedOperationState for LnReceiveState {
    type Details = LnReceiveDetails;
}

/// Placeholder for the lightning-module state this facade operates on.
#[derive(Debug)]
struct LightningInner;

/// Placeholder for a quote's frozen plan: invoice, the amount it names,
/// verified gateway, the aggregate fee and its components, the bound note
/// selection, and the configuration context they were computed against.
#[derive(Debug)]
struct LnQuoteInner;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::DetailedOperationState;

    /// A real compressed secp256k1 public key, from that crate's own test
    /// suite. A gateway id is a public key, so the 30-character hex string this
    /// fixture used before could never have been one.
    const GATEWAY_ID: &str = "0218845781f631c48f1c9709e23092067d06837f30aa0cd0544ac887fe91ddd166";

    /// A real regtest invoice for 100_000 msat, so the fixture's numbers and
    /// its invoice agree. Built once from fixed inputs; the parse now checks a
    /// bech32 checksum, so a placeholder string no longer works here.
    const SEND_INVOICE: &str = "lnbcrt1u1pj48ugqdq2vdhkven9v5pp5g3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zqsp5242424242424242424242424242424242424242424242424242s9qrsgqcqzys2reg4wsryjt5w8z33ugydecgfmgyvtttwa7e0yzlm803z203j9hqspa4lr6m09cd808xkw9uh4sxc8wf3w6k0gaf5zrqm7zhcxug0vqqpdkpja";
    /// A real regtest invoice for 50_000 msat with the description `"coffee"`.
    const RECEIVE_INVOICE: &str = "lnbcrt500n1pj48ugqdq2vdhkven9v5pp5venxvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxvenqsp5wamhwamhwamhwamhwamhwamhwamhwamhwamhwamhwamhwamhwams9qrsgqcqzys5kgh9l4h6e67k4ettmrxnlqvw233ym34zqj6fyhypm4ajk2uu635nnv56unxj48ptuy9p00ezgwjc47nvd7m5w38p5qey34eyulqatqpjh6ntz";

    /// A send record with the numbers of one plausible payment: 100,000 msat
    /// to the payee, 1,050 msat of aggregate fee, 101,050 msat debited.
    fn send_details() -> LnSendDetails {
        LnSendDetails {
            invoice: SEND_INVOICE.parse().expect("a valid regtest invoice"),
            invoice_amount: Amount::from_msats(100_000),
            fee: Amount::from_msats(1_050),
            total: Amount::from_msats(101_050),
            route: LightningRoute::Gateway {
                gateway_id: GATEWAY_ID.parse().expect("a valid gateway id"),
            },
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    /// A receive record for an invoice of 50,000 msat with a 500 msat
    /// receive-side fee: the payer is asked for exactly what was requested
    /// and the fee comes out of it.
    fn receive_details() -> LnReceiveDetails {
        LnReceiveDetails {
            invoice: RECEIVE_INVOICE.parse().expect("a valid regtest invoice"),
            description: "coffee".to_owned(),
            requested_amount: Amount::from_msats(50_000),
            invoice_amount: Amount::from_msats(50_000),
            fee: Amount::from_msats(500),
            net_credit: Amount::from_msats(49_500),
            gateway_id: Some(GATEWAY_ID.parse().expect("a valid gateway id")),
            expires_at: Timestamp::from_epoch_millis(1_700_000_600_000),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    /// Generic over the pattern, so this compiles only if each state type
    /// names its record and the record satisfies every bound
    /// `OperationDetails` imposes.
    fn round_trip<S: DetailedOperationState>(details: S::Details) -> S::Details {
        details
    }

    #[test]
    fn ln_send_state_names_its_details_record() {
        let details = send_details();
        assert_eq!(round_trip::<LnSendState>(details.clone()), details);
    }

    #[test]
    fn ln_receive_state_names_its_details_record() {
        let details = receive_details();
        assert_eq!(round_trip::<LnReceiveState>(details.clone()), details);
    }

    #[test]
    fn ln_send_details_total_is_the_amount_plus_the_aggregate_fee() {
        let details = send_details();
        assert_eq!(
            details.invoice_amount.checked_add(details.fee),
            Some(details.total),
        );
    }

    #[test]
    fn ln_send_details_keep_the_fee_and_route_of_a_payment_that_was_refunded() {
        // A refunded send carries no fee and no route on its state; the record
        // is what keeps both readable.
        let details = send_details();
        let state = LnSendState::Refunded;
        assert!(state.is_final());
        assert_eq!(details.fee, Amount::from_msats(1_050));
        assert_eq!(
            details.route,
            LightningRoute::Gateway {
                gateway_id: GATEWAY_ID.parse().expect("a valid gateway id"),
            },
        );
    }

    #[test]
    fn ln_send_details_and_success_agree_on_the_fee_and_route() {
        // Two copies of the same value from the same quote, never two
        // different numbers.
        let details = send_details();
        let state = LnSendState::Success {
            preimage: "0000000000000000000000000000000000000000000000000000000000000000"
                .parse()
                .expect("a well-formed preimage"),
            fee: details.fee,
            route: details.route.clone(),
        };
        match state {
            LnSendState::Success { fee, route, .. } => {
                assert_eq!(fee, details.fee);
                assert_eq!(route, details.route);
            }
            other => panic!("expected Success, got {other:?}"),
        }
    }

    #[test]
    fn ln_send_details_can_record_an_internal_route() {
        // An internal payment has no gateway, and still has a fee.
        let details = LnSendDetails {
            route: LightningRoute::Internal,
            ..send_details()
        };
        assert_eq!(details.route, LightningRoute::Internal);
        assert_ne!(details.fee, Amount::from_msats(0));
    }

    #[test]
    fn ln_receive_details_invoice_amount_is_the_net_credit_plus_the_fee() {
        let details = receive_details();
        assert_eq!(
            details.net_credit.checked_add(details.fee),
            Some(details.invoice_amount),
        );
    }

    #[test]
    fn ln_receive_details_follow_the_deducted_fee_convention() {
        // The payer is asked for exactly what the application requested, and
        // the fee comes out of it.
        let details = receive_details();
        assert_eq!(details.invoice_amount, details.requested_amount);
        assert!(details.net_credit < details.invoice_amount);
    }

    #[test]
    fn ln_receive_details_can_record_that_no_gateway_took_part() {
        let details = LnReceiveDetails {
            gateway_id: None,
            ..receive_details()
        };
        assert_eq!(details.gateway_id, None);
    }

    #[test]
    fn ln_receive_details_keep_the_invoice_and_expiry_a_waiting_state_omits() {
        // The QR code and the countdown come from the record, not from the
        // state, which carries neither.
        let details = receive_details();
        assert!(!LnReceiveState::WaitingForPayment.is_final());
        assert_eq!(
            details.invoice,
            RECEIVE_INVOICE
                .parse::<Bolt11Invoice>()
                .expect("a valid regtest invoice"),
        );
        assert!(details.expires_at > details.created_at);
    }

    #[test]
    fn ln_fee_breakdown_components_sum_to_the_aggregate() {
        let breakdown = LnFeeBreakdown {
            gateway: Amount::from_msats(1_000),
            lightning_module: Amount::from_msats(25),
            primary_module: Amount::from_msats(20),
            dust: Amount::from_msats(5),
        };
        let summed = [
            breakdown.gateway,
            breakdown.lightning_module,
            breakdown.primary_module,
            breakdown.dust,
        ]
        .into_iter()
        .try_fold(Amount::from_msats(0), Amount::checked_add);
        assert_eq!(summed, Some(send_details().fee));
    }

    #[test]
    fn ln_fee_breakdown_charges_no_gateway_on_an_internal_route() {
        // No gateway means no gateway fee, not no fee.
        let breakdown = LnFeeBreakdown {
            gateway: Amount::from_msats(0),
            lightning_module: Amount::from_msats(25),
            primary_module: Amount::from_msats(20),
            dust: Amount::from_msats(5),
        };
        assert_eq!(breakdown.gateway, Amount::from_msats(0));
        assert_ne!(breakdown.primary_module, Amount::from_msats(0));
    }

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
                preimage: "0000000000000000000000000000000000000000000000000000000000000000"
                    .parse()
                    .expect("a well-formed preimage"),
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

    #[test]
    fn ln_receive_state_failed_is_final() {
        assert!(LnReceiveState::Failed.is_final());
    }
}
