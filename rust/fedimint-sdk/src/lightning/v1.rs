//! The v1 lightning module (`ln`): mappings, subscriptions, and the facade operations.

use std::sync::{Arc, Weak};

use fedimint_client::Client;
use fedimint_client_module::ClientModuleInstance;
use fedimint_core::core::OperationId;
use fedimint_core::db::{Database, IDatabaseTransactionOpsCoreTyped};
use fedimint_core::util::BoxStream;
use fedimint_ln_client::LnReceiveState as UpstreamReceiveState;
use fedimint_ln_client::db::PaymentResultKey;
use fedimint_ln_client::receive::LightningReceiveError;
use fedimint_ln_client::{
    InternalPayState, LightningClientModule, LightningOperationMeta, LightningOperationMetaVariant,
    LnPayState, OutgoingLightningPayment, PayBolt11InvoiceError, PayType,
};
use fedimint_ln_common::LightningGateway;
use fedimint_ln_common::config::FeeToAmount;
use fedimint_ln_common::lightning_invoice::{Bolt11InvoiceDescription, Description};
use futures::{StreamExt, stream};

use super::driver::{LnReceiveDriver, LnSendDriver, until_final};
use super::wire::{self, PHASE_FUNDED};
use super::{
    INVOICE_EXPIRY_SECS, LnQuoteInner, Plan, Terms, add, from_upstream, gateway_unavailable,
    insufficient, internal, now, plan_of, quote_changed, quote_expired, subscribe_error,
    to_upstream, unreachable,
};
use crate::federation::FederationInner;
use crate::operation::{Backfilled, Driver, kinds, record_phase_in, write_details_in};
use crate::sdk::SdkInner;
use crate::{
    Amount, Bolt11Invoice, Error, ErrorCode, GatewayId, LightningRoute, LnReceive,
    LnReceiveDetails, LnReceiveState, LnSendDetails, LnSendState, Operation, OperationState,
    Preimage, Result, Timestamp,
};

// Upstream `LnPayState` onto `LnSendState`. The fee and the route come from the executed quote:
// the v1 progress stream carries neither.
//
// | upstream                          | here                                   |
// | --------------------------------- | -------------------------------------- |
// | `Created`                         | `Created`                              |
// | `Funded`, `AwaitingChange`        | `Funded`                               |
// | `WaitingForRefund`                | `Funded` (a refund is still in flight) |
// | `Success { preimage }`            | `Success`                              |
// | `Canceled` (never funded)         | `Refunded`                             |
// | `Refunded`                        | `Refunded`                             |
// | `UnexpectedError`                 | `Failed`                               |
pub(super) fn map_ln_pay(
    state: &LnPayState,
    fee: Amount,
    route: &LightningRoute,
) -> Result<LnSendState> {
    Ok(match state {
        LnPayState::Created => LnSendState::Created,
        LnPayState::Funded { .. }
        | LnPayState::AwaitingChange
        | LnPayState::WaitingForRefund { .. } => LnSendState::Funded,
        LnPayState::Success { preimage } => LnSendState::Success {
            // v1 reports the preimage as the hex text the gateway answered with, unvalidated.
            preimage: preimage.parse::<Preimage>().map_err(|err| {
                Error::new(
                    ErrorCode::Internal,
                    format!("the gateway reported an unusable preimage: {err}"),
                )
            })?,
            fee,
            route: route.clone(),
        },
        LnPayState::Canceled | LnPayState::Refunded { .. } => LnSendState::Refunded,
        LnPayState::UnexpectedError { error_message } => LnSendState::Failed {
            reason: error_message.clone(),
        },
    })
}

// Upstream `InternalPayState` onto `LnSendState`, for a payment settled inside the federation.
//
// | upstream                       | here        |
// | ------------------------------ | ----------- |
// | `Funding`                      | `Created`   |
// | `Preimage`                     | `Success`   |
// | `RefundSuccess`                | `Refunded`  |
// | `FundingFailed` (never debited)| `Refunded`  |
// | `RefundError`                  | `Failed`    |
// | `UnexpectedError`              | `Failed`    |
pub(super) fn map_internal_pay(
    state: &InternalPayState,
    fee: Amount,
    route: &LightningRoute,
) -> LnSendState {
    match state {
        InternalPayState::Funding => LnSendState::Created,
        InternalPayState::Preimage(preimage) => LnSendState::Success {
            preimage: Preimage::from_bytes(preimage.0),
            fee,
            route: route.clone(),
        },
        InternalPayState::RefundSuccess { .. } | InternalPayState::FundingFailed { .. } => {
            LnSendState::Refunded
        }
        InternalPayState::RefundError { error_message, .. } => LnSendState::Failed {
            reason: error_message.clone(),
        },
        InternalPayState::UnexpectedError(message) => LnSendState::Failed {
            reason: message.clone(),
        },
    }
}

