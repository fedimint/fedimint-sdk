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
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct Lightning {
    inner: Arc<LightningInner>,
}

// `quote` is exported under its own name, unchanged: `LnQuote` is a UniFFI
// object (see below), and a bare object returned through `Result<T>`
// crosses the boundary with no adapter needed. `send` and `receive` still
// need one, in the block below `new`: `send`'s real parameter is an owned
// `LnQuote`, which an object can never cross as (only `Arc<LnQuote>` can),
// and `receive`'s real return type names the generic
// `Operation<LnReceiveState>`, which cannot cross at all.
#[cfg_attr(feature = "uniffi", uniffi::export(async_runtime = "tokio"))]
impl Lightning {
    /// Plans a payment and returns an executable quote for it.
    ///
    /// The returned [`LnQuote`] is the frozen plan for paying `invoice`: the
    /// amount the invoice names, the route, the aggregate fee and the total
    /// debit. Show those numbers to the user, then pass the quote to
    /// [`Lightning::send`], whose docs say exactly what executing it
    /// guarantees.
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
        Ok(LnQuote::new(LnQuoteInner {
            federation_id: federation.id,
            invoice: invoice.clone(),
            invoice_amount,
            plan,
            expires_at,
        }))
    }
}

impl Lightning {
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
        self.send_authorized(&quote).await
    }

    /// The body of [`Lightning::send`], taking the quote by reference so the
    /// UniFFI-facing `send` (which can only ever hold a shared `Arc<LnQuote>`,
    /// never an owned one) can call it too, after checking the quote's
    /// single-use flag itself. Never reads or writes that flag: whether a
    /// quote has already been paid is a UniFFI-only concern, checked once at
    /// the boundary before this runs.
    async fn send_authorized(&self, quote: &LnQuote) -> Result<Operation<LnSendState>> {
        let federation = &self.inner.federation;
        let quote = &quote.inner;
        ensure_executable(quote, federation.id, crate::db::now_millis())?;
        // The guard is held across the re-check, the funding and the record write, which is what
        // `create_operation` requires of its caller.
        let client = federation.client(true).await?;
        match (module(&client)?, &quote.plan.terms) {
            (LnModule::V1(module), Terms::V1 { gateway }) => {
                v1::send(federation, &client, &module, quote, gateway.clone()).await
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
                    quote,
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
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a zero amount, a
    /// description the invoice format cannot carry, or an amount too small
    /// to cover the fee of claiming it,
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

// The UniFFI view of `send`/`receive` above, under their real names but
// different Rust identifiers: `send`'s real parameter is an owned
// `LnQuote`, which can only ever cross the boundary as `Arc<LnQuote>` (see
// `LnQuote`'s own `#[uniffi::export]` block for how it enforces single
// use), and `receive`'s real return type names the generic
// `Operation<LnReceiveState>`, which cannot cross at all — its UniFFI view
// is `LnReceiveHandle`, defined below. `quote` needs no such adapter: see
// the export attribute directly on it, above.
#[cfg(feature = "uniffi")]
#[uniffi::export(async_runtime = "tokio")]
impl Lightning {
    /// See [`Lightning::send`]. Fails with
    /// [`ErrorCode::QuoteExpired`] if `quote` was already sent.
    #[uniffi::method(name = "send")]
    pub async fn ffi_send(&self, quote: Arc<LnQuote>) -> Result<LnSendOperation> {
        quote.used.claim(quote.expires_at())?;
        Ok(self.send_authorized(&quote).await?.into())
    }

    /// See [`Lightning::receive`].
    #[uniffi::method(name = "receive")]
    pub async fn ffi_receive(&self, amount: Amount, description: &str) -> Result<LnReceiveHandle> {
        Ok(self.receive(amount, description).await?.into())
    }
}

/// A frozen, executable plan for one lightning payment.
///
/// Produced by [`Lightning::quote`] and consumed by [`Lightning::send`], whose
/// docs say exactly what is guaranteed to hold at execution. Everything a
/// user needs to approve is readable through the accessors below.
#[derive(Debug)]
// Crosses a UniFFI boundary as an opaque object rather than a plain record:
// a record crosses by value, so nothing would stop a caller from passing
// the same field values into `send` twice and paying twice. `used` gives it
// real interior state instead, checked and set once by `send`'s colocated
// adapter, so a second attempt fails with `QuoteExpired` the same way it is
// a compile error in plain Rust (`send` takes the quote by value). Behind
// the `uniffi` feature; absent from every other build, including plain
// Rust, where the type system already enforces single use.
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct LnQuote {
    inner: LnQuoteInner,
    #[cfg(feature = "uniffi")]
    used: crate::ffi::QuoteClaim,
}

impl LnQuote {
    /// Wraps a frozen plan in a fresh, unclaimed quote.
    fn new(inner: LnQuoteInner) -> Self {
        Self {
            inner,
            #[cfg(feature = "uniffi")]
            used: crate::ffi::QuoteClaim::default(),
        }
    }
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl LnQuote {
    /// The invoice's amount: what reaches the payee if the payment succeeds.
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
    /// This is the number to show as "you will pay", and it is exact: the
    /// debit execution is authorised to make, not a ceiling or an estimate.
    /// A payment that would cost anything else is refused with
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged), whose
    /// [`ErrorDetails::QuoteTermsChanged`](crate::ErrorDetails::QuoteTermsChanged)
    /// names this total and the one the payment would now cost, so the user
    /// re-approves a new number rather than quietly paying a different one.
    /// The same figure is what [`LnSendDetails::total`] records.
    ///
    /// "This much or nothing" is a statement about the authorised debit, not
    /// about what the balance does moment to moment. Executing the quote
    /// submits a transaction, and submitting one takes the notes that are to
    /// pay for it out of the spendable set before the federation has
    /// accepted anything, so a funding attempt the federation then rejects
    /// can remove value and restore it afterwards.
    ///
    /// Nothing else is ever debited than this total. What becomes of it when
    /// the payment does not succeed is the ending's to say rather than the
    /// quote's: [`LnSendState::Refunded`] is the one that promises no
    /// lasting debit, reported only once the value is spendable again, and
    /// [`LnSendState::Failed`] is the one that cannot promise it.
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
/// [`Refunded`](Self::Refunded) means the payment left no lasting debit and
/// the value is spendable again; [`Failed`](Self::Failed) means the payment
/// did not resolve into either. A payment has no cancellation: once sent it
/// runs to one of those endings.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
    /// Final: the payment did not go through and the value it was
    /// authorised for is spendable again.
    ///
    /// This is the ordinary failure of a lightning payment: no route, the
    /// payee went away, the gateway gave up, or the federation rejected the
    /// transaction that would have funded it. What the endings have in
    /// common is the promise this state makes, which is about the balance
    /// now and not about the route taken to it: whatever the payment
    /// removed along the way, no part of it is still standing against the
    /// balance. The money is safe.
    ///
    /// Reaching that promise is not always instant, and this state waits
    /// for it. Funding a payment selects the notes that are to pay for it
    /// before consensus has accepted anything, so a rejected funding
    /// transaction leaves value that is neither spent nor yet spendable,
    /// and returning it is a later transaction of the mint's own. The
    /// payment stays non-final for as long as that runs. Only once it has
    /// settled, and settled in a way that establishes the value came back,
    /// is this state reported; a recovery that settles without
    /// establishing it ends the payment in [`Failed`](Self::Failed)
    /// instead.
    Refunded,
    /// Final: the payment failed in a way that did not resolve into a clean
    /// refund.
    ///
    /// Either the payment got far enough to be at risk and no refund
    /// followed, or its funding was rejected and the value that attempt
    /// removed could not be established as spendable again. Both mean the
    /// same thing to an application: this state cannot say where the money
    /// is, so read the balance rather than inferring one.
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[non_exhaustive]
pub struct LnSendDetails {
    /// The invoice this payment was authorised to pay.
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
    ///
    /// Exact for an operation this SDK created, including one recovered
    /// after a restart, except for the residual case above.
    ///
    /// That residual case is never corrected: even once recovered after a
    /// restart, it still reports the quote's upper bound, not the settled
    /// figure.
    ///
    /// An estimate only for a log entry this SDK did not create.
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
    ///
    /// Exact for an operation this SDK created, even one recovered after a
    /// restart; a record rebuilt from a log entry this SDK did not create
    /// has no fee to recover and reports zero.
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
// The dry run balances the funding transaction against the real notes and fails inside the
// primary module when they cannot cover it. The v2 mint (`fedimint-mintv2-client`) reports
// that as a plain-text context, `"Insufficient funds"` (`fedimint-mintv2-client/src/lib.rs:503`).
// The v1 mint (`fedimint-mint-client`) reports it as the typed
// `fedimint_mint_client::InsufficientBalanceError`, which carries the amounts that were short,
// but by the time this SDK sees it that type can no longer be recovered (see
// `classify_fee_quote_failure`'s comment), so it is recognized the same way as the v2 mint's
// refusal: by its own fixed message, `"Insufficient balance"`, with no amounts of its own.
#[derive(Debug)]
enum FeeQuoteFailure {
    /// The v1 mint's typed error, carrying its own requested and total amounts. Kept for the
    /// day the client stops re-boxing the primary module's error through `anyhow`, at which
    /// point the type survives and this arm starts firing again.
    Typed { requested: Amount, total: Amount },
    /// Either mint's refusal recognized by its own message, naming no amounts.
    Text,
}

/// Walks an error and every one of its `source()`s, outermost first. The chain-walking
/// counterpart of `anyhow::Error::chain`, over the plain [`std::error::Error`] this crate can
/// name without depending on a foreign error-type crate itself.
fn error_chain<'a>(
    err: &'a (dyn std::error::Error + 'static),
) -> impl Iterator<Item = &'a (dyn std::error::Error + 'static)> {
    std::iter::successors(Some(err), |err| err.source())
}

/// Recognizes either mint's insufficient-balance refusal anywhere in a fee-quote failure's
/// error chain, or reports neither is a match. Pure so the mapping can be checked without a
/// live `Client`.
// By the time a call site sees this failure it is already wrapped as
// `fedimint_client_module::TransactionSubmitError::PrimaryModule`, whose own `Display` is just
// "The primary module failed": neither mint's refusal survives in the outer error's own
// message, so both are looked for through the whole chain instead of reading only the top.
//
// The typed downcast is tried first and kept even though it cannot succeed on the shape the
// client actually produces today: the client builds that wrapper from the mint's `anyhow`
// error via `anyhow::Error::into_boxed_dyn_error`, which reattaches the original error's own
// `Display`/`source` but not its `Any` identity (anyhow's own docs on that method say exactly
// this: the result "can no longer downcast"). So `InsufficientBalanceError` survives this
// boundary only as text, the same as the v2 mint's refusal, and is matched that way below.
fn classify_fee_quote_failure(err: &(dyn std::error::Error + 'static)) -> Option<FeeQuoteFailure> {
    if let Some(short) = error_chain(err)
        .find_map(|cause| cause.downcast_ref::<fedimint_mint_client::InsufficientBalanceError>())
    {
        return Some(FeeQuoteFailure::Typed {
            requested: from_upstream(short.requested_amount),
            total: from_upstream(short.total_amount),
        });
    }
    if error_chain(err).any(|cause| {
        let text = cause.to_string();
        text.contains("Insufficient funds") || text.contains("Insufficient balance")
    }) {
        return Some(FeeQuoteFailure::Text);
    }
    None
}

/// Joins every cause in `err`'s chain with `": "`, outermost first.
///
/// `err`'s own type is never named in this crate (see `classify_fee_quote_failure`'s comment),
/// so its real `Display` (`anyhow::Error`'s, which joins its own chain in alternate mode) is not
/// reachable through the `std::error::Error` reference this crate holds instead; this rebuilds
/// the same joined text by hand, one cause's own message at a time.
fn chain_text(err: &(dyn std::error::Error + 'static)) -> String {
    error_chain(err)
        .map(|cause| cause.to_string())
        .collect::<Vec<_>>()
        .join(": ")
}

/// What an insufficient-balance refusal means at a fee-quote call site.
///
/// Funding a send is a balance problem: the wallet cannot cover the payment. Funding a
/// receive's claim fee is an amount problem instead: the wallet being asked to front the
/// shortfall is itself the symptom that the claim fee exceeds the amount being received.
pub(super) enum Shortfall {
    /// Report the refusal as
    /// [`ErrorCode::InsufficientBalance`](crate::ErrorCode::InsufficientBalance).
    Balance,
    /// Report the refusal as [`ErrorCode::InvalidInput`](crate::ErrorCode::InvalidInput).
    Amount,
}

/// Turns a fee-quote dry run's failure into the [`Error`] it represents, for the four call sites
/// (v1's and v2's `terms_for` and `receive`) that run one.
///
/// `shortfall` says what an insufficient-balance refusal means at the caller's call site.
/// `required` is the amount the failed quote was for, used to report the shortfall when
/// `shortfall` is [`Shortfall::Balance`] and the mint's answer carries no amounts of its own.
/// `context` names the quote for the fallback message, when neither mint's refusal is found
/// anywhere in `err`'s chain.
pub(super) async fn fee_quote_failure(
    client: &Client,
    err: &(dyn std::error::Error + Send + Sync + 'static),
    shortfall: Shortfall,
    required: Amount,
    context: &str,
) -> Error {
    match classify_fee_quote_failure(err) {
        Some(FeeQuoteFailure::Typed { requested, total }) => match shortfall {
            Shortfall::Balance => insufficient(requested, total),
            Shortfall::Amount => amount_too_small(),
        },
        Some(FeeQuoteFailure::Text) => match shortfall {
            Shortfall::Balance => {
                // Neither mint's text names amounts, so the balance is read again here. A
                // failed read must not mask the real refusal that was already found, so it
                // falls back to zero rather than turning this into an unrelated error.
                let available = balance_of(client).await.unwrap_or(Amount::from_msats(0));
                insufficient(required, available)
            }
            Shortfall::Amount => amount_too_small(),
        },
        None => internal(format!("{context}: {}", chain_text(err))),
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

/// A receive whose claim fee would exceed the amount being received.
pub(super) fn amount_too_small() -> Error {
    Error::new(
        ErrorCode::InvalidInput,
        "the amount is too small to cover the fees of claiming it",
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

// The UniFFI views of `Operation<LnSendState>` and `Operation<LnReceiveState>`:
// `Operation<S>` is generic and UniFFI objects cannot be, so
// `crate::ffi::ffi_operation!` monomorphises one newtype object per
// state, forwarding every method to the real handle. See that macro's
// documentation in `ffi.rs`.
#[cfg(feature = "uniffi")]
crate::ffi::ffi_operation!(
    LnSendOperation,
    LnSendOperationUpdates,
    LnSendState,
    details: LnSendDetails
);
#[cfg(feature = "uniffi")]
crate::ffi::ffi_operation!(
    LnReceiveOperation,
    LnReceiveOperationUpdates,
    LnReceiveState,
    details: LnReceiveDetails
);

/// The result of [`Lightning::receive`], with `operation` crossing as
/// [`LnReceiveOperation`] rather than the generic `Operation<LnReceiveState>`
/// the real [`LnReceive`] carries.
#[cfg(feature = "uniffi")]
#[derive(Debug, uniffi::Record)]
pub struct LnReceiveHandle {
    /// See [`LnReceive::invoice`].
    pub invoice: Bolt11Invoice,
    /// See [`LnReceive::operation`].
    pub operation: Arc<LnReceiveOperation>,
}

#[cfg(feature = "uniffi")]
impl From<LnReceive> for LnReceiveHandle {
    fn from(receive: LnReceive) -> Self {
        Self {
            invoice: receive.invoice,
            operation: Arc::new(receive.operation.into()),
        }
    }
}

/// Realistic lightning records for other modules' tests, so a test elsewhere does not have to
/// hand-assemble one.
//
// At file scope rather than inside `mod tests`, because a `mod tests` is private to its own
// file and the page walk's own tests (`src/activity/page.rs`) need these too.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::wire::{LnReceiveDetailsWire, LnSendDetailsWire};
    use crate::{Amount, LightningRoute, LnReceiveDetails, LnSendDetails, Timestamp};

    /// A real regtest invoice for 100 000 msat, the same literal `lightning/wire.rs`'s own
    /// tests use.
    const INVOICE: &str = "lnbcrt1u1pj48ugqdq2vdhkven9v5pp5g3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zqsp5242424242424242424242424242424242424242424242424242s9qrsgqcqzys2reg4wsryjt5w8z33ugydecgfmgyvtttwa7e0yzlm803z203j9hqspa4lr6m09cd808xkw9uh4sxc8wf3w6k0gaf5zrqm7zhcxug0vqqpdkpja";
    /// A real compressed secp256k1 public key, from that crate's own test suite.
    const GATEWAY_ID: &str = "0218845781f631c48f1c9709e23092067d06837f30aa0cd0544ac887fe91ddd166";

    /// A send of 100 000 msat with a 1 000 msat fee, routed through a gateway.
    pub(crate) fn send_details() -> LnSendDetails {
        LnSendDetails {
            invoice: INVOICE.parse().expect("a valid regtest invoice"),
            invoice_amount: Amount::from_msats(100_000),
            fee: Amount::from_msats(1_000),
            total: Amount::from_msats(101_000),
            route: LightningRoute::Gateway {
                gateway_id: GATEWAY_ID.parse().expect("a valid gateway id"),
            },
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    /// A receive of the same invoice with a 500 msat fee, crediting 99 500 msat.
    pub(crate) fn receive_details() -> LnReceiveDetails {
        LnReceiveDetails {
            invoice: INVOICE.parse().expect("a valid regtest invoice"),
            description: "coffee".to_owned(),
            requested_amount: Amount::from_msats(100_000),
            invoice_amount: Amount::from_msats(100_000),
            fee: Amount::from_msats(500),
            net_credit: Amount::from_msats(99_500),
            gateway_id: Some(GATEWAY_ID.parse().expect("a valid gateway id")),
            expires_at: Timestamp::from_epoch_millis(1_700_003_600_000),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    /// [`send_details`], as it is actually persisted: through the wire type, the same JSON a
    /// real send record's `details` field holds.
    pub(crate) fn send_details_json(details: &LnSendDetails) -> String {
        serde_json::to_string(&LnSendDetailsWire::from(details)).expect("a well-formed record")
    }

    /// [`receive_details`], as it is actually persisted.
    pub(crate) fn receive_details_json(details: &LnReceiveDetails) -> String {
        serde_json::to_string(&LnReceiveDetailsWire::from(details)).expect("a well-formed record")
    }
}

#[cfg(test)]
mod tests {
    use fedimint_client_module::TransactionSubmitError;

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
        let quote = LnQuote::new(a_quote(5));
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

    /// A minimal cause for a flat (unwrapped) error chain in a test, without needing a real
    /// mint error for messages that name no amounts of their own.
    #[derive(Debug)]
    struct Cause(&'static str);

    impl std::fmt::Display for Cause {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }

    impl std::error::Error for Cause {}

    #[test]
    fn fee_quote_failure_is_classified_before_either_mint_is_asked() {
        // The typed case is checked across the whole chain before the text is, so it wins even
        // if a chain somehow carried both. This is only reachable when the typed error is not
        // itself hidden behind `anyhow`'s type erasure, which the real wrapper always does
        // today (see the test below); this covers the classifier's own logic in isolation.
        let typed = fedimint_mint_client::InsufficientBalanceError {
            requested_amount: fedimint_core::Amount::from_msats(10),
            total_amount: fedimint_core::Amount::from_msats(3),
        };
        match classify_fee_quote_failure(&typed) {
            Some(FeeQuoteFailure::Typed { requested, total }) => {
                assert_eq!(requested, Amount::from_msats(10));
                assert_eq!(total, Amount::from_msats(3));
            }
            other => panic!("expected the typed case, got {other:?}"),
        }
        // The v2 mint's plain-text refusal, with no typed error at all.
        assert!(matches!(
            classify_fee_quote_failure(&Cause("Insufficient funds")),
            Some(FeeQuoteFailure::Text)
        ));
        // The v1 mint's own wording, recognized the same way once its type is unrecoverable.
        assert!(matches!(
            classify_fee_quote_failure(&Cause("Insufficient balance: requested 10 but only 3")),
            Some(FeeQuoteFailure::Text)
        ));
        // Neither mint's wording: not this crate's problem to interpret.
        assert!(classify_fee_quote_failure(&Cause("the federation timed out")).is_none());
    }

    #[test]
    fn fee_quote_failure_is_recognised_through_the_transaction_submit_wrapper() {
        // The real shape the client produces: `Client::fee_quote` converts the mint's own
        // `anyhow::Error` into a `Box<dyn Error + Send + Sync>` (`anyhow::Error::into`, called
        // `into_boxed_dyn_error` on stable) and wraps that box as
        // `TransactionSubmitError::PrimaryModule`; the SDK then converts that whole
        // `TransactionSubmitError` into another `anyhow::Error` in turn (upstream's own
        // `send_fee_quote`/`receive_fee_quote` do this via `map_err(anyhow::Error::from)`) and
        // hands this crate `.as_ref()` of it, exactly as the four call sites do below. That
        // double conversion is what drops `InsufficientBalanceError`'s own type, so both
        // refusals must be found by text through it, not just when they are the outermost
        // error.
        let typed = fedimint_mint_client::InsufficientBalanceError {
            requested_amount: fedimint_core::Amount::from_msats(10),
            total_amount: fedimint_core::Amount::from_msats(3),
        };
        let mint_v1_refusal = anyhow::Error::from(TransactionSubmitError::PrimaryModule(
            anyhow::Error::from(typed).into(),
        ));
        assert!(matches!(
            classify_fee_quote_failure(mint_v1_refusal.as_ref()),
            Some(FeeQuoteFailure::Text)
        ));

        let mint_v2_refusal = anyhow::Error::from(TransactionSubmitError::PrimaryModule(
            anyhow::anyhow!("select_funding_input")
                .context("Insufficient funds")
                .into(),
        ));
        assert!(matches!(
            classify_fee_quote_failure(mint_v2_refusal.as_ref()),
            Some(FeeQuoteFailure::Text)
        ));

        // An unrelated failure inside the same wrapper is still not this crate's problem to
        // interpret, and the fallback message for it still carries the whole chain by hand,
        // since neither this crate nor `TransactionSubmitError`'s own `Display` can print it.
        let unrelated = anyhow::Error::from(TransactionSubmitError::PrimaryModule(
            anyhow::anyhow!("the federation timed out").into(),
        ));
        assert!(classify_fee_quote_failure(unrelated.as_ref()).is_none());
        let joined = chain_text(unrelated.as_ref());
        assert!(joined.contains("The primary module failed"), "{joined}");
        assert!(joined.contains("the federation timed out"), "{joined}");
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

    // The three futures are awaited from spawned tasks by applications, so each must stay
    // `Send`. The check is done by the type checker: `check` is never called, only named, and
    // a non-`Send` future fails to compile it. `fee_quote_failure` once took a trait object
    // without `Send + Sync`, which made every one of these futures non-`Send`.
    #[test]
    fn quote_send_and_receive_futures_are_send() {
        fn assert_send<T: Send>(_: T) {}
        fn check(lightning: &Lightning, invoice: &Bolt11Invoice, quote: LnQuote, amount: Amount) {
            assert_send(lightning.quote(invoice));
            assert_send(lightning.send(quote));
            assert_send(lightning.receive(amount, ""));
        }
        let _: fn(&Lightning, &Bolt11Invoice, LnQuote, Amount) = check;
    }
}
