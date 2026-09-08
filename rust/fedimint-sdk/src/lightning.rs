//! Bolt11 lightning: paying invoices and getting paid.

use std::sync::Arc;

use fedimint_client::Client;
use fedimint_client_module::ClientModuleInstance;
use fedimint_client_module::transaction::FeeQuote;
use fedimint_core::config;
use fedimint_core::util::SafeUrl;
use fedimint_lnv2_common::gateway_api::PaymentFee;

use crate::{
    Amount, Bolt11Invoice, Error, ErrorCode, ErrorDetails, GatewayId, Network, Operation,
    OperationState, Preimage, Result, Timestamp,
};

mod driver;
mod v1;
mod v2;
mod wire;

pub(crate) use driver::{LnBackfiller, LnReceiveDriver, LnSendDriver};

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
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Internal`](crate::ErrorCode::Internal) for a failure this crate
    /// does not expect, which indicates a bug, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn quote(&self, invoice: &Bolt11Invoice) -> Result<LnQuote> {
        let federation = &self.inner.federation;
        // The three generation-independent refusals run before the client is touched, so an
        // amountless or foreign-network invoice fails the same way on both generations and on
        // a recovering federation alike.
        let invoice_amount = preflight(invoice, federation.record().network.into())?;
        let client = federation.client(true).await?;
        let available = balance_of(&client).await?;
        // The plan's fee dry-run needs the notes on hand to cover the whole contract (the
        // amount plus fees) and fails inside the primary module rather than reporting a
        // shortfall when they do not, so a balance that cannot even cover the invoice's own
        // amount is refused here first, against that amount: no fee has been quoted yet, which
        // is what `ErrorDetails::InsufficientBalance::required` documents.
        if available < invoice_amount {
            return Err(insufficient(invoice_amount, available));
        }
        let plan = match module(&client)? {
            LnModule::V1(module) => v1::plan(&client, &module, invoice, invoice_amount).await?,
            LnModule::V2(module) => v2::plan(&client, &module, invoice, invoice_amount).await?,
        };
        if available < plan.total {
            return Err(insufficient(plan.total, available));
        }
        let issued = crate::db::now_millis();
        let expires_at = Timestamp::from_epoch_millis(
            issued
                .saturating_add(QUOTE_VALIDITY_MILLIS)
                .min(invoice.expires_at().epoch_millis()),
        );
        Ok(LnQuote {
            inner: LnQuoteInner {
                federation_id: federation.id,
                invoice: invoice.clone(),
                invoice_amount,
                plan,
                expires_at,
            },
        })
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
    /// [`NetworkMismatch`](crate::ErrorCode::NetworkMismatch) on a
    /// federation running testnet4, whose lightning module cannot pay a
    /// `tb` invoice at all,
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance),
    /// [`GatewayUnavailable`](crate::ErrorCode::GatewayUnavailable),
    /// [`Recovering`](crate::ErrorCode::Recovering) while the federation's
    /// recovery is incomplete,
    /// [`NotSupported`](crate::ErrorCode::NotSupported),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Storage`](crate::ErrorCode::Storage),
    /// [`Internal`](crate::ErrorCode::Internal) for a failure this crate
    /// does not expect, which indicates a bug, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn send(&self, quote: LnQuote) -> Result<Operation<LnSendState>> {
        let federation = &self.inner.federation;
        let quote = quote.inner;
        ensure_executable(&quote, federation.id, crate::db::now_millis())?;
        // The guard is held across the re-check, the funding and the record write, which is what
        // `create_operation` requires of its caller.
        let client = federation.client(true).await?;
        match (module(&client)?, &quote.plan.terms) {
            (LnModule::V1(module), Terms::V1 { gateway }) => {
                v1::send(federation, &client, &module, &quote, gateway.clone()).await
            }
            (
                LnModule::V2(module),
                Terms::V2 {
                    gateway,
                    send_fee,
                    expiration_delta,
                },
            ) => {
                v2::send(
                    federation,
                    &client,
                    &module,
                    &quote,
                    gateway.clone(),
                    *send_fee,
                    *expiration_delta,
                )
                .await
            }
            // The federation changed generation between the quote and now, which the
            // generation rule makes a different federation for every practical purpose.
            _ => Err(Error::new(
                ErrorCode::QuoteChanged,
                "this federation's lightning module changed since the quote was issued",
            )),
        }
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
    /// [`Storage`](crate::ErrorCode::Storage),
    /// [`Internal`](crate::ErrorCode::Internal) for a failure this crate
    /// does not expect, which indicates a bug, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn receive(&self, amount: Amount, description: &str) -> Result<LnReceive> {
        if amount == Amount::from_msats(0) {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "an invoice for nothing cannot be issued",
            ));
        }
        check_description(description)?;
        let federation = &self.inner.federation;
        let client = federation.client(true).await?;
        match module(&client)? {
            LnModule::V1(module) => {
                v1::receive(federation, &client, &module, amount, description).await
            }
            LnModule::V2(module) => {
                v2::receive(federation, &client, &module, amount, description).await
            }
        }
    }

    /// Builds the facade for one federation. Handed out by `Federation::lightning`.
    pub(crate) fn new(federation: Arc<crate::federation::FederationInner>) -> Lightning {
        Lightning {
            inner: Arc::new(LightningInner { federation }),
        }
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
        self.inner.invoice_amount
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
        self.inner.plan.fee
    }

    /// The parts [`LnQuote::fee`] is made of, for an approval screen that
    /// itemises them.
    pub fn fee_breakdown(&self) -> LnFeeBreakdown {
        self.inner.plan.breakdown.clone()
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
        self.inner.plan.total
    }

    /// How this payment will be routed.
    pub fn route(&self) -> LightningRoute {
        self.inner.plan.route.clone()
    }

    /// When this quote stops being executable.
    ///
    /// Past this point [`Lightning::send`] fails with
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired).
    pub fn expires_at(&self) -> Timestamp {
        self.inner.expires_at
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
    /// The one exception is a payment quoted through a gateway that the
    /// module settled inside the federation after all, where the gateway's
    /// charge is known not to have applied and the rest of the fee is the
    /// quote's estimate, so this is an upper bound.
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
    /// not, since issuing the notes is itself a federation transaction. A
    /// record rebuilt from the client's own log after a crash reports zero,
    /// because the log does not keep the fee.
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

/// The federation this facade operates on.
///
/// Held rather than a lightning-module handle, because a facade outlives the client behind it: a
/// call on a closed federation has to report `FederationClosed` rather than find nothing to talk
/// to.
#[derive(Debug)]
struct LightningInner {
    federation: Arc<crate::federation::FederationInner>,
}

/// A quote's frozen plan: the invoice, the amount it names, the fee and its parts, the route, and
/// the upstream terms the fee was computed from, so that `send` can tell whether they moved.
#[derive(Debug)]
pub(super) struct LnQuoteInner {
    /// The federation the quote was made against. A quote is refused on any other.
    pub(super) federation_id: config::FederationId,
    pub(super) invoice: Bolt11Invoice,
    pub(super) invoice_amount: Amount,
    pub(super) plan: Plan,
    pub(super) expires_at: Timestamp,
}

/// What a payment will cost and how it will go, for either module generation.
#[derive(Debug)]
pub(super) struct Plan {
    pub(super) breakdown: LnFeeBreakdown,
    /// The sum of `breakdown`.
    pub(super) fee: Amount,
    /// The invoice amount plus `fee`.
    pub(super) total: Amount,
    pub(super) route: LightningRoute,
    pub(super) terms: Terms,
}

/// The upstream inputs a plan was computed from. `send` recomputes the plan from the same
/// inputs read again and refuses on any difference in the total.
#[derive(Debug)]
pub(super) enum Terms {
    /// v1: the gateway the payment goes out through, or `None` for an internal payment.
    ///
    /// Boxed: `LightningGateway` is large enough on its own to make this the dominant variant,
    /// which `clippy::large_enum_variant` flags across every `Terms` value, most of which carry
    /// no gateway at all.
    V1 {
        gateway: Option<Box<fedimint_ln_common::LightningGateway>>,
    },
    /// lnv2: the gateway's API and the fee schedule it quoted for this invoice.
    V2 {
        gateway: SafeUrl,
        send_fee: PaymentFee,
        expiration_delta: u64,
    },
}

/// How long a quote stays executable after it is issued, unless the invoice expires first.
const QUOTE_VALIDITY_MILLIS: u64 = 60_000;

/// The expiry every invoice this facade issues carries. lnv2 refuses anything over one day
/// (`MAX_INVOICE_EXPIRY_SECS` in fedimint-lnv2-common's gateway_api.rs).
pub(super) const INVOICE_EXPIRY_SECS: u32 = 3_600;

/// The longest description a BOLT11 invoice can carry, in bytes: 1023 five-bit groups
/// (lightning-invoice-0.33.3/src/lib.rs:1687-1697, `Description::new`).
const MAX_DESCRIPTION_BYTES: usize = 639;

/// The lightning module the live client has, whichever generation it is.
enum LnModule<'a> {
    V1(ClientModuleInstance<'a, fedimint_ln_client::LightningClientModule>),
    V2(ClientModuleInstance<'a, fedimint_lnv2_client::LightningClientModule>),
}

/// Picks the generation by asking the client, not the stored record: a facade obtained while a
/// module was present and used after the configuration dropped it is the `NotSupported` case.
fn module(client: &Client) -> Result<LnModule<'_>> {
    if let Ok(module) = client.get_first_module::<fedimint_lnv2_client::LightningClientModule>() {
        return Ok(LnModule::V2(module));
    }
    if let Ok(module) = client.get_first_module::<fedimint_ln_client::LightningClientModule>() {
        return Ok(LnModule::V1(module));
    }
    Err(Error::new(
        ErrorCode::NotSupported,
        "this federation no longer has a lightning module",
    ))
}

/// The checks every quote runs before anything touches the network, in the documented order:
/// amountless first, then the network, then expiry.
fn preflight(invoice: &Bolt11Invoice, network: Network) -> Result<Amount> {
    let Some(amount) = invoice.amount() else {
        return Err(Error::new(
            ErrorCode::AmountlessInvoice,
            "this invoice names no amount and cannot be paid through fedimint",
        ));
    };
    check_network(invoice, network)?;
    if invoice.is_expired() {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "this invoice has already expired",
        ));
    }
    Ok(amount)
}