/// The v1 module on a live client, or `NotSupported` when the federation dropped it.
pub(super) fn module_of(
    client: &Client,
) -> Result<ClientModuleInstance<'_, LightningClientModule>> {
    client
        .get_first_module::<LightningClientModule>()
        .map_err(|_| {
            Error::new(
                ErrorCode::NotSupported,
                "this federation no longer has a v1 lightning module",
            )
        })
}

/// A fresh stream over a v1 send, chosen by the route the record says the payment took.
///
/// The client guard is held only for the bounded upstream read that produces the stream; the
/// stream itself registers with the module's notifier when first polled, as the driver contract
/// requires.
pub(super) async fn subscribe_send(
    federation: &FederationInner,
    id: OperationId,
    details: &LnSendDetails,
) -> Result<BoxStream<'static, Result<LnSendState>>> {
    let client = federation.client(false).await?;
    let module = module_of(&client)?;
    let fee = details.fee;
    let route = details.route.clone();
    let stream: BoxStream<'static, Result<LnSendState>> = match &route {
        LightningRoute::Internal => {
            let upstream = module
                .subscribe_internal_pay(id)
                .await
                .map_err(subscribe_error)?
                .into_stream();
            Box::pin(upstream.map(move |state| Ok(map_internal_pay(&state, fee, &route))))
        }
        LightningRoute::Gateway { .. } => {
            let upstream = module
                .subscribe_ln_pay(id)
                .await
                .map_err(subscribe_error)?
                .into_stream();
            Box::pin(upstream.map(move |state| map_ln_pay(&state, fee, &route)))
        }
    };
    Ok(until_final(stream))
}

/// What the original receive's next upstream state means: a state to hand out, or the signal to
/// retry the claim.
pub(super) enum ReceiveStep {
    State(LnReceiveState),
    Reclaim,
}

// Upstream v1 `LnReceiveState` onto `LnReceiveState`, keyed on whether the receive was ever seen
// funded. `Rejected` means two things: the invoice-registration transaction refused (before any
// funding, so `Canceled`), or the claim's primary outputs failing after a confirmed payment (at
// or after funding, so `Failed`). `ClaimRejected` and `InvalidPreimage` presuppose a funded
// contract and are not phase-keyed.
//
// | upstream                       | phase reached        | here                   |
// | ------------------------------ | -------------------- | ---------------------- |
// | `Created`                      | any                  | `Created`              |
// | `WaitingForPayment`            | any                  | `WaitingForPayment`    |
// | `Funded`, `AwaitingFunds`      | any                  | `Funded`               |
// | `Claimed`                      | any                  | `Claimed`              |
// | `Canceled { Timeout }`         | any                  | `Expired`              |
// | `Canceled { ClaimRejected }`   | any                  | `Funded`, then reclaim |
// | `Canceled { InvalidPreimage }` | any                  | `Failed`               |
// | `Canceled { Rejected }`        | before `Funded`      | `Canceled`             |
// | `Canceled { Rejected }`        | at or after `Funded` | `Failed`               |
pub(super) fn map_receive(state: &UpstreamReceiveState, funded_before: bool) -> ReceiveStep {
    ReceiveStep::State(match state {
        UpstreamReceiveState::Created => LnReceiveState::Created,
        UpstreamReceiveState::WaitingForPayment { .. } => LnReceiveState::WaitingForPayment,
        UpstreamReceiveState::Funded | UpstreamReceiveState::AwaitingFunds => {
            LnReceiveState::Funded
        }
        UpstreamReceiveState::Claimed => LnReceiveState::Claimed,
        UpstreamReceiveState::Canceled { reason } => match reason {
            LightningReceiveError::Timeout => LnReceiveState::Expired,
            LightningReceiveError::ClaimRejected => return ReceiveStep::Reclaim,
            LightningReceiveError::InvalidPreimage => LnReceiveState::Failed,
            LightningReceiveError::Rejected if funded_before => LnReceiveState::Failed,
            LightningReceiveError::Rejected => LnReceiveState::Canceled {
                reason: reason.to_string(),
            },
        },
    })
}

// A retried claim runs as its own upstream operation, started in the confirmed-invoice state:
// the money is in the contract, so everything short of the end is `Funded`, and any cancellation
// of the retry is the end of the road.
pub(super) fn map_reclaim(state: &UpstreamReceiveState) -> LnReceiveState {
    match state {
        UpstreamReceiveState::Claimed => LnReceiveState::Claimed,
        UpstreamReceiveState::Canceled { .. } => LnReceiveState::Failed,
        UpstreamReceiveState::Created
        | UpstreamReceiveState::WaitingForPayment { .. }
        | UpstreamReceiveState::Funded
        | UpstreamReceiveState::AwaitingFunds => LnReceiveState::Funded,
    }
}

/// What a receive subscription needs to retry a rejected claim from inside its own stream.
///
/// A stream outlives the borrow it was made from, so it cannot hold a client guard; it holds the
/// instance and the federation id instead and takes a guard again when the retry happens.
struct ReclaimContext {
    sdk: Weak<SdkInner>,
    federation_id: fedimint_core::config::FederationId,
    db: Database,
    id: OperationId,
}

