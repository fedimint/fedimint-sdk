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
///
/// # Lightning is unavailable on a Testnet4 federation
///
/// A federation on [`Network::Testnet4`](crate::Network::Testnet4) offers no
/// lightning at all: [`Federation::lightning`](crate::Federation::lightning)
/// returns `None` for it, and
/// [`Capabilities::lightning`](crate::Capabilities::lightning) is `false`.
/// This is an **absent capability, not a broken one** — the same shape as a
/// federation that simply has no lightning module, and a value to branch on
/// rather than a failure to provoke.
///
/// The reason is upstream, and it is not something this SDK can paper over:
///
/// - **BOLT11 has no Testnet4 currency.** An invoice carries a currency
///   prefix, and the one for a test network is `tb` — shared by testnet3 and
///   testnet4. The invoice format itself cannot tell the two apart, so there
///   is no such thing as "a Testnet4 invoice" to build or to recognise.
/// - **The pinned `lightning-invoice` maps Testnet4 to Regtest.** Its
///   `Network -> Currency` conversion has no Testnet4 case: it trips a debug
///   assertion and falls back to the *regtest* currency, while the reverse
///   conversion turns the test currency back into testnet3. The mapping is
///   lossy in both directions, and in a release build the fallback is silent.
/// - **Fedimint 0.12 relies on those conversions.** lnv1 uses
///   `Network -> Currency` to compare an invoice against the federation and to
///   build the invoices it issues; lnv2 compares the federation's network
///   against `invoice.currency().into()`. On a Testnet4 federation both
///   therefore reject a perfectly valid `tb` invoice — and an invoice issued
///   for such a federation would be minted with the wrong currency.
///
/// Until upstream can distinguish testnet3 from testnet4, an invoice for a
/// Testnet4 federation can be neither built nor matched reliably, and
/// silently paying against a mismatched currency is worse than not offering
/// the facility. Withholding the facade is the honest outcome: an application
/// renders "this federation does not support lightning" once, instead of
/// discovering a currency mismatch mid-payment. When the upstream fixes land
/// (or are backported), the capability appears with no change to this API.
///
/// Every other network — mainnet, testnet3, signet, regtest — is unaffected,
/// and the check that keeps a wrong-network invoice out of a payment is
/// described on [`Lightning::quote`].
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
    /// [`Lightning::send`], which executes that plan or refuses it.
    ///
    /// The fee and total it names are **quoted** figures — what the payment
    /// is expected to cost, and what the user approves — not a bound on what
    /// will be debited. [`LnQuote::total`] explains where the gap comes from
    /// and why this SDK cannot close it; what the payment actually cost is
    /// reported afterwards on [`LnSendDetails::realized_total_debited`] and
    /// [`LnSendDetails::realized_fee`].
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
    /// # This is where the currency class is checked
    ///
    /// A bolt11 invoice does not name a [`Network`](crate::Network). It names
    /// a **currency**, and the two are not in one-to-one correspondence: the
    /// test currency `tb` covers testnet3 and testnet4 alike, and the
    /// conversions between the two vocabularies lose information in both
    /// directions. So the check made here is the one the invoice can actually
    /// support — the invoice's currency class against the class the
    /// federation's network belongs to
    /// ([`Federation::network`](crate::Federation::network)) — and a
    /// disagreement fails with
    /// [`NetworkMismatch`](crate::ErrorCode::NetworkMismatch).
    ///
    /// The classes are mainnet (`bc`), test (`tb`), signet (`tbs`) and regtest
    /// (`bcrt`). A mainnet invoice against a signet federation, or a regtest
    /// invoice against mainnet, is refused here and cannot reach
    /// [`Lightning::send`].
    ///
    /// **This call does not claim to identify the invoice's exact network,
    /// and no caller should read it as doing so.** Testnet3 and testnet4 share
    /// one currency, so an invoice in the test class names no single network,
    /// and asserting one would be an invention. The structured
    /// [`ErrorDetails`](crate::ErrorDetails) accompanying the failure
    /// therefore reports the federation's network — which *is* known exactly —
    /// beside the invoice's currency class, rather than pretending to a
    /// precise `Network` for the invoice; that reshaping is why the detail is
    /// described here in prose instead of by its fields. Lightning is not
    /// offered on a Testnet4 federation at all (see [`Lightning`]), so the
    /// class check never has to arbitrate between the two testnets on the
    /// federation's side either.
    ///
    /// Quoting is the deterministic place for that check: every payment
    /// passes through it, it happens before anything is committed, and the
    /// answer does not depend on which gateway is picked or which module
    /// generation serves the payment. lnv2 has its own `WrongCurrency`
    /// failure downstream of here, but relying on it would mean discovering
    /// the mismatch mid-payment on one generation and not at all on the
    /// other. A syntactically valid invoice for an incompatible currency
    /// therefore cannot survive quoting and reach [`Lightning::send`].
    ///
    /// # Errors
    ///
    /// [`AmountlessInvoice`](crate::ErrorCode::AmountlessInvoice) for an
    /// invoice that names no amount,
    /// [`NetworkMismatch`](crate::ErrorCode::NetworkMismatch) for an invoice
    /// whose currency class is incompatible with the federation's network,
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for an invoice that
    /// has already expired,
    /// [`GatewayUnavailable`](crate::ErrorCode::GatewayUnavailable) when no
    /// gateway can be selected and verified,
    /// [`InsufficientBalance`](crate::ErrorCode::InsufficientBalance) when
    /// the balance cannot cover the quoted [`LnQuote::total`],
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
    /// payment. Execution follows the plan — same invoice, same amount, same
    /// route — or it does not happen:
    /// [`QuoteExpired`](crate::ErrorCode::QuoteExpired) if the quote's
    /// validity window has passed,
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) if something the
    /// quote depends on moved underneath it (the gateway withdrew, its fee
    /// changed, the federation configuration was updated). Both mean the
    /// same thing to a caller: quote again and re-confirm with the user. That
    /// refusal is worth having: a user is not charged against terms they never
    /// saw.
    ///
    /// That refusal is **not** a spending ceiling, though an earlier draft of
    /// this documentation said it was. [`LnQuote::total`] is the debit this
    /// payment was quoted at, not a maximum this call is authorised against:
    /// published Fedimint offers no way to bind a total inside the funding
    /// transaction that finally commits, so the realized debit can land above
    /// the quoted one and refusing a visibly stale quote does not stop it.
    /// [`LnQuote::total`] gives the mechanism in full and names what upstream
    /// would have to add before a ceiling could be promised.
    ///
    /// The returned operation tracks the payment from funding to preimage;
    /// a payment that fails ends in a final state, not an error from this
    /// call.
    ///
    /// The terms this call executed on are persisted as
    /// [`LnSendDetails`] before it returns, so the invoice, the amounts, the
    /// quoted fee and the route stay readable from
    /// [`Operation::details`](crate::Operation::details) for the life of the
    /// operation — after a restart, and however the payment ends. What the
    /// balance *actually* did is added to that same record as the payment
    /// settles — [`LnSendDetails::realized_total_debited`] is what a "you
    /// paid" line must read from — and the two halves are described on
    /// [`LnSendDetails`], where for a refunded payment they differ.
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
    /// that convention exactly and records each of the numbers involved.
    ///
    /// The fee quoted here is an **estimate of what a later claim will cost**,
    /// not a commitment. Nobody has paid yet, and the federation-side fees are
    /// chosen only when the claiming transaction is actually assembled and
    /// accepted, against the note inventory at that future time — 0.12's
    /// receive fee quote is an explicitly non-committable dry run. So
    /// [`LnReceiveDetails`] keeps the quoted terms and the realized outcome
    /// apart, and an invoice that expires unpaid realizes no credit at all.
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
/// report the fee the payment was executed on, and what
/// [`Lightning::send`] persists into [`LnSendDetails`] so that the quoted fee
/// and the route survive a restart and remain readable however the payment
/// ends.
///
/// Everything here is a **quoted** term: the numbers the user approved,
/// fixed when this quote was executed, and a prediction of what the payment
/// will cost rather than a measurement of what it did — see the
/// quoted-versus-realized split described on [`LnSendDetails`] and, for the
/// reason the distinction cannot be engineered away, [`LnQuote::total`]. The
/// realized counterparts live on [`LnSendDetails`].
///
/// What an executed quote *does* bind is the plan: the invoice, the amount,
/// and the gateway the payment is routed through. What it cannot bind is the
/// debit.
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

    /// The quoted aggregate cost of this payment, over and above
    /// [`LnQuote::invoice_amount`].
    ///
    /// **Every debit that funding this payment incurs is accounted for in
    /// this one number**, not just the gateway's cut. Paying an invoice out of ecash
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
    ///
    /// "Quoted" is the other half. The gateway's share is refetched when the
    /// payment is sent, and the mint-side components are chosen when the
    /// funding transaction is assembled — which happens after this quote has
    /// been discarded — so this is the figure the user approves and not a
    /// measurement of what the payment cost; see [`LnQuote::total`] for the
    /// mechanism. What it actually cost is [`LnSendDetails::realized_fee`],
    /// recorded as the payment settles, and it can land on either side of
    /// this number. That is the figure a receipt shows.
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

    /// The total this payment is quoted at: [`LnQuote::invoice_amount`] plus
    /// [`LnQuote::fee`].
    ///
    /// This is the number to show as "you will pay" on an approval screen, and
    /// it is exact in the sense that matters there — the point of aggregating
    /// the fee in millisatoshis is that the figure the user says yes to does
    /// not have to be approximated.
    ///
    /// # It is an estimate, not an enforced ceiling
    ///
    /// An earlier draft of this API called this a ceiling that
    /// [`Lightning::send`] was authorised against and could not exceed, with
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged) as the enforcement.
    /// That claim is **retracted**: published Fedimint cannot enforce it, and
    /// no amount of care inside this SDK can supply the enforcement from
    /// outside. The mechanism is worth stating precisely, because its shape is
    /// what decides whether it can be worked around.
    ///
    /// - **The gateway's terms are refetched during the send, not carried
    ///   from the quote.** 0.12's lnv2 send path asks the gateway for its
    ///   routing info again as part of paying, and pays on whatever comes
    ///   back. Nothing in that path takes the figure this quote agreed as an
    ///   argument, so nothing there can compare against it.
    /// - **Funding is finalized without any caller-provided maximum.**
    ///   Assembling the funding transaction takes no expected-total or
    ///   maximum-total argument on either module generation. The mint input
    ///   fees, the output fee on the contract, the change fees and the
    ///   denomination dust — every component
    ///   [`LnQuote::fee_breakdown`] itemises except the gateway's — are chosen
    ///   at that moment, and can differ from the ones this quote implied.
    /// - **Re-checking the terms immediately before submitting does not close
    ///   the gap, it only narrows the window.** Between the check and the
    ///   commit the gateway's terms and the note inventory can still move — a
    ///   time-of-check to time-of-use race — and a check that leaves a race is
    ///   not a guarantee. Saying so is more useful than implying one.
    ///
    /// So the realized debit can land **above** this figure as well as below
    /// it. Neither direction is an error, and neither is a broken promise,
    /// because no promise of a maximum is being made.
    ///
    /// What would turn this into a real ceiling is an upstream change, not an
    /// SDK one: either an atomic maximum-total (or expected-fee) guard
    /// *inside* funding finalization, so that assembly itself refuses to
    /// exceed a figure the caller named, or a persisted reservation of the
    /// gateway's terms and the notes with defined drop, expiry and restart
    /// semantics, so that the terms quoted are the terms held. Either is a
    /// prerequisite this API is documenting rather than pretending to have.
    ///
    /// # What a caller gets instead
    ///
    /// Two things, and between them they cover the honest cases.
    ///
    /// [`Lightning::send`] still refuses a quote whose terms have visibly
    /// moved — [`QuoteExpired`](crate::ErrorCode::QuoteExpired) once the
    /// validity window has passed, and
    /// [`QuoteChanged`](crate::ErrorCode::QuoteChanged), with
    /// [`ErrorDetails::QuoteTermsChanged`](crate::ErrorDetails::QuoteTermsChanged)
    /// naming this total beside the one the payment would now cost, when the
    /// gateway or the federation configuration has moved underneath it. So a
    /// stale quote is never executed silently. That is a genuine protection
    /// against staleness; it is not a bound on the commit. The plan itself
    /// *is* bound: the invoice, the amount and the gateway are the ones that
    /// were shown.
    ///
    /// And the receipt reports the truth.
    /// [`LnSendDetails::realized_total_debited`] is what the balance actually
    /// paid, recorded from the accepted funding transaction's own fees, and it
    /// is what a "you paid" line must read from. A caller that renders this
    /// quoted total after the fact will eventually render a number that is not
    /// what happened.
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
/// [`LnQuote::total`] — is what a caller charges the user. A caller that only
/// shows the total can ignore this type completely; a caller that shows the
/// parts must still take the total from [`LnQuote::fee`] rather than adding
/// these up itself, so that the number on screen is the number the quote
/// named even if a later release itemises the same fee more finely.
///
/// # This explains a quote, not an outcome
///
/// These are quoted components, and they inherit everything
/// [`LnQuote::total`] says about quoted figures: the gateway's share is
/// refetched during the send and every other line is re-decided when the
/// funding transaction is assembled. A payment's realized cost is reported as
/// a single aggregate on [`LnSendDetails::realized_fee`] and is deliberately
/// not broken down this way — splitting the accepted transactions' cost along
/// these lines after the fact would be presenting a guess as a measurement.
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
    /// approves rather than in a footnote. Like the two module components
    /// beside it, it depends on the notes the funding transaction actually
    /// spends, which are selected long after this quote was built — one of the
    /// reasons [`LnQuote::total`] is a prediction and not a bound.
    pub dust: Amount,
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
///
/// This value is a convenience, not the only copy. Everything on it is also
/// persisted as [`LnReceiveDetails`] before [`Lightning::receive`] returns,
/// so an application that dropped it, or that is running again after a
/// restart, re-reads the invoice — and the amounts, the quoted fee and the
/// expiry — from [`Operation::details`](crate::Operation::details) with nothing
/// but the operation's id. The QR code is never lost with the value that first
/// carried it, and what the payment finally credited is added to that same
/// record when it settles.
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
/// preimage-obtained states onto [`Success`](Self::Success), all the
/// refund-completed states onto [`Refunded`](Self::Refunded), and the
/// error and refund-failure states onto [`Failed`](Self::Failed).
///
/// The preimage those success states carry is normalised on the way
/// through: v1 reports it as a hex string and lnv2 reports it as raw bytes,
/// and both arrive at a caller as one [`Preimage`].
///
/// Two upstream variants fall outside those four buckets, and are called
/// out rather than left to be discovered:
///
/// - **`LnPayState::Canceled`.** The payment was called off before the
///   gateway took it on, so the funds never left and no refund was needed.
///   Nothing was paid, so it is not [`Success`](Self::Success); the money is
///   in the balance, which is exactly what [`Refunded`](Self::Refunded)
///   promises, so that is where it lands — and the realized fields of
///   [`LnSendDetails`] are what distinguish it from a refund that cost
///   something, since an attempt that never assembled a funding transaction
///   moved nothing at all. There is no `Canceled` variant
///   here: an outgoing payment offers no cancellation to a caller of this
///   SDK (the only cancellation in the crate is
///   [`Operation::request_cancel`](crate::Operation::request_cancel) for
///   out-of-band ecash), so a distinct variant would name something no
///   application could ever have asked for.
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
/// # Two obligations this enum places on the implementation
///
/// **The quoted terms must be carried forward.**
/// [`Success`](Self::Success) carries the quoted fee and the route, and
/// **neither is available from the v1 upstream progress stream.** Upstream
/// reports a fee exactly once, synchronously, as the `fee` field of the
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
///
/// **What actually moved must be recorded as it moves.** A quoted fee is not
/// a realized one: fedimint chooses a payment's federation-side fees — the
/// mint input and output fees, the change, the denomination dust — only when
/// a transaction is assembled and accepted, so the real figures exist only
/// afterwards. Worse for a receipt, a [`Refunded`](Self::Refunded) payment
/// executes a *second* transaction to claim the funding back, which costs
/// money of its own, and no upstream state reports what either transaction
/// charged. The SDK must therefore record, from the accepted transactions it
/// submitted, what actually left the balance, what actually came back, and the
/// difference that is gone for good — the three `realized_` fields of
/// [`LnSendDetails`]. Without them a refunded send cannot be reconciled
/// against the balance at all.
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
        /// The aggregate fee the payment was **quoted at** —
        /// [`LnQuote::fee`], every component of the debit and not only the
        /// gateway's cut.
        ///
        /// This is the number the user approved, not a measurement of what the
        /// balance did. The two can differ in either direction: the gateway's
        /// terms are refetched during the send and the federation's share is
        /// fixed only when the funding transaction is accepted, so the real
        /// cost of this payment is [`LnSendDetails::realized_fee`], which is
        /// `Some` by the time this state is reached. A receipt that wants one
        /// number should show that one; a receipt that wants to explain itself
        /// shows both.
        ///
        /// Also in [`LnSendDetails::quoted_fee`], and the duplication is
        /// deliberate: it is placement-rule case 3, the one case that licenses
        /// it. The quoted fee is announced by this state and by no other — a
        /// [`Refunded`](Self::Refunded) or [`Failed`](Self::Failed) payment
        /// carries none — so the state alone would lose it for exactly the
        /// endings a receipt is most needed for, and the record alone would
        /// break every caller that reads a fee off a successful payment
        /// without a second call. The two copies never disagree: both are the
        /// number the executed quote committed to, written once.
        quoted_fee: Amount,
        /// How the payment was routed, carried forward from the executed
        /// quote.
        ///
        /// Also in [`LnSendDetails::route`], for the same case-3 reason as
        /// the quoted fee above: announced here, absent from every other
        /// ending, and kept by the record so a refunded payment can still say
        /// whether it ever needed a gateway.
        route: LightningRoute,
    },
    /// Final: the payment did not go through and the funding has been claimed
    /// back into the spendable balance.
    ///
    /// This is the ordinary failure of a lightning payment — no route, the
    /// payee went away, the gateway gave up — and it is a success from the
    /// SDK's point of view in that the money is safe.
    ///
    /// **Not all of it comes back, and the shortfall is not zero.** The refund
    /// is itself a federation transaction: it spends the funding contract as an
    /// input and reissues notes as outputs, paying the primary module's input,
    /// output and change fees and losing whatever will not fit a note
    /// denomination. The gateway's cut may well return with the contract while
    /// the fees that funded the attempt stay sunk. So the honest receipt for
    /// this ending is three numbers, all on [`LnSendDetails`]:
    /// [`realized_total_debited`](LnSendDetails::realized_total_debited) left,
    /// [`restored_amount`](LnSendDetails::restored_amount) came back, and
    /// [`realized_fee`](LnSendDetails::realized_fee) is the difference that
    /// did not. Reporting "refunded" alone would leave a user's balance
    /// quietly disagreeing with the story on screen.
    Refunded,
    /// Final: the payment failed in a way that did not resolve into a
    /// clean refund.
    ///
    /// This is the one ending whose money story may be genuinely
    /// unestablishable: the funding may have been accepted and the refund may
    /// not have completed, so [`LnSendDetails::restored_amount`] and
    /// [`LnSendDetails::realized_fee`] can stay `None` here — absent because
    /// nobody knows, never zero to make a receipt look tidy.
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

