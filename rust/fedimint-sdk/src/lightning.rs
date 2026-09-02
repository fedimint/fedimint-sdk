//! Bolt11 lightning: paying invoices and getting paid.

use std::sync::Arc;

use crate::{
    Amount, Bolt11Invoice, GatewayId, Operation, OperationState, Preimage, Result, Timestamp,
};

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
    /// binds the invoice, the amount the invoice names, the selected and
    /// verified gateway (or the discovery that no gateway is needed at all),
    /// the aggregate fee, the total debit, and the federation configuration
    /// those were computed against. Show it, then hand it back to
    /// [`Lightning::send`] to execute exactly what was shown. A user cannot
    /// be quoted one fee and charged another.
    ///
    /// The amount is the invoice's own, always. This call takes no amount
    /// parameter, and the section below is why.
    ///
    /// Quotes expire; see [`LnQuote::expires_at`].
    ///
    /// # An amountless invoice is refused, and no amount can rescue it
    ///
    /// An invoice that names no amount fails here with
    /// [`AmountlessInvoice`](crate::ErrorCode::AmountlessInvoice), and there
    /// is deliberately nowhere to supply an amount instead. Fedimint does not
    /// support paying amountless bolt11 invoices: that is a **permanent
    /// upstream position**, confirmed as something that cannot be implemented
    /// safely, and both the v1 and the lnv2 payment paths reject such an
    /// invoice outright. It is not a gap in this SDK that a later release
    /// fills.
    ///
    /// So a payer-supplied amount could never make such an invoice payable,
    /// and a parameter for one would be a permanently-unusable argument in
    /// every generated binding — always null in Swift, Kotlin and
    /// TypeScript, with no value for it that changes the outcome. The way an
    /// application declines the invoice with a useful message is to look
    /// before it quotes:
    /// [`Bolt11Invoice::amount`](crate::Bolt11Invoice::amount) returning
    /// `None` is exactly the invoice this call refuses, and "this invoice
    /// does not specify an amount and cannot be paid here" is a better thing
    /// to show than a failed quote.
    ///
    /// # This is where the network is checked
    ///
    /// The invoice's currency — and therefore the Bitcoin network it is
    /// denominated for — is compared against the federation's
    /// ([`Federation::network`](crate::Federation::network)) here, and a
    /// disagreement fails with
    /// [`NetworkMismatch`](crate::ErrorCode::NetworkMismatch). The comparison
    /// is by BOLT11 currency class rather than exact network, because that
    /// is all an invoice can say: the `tb` currency covers testnet3 and
    /// testnet4 alike, so a `tb` invoice is compatible with a federation on
    /// either, and a federation on
    /// [`Network::Testnet4`](crate::Network::Testnet4) issues `tb` invoices
    /// of its own. The structured
    /// [`ErrorDetails::NetworkMismatch`](crate::ErrorDetails::NetworkMismatch)
    /// carries the federation's network as `expected`, every network the
    /// invoice could have been for as `compatible` — one entry for a
    /// currency that pins the network, two for `tb` — and the currency
    /// prefix itself as `observed_prefix`, so a caller can name them without
    /// parsing the message.
    ///
    /// Quoting is the deterministic place for that check: every payment
    /// passes through it, it happens before anything is committed, and the
    /// answer does not depend on which gateway is picked or which module
    /// generation serves the payment. lnv2 has its own `WrongCurrency`
    /// failure downstream of here, but relying on it would mean discovering
    /// the mismatch mid-payment on one generation and not at all on the
    /// other. A syntactically valid invoice for the wrong network therefore
    /// cannot survive quoting and reach [`Lightning::send`].
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
    /// The terms this call executed on are persisted as
    /// [`LnSendDetails`] before it returns, so the invoice, the amounts, the
    /// fee and the route stay readable from
    /// [`Operation::details`](crate::Operation::details) for the life of the
    /// operation — after a restart, and however the payment ends.
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
    /// `amount` is what the payer is asked for: the invoice's face value is
    /// exactly this amount, and the receive-side fee is taken out of it, so
    /// the credit that lands is slightly smaller. [`LnReceiveDetails`] states
    /// that convention exactly and records every one of the three numbers.
    ///
    /// The invoice and the terms it was issued on are persisted as
    /// [`LnReceiveDetails`] before this call returns, so the QR code can be
    /// re-displayed and the expiry counted down after a restart, from nothing
    /// but the operation's id.
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
/// progress stream — so the quote is what lets [`LnSendState::Success`]
/// report the fee that was actually charged, and what
/// [`Lightning::send`] persists into [`LnSendDetails`] so that the fee and
/// the route survive a restart and remain readable however the payment ends.
#[derive(Debug)]
pub struct LnQuote {
    inner: LnQuoteInner,
}

