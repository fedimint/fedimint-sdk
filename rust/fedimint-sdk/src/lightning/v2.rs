//! The lnv2 lightning module: mappings, subscriptions, and the facade operations.

use std::sync::Arc;

use fedimint_client::Client;
use fedimint_client_module::ClientModuleInstance;
use fedimint_core::core::OperationId;
use fedimint_core::util::{BoxStream, SafeUrl};
use fedimint_lnv2_client::{
    LightningClientModule, LightningOperationMeta, ReceiveError, ReceiveOperationState,
    SelectGatewayError, SendOperationMeta, SendOperationState, SendPaymentError,
};
use fedimint_lnv2_common::gateway_api::{PaymentFee, RoutingInfo};
use fedimint_lnv2_common::{Bolt11InvoiceDescription, LightningInvoice};
use futures::StreamExt;

use super::driver::{LnReceiveDriver, LnSendDriver, until_final};
use super::wire::{self, PHASE_FUNDED};
use super::{
    INVOICE_EXPIRY_SECS, LnQuoteInner, Plan, Terms, add, balance_of, fee_quote_failure,
    from_upstream, gateway_unavailable, insufficient, internal, network_refusal, now, plan_of,
    quote_changed, quote_expired, subscribe_error, to_upstream, unreachable,
};
use crate::federation::FederationInner;
use crate::operation::{Backfilled, Driver, kinds, record_phase_in};
use crate::{
    Amount, Bolt11Invoice, Error, ErrorCode, GatewayId, LightningRoute, LnReceive,
    LnReceiveDetails, LnReceiveState, LnSendDetails, LnSendState, Network, Operation, Preimage,
    Result, Timestamp,
};

// lnv2 `SendOperationState` onto `LnSendState`. `Failure` means two things: the funding
// transaction was refused (before `Funded`, nothing debited, so `Refunded`) or the refund of a
// funded contract failed (after it, so `Failed`). After a restart the persisted phase is what
// tells them apart. `Refunding` is in progress, not final.
//
// | upstream        | phase reached   | here       |
// | --------------- | --------------- | ---------- |
// | `Funding`       | any             | `Created`  |
// | `Funded`        | any             | `Funded`   |
// | `Refunding`     | any             | `Funded`   |
// | `Success`       | any             | `Success`  |
// | `Refunded`      | any             | `Refunded` |
// | `Failure`       | before `Funded` | `Refunded` |
// | `Failure`       | at/after        | `Failed`   |
pub(super) fn map_send(
    state: &SendOperationState,
    funded_before: bool,
    fee: Amount,
    route: &LightningRoute,
) -> LnSendState {
    match state {
        SendOperationState::Funding => LnSendState::Created,
        SendOperationState::Funded | SendOperationState::Refunding => LnSendState::Funded,
        SendOperationState::Success(preimage) => LnSendState::Success {
            preimage: Preimage::from_bytes(*preimage),
            fee,
            route: route.clone(),
        },
        SendOperationState::Refunded => LnSendState::Refunded,
        SendOperationState::Failure if funded_before => LnSendState::Failed {
            reason: "the payment's contract could not be refunded".to_owned(),
        },
        SendOperationState::Failure => LnSendState::Refunded,
    }
}

/// The phase an upstream state proves was reached.
pub(super) fn send_phase(state: &SendOperationState) -> u32 {
    match state {
        SendOperationState::Funded
        | SendOperationState::Refunding
        | SendOperationState::Success(_)
        | SendOperationState::Refunded => PHASE_FUNDED,
        SendOperationState::Funding | SendOperationState::Failure => 0,
    }
}

/// The lnv2 module on a live client, or `NotSupported` when the federation dropped it.
pub(super) fn module_of(
    client: &Client,
) -> Result<ClientModuleInstance<'_, LightningClientModule>> {
    client
        .get_first_module::<LightningClientModule>()
        .map_err(|_| {
            Error::new(
                ErrorCode::NotSupported,
                "this federation no longer has an lnv2 lightning module",
            )
        })
}

/// A fresh stream over an lnv2 send, persisting the funded phase the moment it is seen.
pub(super) async fn subscribe_send(
    federation: &FederationInner,
    id: OperationId,
    phase: u32,
    details: &LnSendDetails,
) -> Result<BoxStream<'static, Result<LnSendState>>> {
    let client = federation.client(false).await?;
    let module = module_of(&client)?;
    let upstream = module
        .subscribe_send_operation_state_updates(id)
        .await
        .map_err(subscribe_error)?
        .into_stream();
    let db = federation.db();
    let fee = details.fee;
    let route = details.route.clone();
    let stream = upstream.scan((phase, db), move |(phase, db), state| {
        let reached = send_phase(&state);
        let advance = reached > *phase;
        if advance {
            *phase = reached;
        }
        let mapped = map_send(&state, *phase >= PHASE_FUNDED, fee, &route);
        let db = db.clone();
        async move {
            if advance && let Err(err) = record_phase_in(&db, id, reached).await {
                return Some(Err(err));
            }
            Some(Ok(mapped))
        }
    });
    Ok(until_final(stream))
}