/// Every network a BOLT11 currency class could stand for: `tb` is both public testnets, and a
/// class this crate cannot name (simnet) is the empty set, which still proves a mismatch.
pub(super) fn compatible_networks(from_invoice: Option<Network>) -> Vec<Network> {
    match from_invoice {
        Some(Network::Testnet) => vec![Network::Testnet, Network::Testnet4],
        Some(network) => vec![network],
        None => Vec::new(),
    }
}

fn check_network(invoice: &Bolt11Invoice, expected: Network) -> Result<()> {
    let compatible = compatible_networks(invoice.network());
    if compatible.contains(&expected) {
        return Ok(());
    }
    let observed_prefix = invoice.observed_prefix();
    Err(Error::with_details(
        ErrorCode::NetworkMismatch,
        format!(
            "the invoice is for {observed_prefix} but the federation runs on {}",
            expected.as_str()
        ),
        ErrorDetails::NetworkMismatch {
            expected,
            compatible,
            observed_prefix,
        },
    ))
}

fn check_description(description: &str) -> Result<()> {
    if description.len() > MAX_DESCRIPTION_BYTES {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            format!(
                "the description exceeds the {MAX_DESCRIPTION_BYTES} bytes an invoice can carry"
            ),
        ));
    }
    Ok(())
}