impl LnQuote {
    /// The invoice's amount: what will reach the payee.
    ///
    /// The amount the invoice itself names, and nothing else — the payer
    /// contributes no number of their own, and an invoice that names no amount
    /// never reaches a quote at all (see [`Lightning::quote`]).
    pub fn invoice_amount(&self) -> Amount {
        unimplemented!()
    }

    /// The aggregate fee this payment will cost, on top of
    /// [`LnQuote::invoice_amount`].
    ///
    /// **Every debit that funding this payment incurs is in this one
    /// number**, not just the gateway's cut. Paying an invoice out of ecash
    /// is a federation transaction, and that transaction has costs of its
    /// own: the fee on the lightning output that funds the payment, the fees
    /// the primary module charges on the ecash inputs it spends and on the
    /// change it issues back, any other per-input or per-output module fee,
    /// and value too small for any note denomination to represent, which is
    /// therefore never reissued as change. Upstream's
    /// `OutgoingLightningPayment.fee` is only the first of those, so a quote
    /// that reported it alone would understate the debit and the balance
    /// would then disagree with the number the user approved.
    ///
    /// It follows that this is **not zero on an internal route**. An internal
    /// payment needs no gateway and so pays no gateway fee, but it is still
    /// funded by a federation transaction and still carries that
    /// transaction's costs. "No gateway" means "no gateway fee", not "free".
    ///
    /// [`LnQuote::fee_breakdown`] itemises this same number for an approval
    /// screen that wants to show the parts. This accessor stays
    /// authoritative: the breakdown sums to it exactly.
    pub fn fee(&self) -> Amount {
        unimplemented!()
    }

    /// The parts [`LnQuote::fee`] is made of, for an approval screen that
    /// itemises them.
    ///
    /// Purely a view of the aggregate — see [`LnFeeBreakdown`] for the
    /// guarantee that the components sum to [`LnQuote::fee`] exactly, and for
    /// which of them can be zero.
    pub fn fee_breakdown(&self) -> LnFeeBreakdown {
        unimplemented!()
    }

    /// The whole debit this payment will make against the balance:
    /// [`LnQuote::invoice_amount`] plus [`LnQuote::fee`], with nothing further
    /// to be added afterwards.
    ///
    /// This is the number to show as "you will pay", and it is more than a
    /// display value: it is **the debit execution is authorised to make**,
    /// exactly. [`Lightning::send`] debits this much or does not run at all
    /// — anything that would make the real debit differ means the plan no
    /// longer holds, and the answer is
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) with
    /// [`ErrorDetails::QuoteTermsChanged`](crate::ErrorDetails::QuoteTermsChanged)
    /// naming this total and the one the payment would now cost. A charge
    /// that differs from the approval is never an outcome, and the same
    /// figure is what [`LnSendDetails::total`] then records.
    ///
    /// What makes that exact rather than a ceiling is that the quote binds
    /// the notes that will fund the payment along with everything else.
    /// Denomination dust depends on which notes are spent, so a quote that
    /// left note selection to execution could only bound the debit; this
    /// one fixes the selection, and a selection that no longer holds when
    /// [`Lightning::send`] runs — a note spent elsewhere in between — is a
    /// changed quote, not a different debit.
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
/// 50 the federation's" than show one unexplained lump. Fedimint's lightning
/// fee genuinely has several payees, and a user told only a total has no way
/// to tell an expensive gateway from an expensive transaction.
///
/// # The aggregate is authoritative
///
/// This is a view of one number, never a second opinion about it: **the
/// components sum to [`LnQuote::fee`] exactly**, and that accessor — with
/// [`LnQuote::total`] — is what a caller charges the user and what execution
/// is bound by. A caller that only shows the total can ignore this type
/// completely; a caller that shows the parts must still take the total from
/// [`LnQuote::fee`] rather than adding these up itself, so that the number on
/// screen is the number the quote committed to even if a later release
/// itemises the same fee more finely.
///
/// Any component may be zero — on [`LightningRoute::Internal`] the gateway
/// one always is, because no gateway takes part. Zero components are
/// reported as zero rather than omitted: a fee line reading "0" is
/// information, and leaving one out would make the sum unverifiable.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LnFeeBreakdown {
    /// The gateway's own charge for carrying the payment out to the lightning
    /// network.
    ///
    /// This is upstream's `OutgoingLightningPayment.fee` and nothing more —
    /// the component people mean when they say "the lightning fee", and the
    /// only one a caller could have discovered for itself. Zero on
    /// [`LightningRoute::Internal`].
    pub gateway: Amount,
    /// The lightning module's fee on the output that funds the payment.
    ///
    /// Charged by the federation for the outgoing-contract output the funding
    /// transaction creates, on an internal route as much as a gateway one.
    pub lightning_module: Amount,
    /// The primary module's fees for assembling the funding transaction: what
    /// it charges on the ecash inputs spent and on the change reissued.
    ///
    /// Grouped into one component rather than split into inputs and change
    /// because the split depends on which notes the implementation happens to
    /// select, which is not a distinction a user can act on.
    pub primary_module: Amount,
    /// Value lost to denominations: the part of the change too small for any
    /// note denomination to represent, which is therefore never reissued.
    ///
    /// Nobody charges it and it appears in no fee schedule, but it leaves the
    /// balance and does not come back, so it belongs in the number a user
    /// approves rather than in a footnote. It is also the one component that
    /// depends on which notes are spent, which is why the quote binds its
    /// note selection: that is what lets [`LnQuote::total`] be exact rather
    /// than a bound.
    pub dust: Amount,
}