// lnv2 `ReceiveOperationState` onto `LnReceiveState`. Direct: lnv2 names its stages.
//
// | upstream       | here                |
// | -------------- | ------------------- |
// | `Pending`      | `WaitingForPayment` |
// | `Claiming`     | `Funded`            |
// | `Claimed`      | `Claimed`           |
// | `Expired`      | `Expired`           |
// | `Failure`      | `Failed`            |
// | `Uneconomical` | `Failed`            |
pub(super) fn map_receive(state: &ReceiveOperationState) -> LnReceiveState {
    match state {
        ReceiveOperationState::Pending => LnReceiveState::WaitingForPayment,
        ReceiveOperationState::Claiming => LnReceiveState::Funded,
        ReceiveOperationState::Claimed => LnReceiveState::Claimed,
        ReceiveOperationState::Expired => LnReceiveState::Expired,
        ReceiveOperationState::Failure | ReceiveOperationState::Uneconomical => {
            LnReceiveState::Failed
        }
    }
}

/// A fresh stream over an lnv2 receive. The funded phase is recorded for symmetry with v1, though
/// no lnv2 receive mapping reads it.
pub(super) async fn subscribe_receive(
    federation: &FederationInner,
    id: OperationId,
    phase: u32,
) -> Result<BoxStream<'static, Result<LnReceiveState>>> {
    let client = federation.client(false).await?;
    let module = module_of(&client)?;
    let upstream = module
        .subscribe_receive_operation_state_updates(id)
        .await
        .map_err(subscribe_error)?
        .into_stream();
    let db = federation.db();
    let stream = upstream.scan((phase, db), move |(phase, db), state| {
        let mapped = map_receive(&state);
        let advance = matches!(mapped, LnReceiveState::Funded) && *phase < PHASE_FUNDED;
        if advance {
            *phase = PHASE_FUNDED;
        }
        let db = db.clone();
        async move {
            if advance && let Err(err) = record_phase_in(&db, id, PHASE_FUNDED).await {
                return Some(Err(err));
            }
            Some(Ok(mapped))
        }
    });
    Ok(until_final(stream))
}

/// Plans an lnv2 payment: the gateway the module would pick for this invoice, its fee schedule
/// for it, and the shared quote for funding the contract.
pub(super) async fn plan(
    client: &Client,
    module: &LightningClientModule,
    invoice: &Bolt11Invoice,
    amount: Amount,
) -> Result<Plan> {
    let (gateway, routing) = module
        .select_gateway(Some(invoice.inner().clone()))
        .await
        .map_err(select_error)?;
    let (send_fee, expiration_delta) = routing.send_parameters(invoice.inner());
    terms_for(
        client,
        module,
        amount,
        gateway,
        &routing,
        send_fee,
        expiration_delta,
    )
    .await
}

/// Prices a payment through `gateway` on the given schedule. lnv2 has no internal route: a payee
/// on the same gateway is a direct swap, still through the gateway.
async fn terms_for(
    client: &Client,
    module: &LightningClientModule,
    amount: Amount,
    gateway: SafeUrl,
    routing: &RoutingInfo,
    send_fee: PaymentFee,
    expiration_delta: u64,
) -> Result<Plan> {
    let contract_amount = from_upstream(send_fee.add_to(amount.msats()));
    let gateway_fee = contract_amount.checked_sub(amount).ok_or_else(|| {
        internal("the gateway's fee schedule produced a contract below the amount")
    })?;
    // The dry run balances the transaction against the real notes, so it fails when they
    // cannot cover the contract; the mint reports that as either a typed error (mint v1) or a
    // plain-text one (mint v2, `fee_quote_failure`'s doc comment says where).
    let quote = match module.send_fee_quote(to_upstream(contract_amount)).await {
        Ok(quote) => quote,
        Err(err) => {
            let text = err.to_string();
            let short = err.downcast_ref::<fedimint_mint_client::InsufficientBalanceError>();
            return Err(fee_quote_failure(
                client,
                short,
                &text,
                contract_amount,
                "could not quote the funding fee",
            )
            .await);
        }
    };
    let lightning_module = from_upstream(
        fee_consensus(client)
            .await?
            .fee(to_upstream(contract_amount)),
    );
    plan_of(
        gateway_fee,
        lightning_module,
        &quote,
        amount,
        LightningRoute::Gateway {
            gateway_id: GatewayId::from_upstream(routing.module_public_key),
        },
        Terms::V2 {
            gateway,
            send_fee,
            expiration_delta,
        },
    )
}

/// The lnv2 module's fee consensus, from the client's decoded configuration: the module keeps
/// its own copy private.
async fn fee_consensus(client: &Client) -> Result<fedimint_lnv2_common::config::FeeConsensus> {
    let config = client.config().await;
    let (_, config) = config
        .get_first_module_by_kind::<fedimint_lnv2_common::config::LightningClientConfig>("lnv2")
        .map_err(|err| {
            Error::new(
                ErrorCode::NotSupported,
                format!("this federation's lnv2 configuration is unreadable: {err}"),
            )
        })?;
    Ok(config.fee_consensus.clone())
}