/// Refuses a quote made for another federation or past its window.
fn ensure_executable(
    quote: &LnQuoteInner,
    federation_id: config::FederationId,
    now_millis: u64,
) -> Result<()> {
    if quote.federation_id != federation_id {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "this quote was issued by another federation",
        ));
    }
    if now_millis > quote.expires_at.epoch_millis() {
        return Err(quote_expired(quote.expires_at, false));
    }
    Ok(())
}

/// Assembles a plan from the gateway's charge, the lightning module's own fee on the explicit
/// output (or input), and the shared fee quote, whose `output` (or `input`) total already
/// includes that explicit fee (`fedimint-client/src/client.rs:865-935`): the primary module's
/// share is what is left of the quote's input and output fees once the lightning module's
/// explicit fee is taken back out.
pub(super) fn plan_of(
    gateway: Amount,
    lightning_module: Amount,
    quote: &FeeQuote,
    invoice_amount: Amount,
    route: LightningRoute,
    terms: Terms,
) -> Result<Plan> {
    let input = from_upstream(quote.input.get_bitcoin());
    let output = from_upstream(quote.output.get_bitcoin());
    let dust = from_upstream(quote.dust.get_bitcoin());
    let primary_module = add(input, output)?
        .checked_sub(lightning_module)
        .ok_or_else(|| {
            Error::new(
                ErrorCode::Internal,
                "the fee quote is smaller than the lightning module's own fee",
            )
        })?;
    let breakdown = LnFeeBreakdown {
        gateway,
        lightning_module,
        primary_module,
        dust,
    };
    let fee = add(add(add(gateway, lightning_module)?, primary_module)?, dust)?;
    let total = add(invoice_amount, fee)?;
    Ok(Plan {
        breakdown,
        fee,
        total,
        route,
        terms,
    })
}