/// How a lightning payment is, or was, routed.
///
/// Available from the quote before paying and from the final state
/// afterwards, so an application can both preview and receipt it. The
/// distinction is worth surfacing because it is the difference between a
/// payment that pays a gateway and one that does not — the federation's own
/// transaction costs apply either way, see [`LnQuote::fee`] — and because
/// "this stayed inside the federation" is meaningful privacy information for
/// a user.
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
///
/// This value is a convenience, not the only copy. Everything on it is also
/// persisted as [`LnReceiveDetails`] before [`Lightning::receive`] returns,
/// so an application that dropped it, or that is running again after a
/// restart, re-reads the invoice — and the amounts, the fee and the expiry —
/// from [`Operation::details`](crate::Operation::details) with nothing but
/// the operation's id. The QR code is never lost with the value that first
/// carried it.
#[derive(Debug)]
#[non_exhaustive]
pub struct LnReceive {
    /// The invoice to display, encode as a QR code, or send to the payer.
    ///
    /// Also persisted, and re-readable after a restart, as
    /// [`LnReceiveDetails::invoice`].
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
/// preimage-obtained states onto [`Success`](Self::Success), everything
/// that ends with the funds spendable again onto
/// [`Refunded`](Self::Refunded), and only a refund that itself failed, or
/// an error nobody resolved, onto [`Failed`](Self::Failed). That last line
/// is drawn by what happened to the money, not by what upstream named the
/// state — the distinction this enum's own variants make is that
/// [`Refunded`](Self::Refunded) means the funds are safe and
/// [`Failed`](Self::Failed) means the operation did not resolve into a
/// clean return.
///
/// The preimage those success states carry is normalised on the way
/// through: v1 reports it as a hex string and lnv2 reports it as raw bytes,
/// and both arrive at a caller as one [`Preimage`].
///
/// Four upstream variants fall outside a one-to-one reading of those
/// buckets, and are called out rather than left to be discovered:
///
/// - **`LnPayState::Canceled`.** The payment was called off before the
///   gateway took it on, so the funds never left and no refund was needed.
///   Nothing was paid, so it is not [`Success`](Self::Success); the money is
///   in the balance, which is exactly what [`Refunded`](Self::Refunded)
///   promises, so that is where it lands. There is no `Canceled` variant
///   here: an outgoing payment offers no cancellation to a caller of this
///   SDK (the only cancellation in the crate is
///   [`Operation::request_cancel`](crate::Operation::request_cancel) for
///   out-of-band ecash), so a distinct variant would name something no
///   application could ever have asked for.
/// - **`InternalPayState::FundingFailed`**, and **lnv2's `Failure` when it
///   follows `Funding` directly.** Both are the federation rejecting the
///   funding transaction: the notes were never spent, so nothing left the
///   balance and nothing needs refunding. Safe funds are
///   [`Refunded`](Self::Refunded), as for `Canceled` above, and not
///   [`Failed`](Self::Failed) — the operation did resolve cleanly, into no
///   effect. lnv2 reports this `Failure` and the one below through the same
///   public variant, so the implementation keys on whether the payment had
///   reached [`Funded`](Self::Funded) — a `Failure` before it is a
///   rejection, one after it is a refund that failed — and persists that
///   phase, exactly as [`LnReceiveState`] requires of a receive.
/// - **lnv2's `Failure` after `Refunding`.** The refund transaction was
///   rejected and no preimage turned up either, so the money is neither
///   paid nor back: [`Failed`](Self::Failed), the case that variant exists
///   for.
/// - **lnv2's `SendOperationState::Refunding`.** A refund that is *in
///   progress*, not one that completed — the money is neither paid nor back
///   yet. It is therefore **not final**, and maps onto
///   [`Funded`](Self::Funded), the crate's other non-final in-flight state,
///   rather than onto [`Refunded`](Self::Refunded), which promises the funds
///   are already spendable again. A subscriber sees it as the payment still
///   running and then the terminal [`Refunded`](Self::Refunded) when the
///   refund lands.
///
/// Because these are judgements about which upstream distinctions matter to
/// an application rather than a one-to-one mapping, this variant set is
/// provisional and will be reconciled against the lightning client when this
/// facade is implemented.
///
/// # An obligation this enum places on the implementation
///
/// [`Success`](Self::Success) carries the fee and the route. **Neither is
/// available from the v1 upstream progress stream.** Upstream reports a fee
/// exactly once, synchronously, as the `fee` field of the
/// `OutgoingLightningPayment` returned when the payment is initiated — and
/// that field is only the gateway's cut, not the whole debit
/// ([`LnQuote::fee`] is) — and it does not put the gateway id into the
/// pay-state stream at all. So the SDK must capture both from the quote it
/// executed and carry them forward itself. [`LnSendDetails`] is where they
/// are persisted, in the same write that creates the operation, which is what
/// makes them survive a restart and stay readable for a
/// [`Refunded`](Self::Refunded) or [`Failed`](Self::Failed) payment as well
/// as a successful one. That is a real obligation on whoever implements this
/// facade, and it is precisely why [`LnQuote`] is an executable object rather
/// than a set of numbers to display and discard.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LnSendState {
    /// The payment has been accepted and is being funded.
    Created,
    /// The payment is funded and in flight — handed to the gateway, or
    /// committed internally.
    Funded,
    /// Final: the payee was paid and the [`Preimage`] proves it.
    Success {
        /// The payment preimage. This is the receipt: it proves to anyone
        /// holding the invoice that it was paid.
        ///
        /// It lives here and nowhere else — placement-rule case 2: it comes
        /// into existence at this transition and this state is sticky, so
        /// copying it into [`LnSendDetails`] would duplicate a value that can
        /// never be missed.
        preimage: Preimage,
        /// The aggregate fee actually charged, carried forward from the
        /// executed quote — [`LnQuote::fee`], the whole debit and not only the
        /// gateway's cut.
        ///
        /// Also in [`LnSendDetails::fee`], and the duplication is deliberate:
        /// it is placement-rule case 3, the one case that licenses it. The fee
        /// is announced by this state and by no other — a
        /// [`Refunded`](Self::Refunded) or [`Failed`](Self::Failed) payment
        /// carries none — so the state alone would lose it for exactly the
        /// endings a receipt is most needed for, and the record alone would
        /// break every caller that reads a fee off a successful payment
        /// without a second call. The two never disagree: both are the number
        /// the executed quote committed to, written once.
        fee: Amount,
        /// How the payment was routed, carried forward from the executed
        /// quote.
        ///
        /// Also in [`LnSendDetails::route`], for the same case-3 reason as
        /// the fee above: announced here, absent from every other ending, and
        /// kept by the record so a refunded payment can still say whether it
        /// ever needed a gateway.
        route: LightningRoute,
    },
    /// Final: the payment did not go through and the funds are in the
    /// spendable balance — returned, or never debited.
    ///
    /// This is the ordinary failure of a lightning payment — no route, the
    /// payee went away, the gateway gave up, or the funding transaction was
    /// rejected before anything left — and it is a success from the SDK's
    /// point of view in that the money is safe.
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

/// What an outgoing lightning payment *is*: the invoice it pays and the terms
/// it was executed on.
///
/// The persisted half of an [`LnSendState`] operation, read with
/// [`Operation::details`](crate::Operation::details). Written in the same
/// storage transaction that creates the operation, so a process that dies the
/// instant [`Lightning::send`] returns still finds all of it on the next
/// start.
///
/// # What it is for
///
/// Two things a state stream cannot give a caller:
///
/// - **A receipt for a payment nobody watched.** An operation picked up by id
///   after a restart yields its current state and no history, so the invoice
///   that was paid and the amounts it was paid at exist nowhere else.
/// - **A fee and a route for a payment that did not succeed.**
///   [`LnSendState::Success`] announces them and no other ending carries
///   them, so without this record a refunded or failed send could never say
///   what it would have cost or which gateway had it. A receipt that only
///   exists for successes is not a receipt.
///
/// Everything here is fixed when the quote is executed and never changes
/// afterwards, so there is no `Option` and no field that fills in later:
/// reading this twice gives the same answer, before or after any transition.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LnSendDetails {
    /// The invoice this payment pays, as it was quoted.
    ///
    /// The payee, the payment hash, the description and the expiry all read
    /// back off it, which is what lets a history screen render the payment
    /// without the application having kept the invoice itself.
    pub invoice: Bolt11Invoice,
    /// The invoice's own amount, from [`LnQuote::invoice_amount`]: what the
    /// payee receives when the payment succeeds.
    pub invoice_amount: Amount,
    /// The aggregate fee the executed quote committed to — [`LnQuote::fee`],
    /// every debit that funded the payment and not only the gateway's cut.
    ///
    /// Also on [`LnSendState::Success`], which is placement-rule case 3 and
    /// the one licensed duplication: the fee is announced by that state
    /// alone, so keeping it only there would lose it for the refunded and
    /// failed endings that most need a number to show. Both copies are the
    /// same value from the same quote, written once and never revised.
    pub fee: Amount,
    /// The total the payment was authorised for: [`LnQuote::total`], equal
    /// to [`invoice_amount`](LnSendDetails::invoice_amount) plus
    /// [`fee`](LnSendDetails::fee).
    ///
    /// This is a term, not an outcome. It is what
    /// [`LnSendState::Success`] debited, and what a
    /// [`LnSendState::Refunded`] send either debited and got back or, when
    /// the funding transaction was rejected, never debited at all; the state
    /// says which, this record says how much was at stake.
    ///
    /// Stored rather than left to the caller's arithmetic so that a receipt
    /// screen and the approval screen before it can never disagree about the
    /// number the user said yes to.
    pub total: Amount,
    /// How the payment is routed, from [`LnQuote::route`] — whether it
    /// leaves the federation through a gateway, and which one.
    ///
    /// Duplicated onto [`LnSendState::Success`] for the same case-3 reason as
    /// [`fee`](LnSendDetails::fee), and kept here so that "this stayed inside
    /// the federation" remains answerable for a payment that was refunded.
    pub route: LightningRoute,
    /// When the payment was started, as the SDK recorded it.
    ///
    /// The timestamp to sort and label a history row by. It is the moment
    /// [`Lightning::send`] committed the payment, not the moment it settled;
    /// how far it has got is [`Operation::state`](crate::Operation::state)'s
    /// answer, not this record's.
    pub created_at: Timestamp,
}