/// Executes an lnv2 quote: asks the gateway for its schedule again, refuses on drift, funds,
/// records.
pub(super) async fn send(
    federation: &Arc<FederationInner>,
    client: &Client,
    module: &LightningClientModule,
    quote: &LnQuoteInner,
    gateway: SafeUrl,
    send_fee: PaymentFee,
    expiration_delta: u64,
) -> Result<Operation<LnSendState>> {
    // The module derives this invoice's operation id independently of the gateway or fee
    // schedule (`fedimint-lnv2-client/src/lib.rs:567-574`) and later refuses a duplicate
    // attempt against it with `DuplicatePaymentAttempt`, which `send_error` below maps to the
    // same `QuoteExpired` this reports. Checking first, before the gateway is asked to
    // re-quote, keeps an already-paid or still-in-flight invoice from being reported as a
    // quote whose terms moved: paying the first quote can change the fee a second quote for
    // the same invoice would be re-priced at.
    let id = OperationId::from_encodable(&(quote.invoice.inner().clone(), 0u64));
    if client.operation_exists(id).await {
        return Err(quote_expired(quote.expires_at, true));
    }
    // Checked again here, after the already-executed check above and before the gateway is
    // re-quoted below: the balance can drop between a quote and this call, and it must be asked
    // whether the invoice was already paid before it is asked whether the balance still covers
    // it, or a second quote for an already-paid invoice is misreported as a shortfall instead of
    // the truth.
    let available = balance_of(client).await?;
    if available < quote.plan.total {
        return Err(insufficient(quote.plan.total, available));
    }
    let Some(routing) = module
        .routing_info(&gateway)
        .await
        .map_err(gateway_unavailable)?
    else {
        return Err(Error::new(
            ErrorCode::QuoteChanged,
            "the quoted gateway no longer serves this federation",
        ));
    };
    let current = routing.send_parameters(quote.invoice.inner());
    let fresh = terms_for(
        client,
        module,
        quote.invoice_amount,
        gateway.clone(),
        &routing,
        current.0,
        current.1,
    )
    .await?;
    if current != (send_fee, expiration_delta) || fresh.total != quote.plan.total {
        return Err(quote_changed(quote.plan.total, fresh.total));
    }
    let mut details = crate::LnSendDetails {
        invoice: quote.invoice.clone(),
        invoice_amount: quote.invoice_amount,
        fee: quote.plan.fee,
        total: quote.plan.total,
        route: quote.plan.route.clone(),
        created_at: now(),
    };
    // Carried inside upstream's own metadata so a record rebuilt from the log after a crash
    // between upstream's commit and the SDK's write (`federation.create_operation` below) has
    // the exact quoted terms, not just the contract amount the log entry gives on its own.
    let id = module
        .send(
            quote.invoice.inner().clone(),
            Some(gateway),
            wire::custom_meta(&wire::LnSendDetailsWire::from(&details))?,
        )
        .await
        .map_err(|err| send_error(err, quote, federation.record().network.into()))?;
    // Upstream re-derives the gateway's terms itself inside `send` and funds the contract at
    // whatever it reads there; nothing binds that read to the `routing_info` re-check above, so a
    // gateway that changes its fee in the instant between the two funds a different contract than
    // the one this record is about to describe (an API that takes the checked terms is requested
    // as fedimint/fedimint#9124). The committed contract, read back from the
    // operation's own log entry, is the only place that says what was actually funded, so it
    // corrects the record's fee and total after the fact. A read or a computation that does not
    // come back clean leaves the quoted figures in place: an `Internal` error here would report a
    // payment that was in fact started as failed, which is worse than an inexact record. The
    // metadata copy already handed to the module above still keeps the quoted figures, since it
    // was built before this correction; only this SDK's own record ends up with the corrected
    // ones.
    if let Some(committed) = committed_contract_amount(federation, id).await
        && let Ok((fee, total)) = committed_fee(quote, committed)
    {
        details.fee = fee;
        details.total = total;
    }
    federation
        .create_operation(
            id,
            kinds::LN_SEND,
            "lnv2",
            &wire::LnSendDetailsWire::from(&details),
            Arc::new(LnSendDriver) as Arc<dyn Driver<LnSendState>>,
        )
        .await
}

/// The amount of the contract upstream actually funded for `id`'s send, read back from the
/// operation's own log entry. `None` when the entry is not there, or its meta does not decode as
/// an `lnv2` send: both are read failures the caller treats as "unknown", never as proof that
/// nothing was funded.
async fn committed_contract_amount(
    federation: &FederationInner,
    id: OperationId,
) -> Option<Amount> {
    let entry = fedimint_client::oplog::OperationLog::new(federation.db())
        .get_operation(id)
        .await?;
    let LightningOperationMeta::Send(SendOperationMeta { contract, .. }) =
        entry.try_meta::<LightningOperationMeta>().ok()?
    else {
        return None;
    };
    Some(from_upstream(contract.amount))
}