/// What a fee-quote dry run's failure means, before either mint's answer is turned into an
/// [`Error`].
///
/// The dry run balances the funding transaction against the real notes and fails inside the
/// primary module when they cannot cover it. The v1 mint (`fedimint-mint-client`) reports that
/// with the typed [`fedimint_mint_client::InsufficientBalanceError`], which already carries the
/// amounts that were short; the v2 mint (`fedimint-mintv2-client`) reports the same condition as
/// a plain-text `anyhow` context, `"Insufficient funds"`
/// (`fedimint-mintv2-client/src/lib.rs:503`), with no amounts of its own.
#[derive(Debug)]
enum FeeQuoteFailure {
    /// The v1 mint's typed error, carrying its own requested and total amounts.
    Typed { requested: Amount, total: Amount },
    /// The v2 mint's plain-text refusal, which names no amounts.
    Text,
}

/// Recognizes either mint's insufficient-balance refusal from a fee-quote failure, or reports
/// neither is a match. Pure so the mapping can be checked without a live `Client`.
fn classify_fee_quote_failure(
    short: Option<&fedimint_mint_client::InsufficientBalanceError>,
    text: &str,
) -> Option<FeeQuoteFailure> {
    if let Some(short) = short {
        return Some(FeeQuoteFailure::Typed {
            requested: from_upstream(short.requested_amount),
            total: from_upstream(short.total_amount),
        });
    }
    if text.contains("Insufficient funds") {
        return Some(FeeQuoteFailure::Text);
    }
    None
}

/// Turns a fee-quote dry run's failure into the [`Error`] it represents, for the four call sites
/// (v1's and v2's `terms_for` and `receive`) that run one.
///
/// `required` is the amount the failed quote was for, used to report the shortfall when the
/// mint's answer carries no amounts of its own. `context` names the quote for the fallback
/// message, when `short` is absent and `text` does not match either mint's wording for "the
/// notes on hand are short".
pub(super) async fn fee_quote_failure(
    client: &Client,
    short: Option<&fedimint_mint_client::InsufficientBalanceError>,
    text: &str,
    required: Amount,
    context: &str,
) -> Error {
    match classify_fee_quote_failure(short, text) {
        Some(FeeQuoteFailure::Typed { requested, total }) => insufficient(requested, total),
        Some(FeeQuoteFailure::Text) => {
            // The v2 mint's text names no amounts, so the balance is read again here. A
            // failed read must not mask the real refusal that was already found, so it
            // falls back to zero rather than turning this into an unrelated error.
            let available = balance_of(client).await.unwrap_or(Amount::from_msats(0));
            insufficient(required, available)
        }
        None => internal(format!("{context}: {text}")),
    }
}

pub(super) fn to_upstream(amount: Amount) -> fedimint_core::Amount {
    fedimint_core::Amount::from_msats(amount.msats())
}

pub(super) fn from_upstream(amount: fedimint_core::Amount) -> Amount {
    Amount::from_msats(amount.msats)
}

pub(super) fn add(left: Amount, right: Amount) -> Result<Amount> {
    left.checked_add(right)
        .ok_or_else(|| Error::new(ErrorCode::Internal, "an amount overflowed"))
}

pub(super) fn quote_changed(quoted_total: Amount, current_total: Amount) -> Error {
    Error::with_details(
        ErrorCode::QuoteChanged,
        format!(
            "the payment would now debit {} msat instead of the quoted {} msat",
            current_total.msats(),
            quoted_total.msats()
        ),
        ErrorDetails::QuoteTermsChanged {
            quoted_total,
            current_total,
        },
    )
}