impl crate::operation::sealed::Sealed for LnSendDetails {}

impl crate::operation::OperationDetails for LnSendDetails {}

impl crate::operation::DetailedOperationState for LnSendState {
    type Details = LnSendDetails;
}

/// The lifecycle of an incoming lightning payment.
///
/// # Relationship to the upstream state machine
///
/// Upstream v1's `LnReceiveState` is `Created`,
/// `WaitingForPayment { invoice, timeout }`, `Canceled { reason }`,
/// `Funded`, `AwaitingFunds`, `Claimed`. This enum tracks it closely: the
/// invoice and timeout that upstream attaches to `WaitingForPayment` are
/// persisted in [`LnReceiveDetails`] instead, as
/// [`invoice`](LnReceiveDetails::invoice) and
/// [`expires_at`](LnReceiveDetails::expires_at), so
/// [`WaitingForPayment`](Self::WaitingForPayment) carries no payload, and
/// upstream's `AwaitingFunds` is folded into [`Funded`](Self::Funded) —
/// both mean "paid, settling".
///
/// That the record is where they live, rather than the value
/// [`Lightning::receive`] returned, is the point: both are fixed when the
/// invoice is created and neither ever changes, so re-delivering them on
/// every transition would copy the same bytes to every subscriber for
/// nothing (placement-rule case 1) — while a caller that never held the
/// returned [`LnReceive`], or that is running again after a restart, still
/// reads them from the operation's id alone.
///
/// [`Expired`](Self::Expired) and [`Failed`](Self::Failed) are additions, and
/// between them they are why a v1 terminal cancellation cannot be mapped by
/// its upstream variant alone.
///
/// [`Expired`](Self::Expired) exists because an invoice that simply lapses
/// unpaid is the most common way a receive ends and is not a failure worth
/// alarming a user about. v1 has no dedicated variant for it and reports it as
/// a cancellation whose reason is a timeout; lnv2 has an explicit expired
/// state. Splitting it out lets an application render "this invoice expired"
/// as its own outcome.
///
/// [`Failed`](Self::Failed) exists because a receive can get past "nobody
/// paid" and still not end in a credit: a payment can arrive and then fail
/// to become spendable ecash, or the protocol can go wrong on a contract that
/// was funded, unwinding the payment before anyone was paid. Neither is
/// [`Canceled`](Self::Canceled), which this enum reserves for a receive that
/// ended before anything was funded, nor [`Claimed`](Self::Claimed), which
/// would assert the funds are spendable when they are not.
///
/// # The v1 mapping is keyed on the reason, and for one reason on the phase
///
/// v1's cancellation reason is **typed, not free-form**: upstream's
/// `LnReceiveState::Canceled` carries a `LightningReceiveError`, whose variants
/// are `Timeout`, `Rejected`, `ClaimRejected` and `InvalidPreimage`. No string
/// is parsed anywhere in this mapping, and none needs to be.
///
/// Three of the four reasons map on their own. The fourth, `Rejected`, is
/// emitted at two entirely different moments: for the transaction that
/// registers the invoice being refused before anybody paid, and again after
/// a claim had been accepted but the primary-module outputs failed to
/// produce notes. The first means nothing happened; the second means
/// somebody paid and the money did not arrive. For that one reason the
/// mapping key is the pair **(reason, phase the operation had reached)**,
/// where the phase that matters is whether the receive had ever reached
/// [`Funded`](Self::Funded) — that is, whether a payment had been confirmed
/// for it.
///
/// | upstream v1 | phase reached | here |
/// | --- | --- | --- |
/// | `Created` | — | [`Created`](Self::Created) |
/// | `WaitingForPayment { .. }` | — | [`WaitingForPayment`](Self::WaitingForPayment) |
/// | `Funded`, `AwaitingFunds` | — | [`Funded`](Self::Funded) |
/// | `Claimed` | — | [`Claimed`](Self::Claimed) |
/// | `Canceled { Timeout }` | any | [`Expired`](Self::Expired) |
/// | `Canceled { ClaimRejected }` | any | [`Failed`](Self::Failed) |
/// | `Canceled { InvalidPreimage }` | any | [`Failed`](Self::Failed) |
/// | `Canceled { Rejected }` | before [`Funded`](Self::Funded) | [`Canceled`](Self::Canceled) |
/// | `Canceled { Rejected }` | at or after [`Funded`](Self::Funded) | [`Failed`](Self::Failed) |
///
/// Three rules generate the whole table, and they are what an implementation
/// should encode:
///
/// 1. `Timeout` is [`Expired`](Self::Expired). Nobody paid within the
///    invoice's lifetime; that is the benign ending.
/// 2. `ClaimRejected` and `InvalidPreimage` end in [`Failed`](Self::Failed),
///    whatever phase the operation shows. Both presuppose a funded contract
///    — a claim is only attempted for one, and a preimage is only decrypted
///    for one — so neither is a pre-payment cancellation, though their
///    economics differ: after `ClaimRejected` the payment is confirmed and
///    locked to this wallet, while `InvalidPreimage` unwinds it (see the
///    variant). The phase cannot be consulted for them, and must not be:
///    upstream reports its own `Funded` only once the claim transaction has
///    been accepted, so both of these reasons arrive *before* it, on a
///    receive still showing [`WaitingForPayment`](Self::WaitingForPayment).
///    Reading that as a pre-payment cancellation would be exactly wrong.
/// 3. `Rejected` is [`Canceled`](Self::Canceled) on a receive that never got
///    past [`WaitingForPayment`](Self::WaitingForPayment) — a genuine refusal
///    before payment, with nothing owed to anyone — and
///    [`Failed`](Self::Failed) on one that had reached
///    [`Funded`](Self::Funded), where it is the claim's primary outputs
///    failing after a payment was confirmed.
///
/// **Rule 3 obliges the implementation to persist the phase.** The terminal
/// upstream event does not say which moment it belongs to, and after a restart
/// the SDK is not the process that watched the operation, so "did this receive
/// ever reach [`Funded`](Self::Funded)?" must be durable rather than
/// remembered. Without it a post-claim failure and a pre-payment refusal are
/// indistinguishable, which is precisely the bug this mapping fixes. The
/// amounts on [`LnReceiveDetails`] are not a substitute: they answer what the
/// receive was for, not how far it got.
///
/// **Rule 2 obliges the implementation not to give up early.** A rejected
/// claim is not the end of the payment: the contract is funded, still locked
/// to this wallet's key, and the lightning client offers a reclaim for
/// exactly that case. So `ClaimRejected` moves the receive to
/// [`Funded`](Self::Funded) — a payment is confirmed — and the SDK drives the
/// reclaim under the same operation id, whatever operation identity the
/// underlying client gives the attempt, until the receive is
/// [`Claimed`](Self::Claimed) or the reclaim itself fails and
/// [`Failed`](Self::Failed) is the truth. [`Failed`](Self::Failed) is
/// terminal, so it is only emitted once no further claim is possible; an
/// application never sees a still-claimable payment finalised.
///
/// lnv2 needs none of this arbitration, because it draws the distinction
/// itself: its `ReceiveOperationState` has explicit pending and claiming
/// states, mapping onto [`WaitingForPayment`](Self::WaitingForPayment) and
/// [`Funded`](Self::Funded); its claimed state maps onto
/// [`Claimed`](Self::Claimed); its expired state onto
/// [`Expired`](Self::Expired); and `Failure` — the payment confirmed, the
/// ecash issuance failed — onto [`Failed`](Self::Failed) directly.
///
/// Because those splits are judgements rather than a one-to-one mapping,
/// this variant set is provisional and will be reconciled against the
/// lightning client when this facade is implemented.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LnReceiveState {
    /// The invoice is being created and registered with the gateway.
    Created,
    /// The invoice exists and nobody has paid it yet.
    ///
    /// The invoice to show and the expiry to count down to are in
    /// [`LnReceiveDetails`]; this state deliberately carries neither. See the
    /// enum's mapping notes.
    WaitingForPayment,
    /// Someone paid; the funds are being settled into the federation.
    Funded,
    /// Final: the amount is in the spendable balance.
    ///
    /// The amount that landed is [`LnReceiveDetails::net_credit`] — the
    /// invoice's face value less the receive-side fee — not the invoice's face
    /// value.
    Claimed,
    /// Final: the receive was called off before anything was funded — for
    /// example because the gateway withdrew the offer.
    Canceled {
        /// Human-readable explanation. Diagnostic only — not a stable
        /// contract, and not something to match on.
        reason: String,
    },
    /// Final: the invoice's expiry passed without it being paid.
    Expired,
    /// Final: the receive got past "nobody paid" and no ecash was issued for
    /// it.
    ///
    /// This is the one genuinely bad outcome of a receive, and it is not the
    /// ordinary "nobody paid" ending. The amount is **not in the balance**
    /// and will not arrive by waiting, and no further claim is possible —
    /// the implementation exhausts the underlying client's reclaim before
    /// emitting this (see the mapping notes). It covers two economies, and
    /// the application cannot tell them apart from this state:
    ///
    /// - **A payment was confirmed and did not become spendable notes.** The
    ///   payer is out of pocket, the funds exist somewhere between the payer
    ///   and this wallet, and recovering them is an operator's job.
    /// - **The protocol went wrong on a funded contract and unwound the
    ///   payment.** An invalid preimage is the case: the gateway takes its
    ///   funding back and fails the payer's payment, so nobody was paid and
    ///   nothing is owed — but it is a protocol anomaly, not a lapse, and
    ///   it is not [`Canceled`](Self::Canceled).
    ///
    /// Unlike [`Expired`](Self::Expired) and [`Canceled`](Self::Canceled),
    /// where nothing was ever funded, this warrants attention. Render it as an
    /// error the user should report, not as an expired invoice. Deliberately
    /// payload-free; see the enum's mapping notes for why, and for how v1 and
    /// lnv2 reach (or do not reach) this state.
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

