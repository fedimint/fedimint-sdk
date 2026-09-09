//! The v1 lightning module (`ln`): mappings, subscriptions, and the facade operations.

use std::sync::{Arc, Weak};

use fedimint_client::Client;
use fedimint_client_module::ClientModuleInstance;
use fedimint_core::bitcoin::hashes::{Hash, sha256};
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
    INVOICE_EXPIRY_SECS, LnQuoteInner, Plan, Terms, add, balance_of, fee_quote_failure,
    from_upstream, gateway_unavailable, insufficient, internal, network_refusal, now, plan_of,
    quote_changed, quote_expired, subscribe_error, to_upstream, unreachable,
};
use crate::federation::FederationInner;
use crate::operation::{Backfilled, Driver, kinds, record_phase_in, write_details_in};
use crate::sdk::SdkInner;
use crate::{
    Amount, Bolt11Invoice, Error, ErrorCode, GatewayId, LightningRoute, LnReceive,
    LnReceiveDetails, LnReceiveState, LnSendDetails, LnSendState, Network, Operation,
    OperationState, Preimage, Result, Timestamp,
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
///
/// `Clone` so a copy can be moved into the spawned task that runs `start`: the fields are a
/// `Weak`, a `Copy` id, a `Clone` handle and another `Copy` id, none of which mind two owners.
#[derive(Clone)]
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
    ///
    /// Takes `self` by value because `step` runs this as a spawned task (see its comment on
    /// why), and a spawned future must own everything it touches rather than borrow from a
    /// stream state that may be dropped before the task finishes.
    async fn start(self) -> Result<ReclaimStart> {
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

        // Upstream commits the reclaim's own operation entry inside `reclaim_ln_receive`
        // (`manual_operation_start_dbtx` then `dbtx.commit_tx()`,
        // fedimint-ln-client/src/lib.rs) before this function gets to persist its id on the
        // record below. A crash in that window would otherwise call `reclaim_ln_receive` again
        // here and start a second claim state machine competing with the first for the same
        // contract. The scan is bounded: a reclaim is always newer than the receive it retries,
        // so nothing the walk finds older than this record's `created_at` can be it.
        if let Some(reclaim_id) = existing_reclaim(&self.db, self.id, record.created_at).await? {
            // Persisted before it is followed: same reasoning as below, and this path exists
            // because that persist step is exactly what a crash could have skipped last time.
            details.reclaim_operation_id =
                Some(crate::OperationId::from_upstream(reclaim_id).to_string());
            write_details_in(&self.db, self.id, wire::encode_receive_wire(&details)?).await?;
            return Ok(ReclaimStart::Started(
                module
                    .subscribe_ln_receive(reclaim_id)
                    .await
                    .map_err(subscribe_error)?
                    .into_stream(),
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

/// Looks for a reclaim of `original` that upstream already started and committed, so a retry
/// after a crash follows it instead of starting a competing one.
///
/// Walks the client's chronological index newest first, the way
/// [`FederationInner::creation_time_of`](crate::federation::FederationInner::creation_time_of)
/// does, and stops at the first entry older than `created_at` (the receive's own creation time):
/// a reclaim is always newer than the receive it retries, so nothing older can be it.
async fn existing_reclaim(
    db: &Database,
    original: OperationId,
    created_at: u64,
) -> Result<Option<OperationId>> {
    let mut dbtx = db.begin_transaction_nc().await;
    let keys: Vec<fedimint_client::db::ChronologicalOperationLogKey> = dbtx
        .find_by_prefix_sorted_descending(&fedimint_client::db::ChronologicalOperationLogKeyPrefix)
        .await
        .map(|(key, ())| key)
        .collect()
        .await;
    drop(dbtx);

    let log = fedimint_client::oplog::OperationLog::new(db.clone());
    for key in keys {
        if crate::db::millis_of(key.creation_time) < created_at {
            break;
        }
        let Some(entry) = log.get_operation(key.operation_id).await else {
            continue;
        };
        if entry.operation_module_kind() != "ln" {
            continue;
        }
        let Ok(meta) = entry.try_meta::<LightningOperationMeta>() else {
            continue;
        };
        if let LightningOperationMetaVariant::ReceiveReclaim {
            original_operation_id,
            ..
        } = meta.variant
            && original_operation_id == original
        {
            return Ok(Some(key.operation_id));
        }
    }
    Ok(None)
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
            //
            // `start` registers the retry with the module and then persists its id; the
            // registration upstream and the record of it must not be separated by a poll that
            // `settle` cancels between the two, or the SDK's record never learns the retry's id
            // and the next subscription repeats the sequence, starting another retry forever.
            // Running it as a task the poller only waits for keeps the two together: a poller
            // dropped mid-wait leaves the task to finish on its own, and the next subscription
            // then finds the persisted id and follows it instead of registering yet another
            // retry. The stream a completed task returns is dropped along with its result if
            // nobody awaited it in time; that is fine, since the persisted id, not this stream,
            // is what the next subscription follows.
            ReceiveStep::Reclaim => {
                let started = match fedimint_core::task::spawn(
                    "ln receive: start a claim retry",
                    follow.context.clone().start(),
                )
                .await
                {
                    Ok(result) => result,
                    Err(err) => Err(Error::new(
                        ErrorCode::Internal,
                        format!("the claim retry's startup task did not finish: {err}"),
                    )),
                };
                match started {
                    Ok(ReclaimStart::Started(upstream)) => {
                        follow.upstream = upstream;
                        follow.following = Following::Reclaim;
                        LnReceiveState::Funded
                    }
                    // The module says no further claim is possible: that is the one ending the
                    // contract reserves `Failed` for. The reason is logged here, its only
                    // reader, since `LnReceiveState::Failed` does not carry it.
                    Ok(ReclaimStart::Impossible(reason)) => {
                        tracing::warn!(
                            target: "fedimint_sdk",
                            operation = %follow.context.id.fmt_full(),
                            reason = %reason,
                            "a rejected lightning claim cannot be retried",
                        );
                        LnReceiveState::Failed
                    }
                    // Anything else is an observation failure, not an outcome: the next
                    // subscription replays the rejection and tries again.
                    Err(err) => {
                        follow.done = true;
                        return Some((Err(err), follow));
                    }
                }
            }
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
    terms_for(client, module, invoice, amount, gateway).await
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
    client: &Client,
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
    // A dry run of the primary module's balancing fails when the notes on hand cannot cover
    // the contract; that is reported as the balance problem it is, rather than as an opaque
    // internal failure, on either mint generation this module can run against.
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
    client: &Client,
    module: &ClientModuleInstance<'_, LightningClientModule>,
    quote: &LnQuoteInner,
    gateway: Option<Box<LightningGateway>>,
) -> Result<Operation<LnSendState>> {
    // Whether the module has already completed a payment for this invoice, or is still running
    // one, is decided before anything else. Paying a quote moves the note inventory, so a second
    // quote for the same invoice re-prices to a different federation fee once the first is
    // funded, and the terms re-check below would otherwise report that as a moved quote instead
    // of the truth: the invoice is already paid. `pay_bolt11_invoice` itself runs these same two
    // idempotency checks first, ahead of anything else it does
    // (`fedimint-ln-client/src/lib.rs:1356-1368`).
    let payment_hash = *quote.invoice.inner().payment_hash();
    let record = module
        .db
        .begin_transaction_nc()
        .await
        .get_value(&PaymentResultKey { payment_hash })
        .await;
    if let Some(record) = record {
        if record.completed_payment.is_some() {
            return Err(quote_expired(quote.expires_at, true));
        }
        if client
            .has_active_states(v1_payment_operation_id(&payment_hash, record.index))
            .await
        {
            return Err(quote_expired(quote.expires_at, true));
        }
    }
    // Checked again here, after the already-executed checks above and before the route and
    // terms re-checks below: the balance can drop between a quote and this call, and it must be
    // asked whether the invoice was already paid before it is asked whether the balance still
    // covers it, or a second quote for an already-paid invoice is misreported as a shortfall
    // instead of the truth.
    let available = balance_of(client).await?;
    if available < quote.plan.total {
        return Err(insufficient(quote.plan.total, available));
    }
    // `pay_bolt11_invoice` decides internal-vs-gateway for itself, from the invoice and the
    // gateway cache as they stand when it is called (`fedimint-ln-client/src/lib.rs:1405-1418`).
    // That cache is shared and can move between the quote and this call, so the same decision is
    // read again here, before anything is funded: a route that moved is `QuoteChanged`, not a
    // fee mismatch discovered only after the payment went out.
    let quoted_internal = matches!(quote.plan.route, LightningRoute::Internal);
    if is_internal(client, module, &quote.invoice).await? != quoted_internal {
        return Err(Error::new(
            ErrorCode::QuoteChanged,
            "the payment's route changed since the quote was issued; quote again",
        ));
    }
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
        client,
        module,
        &quote.invoice,
        quote.invoice_amount,
        gateway.clone(),
    )
    .await?;
    if fresh.total != quote.plan.total {
        return Err(quote_changed(quote.plan.total, fresh.total));
    }
    // Read once, ahead of the call below, for the `Invalid invoice currency` branch of its
    // error mapping.
    let expected: Network = federation.record().network.into();
    // Carried inside upstream's own metadata so a record rebuilt from the log after a crash
    // between upstream's commit and the SDK's write (`federation.create_operation` below) has
    // the exact quoted terms, not just the gateway's fee the log entry gives on its own.
    let created_at = now();
    let quoted_details = crate::LnSendDetails {
        invoice: quote.invoice.clone(),
        invoice_amount: quote.invoice_amount,
        fee: quote.plan.fee,
        total: quote.plan.total,
        route: quote.plan.route.clone(),
        created_at,
    };
    let payment: OutgoingLightningPayment = module
        .pay_bolt11_invoice(
            gateway.map(|gateway| *gateway),
            quote.invoice.inner().clone(),
            wire::custom_meta(&wire::LnSendDetailsWire::from(&quoted_details))?,
        )
        .await
        .map_err(|err| {
            if let Some(known) = err.downcast_ref::<PayBolt11InvoiceError>() {
                return match known {
                    PayBolt11InvoiceError::PreviousPaymentAttemptStillInProgress { .. }
                    | PayBolt11InvoiceError::FundedContractAlreadyExists { .. } => {
                        quote_expired(quote.expires_at, true)
                    }
                    // A `None` gateway is only ever passed for a quote whose route was checked
                    // internal just above; this fires only when the shared cache moved again
                    // between that check and this call, so the module's own re-derivation no
                    // longer agrees the payment is internal. The same drift the check above
                    // guards against, caught one call later instead of missed.
                    PayBolt11InvoiceError::NoLnGatewayAvailable => Error::new(
                        ErrorCode::QuoteChanged,
                        "the payment's route changed since the quote was issued; quote again",
                    ),
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
            // v1 converts the configured network through lightning-invoice's
            // `From<bitcoin::Network> for Currency`
            // (lightning-invoice-0.33.3/src/lib.rs:451-463) and compares against it
            // (`ensure!(federation_currency == invoice_currency, "Invalid invoice
            // currency: ...")`, fedimint-ln-client/src/lib.rs:834-839). That conversion has no
            // `Testnet4` arm and falls to a `_` arm yielding `Currency::Regtest`, so a testnet4
            // federation refuses every `tb` invoice here, the same upstream limitation lnv2's
            // `WrongCurrency` reports (tracked as fedimint/fedimint#9100). Reported as the
            // network mismatch it is rather than left to fall through to the internal-failure
            // branch below.
            if text.contains("Invalid invoice currency") {
                return network_refusal(quote, expected);
            }
            internal(format!("the payment could not be started: {text}"))
        })?;
    // The module's answer after funding is authoritative for the route; a payment quoted
    // through a gateway that settled internally paid no gateway fee. The gateway component is
    // the only part of the fee this code knows for certain was not charged, so it is what is
    // backed out. The lightning module's fee is a flat consensus figure and carries over
    // exactly; the primary module's and dust components are the quote's figures for a contract
    // larger by the gateway fee, kept as a best-effort estimate because upstream gives no way to
    // read the assembled fee back after funding. The recorded total is therefore an upper bound
    // on the debit in this residual case, which is reached only if the gateway cache moves
    // between the route re-check above and the module's own decision.
    let (id, route, fee, total) = match payment.payment_type {
        PayType::Internal(id) if quoted_internal => (
            id,
            LightningRoute::Internal,
            quote.plan.fee,
            quote.plan.total,
        ),
        PayType::Internal(id) => {
            let fee = quote
                .plan
                .fee
                .checked_sub(quote.plan.breakdown.gateway)
                .ok_or_else(|| internal("the quoted fee is smaller than the quoted gateway fee"))?;
            let total = add(quote.invoice_amount, fee)?;
            // The metadata copy above carries the quoted `fee`/`total`, not this corrected
            // figure: a record rebuilt from the log after a crash in this residual case reports
            // the quote's upper bound rather than what was actually debited.
            (id, LightningRoute::Internal, fee, total)
        }
        PayType::Lightning(id) => (
            id,
            quote.plan.route.clone(),
            quote.plan.fee,
            quote.plan.total,
        ),
    };
    let details = crate::LnSendDetails {
        invoice: quote.invoice.clone(),
        invoice_amount: quote.invoice_amount,
        fee,
        total,
        route,
        created_at,
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

// Reproduces `LightningClientModule::get_payment_operation_id`, upstream's private helper
// (`fedimint-ln-client/src/lib.rs:798-806`) that derives the operation id of one payment
// attempt from the invoice's payment hash and the attempt's index: a sha256 hash over the 32
// payment-hash bytes followed by the 2-byte little-endian index. This id is a storage format
// upstream, not an implementation detail: it is the operation id under which every v1 payment
// attempt is, and always has been, recorded, so it is as stable as the log itself.
fn v1_payment_operation_id(payment_hash: &sha256::Hash, index: u16) -> OperationId {
    let mut bytes = [0u8; 34];
    bytes[0..32].copy_from_slice(&payment_hash.to_byte_array());
    bytes[32..34].copy_from_slice(&index.to_le_bytes());
    OperationId(sha256::Hash::hash(&bytes).to_byte_array())
}

/// Issues a v1 invoice through the cheapest online gateway and records it.
pub(super) async fn receive(
    federation: &Arc<FederationInner>,
    client: &Client,
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
    // Same as in `terms_for`: a dry run of the primary module's balancing fails when the notes
    // on hand cannot cover the contract, which is reported as the balance problem it is, on
    // either mint generation.
    let quote = match module.receive_fee_quote(to_upstream(amount)).await {
        Ok(quote) => quote,
        Err(err) => {
            let text = err.to_string();
            let short = err.downcast_ref::<fedimint_mint_client::InsufficientBalanceError>();
            return Err(fee_quote_failure(
                client,
                short,
                &text,
                amount,
                "could not quote the claim fee",
            )
            .await);
        }
    };
    let fee = from_upstream(quote.total().get_bitcoin());
    let net_credit = amount.checked_sub(fee).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidInput,
            "the amount does not cover the receive-side fee",
        )
    })?;
    // Carried inside upstream's own metadata so a record rebuilt from the log after a crash
    // between upstream's commit and the SDK's write (`federation.create_operation` below) has
    // the exact quoted terms, not the zero fee the log entry gives on its own. The invoice is
    // not known until the call returns, so the copy carries a placeholder for it and
    // `expires_at`; the backfiller takes both from upstream's own meta instead.
    let created_at = now();
    let custom_meta = wire::custom_meta(&wire::LnReceiveDetailsWire {
        invoice: String::new(),
        description: description.to_owned(),
        requested_amount_msats: amount.msats(),
        invoice_amount_msats: amount.msats(),
        fee_msats: fee.msats(),
        net_credit_msats: net_credit.msats(),
        gateway_id: Some(GatewayId::from_upstream(gateway.gateway_id).to_string()),
        expires_at: 0,
        created_at: created_at.epoch_millis(),
        reclaim_operation_id: None,
    })?;
    let (id, invoice, _preimage) = module
        .create_bolt11_invoice(
            to_upstream(amount),
            Bolt11InvoiceDescription::Direct(bolt11_description),
            Some(u64::from(INVOICE_EXPIRY_SECS)),
            custom_meta,
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
        created_at,
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

/// Rebuilds a record from a v1 log entry: exact for an operation this SDK created, whose
/// metadata carries the quoted terms verbatim; an estimate — the gateway's fee for a send, no
/// fee at all for a receive — for an entry the log holds that this SDK did not create.
pub(super) fn backfill(meta: &serde_json::Value, created_at: u64) -> Option<Backfilled> {
    let meta: LightningOperationMeta = serde_json::from_value(meta.clone()).ok()?;
    let created_at = Timestamp::from_epoch_millis(created_at);
    let LightningOperationMeta {
        variant,
        extra_meta,
    } = meta;
    match variant {
        LightningOperationMetaVariant::Pay(pay) => {
            let invoice = Bolt11Invoice::from_upstream(pay.invoice);
            let invoice_amount = invoice.amount()?;
            // Trusted only when it names this exact invoice: an entry created by something
            // other than this SDK could carry anything under the same metadata key.
            let copy = wire::from_custom_meta::<wire::LnSendDetailsWire>(&extra_meta)
                .filter(|copy| copy.invoice == invoice.to_string());
            let (fee, total, route) = match copy {
                Some(copy) => (
                    Amount::from_msats(copy.fee_msats),
                    Amount::from_msats(copy.total_msats),
                    LightningRoute::try_from(copy.route).ok()?,
                ),
                None => {
                    let fee = from_upstream(pay.fee);
                    let route = if pay.is_internal_payment {
                        LightningRoute::Internal
                    } else {
                        LightningRoute::Gateway {
                            gateway_id: GatewayId::from_upstream(pay.gateway_id?),
                        }
                    };
                    (fee, invoice_amount.checked_add(fee)?, route)
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
        LightningOperationMetaVariant::Receive {
            invoice,
            gateway_id,
            ..
        } => {
            let invoice = Bolt11Invoice::from_upstream(invoice);
            let amount = invoice.amount()?;
            let copy = wire::from_custom_meta::<wire::LnReceiveDetailsWire>(&extra_meta);
            let (description, requested_amount, fee, net_credit, created_at) = match &copy {
                Some(copy) => (
                    copy.description.clone(),
                    Amount::from_msats(copy.requested_amount_msats),
                    Amount::from_msats(copy.fee_msats),
                    Amount::from_msats(copy.net_credit_msats),
                    Timestamp::from_epoch_millis(copy.created_at),
                ),
                None => (
                    invoice.description(),
                    amount,
                    Amount::from_msats(0),
                    amount,
                    created_at,
                ),
            };
            let details = LnReceiveDetails {
                invoice: invoice.clone(),
                description,
                requested_amount,
                invoice_amount: amount,
                fee,
                net_credit,
                gateway_id: gateway_id.map(GatewayId::from_upstream),
                expires_at: invoice.expires_at(),
                created_at,
            };
            Some(Backfilled {
                kind: kinds::LN_RECEIVE,
                details: serde_json::to_string(&wire::LnReceiveDetailsWire::from(&details)).ok()?,
                phase: None,
            })
        }
        // A retried claim runs under the original receive's record; the deprecated claim path
        // and recurring payments are not operations this SDK creates. `reclaim_ln_receive`
        // copies the receive's own `extra_meta` onto the reclaim entry, but nothing here reads
        // it: the reclaim never becomes its own record.
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
    fn v1_payment_operation_id_hashes_the_payment_hash_and_the_little_endian_index() {
        let payment_hash = sha256::Hash::from_byte_array([0xab; 32]);
        let mut expected_bytes = [0u8; 34];
        expected_bytes[0..32].copy_from_slice(&payment_hash.to_byte_array());
        expected_bytes[32..34].copy_from_slice(&1u16.to_le_bytes());
        let expected = OperationId(sha256::Hash::hash(&expected_bytes).to_byte_array());
        assert_eq!(v1_payment_operation_id(&payment_hash, 1), expected);
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
    fn a_pay_log_entry_with_an_sdk_copy_uses_the_copys_fee_and_total() {
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
            "extra_meta": wire::custom_meta(&copy).expect("encode"),
        });
        let claimed = backfill(&meta, 1_700_000_000_000).expect("claimed");
        let details = wire::decode_send_details(&claimed.details).expect("decodes");
        // The copy's figures, not the gateway-fee-only estimate (1_000 / 101_000) the log entry
        // alone would give.
        assert_eq!(details.fee, Amount::from_msats(1_500));
        assert_eq!(details.total, Amount::from_msats(101_500));
        assert_eq!(details.route, gateway_route());
        // The record's own creation time, not the copy's.
        assert_eq!(details.created_at.epoch_millis(), 1_700_000_000_000);
    }

    #[test]
    fn a_pay_log_entry_with_a_copy_naming_a_different_invoice_falls_back_to_the_estimate() {
        // Not a real invoice: `backfill` only ever string-compares the copy's `invoice` field
        // against upstream's, it never parses it, so any value that differs from `REGTEST_INVOICE`
        // exercises the mismatch.
        let copy = wire::LnSendDetailsWire {
            invoice: "not the invoice this entry names".to_owned(),
            invoice_amount_msats: 100_000,
            fee_msats: 1_500,
            total_msats: 101_500,
            route: wire::RouteWire::Gateway {
                gateway_id: GATEWAY_ID.to_owned(),
            },
            created_at: 1_650_000_000_000,
        };
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
            "extra_meta": wire::custom_meta(&copy).expect("encode"),
        });
        let claimed = backfill(&meta, 1_700_000_000_000).expect("claimed");
        let details = wire::decode_send_details(&claimed.details).expect("decodes");
        // A copy naming a different invoice cannot be this entry's: something other than this
        // SDK put it there, so the gateway-fee-only estimate is used instead.
        assert_eq!(details.fee, Amount::from_msats(1_000));
        assert_eq!(details.total, Amount::from_msats(101_000));
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
        let meta = serde_json::json!({
            "variant": {
                "receive": {
                    "out_point": { "txid": "00".repeat(32), "out_idx": 0 },
                    "invoice": REGTEST_INVOICE,
                    "gateway_id": GATEWAY_ID,
                }
            },
            "extra_meta": wire::custom_meta(&copy).expect("encode"),
        });
        let claimed = backfill(&meta, 1_700_000_000_000).expect("claimed");
        let details = wire::decode_receive_details(&claimed.details).expect("decodes");
        // The copy's figures, not the "no fee known" estimate (0 / 100_000) the log entry alone
        // would give.
        assert_eq!(details.fee, Amount::from_msats(750));
        assert_eq!(details.net_credit, Amount::from_msats(99_250));
        assert_eq!(details.description, "coffee");
        // The copy's own creation time, known before the invoice call and carried through.
        assert_eq!(details.created_at.epoch_millis(), 1_650_000_000_000);
        // The invoice and gateway id still come from upstream's own meta, not the copy's
        // placeholder.
        assert_eq!(details.invoice.to_string(), REGTEST_INVOICE);
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

    /// Writes a v1 log entry the way the module itself does, dated at `created_at`.
    async fn write_ln_log_entry(
        db: &fedimint_core::db::Database,
        id: OperationId,
        meta: serde_json::Value,
        created_at: u64,
    ) {
        use fedimint_client::oplog::OperationLog;

        let mut dbtx = db.begin_transaction().await;
        OperationLog::new(db.clone())
            .add_operation_log_entry_dbtx_with_creation_time(
                &mut dbtx.to_ref_nc(),
                id,
                "ln",
                meta,
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(created_at),
            )
            .await;
        dbtx.commit_tx().await;
    }

    fn reclaim_meta(original: OperationId) -> serde_json::Value {
        serde_json::json!({
            "variant": {
                "receive_reclaim": {
                    "original_operation_id": original.fmt_full().to_string(),
                    "invoice": REGTEST_INVOICE,
                    "gateway_id": null,
                }
            },
            "extra_meta": null,
        })
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn existing_reclaim_finds_a_retry_committed_after_the_receive() {
        let db = crate::db::in_memory_root();
        let original = OperationId([7u8; 32]);
        let reclaim = OperationId([8u8; 32]);
        let created_at = 1_700_000_000_000u64;
        write_ln_log_entry(&db, reclaim, reclaim_meta(original), created_at + 1_000).await;

        let found = existing_reclaim(&db, original, created_at)
            .await
            .expect("scan")
            .expect("the retry the crash lost is still found");
        assert_eq!(found, reclaim);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn existing_reclaim_ignores_an_entry_older_than_the_receive() {
        let db = crate::db::in_memory_root();
        let original = OperationId([7u8; 32]);
        let reclaim = OperationId([8u8; 32]);
        let created_at = 1_700_000_000_000u64;
        // Nothing older than the receive it retries can be its reclaim, so the walk stops here
        // rather than reporting this unrelated, earlier entry.
        write_ln_log_entry(&db, reclaim, reclaim_meta(original), created_at - 1_000).await;

        assert_eq!(
            existing_reclaim(&db, original, created_at)
                .await
                .expect("scan"),
            None
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn existing_reclaim_skips_entries_that_are_not_this_receives_retry() {
        let db = crate::db::in_memory_root();
        let original = OperationId([7u8; 32]);
        let other_original = OperationId([9u8; 32]);
        let created_at = 1_700_000_000_000u64;

        // A reclaim of a different receive: newer, but not a match.
        write_ln_log_entry(
            &db,
            OperationId([8u8; 32]),
            reclaim_meta(other_original),
            created_at + 1_000,
        )
        .await;
        // The receive entry itself, at its own creation time: a `Receive`, not a
        // `ReceiveReclaim`, so it is skipped rather than mistaken for the retry.
        write_ln_log_entry(
            &db,
            original,
            serde_json::json!({
                "variant": {
                    "receive": {
                        "out_point": { "txid": "00".repeat(32), "out_idx": 0 },
                        "invoice": REGTEST_INVOICE,
                        "gateway_id": GATEWAY_ID,
                    }
                },
                "extra_meta": null,
            }),
            created_at,
        )
        .await;

        assert_eq!(
            existing_reclaim(&db, original, created_at)
                .await
                .expect("scan"),
            None
        );
    }
}