/// What an outgoing lightning payment *is*: the invoice it pays, the terms it
/// was executed on, and — once they are known — what the balance actually did.
///
/// The persisted half of an [`LnSendState`] operation, read with
/// [`Operation::details`](crate::Operation::details). The quoted terms are
/// written in the same storage transaction that creates the operation, so a
/// process that dies the instant [`Lightning::send`] returns still finds them
/// on the next start; the realized figures are added as the transactions that
/// establish them are accepted.
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
/// # Quoted terms and realized outcome are different halves
///
/// This record has two halves, and conflating them is what makes a refund
/// impossible to reconcile:
///
/// - **The quoted half** — [`invoice_amount`](LnSendDetails::invoice_amount),
///   [`quoted_fee`](LnSendDetails::quoted_fee),
///   [`quoted_total_debited`](LnSendDetails::quoted_total_debited),
///   [`route`](LnSendDetails::route) — is fixed when the quote is executed and
///   never changes. It is what the user approved, and it describes the
///   *attempt*. Plain fields, always readable, from the moment the operation
///   exists.
/// - **The realized half** —
///   [`realized_total_debited`](LnSendDetails::realized_total_debited),
///   [`restored_amount`](LnSendDetails::restored_amount),
///   [`realized_fee`](LnSendDetails::realized_fee) — is what the balance
///   actually did. `Option`, absent until the fact is established by an
///   accepted federation transaction, then written once and never revised.
///
/// They can differ — in either direction, and for anything but a plain
/// success they usually do. Fedimint refetches the gateway's terms during the
/// send rather than honouring the ones the quote agreed, and it fixes a
/// payment's federation-side fees — the mint's input and output fees, the
/// change it reissues, the value too small for any note denomination — only
/// when a transaction is assembled and accepted; a fee quote is an explicitly
/// non-committable dry run, and funding is finalized without any
/// caller-provided maximum. [`LnQuote::total`] sets out that mechanism in full
/// and retracts the claim that the quoted total was a ceiling. And a
/// [`Refunded`](LnSendState::Refunded) send runs a *second* transaction to
/// claim its funding back, which costs money of its own: the gateway's cut may
/// come back with the contract while the fees that funded the attempt stay
/// sunk.
///
/// ## The identity that reconciles a receipt with the balance
///
/// Whenever all three realized figures are present:
///
/// ```text
/// realized_total_debited == delivered + restored_amount + realized_fee
/// ```
///
/// where `delivered` is [`invoice_amount`](LnSendDetails::invoice_amount) for
/// a payment that reached [`Success`](LnSendState::Success) and zero for one
/// that was [`Refunded`](LnSendState::Refunded). Read the other way round:
/// `realized_fee` is every millisatoshi that left the balance and neither
/// reached the payee nor came back — the sunk cost, and for a refund the whole
/// cost.
///
/// ## When each realized field fills in
///
/// | state | `realized_total_debited` | `restored_amount` | `realized_fee` |
/// | --- | --- | --- | --- |
/// | [`Created`](LnSendState::Created) | `None` | `None` | `None` |
/// | [`Funded`](LnSendState::Funded) | `Some` — the funding transaction was accepted | `None` | `None` |
/// | [`Success`](LnSendState::Success) | `Some` | `Some(0)` — nothing came back | `Some` — `realized_total_debited` less the invoice amount |
/// | [`Refunded`](LnSendState::Refunded) | `Some`, or `Some(0)` if funding never happened | `Some` — what the claim actually restored | `Some` — the difference, which is sunk |
/// | [`Failed`](LnSendState::Failed) | `Some` if funding was accepted, else `None` | `None` — it did not resolve | `None` — not establishable |
///
/// `None` always means "not established", never "lost" and never "zero":
/// `Some(0)` is a measurement and `None` is the absence of one, and the two
/// must not be rendered the same way. Each field goes from `None` to `Some`
/// exactly once, in the same write that records the transition establishing
/// it, so this record can be read at any time without ordering it against
/// [`Operation::state`](crate::Operation::state) and can never produce two
/// contradictory receipts.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LnSendDetails {
    /// The invoice this payment pays, as it was quoted.
    ///
    /// The payee, the payment hash, the description and the expiry all read
    /// back off it, which is what lets a history screen render the payment
    /// without the application having kept the invoice itself.
    pub invoice: Bolt11Invoice,
    /// Quoted half. What reaches the payee if the payment succeeds: the
    /// invoice's own amount, from [`LnQuote::invoice_amount`].
    ///
    /// Exact rather than estimated — an invoice names its own amount — but
    /// still a term of the attempt: a payment that was refunded delivered
    /// nothing, and this number says what it would have delivered.
    pub invoice_amount: Amount,
    /// Quoted half. The aggregate fee the executed quote named —
    /// [`LnQuote::fee`], every component of the cost of funding the payment
    /// and not only the gateway's cut.
    ///
    /// The number the user approved. It is **not** a measurement of what the
    /// payment cost: that is [`realized_fee`](LnSendDetails::realized_fee),
    /// and the two differ, in either direction, whenever the gateway's
    /// refetched terms or the mint's assembly-time components came out other
    /// than quoted, or the payment was refunded.
    ///
    /// Also on [`LnSendState::Success`], which is placement-rule case 3 and
    /// the one licensed duplication: the quoted fee is announced by that state
    /// alone, so keeping it only there would lose it for the refunded and
    /// failed endings that most need a number to show. Both copies are the
    /// same value from the same quote, written once and never revised.
    pub quoted_fee: Amount,
    /// Quoted half. What the payment was quoted to debit from the balance:
    /// [`LnQuote::total`], equal to
    /// [`invoice_amount`](LnSendDetails::invoice_amount) plus
    /// [`quoted_fee`](LnSendDetails::quoted_fee).
    ///
    /// It is an estimate, not a ceiling — [`LnQuote::total`] retracts that
    /// claim and explains why it cannot be made — so it answers "what did you
    /// agree to", never "what did you pay". The figure that says what actually
    /// left is
    /// [`realized_total_debited`](LnSendDetails::realized_total_debited) — the
    /// same noun with the other prefix, and the distinction a refund makes
    /// unavoidable.
    ///
    /// Stored rather than left to the caller's arithmetic so that a receipt
    /// screen and the approval screen before it can never disagree about the
    /// number the user said yes to.
    pub quoted_total_debited: Amount,
    /// Quoted half. How the payment was routed, from [`LnQuote::route`] —
    /// whether it left the federation through a gateway, and which one.
    ///
    /// Duplicated onto [`LnSendState::Success`] for the same case-3 reason as
    /// [`quoted_fee`](LnSendDetails::quoted_fee), and kept here so that "this
    /// stayed inside the federation" remains answerable for a payment that was
    /// refunded.
    pub route: LightningRoute,
    /// Realized half. What actually left the spendable balance to fund this
    /// payment, as the accepted funding transaction charged it.
    ///
    /// `None` until that transaction is accepted — the transition to
    /// [`Funded`](LnSendState::Funded) — because before then no such figure
    /// exists: the gateway's terms are refetched during the send, and the
    /// federation's fees and the dust depend on the notes actually spent.
    ///
    /// It can differ from
    /// [`quoted_total_debited`](LnSendDetails::quoted_total_debited) in
    /// **either** direction, and an earlier draft of this field promised it
    /// could only come in at or under. It could not: funding is finalized
    /// without any caller-provided maximum, so nothing enforces that bound;
    /// [`LnQuote::total`] gives the mechanism. This is the figure a "you paid"
    /// line must read from.
    ///
    /// `Some(0)` is possible and means something precise: the attempt ended
    /// without a funding transaction ever being assembled, so nothing moved.
    pub realized_total_debited: Option<Amount>,
    /// Realized half. What the refund actually put back into the spendable
    /// balance, net of the refund transaction's own cost.
    ///
    /// `Some(0)` for a payment that succeeded — the money went to the payee,
    /// nothing came back. `Some` and less than
    /// [`realized_total_debited`](LnSendDetails::realized_total_debited) for a
    /// [`Refunded`](LnSendState::Refunded) payment, because claiming the
    /// funding back is itself a transaction: it pays the primary module's
    /// input, output and change fees and loses whatever will not fit a note
    /// denomination, even where the gateway's cut returns intact with the
    /// contract.
    ///
    /// `None` while the payment is still in flight, and `None` for a
    /// [`Failed`](LnSendState::Failed) send, where nothing resolved cleanly
    /// and claiming a figure would be a guess.
    pub restored_amount: Option<Amount>,
    /// Realized half. The aggregate fee this payment actually cost: everything
    /// that left the balance and neither reached the payee nor came back.
    ///
    /// For a success,
    /// [`realized_total_debited`](LnSendDetails::realized_total_debited) less
    /// [`invoice_amount`](LnSendDetails::invoice_amount). For a refund, the
    /// whole net cost of a payment that delivered nothing —
    /// [`realized_total_debited`](LnSendDetails::realized_total_debited) less
    /// [`restored_amount`](LnSendDetails::restored_amount) — covering both
    /// the funding transaction's fees and the refund transaction's. This is
    /// the number a balance reconciliation needs, and the number
    /// [`quoted_fee`](LnSendDetails::quoted_fee) only estimated.
    ///
    /// `None` until the operation settles, and `None` for a
    /// [`Failed`](LnSendState::Failed) send: an unresolved payment has no
    /// established cost, and reporting zero for one would understate what a
    /// user lost.
    pub realized_fee: Option<Amount>,
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
/// [`Failed`](Self::Failed) exists because a payment can arrive and then fail
/// to become spendable ecash. That is neither [`Canceled`](Self::Canceled),
/// which this enum reserves for a receive that ended *before* payment, nor
/// [`Claimed`](Self::Claimed), which would assert the funds are spendable when
/// they are not.
///
/// # The v1 mapping is keyed on the reason *and* the phase
///
/// v1's cancellation reason is **typed, not free-form**: upstream's
/// `LnReceiveState::Canceled` carries a `LightningReceiveError`, whose variants
/// are `Timeout`, `Rejected`, `ClaimRejected` and `InvalidPreimage`. No string
/// is parsed anywhere in this mapping, and none needs to be.
///
/// The typed variant is not sufficient on its own, though, because `Rejected`
/// is emitted at two entirely different moments: for an offer the federation
/// refused before anybody paid, and again after a claim had been accepted but
/// the primary-module outputs failed to produce notes. The first means nothing
/// happened; the second means somebody paid and the money did not arrive. So
/// the mapping key is the pair **(typed reason, phase the operation had
/// reached)**, where the phase that matters is whether the receive had ever
/// reached [`Funded`](Self::Funded) — that is, whether a payment had been
/// confirmed for it.
///
/// | upstream v1 | phase reached | here |
/// | --- | --- | --- |
/// | `Created` | — | [`Created`](Self::Created) |
/// | `WaitingForPayment { .. }` | — | [`WaitingForPayment`](Self::WaitingForPayment) |
/// | `Funded`, `AwaitingFunds` | — | [`Funded`](Self::Funded) |
/// | `Claimed` | — | [`Claimed`](Self::Claimed) |
/// | `Canceled { Timeout }` | any | [`Expired`](Self::Expired) |
/// | `Canceled { Rejected }` | before [`Funded`](Self::Funded) | [`Canceled`](Self::Canceled) |
/// | `Canceled { Rejected }` | at or after [`Funded`](Self::Funded) | [`Failed`](Self::Failed) |
/// | `Canceled { ClaimRejected }` | at or after [`Funded`](Self::Funded) | [`Failed`](Self::Failed) |
/// | `Canceled { InvalidPreimage }` | at or after [`Funded`](Self::Funded) | [`Failed`](Self::Failed) |
/// | `Canceled { ClaimRejected \| InvalidPreimage }` | before [`Funded`](Self::Funded) | [`Canceled`](Self::Canceled) |
///
/// The last row is a fallback rather than an expected path — a claim cannot be
/// rejected before there is a payment to claim — and it is listed so that the
/// mapping is total on the pair rather than partial with a hole for an
/// upstream ordering nobody has seen yet.
///
/// Three rules generate the whole table, and they are what an implementation
/// should encode:
///
/// 1. `Timeout` is [`Expired`](Self::Expired). Nobody paid within the
///    invoice's lifetime; that is the benign ending.
/// 2. Any other reason reaching a receive that had got to
///    [`Funded`](Self::Funded) is [`Failed`](Self::Failed). A payment was
///    confirmed and did not become spendable notes — including the `Rejected`
///    that arrives after a claim was accepted and the primary outputs failed,
///    which earlier revisions of this documentation described as a benign
///    pre-payment cancellation. It is the opposite of benign.
/// 3. Any other reason reaching a receive that never got past
///    [`WaitingForPayment`](Self::WaitingForPayment) is
///    [`Canceled`](Self::Canceled) — a genuine refusal before payment, with
///    nothing owed to anyone.
///
/// **This obliges the implementation to persist the phase.** The terminal
/// upstream event does not say which moment it belongs to, and after a restart
/// the SDK is not the process that watched the operation, so "did this receive
/// ever reach [`Funded`](Self::Funded)?" must be durable rather than
/// remembered. Without it a post-claim failure and a pre-payment refusal are
/// indistinguishable, which is precisely the bug this mapping fixes. The
/// realized fields of [`LnReceiveDetails`] are not a substitute: they answer
/// what moved, not how far the operation got.
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
    /// The amount that landed is
    /// [`LnReceiveDetails::realized_net_credit`], which is `Some` by the time
    /// this state is reached: the invoice's face value less the fee the
    /// accepted claim actually charged. It is neither the face value nor the
    /// estimate the invoice was issued against
    /// ([`LnReceiveDetails::expected_net_credit`]) — those two can differ from
    /// it, and only the realized figure says what the balance did.
    Claimed,
    /// Final: the receive was refused **before** anyone paid it — for example
    /// because the gateway withdrew the offer or the federation rejected it.
    ///
    /// Nothing moved and nothing is owed: no payment was confirmed, so no
    /// credit was ever due. A cancellation that arrives *after* a payment was
    /// confirmed is [`Failed`](Self::Failed), not this — see the enum's mapping
    /// rules, which key on the phase for exactly that reason.
    Canceled {
        /// Human-readable explanation. Diagnostic only — not a stable
        /// contract, and not something to match on.
        reason: String,
    },
    /// Final: the invoice's expiry passed without it being paid.
    ///
    /// Nobody paid, so nothing was claimed and no fee was incurred:
    /// [`LnReceiveDetails::realized_net_credit`] and
    /// [`LnReceiveDetails::realized_fee`] both read zero here, while the
    /// quoted terms still show what the invoice would have credited.
    Expired,
    /// Final: the payment arrived but the ecash for it was never issued.
    ///
    /// This is the one genuinely bad outcome of a receive, and it is not the
    /// ordinary "nobody paid" ending — somebody *did* pay. The payment was
    /// confirmed and then the step that turns it into spendable notes did
    /// not complete, so the amount is **not in the balance** and will not
    /// arrive by waiting. Unlike [`Expired`](Self::Expired) and
    /// [`Canceled`](Self::Canceled), where nothing moved and nothing is
    /// owed, this needs an operator's attention: the funds exist somewhere
    /// between the payer and this wallet and recovering them is not
    /// something the application can do by retrying.
    ///
    /// [`LnReceiveDetails::realized_net_credit`] reads zero here — nothing
    /// landed, and nothing will — while
    /// [`LnReceiveDetails::realized_fee`] may be absent: a claim that was
    /// accepted and then failed to produce notes did cost something, and this
    /// is the one ending where how much may not be establishable.
    ///
    /// Render it as an error the user should report, not as an expired
    /// invoice. Deliberately payload-free; see the enum's mapping notes for
    /// why, and for how v1 and lnv2 reach this state.
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