/// The fee and total to record for a send once the contract upstream actually committed is
/// known. Equal to what the quote itself would have funded (`invoice_amount` plus the quoted
/// gateway fee), the quote's own figures come back unchanged. Otherwise the difference between
/// the committed and the quoted gateway fee is folded into both: `fee` moves by exactly that much
/// and `total` follows it, since `total` is always `invoice_amount` plus `fee`. Errors only if
/// `committed_contract` funds less than the invoice amount, which upstream never actually does;
/// the caller keeps the quoted figures rather than surface that as a payment failure.
fn committed_fee(quote: &LnQuoteInner, committed_contract: Amount) -> Result<(Amount, Amount)> {
    let quoted_contract = add(quote.invoice_amount, quote.plan.breakdown.gateway)?;
    if committed_contract == quoted_contract {
        return Ok((quote.plan.fee, quote.plan.total));
    }
    let committed_gateway_fee = committed_contract
        .checked_sub(quote.invoice_amount)
        .ok_or_else(|| internal("the committed contract funds less than the invoice amount"))?;
    let fee = quote
        .plan
        .fee
        .checked_sub(quote.plan.breakdown.gateway)
        .and_then(|fee| fee.checked_add(committed_gateway_fee))
        .ok_or_else(|| internal("an amount overflowed"))?;
    Ok((fee, add(quote.invoice_amount, fee)?))
}

fn select_error(err: SelectGatewayError) -> Error {
    match err {
        SelectGatewayError::FailedToRequestGateways(cause) => unreachable(cause),
        SelectGatewayError::NoGatewaysAvailable | SelectGatewayError::GatewaysUnresponsive => {
            gateway_unavailable(err)
        }
    }
}

fn send_error(err: SendPaymentError, quote: &LnQuoteInner, expected: Network) -> Error {
    match err {
        SendPaymentError::InvoiceMissingAmount => Error::new(
            ErrorCode::AmountlessInvoice,
            "this invoice names no amount and cannot be paid through fedimint",
        ),
        SendPaymentError::InvoiceExpired => quote_expired(quote.expires_at, false),
        SendPaymentError::DuplicatePaymentAttempt(_) => quote_expired(quote.expires_at, true),
        SendPaymentError::SelectGateway(inner) => select_error(inner),
        SendPaymentError::FailedToConnectToGateway(_)
        | SendPaymentError::FederationNotSupported
        | SendPaymentError::GatewayFeeExceedsLimit
        | SendPaymentError::GatewayExpirationExceedsLimit => gateway_unavailable(err),
        SendPaymentError::FailedToRequestBlockCount(cause) => unreachable(cause),
        // The wording differs by which mint funds the contract: the v1 mint says "Insufficient
        // balance" (`fedimint-mint-client/src/lib.rs:2917`), the v2 mint says "Insufficient
        // funds" (`fedimint-mintv2-client/src/lib.rs:503`).
        SendPaymentError::FailedToFundPayment(cause)
            if cause.contains("Insufficient balance") || cause.contains("Insufficient funds") =>
        {
            Error::new(ErrorCode::InsufficientBalance, cause)
        }
        SendPaymentError::FailedToFundPayment(cause) => {
            internal(format!("the payment could not be funded: {cause}"))
        }
        // Reachable on a testnet4 federation: lnv2 compares the invoice's currency to the
        // configured network strictly (`self.cfg.network != invoice.currency().into()`,
        // fedimint-lnv2-client/src/lib.rs:560-565), but BOLT11 spells testnet3 and testnet4
        // the same way (`tb`), so a `tb` invoice this SDK's own `check_network` accepts is
        // still refused by the module. Both generations are affected, not lnv2 alone: v1
        // converts the configured network through lightning-invoice's
        // `From<bitcoin::Network> for Currency` (lightning-invoice-0.33.3/src/lib.rs:451-463),
        // which has no `Testnet4` arm either. Tracked upstream as fedimint/fedimint#9100; the
        // SDK reports this with the same detail `check_network` would have produced, rather
        // than trying to work around it.
        SendPaymentError::WrongCurrency { .. } => network_refusal(quote, expected),
    }
}