pub(super) fn quote_expired(expires_at: Timestamp, already_executed: bool) -> Error {
    let message = if already_executed {
        "this invoice has already been paid or is being paid"
    } else {
        "this quote is no longer executable; quote again"
    };
    Error::with_details(
        ErrorCode::QuoteExpired,
        message,
        ErrorDetails::QuoteExpired {
            expires_at,
            already_executed,
        },
    )
}

pub(super) fn insufficient(required: Amount, available: Amount) -> Error {
    Error::with_details(
        ErrorCode::InsufficientBalance,
        format!(
            "the payment needs {} msat but only {} msat is spendable",
            required.msats(),
            available.msats()
        ),
        ErrorDetails::InsufficientBalance {
            required,
            available,
        },
    )
}

// Neither lightning generation can name testnet4 as such, so a testnet4 federation's module
// refuses a `tb` invoice as an unrelated failure rather than the network mismatch it is: lnv2
// compares the configured network to the invoice's BOLT11 currency class strictly
// (`self.cfg.network != invoice.currency().into()`,
// `fedimint-lnv2-client/src/lib.rs:560-565`), and `Currency::BitcoinTestnet` converts only to
// `bitcoin::Network::Testnet`; v1 converts the configured network through lightning-invoice's
// `From<bitcoin::Network> for Currency` (`lightning-invoice-0.33.3/src/lib.rs:451-463`), which
// has no `Testnet4` arm and falls to a `_` arm yielding `Currency::Regtest`, so its own check
// (`fedimint-ln-client/src/lib.rs:834-839`) never matches a `tb` invoice either. Tracked
// upstream as fedimint/fedimint#9100; both generations report this refusal with the same
// detail `check_network` would have produced, rather than the SDK working around it.
pub(super) fn network_refusal(quote: &LnQuoteInner, expected: Network) -> Error {
    Error::with_details(
        ErrorCode::NetworkMismatch,
        "the lightning module refused the invoice's network",
        ErrorDetails::NetworkMismatch {
            expected,
            compatible: compatible_networks(quote.invoice.network()),
            observed_prefix: quote.invoice.observed_prefix(),
        },
    )
}

pub(super) fn gateway_unavailable(cause: impl core::fmt::Display) -> Error {
    Error::new(
        ErrorCode::GatewayUnavailable,
        format!("no usable lightning gateway: {cause}"),
    )
}

pub(super) fn unreachable(cause: impl core::fmt::Display) -> Error {
    Error::new(
        ErrorCode::FederationUnreachable,
        format!("the federation did not answer: {cause}"),
    )
}

pub(super) fn internal(cause: impl core::fmt::Display) -> Error {
    Error::new(ErrorCode::Internal, cause.to_string())
}

/// An upstream subscription that could not be opened, for either generation's driver.
pub(super) fn subscribe_error(cause: impl core::fmt::Display) -> Error {
    Error::new(
        ErrorCode::Internal,
        format!("could not follow this operation upstream: {cause}"),
    )
}

/// The spendable balance, as `Federation::balance` reads it.
pub(super) async fn balance_of(client: &Client) -> Result<Amount> {
    client
        .get_balance_for_btc()
        .await
        .map(from_upstream)
        .map_err(|err| internal(format!("this federation cannot report a balance: {err}")))
}