/// How starting a retry ended: a stream to follow, or the module's word that no retry is
/// possible.
enum ReclaimStart {
    Started(BoxStream<'static, UpstreamReceiveState>),
    Impossible(String),
}

impl ReclaimContext {
    /// Starts the retry upstream, records which operation it runs under, and returns its
    /// stream — or the module's word that no further claim is possible.
    async fn start(&self) -> Result<ReclaimStart> {
        let closed = || {
            Error::new(
                ErrorCode::FederationClosed,
                "this federation stopped running",
            )
        };
        let sdk = self.sdk.upgrade().ok_or_else(closed)?;
        let federation = sdk
            .federation_inner(&self.federation_id)
            .ok_or_else(closed)?;

        // Two subscribers can both observe the same `Canceled { ClaimRejected }` and both reach
        // here. The SDK is the only process that opens this federation's storage (the `fd-lock`
        // in `src/storage.rs`), so holding this per-federation lock across the record re-read,
        // the upstream call and the write below is enough, in-process, to make "one retry per
        // rejected claim" exact rather than a race one side merely tends to lose. Taken before
        // `federation.client(false)` so a close waiting on the client's write lock is not held
        // off by a retry that has not started yet.
        let _serialised = federation.lock_reclaim_starts().await;

        let client = federation.client(false).await?;
        let module = module_of(&client)?;

        // Re-read the record rather than trust details carried from where this context was
        // built: with the lock held, whichever subscriber runs second now sees the first's
        // `reclaim_operation_id` and follows it instead of starting a second retry.
        let mut dbtx = self.db.begin_transaction_nc().await;
        let record = dbtx
            .get_value(&crate::db::OperationRecordKey(self.id))
            .await
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Internal,
                    format!("no record for operation {}", self.id.fmt_full()),
                )
            })?;
        let mut details = wire::decode_receive_wire(&record.details)?;
        if let Some(reclaim) = details.reclaim_operation_id.as_deref() {
            return Ok(ReclaimStart::Started(
                follow_reclaim(&module, reclaim).await?,
            ));
        }

        let reclaim_id = match module.reclaim_ln_receive(self.id).await {
            Ok(id) => id,
            Err(err) => {
                return classify_reclaim_refusal(&err.to_string()).map(ReclaimStart::Impossible);
            }
        };
        // Persisted before it is followed: after a restart the record is the only thing that
        // says which upstream operation to follow.
        details.reclaim_operation_id =
            Some(crate::OperationId::from_upstream(reclaim_id).to_string());
        write_details_in(&self.db, self.id, wire::encode_receive_wire(&details)?).await?;
        Ok(ReclaimStart::Started(
            module
                .subscribe_ln_receive(reclaim_id)
                .await
                .map_err(subscribe_error)?
                .into_stream(),
        ))
    }
}

/// Follows a retry that was already started, from the id the record stores for it.
async fn follow_reclaim(
    module: &LightningClientModule,
    reclaim: &str,
) -> Result<BoxStream<'static, UpstreamReceiveState>> {
    let reclaim_id = reclaim
        .parse::<crate::OperationId>()
        .map_err(|err| {
            Error::new(
                ErrorCode::Internal,
                format!("a stored operation id does not parse: {err}"),
            )
        })?
        .upstream();
    Ok(module
        .subscribe_ln_receive(reclaim_id)
        .await
        .map_err(subscribe_error)?
        .into_stream())
}

// Upstream refuses `reclaim_ln_receive` two ways. "Cannot reclaim an active lightning receive"
// (fedimint-ln-client/src/lib.rs, in `reclaim_ln_receive`) is the race between its notifier
// reporting `Canceled { ClaimRejected }` and the state machine itself going inactive: retrying
// later succeeds, so this is an observation failure, not a definitive answer. Any other refusal
// (not a reclaimable receive, the receiving key unrecoverable from history) is definitive: no
// further claim is possible.
fn classify_reclaim_refusal(text: &str) -> Result<String> {
    if text.contains("active") {
        return Err(Error::new(
            ErrorCode::Internal,
            format!("the rejected claim cannot be retried yet: {text}"),
        ));
    }
    Ok(text.to_owned())
}

/// Which upstream operation a receive subscription is following.
enum Following {
    Original,
    Reclaim,
}

/// The per-subscription cursor of a v1 receive.
struct Follow {
    upstream: BoxStream<'static, UpstreamReceiveState>,
    following: Following,
    phase: u32,
    done: bool,
    context: ReclaimContext,
}