/// Issues an lnv2 invoice through the gateway the module selects, with both the gateway's and
/// the federation's receive-side fees taken out of what will land.
pub(super) async fn receive(
    federation: &Arc<FederationInner>,
    client: &Client,
    module: &LightningClientModule,
    amount: Amount,
    description: &str,
) -> Result<LnReceive> {
    let (gateway, routing) = module.select_gateway(None).await.map_err(select_error)?;
    // The contract the gateway funds is the invoice amount less its fee
    // (`fedimint-lnv2-client/src/lib.rs:1064`); the federation's claim fee comes off that.
    let contract_amount = from_upstream(routing.receive_fee.subtract_from(amount.msats()));
    let gateway_fee = amount.checked_sub(contract_amount).ok_or_else(|| {
        internal("the gateway's fee schedule produced a contract above the amount")
    })?;
    let quote = match module.receive_fee_quote(to_upstream(contract_amount)).await {
        Ok(quote) => quote,
        Err(err) => {
            let text = err.to_string();
            let short = err.downcast_ref::<fedimint_mint_client::InsufficientBalanceError>();
            return Err(fee_quote_failure(
                client,
                short,
                &text,
                contract_amount,
                "could not quote the claim fee",
            )
            .await);
        }
    };
    let fee = add(gateway_fee, from_upstream(quote.total().get_bitcoin()))?;
    let net_credit = amount.checked_sub(fee).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidInput,
            "the amount does not cover the receive-side fee",
        )
    })?;
    // Carried inside upstream's own metadata so a record rebuilt from the log after a crash
    // between upstream's commit and the SDK's write (`federation.create_operation` below) has
    // the exact quoted terms, not just the gateway's share of the fee the log entry gives on its
    // own. The invoice is not known until the call returns, so the copy carries a placeholder
    // for it and `expires_at`; the backfiller takes both from upstream's own meta instead.
    let created_at = now();
    let (invoice, id) = module
        .receive(
            to_upstream(amount),
            INVOICE_EXPIRY_SECS,
            Bolt11InvoiceDescription::Direct(description.to_owned()),
            Some(gateway),
            wire::custom_meta(&wire::LnReceiveDetailsWire {
                invoice: String::new(),
                description: description.to_owned(),
                requested_amount_msats: amount.msats(),
                invoice_amount_msats: amount.msats(),
                fee_msats: fee.msats(),
                net_credit_msats: net_credit.msats(),
                gateway_id: Some(GatewayId::from_upstream(routing.module_public_key).to_string()),
                expires_at: 0,
                created_at: created_at.epoch_millis(),
                reclaim_operation_id: None,
            })?,
        )
        .await
        .map_err(receive_error)?;
    let invoice = Bolt11Invoice::from_upstream(invoice);
    let details = LnReceiveDetails {
        invoice: invoice.clone(),
        description: description.to_owned(),
        requested_amount: amount,
        invoice_amount: amount,
        fee,
        net_credit,
        gateway_id: Some(GatewayId::from_upstream(routing.module_public_key)),
        expires_at: invoice.expires_at(),
        created_at,
    };
    let operation = federation
        .create_operation(
            id,
            kinds::LN_RECEIVE,
            "lnv2",
            &wire::LnReceiveDetailsWire::from(&details),
            Arc::new(LnReceiveDriver) as Arc<dyn Driver<LnReceiveState>>,
        )
        .await?;
    Ok(LnReceive { invoice, operation })
}

fn receive_error(err: ReceiveError) -> Error {
    match err {
        ReceiveError::SelectGateway(inner) => select_error(inner),
        ReceiveError::FailedToConnectToGateway(_)
        | ReceiveError::FederationNotSupported
        | ReceiveError::GatewayFeeExceedsLimit
        | ReceiveError::InvalidInvoice
        | ReceiveError::IncorrectInvoiceAmount => gateway_unavailable(err),
        ReceiveError::AmountTooSmall => Error::new(
            ErrorCode::InvalidInput,
            "the amount is too small to cover the fees of claiming it",
        ),
        // The expiry is this facade's own constant, well under the module's cap.
        ReceiveError::InvoiceExpiryTooLong => internal(err),
    }
}