/// What an incoming lightning payment *is*: the invoice that was issued and
/// the terms it was issued on.
///
/// The persisted half of an [`LnReceiveState`] operation, read with
/// [`Operation::details`](crate::Operation::details). Written in the same
/// storage transaction that creates the operation, so it is there however
/// soon after [`Lightning::receive`] the process dies.
///
/// # The invoice is the reason this record exists
///
/// A receive screen is a QR code and a countdown, and without this record
/// neither survives a restart. The invoice would exist only in the
/// [`LnReceive`] the facade call returned; a subscription cannot recover it,
/// because a subscription is not a replay and the invoice was never a state to
/// begin with. An application killed while an invoice was pending would have
/// to tell the user to start again — about an invoice that is still live and
/// still payable. [`invoice`](LnReceiveDetails::invoice) and
/// [`expires_at`](LnReceiveDetails::expires_at) are what let it re-display
/// the same QR code and resume the same countdown from the operation's id.
///
/// # Which amount is which
///
/// Three amounts, and the convention that relates them is fixed:
/// **the fee is deducted from the invoice, not added on top of it.** The
/// invoice's face value is exactly what the application asked
/// [`Lightning::receive`] for, so the payer is asked for the number the
/// application chose, and the receive-side fee — the gateway's charge plus
/// the federation's own — comes out of it. Hence
/// [`requested_amount`](LnReceiveDetails::requested_amount) `==`
/// [`invoice_amount`](LnReceiveDetails::invoice_amount), and
/// [`net_credit`](LnReceiveDetails::net_credit) is the smaller number that
/// actually lands in the balance.
///
/// The invariant holds whichever way a future module has to account for it:
///
/// ```text
/// invoice_amount == net_credit + fee
/// ```
///
/// All three are recorded rather than two and a subtraction, so the
/// convention is *observable* rather than assumed. A caller can render "you
/// asked for X, the payer pays Y, you receive Z" without knowing which module
/// generation served the request, and a build that could only add the fee on
/// top would still be reporting the face value the payer will really be asked
/// for instead of a number that quietly disagrees with the invoice.
///
/// Everything here is fixed when the invoice is created and never changes, so
/// there is no field that fills in later; reading it twice gives the same
/// answer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LnReceiveDetails {
    /// The invoice that was issued: the QR code to re-display after a
    /// restart, and the same value [`LnReceive::invoice`] returned.
    pub invoice: Bolt11Invoice,
    /// The description embedded in the invoice, as it was passed to
    /// [`Lightning::receive`].
    ///
    /// Kept as its own field rather than read back off the invoice because an
    /// invoice may carry a *hash* of its description instead of the text, in
    /// which case
    /// [`Bolt11Invoice::description`](crate::Bolt11Invoice::description) has
    /// nothing to return and the words the payer was shown would otherwise be
    /// gone. This is what a history row labels the receive with.
    pub description: String,
    /// The amount asked of [`Lightning::receive`].
    ///
    /// Recorded as it was requested, so the record can be checked against
    /// what the application intended rather than only against what the
    /// invoice says.
    pub requested_amount: Amount,
    /// The invoice's face value: what the payer is asked to pay.
    ///
    /// Equal to [`requested_amount`](LnReceiveDetails::requested_amount)
    /// under the deduct-the-fee convention described on this type, and always
    /// equal to [`net_credit`](LnReceiveDetails::net_credit) plus
    /// [`fee`](LnReceiveDetails::fee).
    pub invoice_amount: Amount,
    /// The receive-side fee: the gateway's charge for taking the payment in,
    /// plus what the federation charges to issue the ecash for it.
    ///
    /// The aggregate, on the same footing as [`LnQuote::fee`] on the sending
    /// side: it is the whole difference between what the payer pays and what
    /// lands, so no other deduction appears later. Zero is possible — a
    /// payment that never left the federation has no gateway to pay — but is
    /// not the rule, since issuing the notes is itself a federation
    /// transaction.
    pub fee: Amount,
    /// What actually lands in the spendable balance:
    /// [`invoice_amount`](LnReceiveDetails::invoice_amount) minus
    /// [`fee`](LnReceiveDetails::fee).
    ///
    /// The number to show as "you will receive". Stored rather than derived
    /// so that the receive screen and the history row can never disagree
    /// about it.
    pub net_credit: Amount,
    /// The gateway that agreed to take the payment in, if there was one.
    ///
    /// `None` means no gateway took part — not that the gateway is unknown.
    /// Fixed when the invoice is created, like everything else here, so it
    /// never turns from `None` into a gateway id later.
    pub gateway_id: Option<GatewayId>,
    /// When the invoice stops being payable.
    ///
    /// The other half of what a reattached receive screen needs: this is the
    /// countdown to render beside the QR code, and the moment after which
    /// [`LnReceiveState::Expired`] is the ending to expect.
    pub expires_at: Timestamp,
    /// When the receive was started, as the SDK recorded it.
    ///
    /// The timestamp to sort and label a history row by; the invoice's own
    /// lifetime is [`expires_at`](LnReceiveDetails::expires_at).
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
/// verified gateway, the aggregate fee and its components, and the
/// configuration context they were computed against. Held by value rather
/// than behind an `Arc`, because a quote is owned by one caller and consumed
/// once, never shared.
#[derive(Debug)]
struct LnQuoteInner;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::DetailedOperationState;

    /// A send record with the numbers of one plausible payment: 100,000 msat
    /// to the payee, 1,050 msat of aggregate fee, 101,050 msat debited.
    fn send_details() -> LnSendDetails {
        LnSendDetails {
            invoice: Bolt11Invoice::from_raw("lnbcrt1000n1pexample".to_owned()),
            invoice_amount: Amount::from_msats(100_000),
            fee: Amount::from_msats(1_050),
            total: Amount::from_msats(101_050),
            route: LightningRoute::Gateway {
                gateway_id: GatewayId::from_raw("0266e4598d1d3c415f572a8488830b".to_owned()),
            },
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    /// A receive record for an invoice of 50,000 msat with a 500 msat
    /// receive-side fee, under the convention this crate fixes: the payer is
    /// asked for exactly what was requested and the fee comes out of it.
    fn receive_details() -> LnReceiveDetails {
        LnReceiveDetails {
            invoice: Bolt11Invoice::from_raw("lnbcrt500n1pexample".to_owned()),
            description: "coffee".to_owned(),
            requested_amount: Amount::from_msats(50_000),
            invoice_amount: Amount::from_msats(50_000),
            fee: Amount::from_msats(500),
            net_credit: Amount::from_msats(49_500),
            gateway_id: Some(GatewayId::from_raw(
                "0266e4598d1d3c415f572a8488830b".to_owned(),
            )),
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
        // The review's point: a refunded send carries no fee and no route on
        // its state, and the record is what keeps both readable.
        let details = send_details();
        let state = LnSendState::Refunded;
        assert!(state.is_final());
        assert_eq!(details.fee, Amount::from_msats(1_050));
        assert_eq!(
            details.route,
            LightningRoute::Gateway {
                gateway_id: GatewayId::from_raw("0266e4598d1d3c415f572a8488830b".to_owned()),
            },
        );
    }

    #[test]
    fn ln_send_details_and_success_agree_on_the_fee_and_route() {
        // The one licensed duplication: two copies of the same value from the
        // same quote, never two different numbers.
        let details = send_details();
        let state = LnSendState::Success {
            preimage: Preimage::from_raw(
                "0000000000000000000000000000000000000000000000000000000000000000".to_owned(),
            ),
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
        // The reattachment fix: the QR code and the countdown come from the
        // record, not from the state, which carries neither.
        let details = receive_details();
        assert!(!LnReceiveState::WaitingForPayment.is_final());
        assert_eq!(
            details.invoice,
            Bolt11Invoice::from_raw("lnbcrt500n1pexample".to_owned()),
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
        // No gateway means no gateway fee — it does not mean no fee.
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
                preimage: Preimage::from_raw(
                    "0000000000000000000000000000000000000000000000000000000000000000".to_owned(),
                ),
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