/// One step of the receive stream: the next mapped state, having persisted the phase it proves
/// and switched to the retried claim if one was needed.
async fn step(mut follow: Follow) -> Option<(Result<LnReceiveState>, Follow)> {
    if follow.done {
        return None;
    }
    let state = follow.upstream.next().await?;
    let mapped = match follow.following {
        Following::Reclaim => map_reclaim(&state),
        Following::Original => match map_receive(&state, follow.phase >= PHASE_FUNDED) {
            ReceiveStep::State(state) => state,
            // `ClaimRejected` is not final here: the claim is retried under the same SDK
            // operation, and only a retry that cannot be started is the end.
            ReceiveStep::Reclaim => match follow.context.start().await {
                Ok(ReclaimStart::Started(upstream)) => {
                    follow.upstream = upstream;
                    follow.following = Following::Reclaim;
                    LnReceiveState::Funded
                }
                // The module says no further claim is possible: that is the one ending the
                // contract reserves `Failed` for. The reason is logged here, its only reader,
                // since `LnReceiveState::Failed` does not carry it.
                Ok(ReclaimStart::Impossible(reason)) => {
                    tracing::warn!(
                        target: "fedimint_sdk",
                        operation = %follow.context.id.fmt_full(),
                        reason = %reason,
                        "a rejected lightning claim cannot be retried",
                    );
                    LnReceiveState::Failed
                }
                // Anything else is an observation failure, not an outcome: the next subscription
                // replays the rejection and tries again.
                Err(err) => {
                    follow.done = true;
                    return Some((Err(err), follow));
                }
            },
        },
    };
    if matches!(mapped, LnReceiveState::Funded) && follow.phase < PHASE_FUNDED {
        follow.phase = PHASE_FUNDED;
        if let Err(err) = record_phase_in(&follow.context.db, follow.context.id, PHASE_FUNDED).await
        {
            follow.done = true;
            return Some((Err(err), follow));
        }
    }
    follow.done = mapped.is_final();
    Some((Ok(mapped), follow))
}

/// A fresh stream over a v1 receive, following the retried claim instead of the original when
/// the record says one was started.
pub(super) async fn subscribe_receive(
    federation: &FederationInner,
    id: OperationId,
    phase: u32,
    details_json: &str,
) -> Result<BoxStream<'static, Result<LnReceiveState>>> {
    let details = wire::decode_receive_wire(details_json)?;
    let client = federation.client(false).await?;
    let module = module_of(&client)?;
    let (upstream, following) = match details.reclaim_operation_id.as_deref() {
        Some(reclaim) => (follow_reclaim(&module, reclaim).await?, Following::Reclaim),
        None => {
            let upstream = module
                .subscribe_ln_receive(id)
                .await
                .map_err(subscribe_error)?
                .into_stream();
            (upstream, Following::Original)
        }
    };
    let follow = Follow {
        upstream,
        following,
        phase,
        done: false,
        context: ReclaimContext {
            sdk: federation.sdk.clone(),
            federation_id: federation.id,
            db: federation.db(),
            id,
        },
    };
    Ok(Box::pin(stream::unfold(follow, step)))
}

/// Plans a v1 payment: refreshes the gateway list, decides the route the module will take, picks
/// the cheapest online gateway when one is needed, and prices the funding transaction.
pub(super) async fn plan(
    client: &Client,
    module: &LightningClientModule,
    invoice: &Bolt11Invoice,
    amount: Amount,
) -> Result<Plan> {
    module
        .update_gateway_cache()
        .await
        .map_err(|err| unreachable(format!("could not refresh the gateway list: {err}")))?;
    let gateway = if is_internal(client, module, invoice).await? {
        None
    } else {
        Some(Box::new(
            module
                .select_available_gateway(None, Some(invoice.inner().clone()))
                .await
                .map_err(gateway_unavailable)?,
        ))
    };
    terms_for(module, invoice, amount, gateway).await
}

/// Whether the module will settle this invoice inside the federation: the same two tests
/// `pay_bolt11_invoice` runs (`fedimint-ln-client/src/lib.rs:1409-1418`), so the quote's route is
/// the route the payment takes. Both look at the last hop of the invoice's first route hint.
async fn is_internal(
    client: &Client,
    module: &LightningClientModule,
    invoice: &Bolt11Invoice,
) -> Result<bool> {
    let last_hop = invoice.inner().route_hints().first().and_then(|hint| {
        hint.0
            .last()
            .map(|hop| (hop.src_node_id, hop.short_channel_id))
    });
    let Some(last_hop) = last_hop else {
        return Ok(false);
    };
    let markers = client
        .get_internal_payment_markers()
        .map_err(|err| internal(format!("no internal payment markers: {err}")))?;
    if last_hop == markers {
        return Ok(true);
    }
    Ok(module
        .list_gateways()
        .await
        .into_iter()
        .any(|gateway| last_hop == (gateway.info.node_pub_key, gateway.info.federation_index)))
}