pub(super) fn now() -> Timestamp {
    Timestamp::from_epoch_millis(crate::db::now_millis())
}

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
            expires_at: Timestamp::from_epoch_millis(1_700_003_600_000),
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

    /// The mainnet fixture from `types/invoice.rs`: 25 mBTC, expired in 2017.
    // The brief's transcription of this and `MAINNET_AMOUNTLESS` below dropped a few characters
    // each, breaking their bech32 checksum; both are corrected here to the byte-exact fixtures
    // `types/invoice.rs` already parses and tests against (`MAINNET_25M` there).
    const MAINNET_EXPIRED: &str = "lnbc25m1pvjluezpp5qqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqypqdq5vdhkven9v5sxyetpdeessp5zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zygs9q5sqqqqqqqqqqqqqqqpqsq67gye39hfg3zd8rgc80k32tvy9xk2xunwm5lzexnvpx6fd77en8qaq424dxgt56cag2dpt359k3ssyhetktkpqh24jqnjyw6uqd08sgptq44qu";
    /// An amountless mainnet invoice, from the same file.
    const MAINNET_AMOUNTLESS: &str = "lnbc1pj48ugqdq0dehjqctdda6kuaqpp5yg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3qsp5xvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxves9qrsgqcqzyswm4efuu52zkzgrcc35fra9fmvj7s9ppxmej85s83hjkh7crcy9vqlradwalsmq40knf3552panjvlhjlrfazmvs86krxuaygut8v30sq0y0422";

    fn regtest(text: &str) -> Bolt11Invoice {
        text.parse().expect("a valid invoice")
    }

    #[test]
    fn preflight_refuses_an_amountless_invoice_first() {
        let err = preflight(&regtest(MAINNET_AMOUNTLESS), crate::Network::Regtest)
            .expect_err("amountless");
        assert_eq!(err.code, crate::ErrorCode::AmountlessInvoice);
    }

    #[test]
    fn preflight_reports_a_network_mismatch_with_details() {
        let err = preflight(&regtest(SEND_INVOICE), crate::Network::Bitcoin).expect_err("bcrt");
        assert_eq!(err.code, crate::ErrorCode::NetworkMismatch);
        match err.detail() {
            Some(crate::ErrorDetails::NetworkMismatch {
                expected,
                compatible,
                observed_prefix,
            }) => {
                assert_eq!(*expected, crate::Network::Bitcoin);
                assert_eq!(compatible, &vec![crate::Network::Regtest]);
                assert_eq!(observed_prefix, "bcrt");
            }
            other => panic!("expected NetworkMismatch details, got {other:?}"),
        }
    }

    #[test]
    fn a_tb_invoice_is_compatible_with_either_public_testnet() {
        // No `tb` fixture exists, so the expansion is checked directly.
        assert_eq!(
            compatible_networks(Some(crate::Network::Testnet)),
            vec![crate::Network::Testnet, crate::Network::Testnet4]
        );
        assert_eq!(compatible_networks(None), Vec::<crate::Network>::new());
        assert_eq!(
            compatible_networks(Some(crate::Network::Signet)),
            vec![crate::Network::Signet]
        );
    }

    #[test]
    fn preflight_refuses_an_expired_invoice_as_invalid_input() {
        let err =
            preflight(&regtest(MAINNET_EXPIRED), crate::Network::Bitcoin).expect_err("expired");
        assert_eq!(err.code, crate::ErrorCode::InvalidInput);
    }

    #[test]
    fn preflight_checks_expiry_last() {
        // The regtest fixture expired in 2023, so on a matching network the expiry is the only
        // refusal left, which is what proves the amount and network checks ran before it.
        let err = preflight(&regtest(SEND_INVOICE), crate::Network::Regtest).expect_err("expired");
        assert_eq!(err.code, crate::ErrorCode::InvalidInput);
        assert_eq!(
            regtest(SEND_INVOICE).amount(),
            Some(Amount::from_msats(100_000))
        );
    }

    #[test]
    fn a_description_longer_than_bolt11_allows_is_invalid_input() {
        assert!(check_description("coffee").is_ok());
        assert!(check_description(&"x".repeat(639)).is_ok());
        assert_eq!(
            check_description(&"x".repeat(640))
                .expect_err("too long")
                .code,
            crate::ErrorCode::InvalidInput
        );
        // Bytes, not characters: a three-byte character counts three times.
        assert_eq!(
            check_description(&"€".repeat(214))
                .expect_err("too long")
                .code,
            crate::ErrorCode::InvalidInput
        );
    }

    #[test]
    fn the_fee_breakdown_is_built_from_the_shared_fee_quote() {
        use fedimint_client_module::transaction::FeeQuote;
        use fedimint_core::module::Amounts;

        let quote = FeeQuote {
            input: Amounts::new_bitcoin(fedimint_core::Amount::from_msats(10)),
            output: Amounts::new_bitcoin(fedimint_core::Amount::from_msats(35)),
            dust: Amounts::new_bitcoin(fedimint_core::Amount::from_msats(5)),
        };
        let plan = plan_of(
            Amount::from_msats(1_000),
            Amount::from_msats(25),
            &quote,
            Amount::from_msats(100_000),
            LightningRoute::Internal,
            Terms::V1 { gateway: None },
        )
        .expect("a plan");
        assert_eq!(
            plan.breakdown,
            LnFeeBreakdown {
                gateway: Amount::from_msats(1_000),
                lightning_module: Amount::from_msats(25),
                primary_module: Amount::from_msats(20),
                dust: Amount::from_msats(5),
            }
        );
        assert_eq!(plan.fee, Amount::from_msats(1_050));
        assert_eq!(plan.total, Amount::from_msats(101_050));
    }

    #[test]
    fn a_fee_quote_below_the_modules_own_fee_is_internal() {
        use fedimint_client_module::transaction::FeeQuote;

        let err = plan_of(
            Amount::from_msats(0),
            Amount::from_msats(25),
            &FeeQuote::ZERO,
            Amount::from_msats(1),
            LightningRoute::Internal,
            Terms::V1 { gateway: None },
        )
        .expect_err("inconsistent");
        assert_eq!(err.code, crate::ErrorCode::Internal);
    }

    fn a_quote(expires_at: u64) -> LnQuoteInner {
        LnQuoteInner {
            federation_id: fedimint_core::config::FederationId::dummy(),
            invoice: regtest(SEND_INVOICE),
            invoice_amount: Amount::from_msats(100_000),
            plan: Plan {
                breakdown: LnFeeBreakdown {
                    gateway: Amount::from_msats(0),
                    lightning_module: Amount::from_msats(0),
                    primary_module: Amount::from_msats(0),
                    dust: Amount::from_msats(0),
                },
                fee: Amount::from_msats(0),
                total: Amount::from_msats(100_000),
                route: LightningRoute::Internal,
                terms: Terms::V1 { gateway: None },
            },
            expires_at: Timestamp::from_epoch_millis(expires_at),
        }
    }

    #[test]
    fn network_refusal_carries_the_regtest_invoices_networks() {
        let quote = a_quote(0);
        let err = network_refusal(&quote, crate::Network::Testnet4);
        assert_eq!(err.code, crate::ErrorCode::NetworkMismatch);
        match err.detail() {
            Some(crate::ErrorDetails::NetworkMismatch {
                expected,
                compatible,
                observed_prefix,
            }) => {
                assert_eq!(*expected, crate::Network::Testnet4);
                assert_eq!(compatible, &vec![crate::Network::Regtest]);
                assert_eq!(observed_prefix, "bcrt");
            }
            other => panic!("expected NetworkMismatch details, got {other:?}"),
        }
    }

    #[test]
    fn a_quote_is_executable_until_it_expires_and_only_on_its_federation() {
        let quote = a_quote(1_000);
        let id = fedimint_core::config::FederationId::dummy();
        assert!(ensure_executable(&quote, id, 999).is_ok());
        assert!(ensure_executable(&quote, id, 1_000).is_ok());
        let err = ensure_executable(&quote, id, 1_001).expect_err("expired");
        assert_eq!(err.code, crate::ErrorCode::QuoteExpired);
        match err.detail() {
            Some(crate::ErrorDetails::QuoteExpired {
                expires_at,
                already_executed,
            }) => {
                assert_eq!(*expires_at, Timestamp::from_epoch_millis(1_000));
                assert!(!already_executed);
            }
            other => panic!("expected QuoteExpired details, got {other:?}"),
        }
        let other = fedimint_core::config::FederationId(
            fedimint_core::bitcoin::hashes::Hash::hash(b"another federation"),
        );
        assert_eq!(
            ensure_executable(&quote, other, 0)
                .expect_err("wrong federation")
                .code,
            crate::ErrorCode::InvalidInput
        );
    }

    #[test]
    fn quote_accessors_read_the_frozen_plan() {
        let quote = LnQuote { inner: a_quote(5) };
        assert_eq!(quote.invoice_amount(), Amount::from_msats(100_000));
        assert_eq!(quote.fee(), Amount::from_msats(0));
        assert_eq!(quote.total(), Amount::from_msats(100_000));
        assert_eq!(quote.route(), LightningRoute::Internal);
        assert_eq!(quote.expires_at(), Timestamp::from_epoch_millis(5));
        assert_eq!(quote.fee_breakdown().gateway, Amount::from_msats(0));
    }

    #[test]
    fn the_error_helpers_carry_their_details() {
        match quote_changed(Amount::from_msats(10), Amount::from_msats(12)).detail() {
            Some(crate::ErrorDetails::QuoteTermsChanged {
                quoted_total,
                current_total,
            }) => {
                assert_eq!(*quoted_total, Amount::from_msats(10));
                assert_eq!(*current_total, Amount::from_msats(12));
            }
            other => panic!("expected QuoteTermsChanged, got {other:?}"),
        }
        match insufficient(Amount::from_msats(10), Amount::from_msats(3)).detail() {
            Some(crate::ErrorDetails::InsufficientBalance {
                required,
                available,
            }) => {
                assert_eq!(*required, Amount::from_msats(10));
                assert_eq!(*available, Amount::from_msats(3));
            }
            other => panic!("expected InsufficientBalance, got {other:?}"),
        }
        assert_eq!(
            quote_expired(Timestamp::from_epoch_millis(1), true).code,
            crate::ErrorCode::QuoteExpired
        );
        assert_eq!(
            gateway_unavailable("offline").code,
            crate::ErrorCode::GatewayUnavailable
        );
        assert_eq!(
            unreachable("down").code,
            crate::ErrorCode::FederationUnreachable
        );
    }

    #[test]
    fn fee_quote_failure_is_classified_before_either_mint_is_asked() {
        // The v1 mint's typed error wins even when the accompanying text also happens to
        // mention the v2 mint's wording; the typed case is unambiguous and checked first.
        let typed = fedimint_mint_client::InsufficientBalanceError {
            requested_amount: fedimint_core::Amount::from_msats(10),
            total_amount: fedimint_core::Amount::from_msats(3),
        };
        match classify_fee_quote_failure(Some(&typed), "Insufficient funds") {
            Some(FeeQuoteFailure::Typed { requested, total }) => {
                assert_eq!(requested, Amount::from_msats(10));
                assert_eq!(total, Amount::from_msats(3));
            }
            other => panic!("expected the typed case, got {other:?}"),
        }
        // The v2 mint's plain-text refusal, with no typed error at all.
        assert!(matches!(
            classify_fee_quote_failure(None, "Insufficient funds"),
            Some(FeeQuoteFailure::Text)
        ));
        // Neither mint's wording: not this crate's problem to interpret.
        assert!(classify_fee_quote_failure(None, "the federation timed out").is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_recorded_send_reads_its_details_back_through_the_engine() {
        use crate::db::{federation_namespace, in_memory_root};
        use crate::federation::FederationInner;
        use crate::operation::{Driver, kinds};

        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db, true);
        let details = send_details();
        let id = fedimint_core::core::OperationId([4u8; 32]);
        let operation = federation
            .create_operation(
                id,
                kinds::LN_SEND,
                "ln",
                &wire::LnSendDetailsWire::from(&details),
                Arc::new(LnSendDriver) as Arc<dyn Driver<LnSendState>>,
            )
            .await
            .expect("create");
        assert_eq!(operation.details().await.expect("details"), details);

        // The lookup path hands the same record to the same driver.
        let any = federation
            .operation(id)
            .await
            .expect("lookup")
            .expect("recorded");
        assert_eq!(any.kind(), crate::OperationKind::LnSend);
        let typed = any.as_ln_send().expect("a typed handle");
        assert_eq!(typed.details().await.expect("details"), details);
        // No client behind a detached federation: observing the state is refused, not faked.
        assert_eq!(
            typed.state().await.expect_err("no client").code,
            crate::ErrorCode::FederationClosed
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_recorded_final_state_is_read_without_a_client() {
        use crate::db::{federation_namespace, in_memory_root};
        use crate::federation::FederationInner;
        use crate::operation::{Driver, kinds};

        let db = federation_namespace(&in_memory_root(), [2u8; 32]);
        let federation = FederationInner::detached(db, true);
        let id = fedimint_core::core::OperationId([5u8; 32]);
        let operation = federation
            .create_operation(
                id,
                kinds::LN_RECEIVE,
                "lnv2",
                &wire::LnReceiveDetailsWire::from(&receive_details()),
                Arc::new(LnReceiveDriver) as Arc<dyn Driver<LnReceiveState>>,
            )
            .await
            .expect("create");
        operation
            .inner()
            .record_final_state(
                wire::encode_receive_state(&LnReceiveState::Claimed).expect("encode"),
            )
            .await
            .expect("record");
        assert_eq!(
            operation.state().await.expect("state"),
            LnReceiveState::Claimed
        );
    }
}