/// Rebuilds a record from an lnv2 log entry: exact for an operation this SDK created, whose
/// custom metadata carries the quoted terms verbatim; an estimate — the contract's own amount
/// only, a send's total a floor and a receive's fee the gateway's share only — for an entry the
/// log holds that this SDK did not create.
pub(super) fn backfill(meta: &serde_json::Value, created_at: u64) -> Option<Backfilled> {
    let meta: LightningOperationMeta = serde_json::from_value(meta.clone()).ok()?;
    let created_at = Timestamp::from_epoch_millis(created_at);
    match meta {
        LightningOperationMeta::Send(SendOperationMeta {
            contract,
            invoice: LightningInvoice::Bolt11(invoice),
            custom_meta,
            ..
        }) => {
            let invoice = Bolt11Invoice::from_upstream(invoice);
            let invoice_amount = invoice.amount()?;
            // Trusted only when it names this exact invoice: an entry created by something
            // other than this SDK could carry anything under the same metadata key. A copy that
            // passes that check but whose route does not parse (a gateway id from a build this
            // one cannot read) is no more trustworthy than no copy at all, so it falls back to
            // the same upstream-derived estimate as a missing copy.
            let copy = wire::from_custom_meta::<wire::LnSendDetailsWire>(&custom_meta)
                .filter(|copy| copy.invoice == invoice.to_string())
                .and_then(|copy| {
                    Some((
                        Amount::from_msats(copy.fee_msats),
                        Amount::from_msats(copy.total_msats),
                        LightningRoute::try_from(copy.route).ok()?,
                    ))
                });
            let (fee, total, route) = match copy {
                Some(values) => values,
                None => {
                    let total = from_upstream(contract.amount);
                    let fee = total.checked_sub(invoice_amount)?;
                    let route = LightningRoute::Gateway {
                        gateway_id: GatewayId::from_upstream(contract.claim_pk),
                    };
                    (fee, total, route)
                }
            };
            let details = crate::LnSendDetails {
                invoice,
                invoice_amount,
                fee,
                total,
                route,
                created_at,
            };
            Some(Backfilled {
                kind: kinds::LN_SEND,
                details: serde_json::to_string(&wire::LnSendDetailsWire::from(&details)).ok()?,
                phase: None,
            })
        }
        LightningOperationMeta::Receive(meta) => {
            let LightningInvoice::Bolt11(invoice) = meta.invoice;
            let invoice = Bolt11Invoice::from_upstream(invoice);
            let amount = invoice.amount()?;
            let copy = wire::from_custom_meta::<wire::LnReceiveDetailsWire>(&meta.custom_meta);
            let (description, requested_amount, fee, net_credit, created_at) = match &copy {
                Some(copy) => (
                    copy.description.clone(),
                    Amount::from_msats(copy.requested_amount_msats),
                    Amount::from_msats(copy.fee_msats),
                    Amount::from_msats(copy.net_credit_msats),
                    Timestamp::from_epoch_millis(copy.created_at),
                ),
                None => {
                    let net_credit = from_upstream(meta.contract.commitment.amount);
                    let fee = amount.checked_sub(net_credit)?;
                    (invoice.description(), amount, fee, net_credit, created_at)
                }
            };
            let details = LnReceiveDetails {
                invoice: invoice.clone(),
                description,
                requested_amount,
                invoice_amount: amount,
                fee,
                net_credit,
                gateway_id: Some(GatewayId::from_upstream(meta.contract.commitment.refund_pk)),
                expires_at: invoice.expires_at(),
                created_at,
            };
            Some(Backfilled {
                kind: kinds::LN_RECEIVE,
                details: serde_json::to_string(&wire::LnReceiveDetailsWire::from(&details)).ok()?,
                phase: None,
            })
        }
        // An lnurl receive is not an operation this SDK creates.
        LightningOperationMeta::LnurlReceive(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Amount, ErrorDetails};

    fn fee() -> Amount {
        Amount::from_msats(1_050)
    }

    fn route() -> LightningRoute {
        LightningRoute::Gateway {
            gateway_id: "0218845781f631c48f1c9709e23092067d06837f30aa0cd0544ac887fe91ddd166"
                .parse()
                .expect("a gateway id"),
        }
    }

    #[test]
    fn send_states_fold_onto_the_send_lifecycle() {
        let cases = [
            (SendOperationState::Funding, false, LnSendState::Created),
            (SendOperationState::Funded, true, LnSendState::Funded),
            (SendOperationState::Refunding, true, LnSendState::Funded),
            (SendOperationState::Refunded, true, LnSendState::Refunded),
            (
                SendOperationState::Success([0x33; 32]),
                true,
                LnSendState::Success {
                    preimage: Preimage::from_bytes([0x33; 32]),
                    fee: fee(),
                    route: route(),
                },
            ),
        ];
        for (upstream, funded_before, expected) in cases {
            assert_eq!(
                map_send(&upstream, funded_before, fee(), &route()),
                expected
            );
        }
    }

    #[test]
    fn a_failure_before_funding_is_a_refund_and_after_it_is_a_failure() {
        assert_eq!(
            map_send(&SendOperationState::Failure, false, fee(), &route()),
            LnSendState::Refunded
        );
        assert!(matches!(
            map_send(&SendOperationState::Failure, true, fee(), &route()),
            LnSendState::Failed { .. }
        ));
    }

    #[test]
    fn only_funding_advances_the_phase() {
        assert_eq!(send_phase(&SendOperationState::Funding), 0);
        assert_eq!(send_phase(&SendOperationState::Funded), PHASE_FUNDED);
        assert_eq!(send_phase(&SendOperationState::Refunding), PHASE_FUNDED);
        assert_eq!(
            send_phase(&SendOperationState::Success([0; 32])),
            PHASE_FUNDED
        );
        assert_eq!(send_phase(&SendOperationState::Refunded), PHASE_FUNDED);
        assert_eq!(send_phase(&SendOperationState::Failure), 0);
    }

    #[test]
    fn receive_states_fold_onto_the_receive_lifecycle() {
        let cases = [
            (
                ReceiveOperationState::Pending,
                LnReceiveState::WaitingForPayment,
            ),
            (ReceiveOperationState::Claiming, LnReceiveState::Funded),
            (ReceiveOperationState::Claimed, LnReceiveState::Claimed),
            (ReceiveOperationState::Expired, LnReceiveState::Expired),
            (ReceiveOperationState::Failure, LnReceiveState::Failed),
            (ReceiveOperationState::Uneconomical, LnReceiveState::Failed),
        ];
        for (upstream, expected) in cases {
            assert_eq!(map_receive(&upstream), expected);
        }
    }

    const GATEWAY_ID: &str = "0218845781f631c48f1c9709e23092067d06837f30aa0cd0544ac887fe91ddd166";
    const REGTEST_INVOICE: &str = "lnbcrt1u1pj48ugqdq2vdhkven9v5pp5g3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zqsp5242424242424242424242424242424242424242424242424242s9qrsgqcqzys2reg4wsryjt5w8z33ugydecgfmgyvtttwa7e0yzlm803z203j9hqspa4lr6m09cd808xkw9uh4sxc8wf3w6k0gaf5zrqm7zhcxug0vqqpdkpja";

    fn outgoing_meta(contract_msats: u64, custom_meta: serde_json::Value) -> serde_json::Value {
        // `all_zeros` is the `bitcoin::hashes::Hash` trait's; `TransactionId` is a hash newtype.
        use fedimint_core::bitcoin::hashes::Hash;
        use fedimint_lnv2_common::contracts::{OutgoingContract, PaymentImage};

        let key: fedimint_core::secp256k1::PublicKey = GATEWAY_ID.parse().expect("a key");
        let invoice: fedimint_ln_common::lightning_invoice::Bolt11Invoice =
            REGTEST_INVOICE.parse().expect("an invoice");
        let contract = OutgoingContract {
            payment_image: PaymentImage::Hash(*invoice.payment_hash()),
            amount: fedimint_core::Amount::from_msats(contract_msats),
            expiration: 0,
            claim_pk: key,
            refund_pk: key,
            ephemeral_pk: key,
        };
        serde_json::to_value(LightningOperationMeta::Send(SendOperationMeta {
            change_outpoint_range: fedimint_core::OutPointRange::new_single(
                fedimint_core::TransactionId::all_zeros(),
                0,
            )
            .expect("a range"),
            gateway: SafeUrl::parse("http://127.0.0.1:1/").expect("a url"),
            contract,
            invoice: fedimint_lnv2_common::LightningInvoice::Bolt11(invoice),
            custom_meta,
        }))
        .expect("serialises")
    }

    /// A v2 receive log entry, `commitment.amount` set to `net_credit_msats` and `refund_pk` to
    /// `GATEWAY_ID`. The payment image and ciphertext are opaque to `backfill` (nothing here
    /// ever checks or decrypts them), so they are fixed, syntactically valid values rather than
    /// a fresh encryption.
    fn incoming_meta(net_credit_msats: u64, custom_meta: serde_json::Value) -> serde_json::Value {
        const PAYMENT_HASH: &str =
            "107661134f21fc7c02223d50ab9eb3600bc3ffc3712423a1e47bb1f9a9dbf55f";
        const CIPHERTEXT_PK: &str = "b2d3ced866057f6caf291f81630301571816663a5189aab23d06d0a1140e86f12232b0c2200513f942db0978a50f09d3";
        const CIPHERTEXT_SIGNATURE: &str = "abcc29588c6342a99d036733ae838445eb78154452a6eea4492f63a5f512182a2ac9840f779b696b19117a204149216d0e58ad3a0f3d01f404bd0697c27c8aa7aee02a84de30c9f9d90e2eb714f5922efb2768f18a31961108a757afc88089ed";
        serde_json::json!({
            "Receive": {
                "gateway": "http://127.0.0.1:1/",
                "contract": {
                    "commitment": {
                        "payment_image": { "Hash": PAYMENT_HASH },
                        "amount": net_credit_msats,
                        "expiration": 2_000_000_000u64,
                        "claim_pk": GATEWAY_ID,
                        "refund_pk": GATEWAY_ID,
                        "ephemeral_pk": GATEWAY_ID,
                    },
                    "ciphertext": {
                        "encrypted_preimage": vec![0u8; 32],
                        "pk": CIPHERTEXT_PK,
                        "signature": CIPHERTEXT_SIGNATURE,
                    },
                },
                "invoice": { "Bolt11": REGTEST_INVOICE },
                "custom_meta": custom_meta,
            }
        })
    }

    #[test]
    fn a_send_log_entry_backfills_a_send_record() {
        let claimed =
            backfill(&outgoing_meta(101_000, serde_json::Value::Null), 9).expect("claimed");
        assert_eq!(claimed.kind, crate::operation::kinds::LN_SEND);
        let details = wire::decode_send_details(&claimed.details).expect("decodes");
        assert_eq!(details.invoice_amount, Amount::from_msats(100_000));
        assert_eq!(details.fee, Amount::from_msats(1_000));
        assert_eq!(details.total, Amount::from_msats(101_000));
        assert_eq!(details.route, route());
        assert_eq!(details.created_at.epoch_millis(), 9);
    }

    #[test]
    fn a_contract_below_the_invoice_amount_is_not_claimed() {
        assert!(backfill(&outgoing_meta(1, serde_json::Value::Null), 0).is_none());
        assert!(backfill(&serde_json::Value::Null, 0).is_none());
    }

    #[test]
    fn a_send_log_entry_with_an_sdk_copy_uses_the_copys_fee_and_total() {
        let copy = wire::LnSendDetailsWire {
            invoice: REGTEST_INVOICE.to_owned(),
            invoice_amount_msats: 100_000,
            fee_msats: 1_500,
            total_msats: 101_500,
            route: wire::RouteWire::Gateway {
                gateway_id: GATEWAY_ID.to_owned(),
            },
            created_at: 1_650_000_000_000,
        };
        let meta = outgoing_meta(101_000, wire::custom_meta(&copy).expect("encode"));
        let claimed = backfill(&meta, 9).expect("claimed");
        let details = wire::decode_send_details(&claimed.details).expect("decodes");
        // The copy's figures, not the contract-derived estimate (1_000 / 101_000) the log entry
        // alone would give.
        assert_eq!(details.fee, Amount::from_msats(1_500));
        assert_eq!(details.total, Amount::from_msats(101_500));
    }

    #[test]
    fn a_receive_log_entry_with_an_sdk_copy_uses_the_copys_fee_and_net_credit() {
        let copy = wire::LnReceiveDetailsWire {
            invoice: String::new(),
            description: "coffee".to_owned(),
            requested_amount_msats: 100_000,
            invoice_amount_msats: 100_000,
            fee_msats: 750,
            net_credit_msats: 99_250,
            gateway_id: None,
            expires_at: 0,
            created_at: 1_650_000_000_000,
            reclaim_operation_id: None,
        };
        let meta = incoming_meta(99_500, wire::custom_meta(&copy).expect("encode"));
        let claimed = backfill(&meta, 9).expect("claimed");
        let details = wire::decode_receive_details(&claimed.details).expect("decodes");
        // The copy's figures, not the contract-derived estimate (fee 500, net credit 99_500)
        // the log entry alone would give.
        assert_eq!(details.fee, Amount::from_msats(750));
        assert_eq!(details.net_credit, Amount::from_msats(99_250));
        assert_eq!(details.description, "coffee");
        // The gateway id still comes from upstream's own meta, not the copy's placeholder.
        assert_eq!(details.gateway_id, Some(GATEWAY_ID.parse().expect("id")));
    }

    fn a_quote() -> LnQuoteInner {
        LnQuoteInner {
            federation_id: fedimint_core::config::FederationId::dummy(),
            invoice: REGTEST_INVOICE.parse().expect("a valid regtest invoice"),
            invoice_amount: Amount::from_msats(100_000),
            plan: Plan {
                breakdown: crate::LnFeeBreakdown {
                    gateway: Amount::from_msats(0),
                    lightning_module: Amount::from_msats(0),
                    primary_module: Amount::from_msats(0),
                    dust: Amount::from_msats(0),
                },
                fee: Amount::from_msats(0),
                total: Amount::from_msats(100_000),
                route: route(),
                terms: Terms::V1 { gateway: None },
            },
            expires_at: Timestamp::from_epoch_millis(0),
        }
    }

    #[test]
    fn a_wrong_currency_refusal_on_testnet4_carries_the_regtest_invoices_networks() {
        use fedimint_ln_common::lightning_invoice::Currency;

        let quote = a_quote();
        let err = send_error(
            SendPaymentError::WrongCurrency {
                invoice_currency: Currency::BitcoinTestnet,
                federation_currency: Currency::BitcoinTestnet,
            },
            &quote,
            Network::Testnet4,
        );
        assert_eq!(err.code, ErrorCode::NetworkMismatch);
        match err.detail() {
            Some(ErrorDetails::NetworkMismatch {
                expected,
                compatible,
                observed_prefix,
            }) => {
                assert_eq!(*expected, Network::Testnet4);
                assert_eq!(compatible, &vec![Network::Regtest]);
                assert_eq!(observed_prefix, "bcrt");
            }
            other => panic!("expected NetworkMismatch details, got {other:?}"),
        }
    }

    /// A quote with a nonzero gateway fee, so `committed_fee`'s folding of the difference
    /// between the quoted and the committed gateway fee is actually exercised rather than
    /// collapsing on a zero.
    fn quote_with_gateway_fee(gateway_fee_msats: u64) -> LnQuoteInner {
        let mut quote = a_quote();
        quote.plan.breakdown.gateway = Amount::from_msats(gateway_fee_msats);
        quote.plan.breakdown.lightning_module = Amount::from_msats(50);
        quote.plan.fee = Amount::from_msats(gateway_fee_msats + 50);
        quote.plan.total =
            Amount::from_msats(quote.invoice_amount.msats() + gateway_fee_msats + 50);
        quote
    }

    #[test]
    fn committed_fee_keeps_the_quoted_figures_when_the_contract_matches_the_quote() {
        let quote = quote_with_gateway_fee(1_000);
        let committed = Amount::from_msats(101_000); // invoice (100_000) + quoted gateway fee.
        let (fee, total) = committed_fee(&quote, committed).expect("committed fee");
        assert_eq!(fee, quote.plan.fee);
        assert_eq!(total, quote.plan.total);
    }

    #[test]
    fn committed_fee_raises_both_by_the_gateways_increase() {
        let quote = quote_with_gateway_fee(1_000);
        let committed = Amount::from_msats(102_000); // gateway actually took 2_000, not 1_000.
        let (fee, total) = committed_fee(&quote, committed).expect("committed fee");
        assert_eq!(fee, Amount::from_msats(2_050));
        assert_eq!(total, Amount::from_msats(102_050));
    }

    #[test]
    fn committed_fee_lowers_both_by_the_gateways_decrease() {
        let quote = quote_with_gateway_fee(1_000);
        let committed = Amount::from_msats(100_500); // gateway actually took 500, not 1_000.
        let (fee, total) = committed_fee(&quote, committed).expect("committed fee");
        assert_eq!(fee, Amount::from_msats(550));
        assert_eq!(total, Amount::from_msats(100_550));
    }

    #[test]
    fn committed_fee_refuses_a_contract_below_the_invoice_amount() {
        let quote = quote_with_gateway_fee(1_000);
        let committed = Amount::from_msats(99_000); // less than the invoice's own 100_000.
        let err = committed_fee(&quote, committed).expect_err("below the invoice amount");
        assert_eq!(err.code, ErrorCode::Internal);
    }
}