/// Prices a payment through `gateway` (or internally for `None`): the gateway's charge from its
/// fee schedule, the module's own output fee, and the shared quote for funding the contract.
///
/// The gateway is boxed as `Terms::V1` holds it: a `LightningGateway` is a few hundred bytes and
/// clippy's `large_enum_variant` refuses it inline.
async fn terms_for(
    module: &LightningClientModule,
    invoice: &Bolt11Invoice,
    amount: Amount,
    gateway: Option<Box<LightningGateway>>,
) -> Result<Plan> {
    // The contract is funded for the invoice amount plus the gateway's fee
    // (`fedimint-ln-client/src/lib.rs:866-867`); an internal payment funds exactly the amount.
    let gateway_fee = gateway.as_ref().map_or(Amount::from_msats(0), |gateway| {
        from_upstream(gateway.fees.to_amount(&to_upstream(amount)))
    });
    let contract_amount = add(amount, gateway_fee)?;
    let quote = module
        .send_fee_quote(to_upstream(contract_amount))
        .await
        .map_err(|err| internal(format!("could not quote the funding fee: {err}")))?;
    let lightning_module = from_upstream(module.cfg.fee_consensus.contract_output);
    let route = match &gateway {
        None => LightningRoute::Internal,
        Some(gateway) => LightningRoute::Gateway {
            gateway_id: GatewayId::from_upstream(gateway.gateway_id),
        },
    };
    plan_of(
        gateway_fee,
        lightning_module,
        &quote,
        amount,
        route,
        Terms::V1 { gateway },
    )
}

/// Executes a v1 quote: re-reads every bound input, refuses on drift, funds, records.
pub(super) async fn send(
    federation: &Arc<FederationInner>,
    module: &ClientModuleInstance<'_, LightningClientModule>,
    quote: &LnQuoteInner,
    gateway: Option<Box<LightningGateway>>,
) -> Result<Operation<LnSendState>> {
    // The gateway may have withdrawn or changed its fees since the quote; the cache is what the
    // module pays through, so it is what is checked.
    let gateway = match gateway {
        None => None,
        Some(quoted) => {
            let Some(current) = module.select_gateway(&quoted.gateway_id).await else {
                return Err(Error::new(
                    ErrorCode::QuoteChanged,
                    "the quoted gateway is no longer registered with this federation",
                ));
            };
            Some(Box::new(current))
        }
    };
    let fresh = terms_for(
        module,
        &quote.invoice,
        quote.invoice_amount,
        gateway.clone(),
    )
    .await?;
    if fresh.total != quote.plan.total {
        return Err(quote_changed(quote.plan.total, fresh.total));
    }
    // A completed earlier payment of this invoice would be answered as if it were new
    // (`fedimint-ln-client/src/lib.rs:1356-1358`); the module's own idempotency record says so
    // before anything is funded.
    let payment_hash = *quote.invoice.inner().payment_hash();
    let already_paid = module
        .db
        .begin_transaction_nc()
        .await
        .get_value(&PaymentResultKey { payment_hash })
        .await
        .is_some_and(|result| result.completed_payment.is_some());
    if already_paid {
        return Err(quote_expired(quote.expires_at, true));
    }
    let payment: OutgoingLightningPayment = module
        .pay_bolt11_invoice(
            gateway.map(|gateway| *gateway),
            quote.invoice.inner().clone(),
            serde_json::Value::Null,
        )
        .await
        .map_err(|err| {
            if let Some(known) = err.downcast_ref::<PayBolt11InvoiceError>() {
                return match known {
                    PayBolt11InvoiceError::PreviousPaymentAttemptStillInProgress { .. }
                    | PayBolt11InvoiceError::FundedContractAlreadyExists { .. } => {
                        quote_expired(quote.expires_at, true)
                    }
                    PayBolt11InvoiceError::NoLnGatewayAvailable => {
                        gateway_unavailable("the module found no gateway for this route")
                    }
                };
            }
            if let Some(short) =
                err.downcast_ref::<fedimint_mint_client::InsufficientBalanceError>()
            {
                return insufficient(
                    from_upstream(short.requested_amount),
                    from_upstream(short.total_amount),
                );
            }
            let text = err.to_string();
            if text.contains("Invoice has expired") {
                return quote_expired(quote.expires_at, false);
            }
            if text.contains("Insufficient balance") {
                return Error::new(ErrorCode::InsufficientBalance, text);
            }
            internal(format!("the payment could not be started: {text}"))
        })?;
    let (id, route) = match payment.payment_type {
        PayType::Internal(id) => (id, LightningRoute::Internal),
        PayType::Lightning(id) => (id, quote.plan.route.clone()),
    };
    let details = crate::LnSendDetails {
        invoice: quote.invoice.clone(),
        invoice_amount: quote.invoice_amount,
        fee: quote.plan.fee,
        total: quote.plan.total,
        route,
        created_at: now(),
    };
    federation
        .create_operation(
            id,
            kinds::LN_SEND,
            "ln",
            &wire::LnSendDetailsWire::from(&details),
            Arc::new(LnSendDriver) as Arc<dyn Driver<LnSendState>>,
        )
        .await
}

