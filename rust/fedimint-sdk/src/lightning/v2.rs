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
    INVOICE_EXPIRY_SECS, LnQuoteInner, Plan, Terms, add, from_upstream, gateway_unavailable,
    insufficient, internal, now, plan_of, quote_changed, quote_expired, subscribe_error,
    to_upstream, unreachable,
};
use crate::federation::FederationInner;
use crate::operation::{Backfilled, Driver, kinds, record_phase_in};
use crate::{
    Amount, Bolt11Invoice, Error, ErrorCode, GatewayId, LightningRoute, LnReceive,
    LnReceiveDetails, LnReceiveState, LnSendDetails, LnSendState, Operation, Preimage, Result,
    Timestamp,
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
    // The dry run balances the transaction against the real notes, so it fails with the mint's
    // own insufficient-balance error when they cannot cover the contract.
    let quote = module
        .send_fee_quote(to_upstream(contract_amount))
        .await
        .map_err(|err| {
            match err.downcast_ref::<fedimint_mint_client::InsufficientBalanceError>() {
                Some(short) => insufficient(
                    from_upstream(short.requested_amount),
                    from_upstream(short.total_amount),
                ),
                None => internal(format!("could not quote the funding fee: {err}")),
            }
        })?;
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
    let id = module
        .send(
            quote.invoice.inner().clone(),
            Some(gateway),
            serde_json::Value::Null,
        )
        .await
        .map_err(|err| send_error(err, quote))?;
    let details = crate::LnSendDetails {
        invoice: quote.invoice.clone(),
        invoice_amount: quote.invoice_amount,
        fee: quote.plan.fee,
        total: quote.plan.total,
        route: quote.plan.route.clone(),
        created_at: now(),
    };
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

fn select_error(err: SelectGatewayError) -> Error {
    match err {
        SelectGatewayError::FailedToRequestGateways(cause) => unreachable(cause),
        SelectGatewayError::NoGatewaysAvailable | SelectGatewayError::GatewaysUnresponsive => {
            gateway_unavailable(err)
        }
    }
}

fn send_error(err: SendPaymentError, quote: &LnQuoteInner) -> Error {
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
        SendPaymentError::FailedToFundPayment(cause) if cause.contains("Insufficient balance") => {
            Error::new(ErrorCode::InsufficientBalance, cause)
        }
        SendPaymentError::FailedToFundPayment(cause) => {
            internal(format!("the payment could not be funded: {cause}"))
        }
        // Unreachable: `preflight` already refused a foreign-network invoice with full details.
        SendPaymentError::WrongCurrency { .. } => Error::new(
            ErrorCode::NetworkMismatch,
            "the invoice is for another network",
        ),
    }
}

/// Issues an lnv2 invoice through the gateway the module selects, with both the gateway's and
/// the federation's receive-side fees taken out of what will land.
pub(super) async fn receive(
    federation: &Arc<FederationInner>,
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
    let quote = module
        .receive_fee_quote(to_upstream(contract_amount))
        .await
        .map_err(|err| {
            match err.downcast_ref::<fedimint_mint_client::InsufficientBalanceError>() {
                Some(short) => insufficient(
                    from_upstream(short.requested_amount),
                    from_upstream(short.total_amount),
                ),
                None => internal(format!("could not quote the claim fee: {err}")),
            }
        })?;
    let fee = add(gateway_fee, from_upstream(quote.total().get_bitcoin()))?;
    let net_credit = amount.checked_sub(fee).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidInput,
            "the amount does not cover the receive-side fee",
        )
    })?;
    let (invoice, id) = module
        .receive(
            to_upstream(amount),
            INVOICE_EXPIRY_SECS,
            Bolt11InvoiceDescription::Direct(description.to_owned()),
            Some(gateway),
            serde_json::Value::Null,
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
        created_at: now(),
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

/// Rebuilds a record from an lnv2 log entry. The contract carries the gateway's key and the
/// gateway's fee (the contract amount less the invoice amount) but not the federation's, so a
/// rebuilt send's total is a floor and a rebuilt receive's fee is the gateway's share only.
pub(super) fn backfill(meta: &serde_json::Value, created_at: u64) -> Option<Backfilled> {
    let meta: LightningOperationMeta = serde_json::from_value(meta.clone()).ok()?;
    let created_at = Timestamp::from_epoch_millis(created_at);
    match meta {
        LightningOperationMeta::Send(SendOperationMeta {
            contract,
            invoice: LightningInvoice::Bolt11(invoice),
            ..
        }) => {
            let invoice = Bolt11Invoice::from_upstream(invoice);
            let invoice_amount = invoice.amount()?;
            let total = from_upstream(contract.amount);
            let fee = total.checked_sub(invoice_amount)?;
            let details = crate::LnSendDetails {
                invoice,
                invoice_amount,
                fee,
                total,
                route: LightningRoute::Gateway {
                    gateway_id: GatewayId::from_upstream(contract.claim_pk),
                },
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
            let net_credit = from_upstream(meta.contract.commitment.amount);
            let fee = amount.checked_sub(net_credit)?;
            let details = LnReceiveDetails {
                description: invoice.description(),
                requested_amount: amount,
                invoice_amount: amount,
                fee,
                net_credit,
                gateway_id: Some(GatewayId::from_upstream(meta.contract.commitment.refund_pk)),
                expires_at: invoice.expires_at(),
                created_at,
                invoice,
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
    use crate::Amount;

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

    fn outgoing_meta(contract_msats: u64) -> serde_json::Value {
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
            custom_meta: serde_json::Value::Null,
        }))
        .expect("serialises")
    }

    #[test]
    fn a_send_log_entry_backfills_a_send_record() {
        let claimed = backfill(&outgoing_meta(101_000), 9).expect("claimed");
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
        assert!(backfill(&outgoing_meta(1), 0).is_none());
        assert!(backfill(&serde_json::Value::Null, 0).is_none());
    }
}