/// What an incoming lightning payment *is*: the invoice that was issued, the
/// terms it was issued on, and — if it is ever paid — what actually landed.
///
/// The persisted half of an [`LnReceiveState`] operation, read with
/// [`Operation::details`](crate::Operation::details). The invoice and the
/// quoted terms are written in the same storage transaction that creates the
/// operation, so they are there however soon after [`Lightning::receive`] the
/// process dies; the realized figures are added when the receive settles.
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
/// The convention that relates the amounts is fixed: **the fee is deducted
/// from the invoice, not added on top of it.** The invoice's face value is
/// exactly what the application asked [`Lightning::receive`] for, so the payer
/// is asked for the number the application chose, and the receive-side fee —
/// the gateway's charge plus the federation's own — comes out of it. Hence
/// [`requested_amount`](LnReceiveDetails::requested_amount) `==`
/// [`invoice_amount`](LnReceiveDetails::invoice_amount), and the credit is the
/// smaller number.
///
/// # Quoted terms and realized outcome are different halves
///
/// - **The quoted half** — [`requested_amount`](LnReceiveDetails::requested_amount),
///   [`invoice_amount`](LnReceiveDetails::invoice_amount),
///   [`quoted_fee`](LnReceiveDetails::quoted_fee),
///   [`expected_net_credit`](LnReceiveDetails::expected_net_credit) — is fixed
///   when the invoice is created. It is what the payer is asked for and what the
///   application shows as "you will receive". Plain fields, readable from the
///   moment the operation exists.
/// - **The realized half** — [`realized_fee`](LnReceiveDetails::realized_fee),
///   [`realized_net_credit`](LnReceiveDetails::realized_net_credit) — is what
///   the balance actually did. `Option`, absent until the receive settles, then
///   written once and never revised.
///
/// The split is not pedantry: **the aggregate receive fee cannot be fixed when
/// the invoice is created.** The gateway's terms are known then, but the
/// federation's input, output, change and denomination-dust costs are chosen
/// only if and when a payment is actually claimed, against the note inventory
/// at that future time — 0.12's receive fee quote is a point-in-time dry run
/// that commits to nothing. And most invoices are never paid at all: an invoice
/// that expires unpaid, or is refused before payment, realizes **no credit
/// whatsoever**, so a plain field documented as "what actually lands" would be
/// false for the most common ending a receive has.
///
/// ## The invariants
///
/// The quoted half is self-consistent from creation:
///
/// ```text
/// invoice_amount == expected_net_credit + quoted_fee
/// ```
///
/// The realized half satisfies the same relation, but **only for a receive
/// that was actually claimed**:
///
/// ```text
/// invoice_amount == realized_net_credit + realized_fee   // Claimed only
/// ```
///
/// Both halves are recorded in full rather than as two numbers and a
/// subtraction, so the convention is *observable* rather than assumed: a caller
/// can render "you asked for X, the payer pays Y, you expect Z, you got W"
/// without knowing which module generation served the request or redoing
/// fallible arithmetic on money.
///
/// ## What the realized fields read at each ending
///
/// | state | `realized_fee` | `realized_net_credit` |
/// | --- | --- | --- |
/// | [`Created`](LnReceiveState::Created), [`WaitingForPayment`](LnReceiveState::WaitingForPayment), [`Funded`](LnReceiveState::Funded) | `None` | `None` |
/// | [`Claimed`](LnReceiveState::Claimed) | `Some` — what the accepted claim charged | `Some` — the invoice amount less that |
/// | [`Expired`](LnReceiveState::Expired) | `Some(0)` | `Some(0)` |
/// | [`Canceled`](LnReceiveState::Canceled) | `Some(0)` | `Some(0)` |
/// | [`Failed`](LnReceiveState::Failed) | `None` — a failed claim's cost may not be establishable | `Some(0)` — nothing landed, and nothing will |
///
/// `Some(0)` and `None` are different answers and must not be rendered the
/// same way: zero is a measurement — the invoice lapsed, the balance did not
/// move — while `None` means the figure is not established. For an invoice
/// still waiting, that is simply the truth; for a
/// [`Failed`](LnReceiveState::Failed) receive it will stay the truth, because a
/// claim that was accepted and then failed to produce notes did cost something
/// and pretending it cost zero would understate what a user lost.
///
/// Each realized field goes from `None` to `Some` exactly once, in the write
/// that records the settling transition, and never changes afterwards — so
/// this record can be read at any time without ordering it against
/// [`Operation::state`](crate::Operation::state).
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
    /// Quoted half. The amount asked of [`Lightning::receive`].
    ///
    /// Recorded as it was requested, so the record can be checked against
    /// what the application intended rather than only against what the
    /// invoice says.
    pub requested_amount: Amount,
    /// Quoted half. The invoice's face value: what the payer is asked to pay.
    ///
    /// Equal to [`requested_amount`](LnReceiveDetails::requested_amount)
    /// under the deduct-the-fee convention described on this type, and always
    /// equal to
    /// [`expected_net_credit`](LnReceiveDetails::expected_net_credit) plus
    /// [`quoted_fee`](LnReceiveDetails::quoted_fee). Exact rather than
    /// estimated: it is encoded in the invoice, and a payer who pays pays this.
    pub invoice_amount: Amount,
    /// Quoted half. The receive-side fee **as quoted when the invoice was
    /// issued**: the gateway's charge for taking the payment in, plus the
    /// federation's own quoted cost of issuing the ecash for it.
    ///
    /// The aggregate, on the same footing as [`LnQuote::fee`] on the sending
    /// side: the whole difference between what the payer pays and what is
    /// expected to land, so no other deduction is expected later. Zero is
    /// possible — a payment that never left the federation has no gateway to
    /// pay — but is not the rule, since issuing the notes is itself a
    /// federation transaction.
    ///
    /// It is an **estimate**, and it is the one figure on this record that a
    /// caller must not present as settled. The gateway's terms are firm, but
    /// the federation's share is chosen only when a claim is assembled and
    /// accepted; what was really charged is
    /// [`realized_fee`](LnReceiveDetails::realized_fee).
    pub quoted_fee: Amount,
    /// Quoted half. What the invoice is expected to credit:
    /// [`invoice_amount`](LnReceiveDetails::invoice_amount) minus
    /// [`quoted_fee`](LnReceiveDetails::quoted_fee).
    ///
    /// The number to show as "you will receive" *before* anyone has paid.
    /// Stored rather than derived so that the receive screen and the history
    /// row can never disagree about it.
    ///
    /// It is what the wallet expects, not what it got: an invoice that expires
    /// unpaid still has an `expected_net_credit`, and it credited nothing. The
    /// figure that says what the balance did is
    /// [`realized_net_credit`](LnReceiveDetails::realized_net_credit).
    pub expected_net_credit: Amount,
    /// Realized half. The receive-side fee actually charged, once the receive
    /// has settled.
    ///
    /// `Some` from the fees the accepted claim recorded — which may differ from
    /// [`quoted_fee`](LnReceiveDetails::quoted_fee), because the federation's
    /// input, output, change and dust costs depend on the inventory at claim
    /// time rather than at invoice time. `Some(0)` for an invoice that expired
    /// or was refused before payment: no claim was assembled, so nothing was
    /// charged.
    ///
    /// `None` while the receive is still running, and `None` for a
    /// [`Failed`](LnReceiveState::Failed) receive, where a claim may have been
    /// accepted before the notes failed to materialise and the cost may not be
    /// establishable. Never zero as a stand-in for unknown.
    pub realized_fee: Option<Amount>,
    /// Realized half. What the balance actually gained.
    ///
    /// `Some` and equal to [`invoice_amount`](LnReceiveDetails::invoice_amount)
    /// less [`realized_fee`](LnReceiveDetails::realized_fee) for a receive that
    /// reached [`Claimed`](LnReceiveState::Claimed) — the same value that state
    /// refers to, so a receipt built from the record and one built from the
    /// state cannot disagree.
    ///
    /// `Some(0)` for every other ending, and that zero is the point of this
    /// field: an [`Expired`](LnReceiveState::Expired) or
    /// [`Canceled`](LnReceiveState::Canceled) invoice was never paid, and a
    /// [`Failed`](LnReceiveState::Failed) one was paid without the ecash ever
    /// being issued — in none of the three did the spendable balance rise, and
    /// [`expected_net_credit`](LnReceiveDetails::expected_net_credit) must not
    /// be read as though it had.
    ///
    /// `None` only while the receive is still in flight: nobody has paid yet,
    /// or the payment has not settled.
    pub realized_net_credit: Option<Amount>,
    /// The gateway that agreed to take the payment in, if there was one.
    ///
    /// `None` means no gateway took part — not that the gateway is unknown.
    /// Fixed when the invoice is created, like the rest of the quoted half, so
    /// it never turns from `None` into a gateway id later. It is the one
    /// `Option` here that is not a realized figure, which is why its `None`
    /// reads as an answer rather than as "not established yet".
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

    /// The gateway used by every fixture here.
    fn gateway() -> GatewayId {
        GatewayId::from_raw("0266e4598d1d3c415f572a8488830b".to_owned())
    }

    /// A send record as [`Lightning::send`] writes it: the quoted terms of one
    /// plausible payment — 100,000 msat to the payee, 1,050 msat of quoted
    /// aggregate fee, 101,050 msat of quoted debit — and no realized figure
    /// yet, because nothing has been accepted.
    fn send_details() -> LnSendDetails {
        LnSendDetails {
            invoice: Bolt11Invoice::from_raw("lnbcrt1000n1pexample".to_owned()),
            invoice_amount: Amount::from_msats(100_000),
            quoted_fee: Amount::from_msats(1_050),
            quoted_total_debited: Amount::from_msats(101_050),
            route: LightningRoute::Gateway {
                gateway_id: gateway(),
            },
            realized_total_debited: None,
            restored_amount: None,
            realized_fee: None,
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    /// A receive record as [`Lightning::receive`] writes it: an invoice of
    /// 50,000 msat quoted against a 500 msat receive-side fee, under the
    /// convention this crate fixes — the payer is asked for exactly what was
    /// requested and the fee comes out of it — with nothing realized yet.
    fn receive_details() -> LnReceiveDetails {
        LnReceiveDetails {
            invoice: Bolt11Invoice::from_raw("lnbcrt500n1pexample".to_owned()),
            description: "coffee".to_owned(),
            requested_amount: Amount::from_msats(50_000),
            invoice_amount: Amount::from_msats(50_000),
            quoted_fee: Amount::from_msats(500),
            expected_net_credit: Amount::from_msats(49_500),
            realized_fee: None,
            realized_net_credit: None,
            gateway_id: Some(gateway()),
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
    fn ln_send_details_quoted_total_is_the_amount_plus_the_quoted_fee() {
        let details = send_details();
        assert_eq!(
            details.invoice_amount.checked_add(details.quoted_fee),
            Some(details.quoted_total_debited),
        );
    }

    #[test]
    fn ln_send_details_realize_nothing_until_a_transaction_is_accepted() {
        // As written by `send`: the quoted half is complete, the realized half
        // is absent — not zero, absent.
        let details = send_details();
        assert!(!LnSendState::Created.is_final());
        assert_eq!(details.realized_total_debited, None);
        assert_eq!(details.restored_amount, None);
        assert_eq!(details.realized_fee, None);
        assert_ne!(details.quoted_fee, Amount::from_msats(0));
    }

    #[test]
    fn ln_send_details_keep_the_quoted_fee_and_route_of_a_payment_that_was_refunded() {
        // A refunded send carries no fee and no route on its state, and the
        // record is what keeps both readable.
        let details = send_details();
        let state = LnSendState::Refunded;
        assert!(state.is_final());
        assert_eq!(details.quoted_fee, Amount::from_msats(1_050));
        assert_eq!(
            details.route,
            LightningRoute::Gateway {
                gateway_id: gateway(),
            },
        );
    }

    #[test]
    fn ln_send_details_and_success_agree_on_the_quoted_fee_and_route() {
        // The one licensed duplication: two copies of the same value from the
        // same quote, never two different numbers.
        let details = send_details();
        let state = LnSendState::Success {
            preimage: Preimage::from_raw(
                "0000000000000000000000000000000000000000000000000000000000000000".to_owned(),
            ),
            quoted_fee: details.quoted_fee,
            route: details.route.clone(),
        };
        match state {
            LnSendState::Success {
                quoted_fee, route, ..
            } => {
                assert_eq!(quoted_fee, details.quoted_fee);
                assert_eq!(route, details.route);
            }
            other => panic!("expected Success, got {other:?}"),
        }
    }

    #[test]
    fn ln_send_details_reconcile_a_successful_payment() {
        // Settled successfully: nothing came back, and the realized fee is
        // everything that left the balance without reaching the payee.
        let details = LnSendDetails {
            realized_total_debited: Some(Amount::from_msats(101_048)),
            restored_amount: Some(Amount::from_msats(0)),
            realized_fee: Some(Amount::from_msats(1_048)),
            ..send_details()
        };
        // realized_total_debited == delivered + restored + fee, delivered
        // being the invoice amount for a payment that succeeded.
        let reconciled = [
            details.invoice_amount,
            details.restored_amount.expect("restored"),
            details.realized_fee.expect("fee"),
        ]
        .into_iter()
        .try_fold(Amount::from_msats(0), Amount::checked_add);
        assert_eq!(reconciled, details.realized_total_debited);
    }

    /// The assertion the retraction of the ceiling claim rests on: the debit
    /// that actually lands can exceed the one that was quoted, because the
    /// gateway's terms are refetched during the send and the mint-side
    /// components are chosen when the funding transaction is assembled. An
    /// earlier revision asserted `realized <= quoted` here, which published
    /// 0.12 cannot enforce.
    #[test]
    fn ln_send_details_realized_may_exceed_quoted() {
        let details = LnSendDetails {
            realized_total_debited: Some(Amount::from_msats(101_400)),
            restored_amount: Some(Amount::from_msats(0)),
            realized_fee: Some(Amount::from_msats(1_400)),
            ..send_details()
        };
        assert!(details.realized_fee > Some(details.quoted_fee));
        assert!(details.realized_total_debited > Some(details.quoted_total_debited));
        // Settlement does not revise the quoted half: it is still exactly what
        // the user approved, which is what makes the pair worth keeping.
        assert_eq!(details.quoted_fee, send_details().quoted_fee);
        assert_eq!(
            details.quoted_total_debited,
            send_details().quoted_total_debited
        );
        // And it still reconciles, on the same identity.
        let reconciled = [
            details.invoice_amount,
            details.restored_amount.expect("restored"),
            details.realized_fee.expect("fee"),
        ]
        .into_iter()
        .try_fold(Amount::from_msats(0), Amount::checked_add);
        assert_eq!(reconciled, details.realized_total_debited);
    }

    /// And it can land below the quote, or at zero for an attempt that never
    /// assembled a funding transaction at all.
    #[test]
    fn ln_send_details_realized_may_be_below_quoted_or_zero() {
        let cheaper = LnSendDetails {
            realized_total_debited: Some(Amount::from_msats(100_900)),
            restored_amount: Some(Amount::from_msats(0)),
            realized_fee: Some(Amount::from_msats(900)),
            ..send_details()
        };
        assert!(cheaper.realized_fee < Some(cheaper.quoted_fee));
        assert!(cheaper.realized_total_debited < Some(cheaper.quoted_total_debited));

        let unfunded = LnSendDetails {
            realized_total_debited: Some(Amount::from_msats(0)),
            restored_amount: Some(Amount::from_msats(0)),
            realized_fee: Some(Amount::from_msats(0)),
            ..send_details()
        };
        assert_eq!(unfunded.realized_total_debited, Some(Amount::from_msats(0)));
        // A payment that moved nothing still has terms to show on a receipt.
        assert_eq!(
            unfunded.quoted_total_debited,
            send_details().quoted_total_debited
        );
        assert!(LnSendState::Refunded.is_final());
    }

    #[test]
    fn ln_send_details_reconcile_a_refunded_payment_whose_realized_fee_is_sunk() {
        // The case the review asks to be checkable. The gateway's 1,000 msat
        // came back inside the refunded contract; the fees that funded the
        // attempt (48 msat) and the refund transaction's own cost (30 msat) did
        // not. So the realized fee is 78 msat against a quoted 1,050 — quoted
        // and realized differ, and the record still reconciles.
        let details = LnSendDetails {
            realized_total_debited: Some(Amount::from_msats(101_048)),
            restored_amount: Some(Amount::from_msats(100_970)),
            realized_fee: Some(Amount::from_msats(78)),
            ..send_details()
        };
        let debited = details.realized_total_debited.expect("debited");
        let restored = details.restored_amount.expect("restored");
        let sunk = details.realized_fee.expect("sunk");

        // Nothing reached the payee, so the whole debit is restored-plus-sunk.
        assert_eq!(debited.checked_sub(restored), Some(sunk));
        assert!(restored < debited);
        // A refund is not free, and it is not the quoted fee either.
        assert_ne!(sunk, Amount::from_msats(0));
        assert_ne!(sunk, details.quoted_fee);
        assert!(sunk < details.quoted_fee);
        // The quoted terms are untouched by the outcome: the receipt can still
        // say what the user approved.
        assert_eq!(details.quoted_fee, Amount::from_msats(1_050));
        assert_eq!(details.quoted_total_debited, Amount::from_msats(101_050));
        assert!(LnSendState::Refunded.is_final());
    }

    #[test]
    fn ln_send_details_leave_a_failed_payment_unreconciled_rather_than_zeroed() {
        // Funding was accepted; nothing resolved after that. `None` is the
        // honest answer, and it is not `Some(0)`.
        let details = LnSendDetails {
            realized_total_debited: Some(Amount::from_msats(101_048)),
            restored_amount: None,
            realized_fee: None,
            ..send_details()
        };
        assert_eq!(details.restored_amount, None);
        assert_ne!(details.realized_fee, Some(Amount::from_msats(0)));
        assert!(
            LnSendState::Failed {
                reason: "gateway vanished mid-payment".to_owned(),
            }
            .is_final()
        );
    }

    #[test]
    fn ln_send_details_can_record_an_internal_route() {
        // An internal payment has no gateway, and still has a fee.
        let details = LnSendDetails {
            route: LightningRoute::Internal,
            ..send_details()
        };
        assert_eq!(details.route, LightningRoute::Internal);
        assert_ne!(details.quoted_fee, Amount::from_msats(0));
    }

    #[test]
    fn ln_receive_details_invoice_amount_is_the_expected_net_credit_plus_the_fee() {
        let details = receive_details();
        assert_eq!(
            details.expected_net_credit.checked_add(details.quoted_fee),
            Some(details.invoice_amount),
        );
    }

    #[test]
    fn ln_receive_details_realize_nothing_until_the_receive_settles() {
        let details = receive_details();
        assert!(!LnReceiveState::WaitingForPayment.is_final());
        assert_eq!(details.realized_fee, None);
        assert_eq!(details.realized_net_credit, None);
    }

    #[test]
    fn ln_receive_details_realized_net_credit_is_the_invoice_amount_less_the_fee() {
        // Claimed against a different inventory than the quote saw: the real
        // fee is 620 msat where 500 was quoted, so the credit is smaller than
        // the invoice promised.
        let details = LnReceiveDetails {
            realized_fee: Some(Amount::from_msats(620)),
            realized_net_credit: Some(Amount::from_msats(49_380)),
            ..receive_details()
        };
        assert_eq!(
            details
                .realized_net_credit
                .and_then(|credit| credit.checked_add(details.realized_fee.expect("fee"))),
            Some(details.invoice_amount),
        );
        assert_ne!(details.realized_fee, Some(details.quoted_fee));
        assert!(details.realized_net_credit < Some(details.expected_net_credit));
        assert!(LnReceiveState::Claimed.is_final());
    }

    #[test]
    fn ln_receive_details_realize_nothing_for_an_invoice_that_expired_unpaid() {
        // Nobody paid: the balance did not move and nothing was charged, and
        // that is a measured zero rather than an absent figure.
        let details = LnReceiveDetails {
            realized_fee: Some(Amount::from_msats(0)),
            realized_net_credit: Some(Amount::from_msats(0)),
            ..receive_details()
        };
        assert_eq!(details.realized_net_credit, Some(Amount::from_msats(0)));
        assert_eq!(details.realized_fee, Some(Amount::from_msats(0)));
        // The quoted half still says what the invoice would have credited.
        assert_eq!(details.expected_net_credit, Amount::from_msats(49_500));
        assert_ne!(
            details.realized_net_credit,
            Some(details.expected_net_credit)
        );
        assert!(LnReceiveState::Expired.is_final());
    }

    #[test]
    fn ln_receive_details_realize_nothing_for_a_refusal_before_payment() {
        let details = LnReceiveDetails {
            realized_fee: Some(Amount::from_msats(0)),
            realized_net_credit: Some(Amount::from_msats(0)),
            ..receive_details()
        };
        assert_eq!(details.realized_net_credit, Some(Amount::from_msats(0)));
        assert!(
            LnReceiveState::Canceled {
                reason: "gateway withdrew the offer".to_owned(),
            }
            .is_final()
        );
    }

    #[test]
    fn ln_receive_details_of_a_failed_receive_know_no_credit_landed_but_not_the_cost() {
        // Somebody paid and the ecash was never issued: the credit is a known
        // zero, the cost is unknown — and unknown is `None`, never `Some(0)`.
        let details = LnReceiveDetails {
            realized_fee: None,
            realized_net_credit: Some(Amount::from_msats(0)),
            ..receive_details()
        };
        assert_eq!(details.realized_net_credit, Some(Amount::from_msats(0)));
        assert_eq!(details.realized_fee, None);
        assert_ne!(details.realized_fee, Some(Amount::from_msats(0)));
        assert!(LnReceiveState::Failed.is_final());
    }

    #[test]
    fn ln_receive_details_follow_the_deducted_fee_convention() {
        // The payer is asked for exactly what the application requested, and
        // the fee comes out of it.
        let details = receive_details();
        assert_eq!(details.invoice_amount, details.requested_amount);
        assert!(details.expected_net_credit < details.invoice_amount);
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
        assert_eq!(summed, Some(send_details().quoted_fee));
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
                quoted_fee: Amount::from_msats(0),
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