/// Issues a v1 invoice through the cheapest online gateway and records it.
pub(super) async fn receive(
    federation: &Arc<FederationInner>,
    module: &LightningClientModule,
    amount: Amount,
    description: &str,
) -> Result<LnReceive> {
    let bolt11_description = Description::new(description.to_owned()).map_err(|err| {
        Error::new(
            ErrorCode::InvalidInput,
            format!("the description cannot be carried by an invoice: {err}"),
        )
    })?;
    module
        .update_gateway_cache()
        .await
        .map_err(|err| unreachable(format!("could not refresh the gateway list: {err}")))?;
    let gateway = module
        .select_available_gateway(None, None)
        .await
        .map_err(gateway_unavailable)?;
    // v1 takes no gateway fee on the way in: the gateway funds the contract for the invoice's
    // amount and the only deduction is the federation's fee for claiming it.
    let quote = module
        .receive_fee_quote(to_upstream(amount))
        .await
        .map_err(|err| internal(format!("could not quote the claim fee: {err}")))?;
    let fee = from_upstream(quote.total().get_bitcoin());
    let net_credit = amount.checked_sub(fee).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidInput,
            "the amount does not cover the receive-side fee",
        )
    })?;
    let (id, invoice, _preimage) = module
        .create_bolt11_invoice(
            to_upstream(amount),
            Bolt11InvoiceDescription::Direct(bolt11_description),
            Some(u64::from(INVOICE_EXPIRY_SECS)),
            serde_json::Value::Null,
            Some(gateway.clone()),
        )
        .await
        .map_err(|err| unreachable(format!("the invoice could not be registered: {err}")))?;
    let invoice = Bolt11Invoice::from_upstream(invoice);
    let details = LnReceiveDetails {
        invoice: invoice.clone(),
        description: description.to_owned(),
        requested_amount: amount,
        invoice_amount: amount,
        fee,
        net_credit,
        gateway_id: Some(GatewayId::from_upstream(gateway.gateway_id)),
        expires_at: invoice.expires_at(),
        created_at: now(),
    };
    let operation = federation
        .create_operation(
            id,
            kinds::LN_RECEIVE,
            "ln",
            &wire::LnReceiveDetailsWire::from(&details),
            Arc::new(LnReceiveDriver) as Arc<dyn Driver<LnReceiveState>>,
        )
        .await?;
    Ok(LnReceive { invoice, operation })
}

/// Rebuilds a record from a v1 log entry. Best effort by nature: the entry carries the gateway's
/// fee but not the federation's, so a rebuilt send's total is a floor and a rebuilt receive
/// reports no fee at all.
pub(super) fn backfill(meta: &serde_json::Value, created_at: u64) -> Option<Backfilled> {
    let meta: LightningOperationMeta = serde_json::from_value(meta.clone()).ok()?;
    let created_at = Timestamp::from_epoch_millis(created_at);
    match meta.variant {
        LightningOperationMetaVariant::Pay(pay) => {
            let invoice = Bolt11Invoice::from_upstream(pay.invoice);
            let invoice_amount = invoice.amount()?;
            let fee = from_upstream(pay.fee);
            let route = if pay.is_internal_payment {
                LightningRoute::Internal
            } else {
                LightningRoute::Gateway {
                    gateway_id: GatewayId::from_upstream(pay.gateway_id?),
                }
            };
            let details = crate::LnSendDetails {
                invoice,
                invoice_amount,
                fee,
                total: invoice_amount.checked_add(fee)?,
                route,
                created_at,
            };
            Some(Backfilled {
                kind: kinds::LN_SEND,
                details: serde_json::to_string(&wire::LnSendDetailsWire::from(&details)).ok()?,
                phase: None,
            })
        }
        LightningOperationMetaVariant::Receive {
            invoice,
            gateway_id,
            ..
        } => {
            let invoice = Bolt11Invoice::from_upstream(invoice);
            let amount = invoice.amount()?;
            let details = LnReceiveDetails {
                description: invoice.description(),
                requested_amount: amount,
                invoice_amount: amount,
                fee: Amount::from_msats(0),
                net_credit: amount,
                gateway_id: gateway_id.map(GatewayId::from_upstream),
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
        // A retried claim runs under the original receive's record; the deprecated claim path
        // and recurring payments are not operations this SDK creates.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use fedimint_ln_client::pay::GatewayPayError;

    use super::*;
    use crate::Amount;

    const GATEWAY_ID: &str = "0218845781f631c48f1c9709e23092067d06837f30aa0cd0544ac887fe91ddd166";
    const REGTEST_INVOICE: &str = "lnbcrt1u1pj48ugqdq2vdhkven9v5pp5g3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zqsp5242424242424242424242424242424242424242424242424242s9qrsgqcqzys2reg4wsryjt5w8z33ugydecgfmgyvtttwa7e0yzlm803z203j9hqspa4lr6m09cd808xkw9uh4sxc8wf3w6k0gaf5zrqm7zhcxug0vqqpdkpja";

    fn fee() -> Amount {
        Amount::from_msats(1_050)
    }

    fn gateway_route() -> LightningRoute {
        LightningRoute::Gateway {
            gateway_id: GATEWAY_ID.parse().expect("a gateway id"),
        }
    }

    #[test]
    fn ln_pay_states_fold_onto_the_send_lifecycle() {
        let cases = [
            (LnPayState::Created, LnSendState::Created),
            (LnPayState::Funded { block_height: 7 }, LnSendState::Funded),
            (LnPayState::AwaitingChange, LnSendState::Funded),
            (
                LnPayState::WaitingForRefund {
                    error_reason: "no route".to_owned(),
                },
                LnSendState::Funded,
            ),
            (LnPayState::Canceled, LnSendState::Refunded),
            (
                LnPayState::Refunded {
                    gateway_error: GatewayPayError::OutgoingContractError,
                },
                LnSendState::Refunded,
            ),
            (
                LnPayState::UnexpectedError {
                    error_message: "boom".to_owned(),
                },
                LnSendState::Failed {
                    reason: "boom".to_owned(),
                },
            ),
        ];
        for (upstream, expected) in cases {
            assert_eq!(
                map_ln_pay(&upstream, fee(), &gateway_route()).expect("maps"),
                expected,
                "{upstream:?}"
            );
        }
    }

    #[test]
    fn a_v1_success_parses_the_hex_preimage_and_carries_the_quoted_terms() {
        let mapped = map_ln_pay(
            &LnPayState::Success {
                preimage: "11".repeat(32),
            },
            fee(),
            &gateway_route(),
        )
        .expect("maps");
        assert_eq!(
            mapped,
            LnSendState::Success {
                preimage: Preimage::from_bytes([0x11; 32]),
                fee: fee(),
                route: gateway_route(),
            }
        );
    }

    #[test]
    fn a_v1_success_with_an_unparsable_preimage_is_internal() {
        let err = map_ln_pay(
            &LnPayState::Success {
                preimage: "not hex".to_owned(),
            },
            fee(),
            &gateway_route(),
        )
        .expect_err("refused");
        assert_eq!(err.code, ErrorCode::Internal);
    }

    #[test]
    fn internal_pay_states_fold_onto_the_send_lifecycle() {
        use fedimint_ln_client::incoming::IncomingSmError;

        let error = IncomingSmError::TimeoutFetchingOffer {
            payment_hash: fedimint_core::bitcoin::hashes::Hash::hash(b"x"),
        };
        let cases = [
            (InternalPayState::Funding, LnSendState::Created),
            (
                InternalPayState::Preimage(fedimint_ln_common::contracts::Preimage([0x22; 32])),
                LnSendState::Success {
                    preimage: Preimage::from_bytes([0x22; 32]),
                    fee: fee(),
                    route: LightningRoute::Internal,
                },
            ),
            (
                InternalPayState::RefundSuccess {
                    out_points: Vec::new(),
                    error: error.clone(),
                },
                LnSendState::Refunded,
            ),
            (
                InternalPayState::FundingFailed {
                    error: error.clone(),
                },
                LnSendState::Refunded,
            ),
            (
                InternalPayState::RefundError {
                    error_message: "stuck".to_owned(),
                    error,
                },
                LnSendState::Failed {
                    reason: "stuck".to_owned(),
                },
            ),
            (
                InternalPayState::UnexpectedError("odd".to_owned()),
                LnSendState::Failed {
                    reason: "odd".to_owned(),
                },
            ),
        ];
        for (upstream, expected) in cases {
            assert_eq!(
                map_internal_pay(&upstream, fee(), &LightningRoute::Internal),
                expected,
                "{upstream:?}"
            );
        }
    }

    #[test]
    fn receive_states_fold_onto_the_receive_lifecycle() {
        use fedimint_ln_client::receive::LightningReceiveError;

        let waiting = UpstreamReceiveState::WaitingForPayment {
            invoice: String::new(),
            timeout: core::time::Duration::from_secs(1),
        };
        let cases = [
            (
                UpstreamReceiveState::Created,
                false,
                LnReceiveState::Created,
            ),
            (waiting, false, LnReceiveState::WaitingForPayment),
            (UpstreamReceiveState::Funded, false, LnReceiveState::Funded),
            (
                UpstreamReceiveState::AwaitingFunds,
                true,
                LnReceiveState::Funded,
            ),
            (UpstreamReceiveState::Claimed, true, LnReceiveState::Claimed),
            (
                UpstreamReceiveState::Canceled {
                    reason: LightningReceiveError::Timeout,
                },
                true,
                LnReceiveState::Expired,
            ),
            (
                UpstreamReceiveState::Canceled {
                    reason: LightningReceiveError::InvalidPreimage,
                },
                false,
                LnReceiveState::Failed,
            ),
            (
                UpstreamReceiveState::Canceled {
                    reason: LightningReceiveError::Rejected,
                },
                false,
                LnReceiveState::Canceled {
                    reason: LightningReceiveError::Rejected.to_string(),
                },
            ),
            (
                UpstreamReceiveState::Canceled {
                    reason: LightningReceiveError::Rejected,
                },
                true,
                LnReceiveState::Failed,
            ),
        ];
        for (upstream, funded_before, expected) in cases {
            match map_receive(&upstream, funded_before) {
                ReceiveStep::State(state) => assert_eq!(state, expected, "{upstream:?}"),
                ReceiveStep::Reclaim => panic!("{upstream:?} must not ask for a reclaim"),
            }
        }
    }

    #[test]
    fn a_rejected_claim_asks_for_a_retry_whatever_the_phase() {
        use fedimint_ln_client::receive::LightningReceiveError;

        for funded_before in [false, true] {
            let step = map_receive(
                &UpstreamReceiveState::Canceled {
                    reason: LightningReceiveError::ClaimRejected,
                },
                funded_before,
            );
            assert!(matches!(step, ReceiveStep::Reclaim));
        }
    }

    #[test]
    fn a_retried_claim_is_funded_until_it_is_claimed_or_fails() {
        use fedimint_ln_client::receive::LightningReceiveError;

        assert_eq!(
            map_reclaim(&UpstreamReceiveState::Created),
            LnReceiveState::Funded
        );
        assert_eq!(
            map_reclaim(&UpstreamReceiveState::Funded),
            LnReceiveState::Funded
        );
        assert_eq!(
            map_reclaim(&UpstreamReceiveState::AwaitingFunds),
            LnReceiveState::Funded
        );
        assert_eq!(
            map_reclaim(&UpstreamReceiveState::Claimed),
            LnReceiveState::Claimed
        );
        for reason in [
            LightningReceiveError::Rejected,
            LightningReceiveError::Timeout,
            LightningReceiveError::ClaimRejected,
            LightningReceiveError::InvalidPreimage,
        ] {
            assert_eq!(
                map_reclaim(&UpstreamReceiveState::Canceled { reason }),
                LnReceiveState::Failed
            );
        }
    }

    #[test]
    fn the_notifiers_active_receive_race_is_retryable() {
        let err = classify_reclaim_refusal("Cannot reclaim an active lightning receive")
            .expect_err("the race is retryable, not definitive");
        assert_eq!(err.code, ErrorCode::Internal);
    }

    #[test]
    fn any_other_refusal_is_definitive() {
        let text = classify_reclaim_refusal("Operation is not a reclaimable lightning receive")
            .expect("a refusal that is not the race is definitive");
        assert_eq!(text, "Operation is not a reclaimable lightning receive");
    }

    #[test]
    fn a_pay_log_entry_backfills_a_send_record() {
        let meta = serde_json::json!({
            "variant": {
                "pay": {
                    "out_point": { "txid": "00".repeat(32), "out_idx": 0 },
                    "invoice": REGTEST_INVOICE,
                    "fee": 1000,
                    "change": [],
                    "is_internal_payment": false,
                    "contract_id": "11".repeat(32),
                    "gateway_id": GATEWAY_ID,
                }
            },
            "extra_meta": null,
        });
        let claimed = backfill(&meta, 1_700_000_000_000).expect("claimed");
        assert_eq!(claimed.kind, crate::operation::kinds::LN_SEND);
        assert_eq!(claimed.phase, None);
        let details = wire::decode_send_details(&claimed.details).expect("decodes");
        assert_eq!(details.invoice_amount, Amount::from_msats(100_000));
        assert_eq!(details.fee, Amount::from_msats(1_000));
        assert_eq!(details.total, Amount::from_msats(101_000));
        assert_eq!(details.route, gateway_route());
        assert_eq!(details.created_at.epoch_millis(), 1_700_000_000_000);
    }

    #[test]
    fn a_receive_log_entry_backfills_a_receive_record() {
        let meta = serde_json::json!({
            "variant": {
                "receive": {
                    "out_point": { "txid": "00".repeat(32), "out_idx": 0 },
                    "invoice": REGTEST_INVOICE,
                    "gateway_id": GATEWAY_ID,
                }
            },
            "extra_meta": null,
        });
        let claimed = backfill(&meta, 7).expect("claimed");
        assert_eq!(claimed.kind, crate::operation::kinds::LN_RECEIVE);
        let details = wire::decode_receive_details(&claimed.details).expect("decodes");
        assert_eq!(details.invoice_amount, Amount::from_msats(100_000));
        assert_eq!(details.requested_amount, Amount::from_msats(100_000));
        // The fee is not in the log entry; a rebuilt record reports none.
        assert_eq!(details.fee, Amount::from_msats(0));
        assert_eq!(details.net_credit, Amount::from_msats(100_000));
        assert_eq!(details.gateway_id, Some(GATEWAY_ID.parse().expect("id")));
    }

    #[test]
    fn other_v1_log_entries_are_not_claimed() {
        let meta = serde_json::json!({
            "variant": { "claim": { "out_points": [] } },
            "extra_meta": null,
        });
        assert!(backfill(&meta, 0).is_none());
        assert!(backfill(&serde_json::Value::Null, 0).is_none());
    }
}
