//! The walletv2 wallet module (`walletv2`): the withdrawal plan and send, the deposit address
//! and its event-log-backed discovery and subscription, and the backfill of this module's own
//! operation log.

use std::sync::{Arc, Weak};

use fedimint_client::Client;
use fedimint_client_module::ClientModuleInstance;
use fedimint_client_module::TransactionSubmitError;
use fedimint_client_module::transaction::FeeQuote;
use fedimint_core::bitcoin;
use fedimint_core::core::{ModuleInstanceId, OperationId};
use fedimint_core::db::{Database, IDatabaseTransactionOpsCoreTyped};
use fedimint_core::util::BoxStream;
use fedimint_eventlog::{Event, EventLogId};
use fedimint_walletv2_client::events::ReceivePaymentEvent;
use fedimint_walletv2_client::{
    FinalReceiveOperationState, FinalSendOperationState, ReceiveMeta, SendError, SendMeta,
    WalletClientModule, WalletOperationMeta,
};
use fedimint_walletv2_common::KIND;
use fedimint_walletv2_common::config::WalletClientConfig;
use futures::StreamExt as _;

use super::{
    OnchainQuoteInner, Plan, Terms, add, balance_of, bitcoin_to_sats, check_amount,
    check_covers_amount, claim_figures, fee_quote_failure, from_upstream, insufficient, internal,
    now, plan_of, quote_changed, sats_to_amount, sats_to_bitcoin, subscribe_error, timeout,
    unreachable, wire,
};
use crate::federation::{FederationInner, wait_holding_client};
use crate::operation::{
    Backfilled, CURRENT_STATE_SETTLE, Driver, custom_meta, from_custom_meta, kinds,
    record_phase_in, until_final, write_details_in,
};
use crate::sdk::{CONTACT_TIMEOUT, SdkInner};
use crate::{
    Address, Amount, Error, ErrorCode, OnchainReceive, OnchainReceiveDetails, OnchainReceiveState,
    OnchainSendDetails, OnchainSendState, Operation, Result, Sats, Txid,
};

/// The walletv2 module's own configuration, cast out of the client's decoded config exactly as
/// the ecash facade reads `MintClientConfig`: this module keeps no public accessor for its
/// `dust_limit` or `fee_consensus` the way the v1 wallet module does.
pub(super) async fn config(client: &Client, id: ModuleInstanceId) -> Result<WalletClientConfig> {
    let client_config = client.config().await;
    let module_config = client_config.get_module_cfg(id).map_err(internal)?;
    module_config
        .cast::<WalletClientConfig>()
        .cloned()
        .map_err(internal)
}

/// The walletv2 module on a live client, or `NotSupported` when the federation dropped it.
pub(super) fn module_of(client: &Client) -> Result<ClientModuleInstance<'_, WalletClientModule>> {
    client
        .get_first_module::<WalletClientModule>()
        .map_err(|_| {
            Error::new(
                ErrorCode::NotSupported,
                "this federation no longer has a walletv2 wallet module",
            )
        })
}

/// A whole-satoshi `bitcoin::Amount` reinterpreted as a millisatoshi upstream `Amount`: the same
/// conversion `send_fee_quote` uses internally to price its own fee consensus
/// (`modules/fedimint-walletv2-client/src/lib.rs:284-294`).
fn to_upstream_sats(amount: bitcoin::Amount) -> fedimint_core::Amount {
    fedimint_core::Amount::from_sats(amount.to_sat())
}

// walletv2 `SendError` onto the facade's errors. `WrongNetwork` cannot happen here in practice
// (the address's network is already checked before either call that can return it), but the arm
// is kept rather than folded into `internal` so a defect in that earlier check still surfaces as
// the right error code.
//
// | upstream                       | here                    |
// | ------------------------------- | ----------------------- |
// | `WrongNetwork`                  | `NetworkMismatch`        |
// | `DustValue`                     | `InvalidInput`           |
// | `InsufficientFunds`             | `InsufficientBalance`    |
// | `NoConsensusFeerateAvailable`   | `FederationUnreachable`  |
// | `FederationError`               | `FederationUnreachable`  |
// | `UnsupportedAddress`            | `InvalidInput`           |
fn map_send_error(err: SendError) -> Error {
    match err {
        SendError::WrongNetwork => Error::new(
            ErrorCode::NetworkMismatch,
            "the address is for a different network than the federation",
        ),
        SendError::DustValue => Error::new(
            ErrorCode::InvalidInput,
            "the amount is below the destination's dust threshold",
        ),
        SendError::InsufficientFunds => Error::new(
            ErrorCode::InsufficientBalance,
            "the client does not have sufficient funds to send the payment",
        ),
        SendError::NoConsensusFeerateAvailable | SendError::FederationError(_) => {
            unreachable(err.to_string())
        }
        SendError::UnsupportedAddress => Error::new(
            ErrorCode::InvalidInput,
            "this address type is not supported",
        ),
    }
}

/// Plans a walletv2 withdrawal: prices the destination output, then the transaction that funds
/// it.
pub(super) async fn plan(
    federation: &Arc<FederationInner>,
    client: &Client,
    module: &ClientModuleInstance<'_, WalletClientModule>,
    address: &Address,
    amount: Sats,
    available: Amount,
) -> Result<Plan> {
    let cfg = config(client, module.id).await?;
    check_amount(amount, cfg.dust_limit)?;
    check_covers_amount(amount, available)?;
    let chain_fee_btc = fedimint_core::runtime::timeout(CONTACT_TIMEOUT, module.send_fee())
        .await
        .map_err(|_| timeout())?
        .map_err(map_send_error)?;
    let output_value = sats_to_bitcoin(amount)
        .checked_add(chain_fee_btc)
        .ok_or_else(|| internal("the withdrawal amount plus its on-chain fee overflowed"))?;
    let chain_fee = from_upstream(fedimint_core::Amount::from(chain_fee_btc));
    let quote = match module.send_fee_quote(output_value).await {
        Ok(quote) => quote,
        Err(err) => {
            let text = format!("{err:#}");
            let short = err.downcast_ref::<fedimint_mint_client::InsufficientBalanceError>();
            // `required` is the amount plus the on-chain fee already quoted above; the dry
            // run that would have priced the funding side is exactly what failed.
            let required = add(sats_to_amount(amount)?, chain_fee)?;
            return Err(fee_quote_failure(
                client,
                federation.status(),
                short,
                &text,
                required,
                "could not quote the withdrawal's funding fee",
            )
            .await);
        }
    };
    let module_fee = from_upstream(cfg.fee_consensus.fee(to_upstream_sats(output_value)));
    plan_of(
        chain_fee,
        module_fee,
        &quote,
        amount,
        Terms::V2 {
            chain_fee: chain_fee_btc,
            quote: quote.clone(),
        },
    )
}

/// Executes a walletv2 withdrawal: re-derives the terms and refuses on drift, then funds and
/// records.
pub(super) async fn send(
    federation: &Arc<FederationInner>,
    client: &Client,
    module: &ClientModuleInstance<'_, WalletClientModule>,
    quote: &OnchainQuoteInner,
    chain_fee: bitcoin::Amount,
    quoted: &FeeQuote,
    driver: Arc<dyn Driver<OnchainSendState>>,
) -> Result<Operation<OnchainSendState>> {
    let cfg = config(client, module.id).await?;
    let fresh_chain_fee = fedimint_core::runtime::timeout(CONTACT_TIMEOUT, module.send_fee())
        .await
        .map_err(|_| timeout())?
        .map_err(map_send_error)?;
    let output_value = sats_to_bitcoin(quote.amount)
        .checked_add(fresh_chain_fee)
        .ok_or_else(|| internal("the withdrawal amount plus its on-chain fee overflowed"))?;
    let fresh_chain_fee_amount = from_upstream(fedimint_core::Amount::from(fresh_chain_fee));
    let fresh_quote = match module.send_fee_quote(output_value).await {
        Ok(quote) => quote,
        Err(err) => {
            let text = format!("{err:#}");
            let short = err.downcast_ref::<fedimint_mint_client::InsufficientBalanceError>();
            // `required` is the amount plus the on-chain fee already re-quoted above; the dry
            // run that would have priced the funding side is exactly what failed.
            let required = add(sats_to_amount(quote.amount)?, fresh_chain_fee_amount)?;
            return Err(fee_quote_failure(
                client,
                federation.status(),
                short,
                &text,
                required,
                "could not re-quote the withdrawal's funding fee",
            )
            .await);
        }
    };
    // Both `send_fee` and `send_fee_quote` must return exactly what they returned when this
    // quote was built, or the federation changing wallet generation between the two: either is
    // reported here, before anything is submitted, as `QuoteChanged`.
    if chain_fee != fresh_chain_fee || quoted != &fresh_quote {
        let module_fee = from_upstream(cfg.fee_consensus.fee(to_upstream_sats(output_value)));
        let current = plan_of(
            fresh_chain_fee_amount,
            module_fee,
            &fresh_quote,
            quote.amount,
            Terms::V2 {
                chain_fee: fresh_chain_fee,
                quote: fresh_quote.clone(),
            },
        )?;
        return Err(quote_changed(quote.plan.total, current.total));
    }
    let available = balance_of(client, federation.status()).await?;
    if available < quote.plan.total {
        return Err(insufficient(quote.plan.total, available));
    }
    // Residual race, not closeable from this side: the re-check above and the funding below are
    // two separate client transactions, and a concurrent spend or issuance on this wallet in
    // between can still change what the funding actually selects and pays. Closing it needs a
    // quote-and-submit that upstream runs as one transaction (fedimint/fedimint#9098).
    let created_at = now();
    let details = OnchainSendDetails {
        address: quote.address.clone(),
        amount: quote.amount,
        fee: quote.plan.fee,
        total: quote.plan.total,
        created_at,
    };
    let wire = wire::OnchainSendDetailsWire::from(&details);
    let id = module
        .send(
            quote.address.inner().clone(),
            sats_to_bitcoin(quote.amount),
            Some(chain_fee),
            custom_meta(&wire)?,
        )
        .await
        .map_err(map_send_error)?;
    federation
        .create_operation(id, kinds::ONCHAIN_SEND, "walletv2", &wire, driver)
        .await
}

// walletv2 `FinalSendOperationState` onto `OnchainSendState`. There is no separate broadcast
// step and no `Created` to map here: that state is reported directly by `current_send` and
// `subscribe_send` before this ever runs. Upstream documents `Failure` itself as "a programming
// error has occurred or the federation is malicious"
// (`modules/fedimint-walletv2-client/src/lib.rs:104`), which is why it maps to `Failed` rather
// than the ordinary `Refunded` ending `Aborted` gets.
//
// | upstream    | here                                                          |
// | ----------- | ------------------------------------------------------------- |
// | `Success`   | `Succeeded { txid }`                                            |
// | `Aborted`   | `Refunded { reason: "the federation rejected ..." }`             |
// | `Failure`   | `Failed { reason: "the funding was accepted but ..." }`          |
pub(super) fn map_final_send(state: &FinalSendOperationState) -> OnchainSendState {
    match state {
        FinalSendOperationState::Success(txid) => OnchainSendState::Succeeded {
            txid: Txid::from_upstream(*txid),
        },
        FinalSendOperationState::Aborted => OnchainSendState::Refunded {
            reason: "the federation rejected the funding transaction".to_owned(),
        },
        FinalSendOperationState::Failure => OnchainSendState::Failed {
            reason: "the funding was accepted but no transaction came of it".to_owned(),
        },
    }
}

/// The current state of a walletv2 withdrawal: the final await bounded to 500 ms, `Created` when
/// it does not resolve in time.
pub(super) async fn current_send(
    federation: &FederationInner,
    id: OperationId,
) -> Result<OnchainSendState> {
    let client = federation.client(false).await?;
    let module = module_of(&client)?;
    match fedimint_core::runtime::timeout(
        CURRENT_STATE_SETTLE,
        module.await_final_send_operation_state(id),
    )
    .await
    {
        Ok(Ok(state)) => Ok(map_final_send(&state)),
        Ok(Err(err)) => Err(subscribe_error(err)),
        Err(_) => Ok(OnchainSendState::Created),
    }
}

/// A fresh stream over a walletv2 withdrawal: `Created` immediately, then the mapped final
/// state, once the module's own unbounded await resolves it.
///
/// The await borrows the module, which borrows the client, so the client guard cannot be held
/// across it: the handle is cloned out of the guard and the wait itself runs on
/// [`wait_holding_client`], the same shape `lightning::v2::subscribe_send` would need if lnv2 had
/// no incremental subscription either.
pub(super) async fn subscribe_send(
    federation: &FederationInner,
    id: OperationId,
) -> Result<BoxStream<'static, Result<OnchainSendState>>> {
    let client = federation.client(false).await?;
    let handle = client.handle();
    drop(client);
    let stop = handle.task_group().make_handle().make_shutdown_rx();
    let created = futures::stream::once(async { Ok(OnchainSendState::Created) });
    let finished = futures::stream::once(async move {
        let state = wait_holding_client(handle, stop, move |client| async move {
            let module = module_of(&client)?;
            module
                .await_final_send_operation_state(id)
                .await
                .map_err(subscribe_error)
        })
        .await?;
        Ok(map_final_send(&state))
    });
    Ok(until_final(created.chain(finished)))
}

/// Allocates a fresh walletv2 deposit address and records it.
///
/// walletv2's `receive` returns only the address, with no operation of its own
/// (`modules/fedimint-walletv2-client/src/lib.rs:579`): the SDK mints its own operation id and
/// records the event log's tail as the position a scan for the matching `ReceivePaymentEvent`
/// should start from. That tail is read before the address is asked for, as upstream's own doc
/// on `receive` requires: the module's scanner can claim a payment to the address in the gap
/// after handing it out, and a position taken afterwards would put that event behind the scan.
pub(super) async fn receive(
    federation: &Arc<FederationInner>,
    client: &Client,
    module: &ClientModuleInstance<'_, WalletClientModule>,
    driver: Arc<dyn Driver<OnchainReceiveState>>,
) -> Result<OnchainReceive> {
    let created_at = now();
    let cursor = event_log_tail(client).await;
    let checked = module.receive().await;
    let address = Address::from_upstream(checked.clone().into_unchecked());
    let id = OperationId::new_random();
    let wire = wire::OnchainReceiveDetailsWire {
        address: checked.to_string(),
        txid: None,
        gross_deposited_sats: None,
        fee_msats: None,
        fee_breakdown: None,
        net_credit_msats: None,
        created_at: created_at.epoch_millis(),
        upstream_operation_id: None,
        event_cursor: Some(cursor),
    };
    let operation = federation
        .create_operation(id, kinds::ONCHAIN_RECEIVE, "walletv2", &wire, driver)
        .await?;
    Ok(OnchainReceive { address, operation })
}

/// The event log's current tail: the position a scan for a deposit's `ReceivePaymentEvent`
/// should start from, so that an event already in the log before this address was handed out is
/// never mistaken for a payment to it.
pub(super) async fn event_log_tail(client: &Client) -> u64 {
    u64::from(client.get_next_event_log_id().await)
}

/// A walletv2 deposit's own upstream operation, discovered by scanning the event log for the
/// `ReceivePaymentEvent` that names the watched address.
///
/// `chain_fee` is only ever read back by the scan that just populated it: a `Link` rebuilt from
/// the wire after a restart carries a placeholder instead, since the claim's own inputs are
/// re-read from the linked operation's own metadata at claim time regardless (see
/// [`claim_from_upstream`]), which is what lets a restart mid-claim resume from nothing but the
/// upstream operation id.
#[derive(Debug, Clone)]
pub(super) struct Link {
    pub(super) upstream: OperationId,
    pub(super) txid: Txid,
    pub(super) gross: Sats,
    pub(super) chain_fee: bitcoin::Amount,
    pub(super) cursor: u64,
}

/// Scans the event log from `from` for the first `ReceivePaymentEvent` naming `address`, one pass
/// over the log as it stands now: this does not wait for new entries, so a caller that wants to
/// keep watching re-derives a live client and calls this again after the next
/// [`Client::log_event_added_rx`] tick.
pub(super) async fn find_link(client: &Client, address: &Address, from: u64) -> Option<Link> {
    const PAGE: u64 = 64;
    let mut pos = EventLogId::LOG_START.saturating_add(from);
    loop {
        let page = client.get_event_log(Some(pos), PAGE).await;
        if page.is_empty() {
            return None;
        }
        for entry in &page {
            let cursor = u64::from(entry.id().saturating_add(1));
            if entry.module_kind() == Some(&KIND)
                && entry.kind == ReceivePaymentEvent::KIND
                && let Some(event) = entry.to_event::<ReceivePaymentEvent>()
            {
                // The module only ever writes `None` for an output it could not resolve; a
                // deposit with no transaction id cannot be reported, so it is skipped rather
                // than linked.
                let Some(outpoint) = event.outpoint else {
                    continue;
                };
                if event.address != *address.inner() {
                    continue;
                }
                return Some(Link {
                    upstream: event.operation_id,
                    txid: Txid::from_upstream(outpoint.txid),
                    gross: bitcoin_to_sats(event.value),
                    chain_fee: event.fee,
                    cursor,
                });
            }
        }
        pos = page
            .last()
            .expect("checked non-empty above")
            .id()
            .saturating_add(1);
    }
}

/// Records a freshly discovered link on the wire: `txid` and `gross_deposited` are set only the
/// first time, but `upstream_operation_id` and `event_cursor` are overwritten every time, since a
/// retry after an `Aborted` claim relinks the same address to a new upstream operation.
pub(super) async fn link(
    federation: &FederationInner,
    id: OperationId,
    found: &Link,
) -> Result<()> {
    let db = federation.db();
    let Some(mut details) = read_wire(&db, id).await? else {
        return Ok(());
    };
    if details.txid.is_none() {
        details.txid = Some(found.txid.to_string());
        details.gross_deposited_sats = Some(found.gross.sats());
    }
    details.upstream_operation_id = Some(found.upstream.fmt_full().to_string());
    details.event_cursor = Some(found.cursor);
    write_details_in(&db, id, wire::encode_receive_wire(&details)?).await?;
    record_phase_in(&db, id, wire::PHASE_SEEN).await
}

/// The linked upstream operation's final state, bounded to 500 ms: `None` means the claim has
/// not settled within the bound, not that it never will.
pub(super) async fn upstream_state(
    client: &Client,
    upstream: OperationId,
) -> Result<Option<FinalReceiveOperationState>> {
    let module = module_of(client)?;
    match fedimint_core::runtime::timeout(
        CURRENT_STATE_SETTLE,
        module.await_final_receive_operation_state(upstream),
    )
    .await
    {
        Ok(Ok(state)) => Ok(Some(state)),
        Ok(Err(err)) => Err(subscribe_error(err)),
        Err(_) => Ok(None),
    }
}

// walletv2 has no per-address state machine to follow the way v1's `DepositStateV2` is one: the
// phases `OnchainReceiveState` reports here are this SDK's own observation of the address, and
// the module's own claim machine lands on them as `Funding` -> `Confirmed`, `Success` (once the
// mint has issued the claimed notes) -> `Claimed`, or `Failed` if the mint could not issue one of
// them, `Aborted` (the claim stays claimable, retried under the same operation id) -> stays
// `Confirmed`.
/// What the bounded (or, once already `Confirmed`, unbounded) check of a linked upstream
/// operation found.
enum ClaimProgress {
    /// The claim has not settled, or has settled but the mint has not finished issuing the
    /// claimed notes yet; still `Confirmed` either way.
    StillFunding,
    /// The federation rejected the claim; still `Confirmed`, and a later `ReceivePaymentEvent`
    /// for the same address relinks it under a fresh upstream operation.
    Aborted,
    /// The claim settled and reached its end: either its notes are issued and the wire record
    /// carries the credit (`Claimed`), or the mint could not issue one of them (`Failed`).
    /// Either way this is the final state to report.
    Final(OnchainReceiveState),
}

async fn observe_link(
    client: &Client,
    db: &Database,
    id: OperationId,
    found: &Link,
) -> Result<ClaimProgress> {
    match upstream_state(client, found.upstream).await? {
        None => Ok(ClaimProgress::StillFunding),
        Some(FinalReceiveOperationState::Aborted) => Ok(ClaimProgress::Aborted),
        Some(FinalReceiveOperationState::Success) => {
            // `claim_from_upstream` waits out however long the mint takes to issue the claimed
            // notes (see its own doc comment). A bounded caller must not be dragged into that
            // wait: while any state machine the claim registered is still running, the mint's
            // output ones included, this reports the claim as still `Confirmed`, exactly like an
            // unsettled claim, and leaves the wait to whichever caller can afford an unbounded
            // one (`ReceiveCursor::Waiting`, on the subscription). Once none is left, the wait
            // in `claim_from_upstream` only reads the outcome already recorded.
            if client.has_active_states(found.upstream).await {
                return Ok(ClaimProgress::StillFunding);
            }
            Ok(ClaimProgress::Final(
                claim_from_upstream(client, db, id, found).await?,
            ))
        }
    }
}

/// Re-reads the linked upstream operation's own terms and reads back what the claim transaction
/// itself minted, so that a restart mid-claim needs nothing but the upstream operation id:
/// `value` and `fee` are read fresh from its `ReceiveMeta` rather than trusted from an earlier
/// pass.
///
/// `FinalReceiveOperationState::Success` only means the claim transaction was accepted into
/// consensus; the mint outputs it creates are issued afterwards by the primary module's own
/// state machines, so this waits for that too before reading the transaction, or an application
/// that spends as soon as it sees `Claimed` could find the balance short of what was just
/// credited. A note the mint fails to issue ends the deposit as `Failed` instead: the claim was
/// accepted, but the credit it promised never became spendable.
async fn claim_from_upstream(
    client: &Client,
    db: &Database,
    id: OperationId,
    found: &Link,
) -> Result<OnchainReceiveState> {
    let entry = client
        .operation_log()
        .get_operation(found.upstream)
        .await
        .ok_or_else(|| internal("the linked deposit is no longer in the operation log"))?;
    let meta: WalletOperationMeta = entry
        .try_meta()
        .map_err(|err| internal(format!("could not read the linked deposit's terms: {err}")))?;
    let WalletOperationMeta::Receive(ReceiveMeta {
        value,
        fee,
        change_outpoint_range,
        ..
    }) = meta
    else {
        return Err(internal("the linked upstream operation is not a receive"));
    };
    let net_of_chain_fee = value
        .checked_sub(fee)
        .ok_or_else(|| internal("the deposit's on-chain claim fee exceeds its value"))?;
    let wallet_instance = module_of(client)?.id;
    let cfg = config(client, wallet_instance).await?;
    let input_amount = to_upstream_sats(net_of_chain_fee);
    let input_fee = cfg.fee_consensus.fee(input_amount);

    // The same wait upstream's own `await_receive` makes before it reports a claim
    // ($FM/modules/fedimint-walletv2-client/src/lib.rs:624-635): it returns once every note the
    // claim minted is spendable, and fails if the mint's own state machine for one of them
    // ended in failure instead. Mint issuance cannot be delayed or failed from a test at this
    // pin, so only the devimint suite exercises the wait, and only its success path.
    let issued = client
        .await_primary_bitcoin_module_outputs(
            found.upstream,
            change_outpoint_range.into_iter().collect(),
        )
        .await;
    match issued {
        Ok(()) => {}
        Err(TransactionSubmitError::PrimaryModule(cause)) => {
            return Ok(OnchainReceiveState::Failed {
                reason: format!(
                    "the claim was accepted but its notes could not be issued: {cause}"
                ),
            });
        }
        Err(err) => {
            return Err(internal(format!(
                "could not wait for the claimed notes to be issued: {err}"
            )));
        }
    }

    let (fee_total, breakdown, net_credit) = claim_figures(
        client,
        found.upstream,
        wallet_instance,
        from_upstream(input_amount),
        from_upstream(input_fee),
        from_upstream(to_upstream_sats(fee)),
        found.gross,
    )
    .await?;

    let Some(mut details) = read_wire(db, id).await? else {
        return Err(internal(format!(
            "no record for operation {}",
            id.fmt_full()
        )));
    };
    details.fee_msats = Some(fee_total.msats());
    details.fee_breakdown = Some(wire::ReceiveFeeBreakdownWire::from(&breakdown));
    details.net_credit_msats = Some(net_credit.msats());
    write_details_in(db, id, wire::encode_receive_wire(&details)?).await?;

    Ok(OnchainReceiveState::Claimed {
        txid: found.txid.clone(),
        gross_deposited: found.gross,
        net_credit,
    })
}

async fn read_wire(
    db: &Database,
    id: OperationId,
) -> Result<Option<wire::OnchainReceiveDetailsWire>> {
    let Some(record) = db
        .begin_transaction_nc()
        .await
        .get_value(&crate::db::OperationRecordKey(id))
        .await
    else {
        return Ok(None);
    };
    Ok(Some(wire::decode_receive_wire(&record.details)?))
}

async fn read_address(db: &Database, id: OperationId) -> Result<Address> {
    let Some(details) = read_wire(db, id).await? else {
        return Err(internal(format!(
            "no record for operation {}",
            id.fmt_full()
        )));
    };
    Ok(OnchainReceiveDetails::try_from(details)?.address)
}

fn claimed_from(details: wire::OnchainReceiveDetailsWire) -> Result<OnchainReceiveState> {
    let public = OnchainReceiveDetails::try_from(details)?;
    Ok(OnchainReceiveState::Claimed {
        txid: public
            .txid
            .ok_or_else(|| internal("a claimed deposit record has no transaction"))?,
        gross_deposited: public
            .gross_deposited
            .ok_or_else(|| internal("a claimed deposit record has no gross amount"))?,
        net_credit: public
            .net_credit
            .ok_or_else(|| internal("a claimed deposit record has no net credit"))?,
    })
}

fn parse_upstream_id(text: &str) -> Result<OperationId> {
    text.parse().map_err(|err| {
        internal(format!(
            "a stored upstream operation id does not parse: {err}"
        ))
    })
}

/// Rebuilds a [`Link`] already recorded on the wire, or `None` when the deposit has not been
/// linked to an upstream operation yet.
fn wire_link(details: &wire::OnchainReceiveDetailsWire) -> Result<Option<Link>> {
    let (Some(upstream), Some(txid), Some(gross)) = (
        details.upstream_operation_id.as_deref(),
        details.txid.as_deref(),
        details.gross_deposited_sats,
    ) else {
        return Ok(None);
    };
    Ok(Some(Link {
        upstream: parse_upstream_id(upstream)?,
        txid: txid
            .parse::<Txid>()
            .map_err(|_| internal("a stored transaction id does not parse"))?,
        gross: Sats::from_sats(gross),
        chain_fee: bitcoin::Amount::ZERO,
        cursor: details.event_cursor.unwrap_or(0),
    }))
}

fn federation_closed() -> Error {
    Error::new(
        ErrorCode::FederationClosed,
        "this federation stopped running",
    )
}

/// The current state of a walletv2 deposit, following the same three-step algorithm
/// `subscribe_receive` runs continuously: a claimed record answers from storage, an unlinked
/// address gets one scan of the event log, and a linked one gets one bounded check of the claim.
pub(super) async fn current_receive(
    federation: &FederationInner,
    id: OperationId,
) -> Result<OnchainReceiveState> {
    let db = federation.db();
    let Some(details) = read_wire(&db, id).await? else {
        return Err(internal(format!(
            "no record for operation {}",
            id.fmt_full()
        )));
    };
    if details.net_credit_msats.is_some() {
        return claimed_from(details);
    }
    let client = federation.client(false).await?;
    let found = match wire_link(&details)? {
        Some(found) => found,
        None => {
            let address = OnchainReceiveDetails::try_from(details.clone())?.address;
            let cursor = details.event_cursor.unwrap_or(0);
            match find_link(&client, &address, cursor).await {
                Some(found) => {
                    link(federation, id, &found).await?;
                    found
                }
                None => return Ok(OnchainReceiveState::WaitingForTransaction),
            }
        }
    };
    match observe_link(&client, &db, id, &found).await? {
        ClaimProgress::StillFunding | ClaimProgress::Aborted => {
            Ok(OnchainReceiveState::Confirmed {
                txid: found.txid,
                gross_deposited: found.gross,
            })
        }
        ClaimProgress::Final(state) => Ok(state),
    }
}

/// Everything [`receive_step`] needs to re-derive a live client from between polls: a stream may
/// be left parked indefinitely by an idle subscriber, so it may not hold a client guard (or even
/// a [`fedimint_client::ClientHandleArc`]) in its own state across a poll; see
/// [`wait_holding_client`]'s own documentation.
///
/// `added` is the one exception: a [`tokio::sync::watch::Receiver`] outlives the client it was
/// obtained from (it is a plain channel handle, not a lock or a handle that keeps a federation
/// from closing), so it is fetched once, in [`subscribe_receive`], and carried here for the whole
/// life of the stream rather than re-derived on every poll the way the client itself is.
#[derive(Clone)]
struct ReceiveCtx {
    sdk: Weak<SdkInner>,
    federation_id: fedimint_core::config::FederationId,
    db: Database,
    id: OperationId,
    added: tokio::sync::watch::Receiver<()>,
}

fn live_federation(ctx: &ReceiveCtx) -> Option<Arc<FederationInner>> {
    ctx.sdk.upgrade()?.federation_inner(&ctx.federation_id)
}

/// Where a walletv2 receive subscription has got to, entirely in memory: the wire record is the
/// durable state, this is only what the stream needs to resume its own waits between polls.
enum ReceiveCursor {
    /// Nothing decided yet; read the wire and take it from there.
    Start,
    /// No upstream operation is linked yet. `announced` says whether `WaitingForTransaction` has
    /// already been handed out, so it is not repeated on every retry.
    Linking {
        address: Address,
        cursor: u64,
        announced: bool,
    },
    /// Linked. `announced` says whether the `Confirmed` a fresh link always earns has already
    /// been reported: the on-chain outcome itself is only checked once that has happened, so a
    /// continuous subscriber can never see a linked deposit jump straight from
    /// `WaitingForTransaction` past `Confirmed` to `Claimed`.
    Linked {
        link: Link,
        announced: bool,
    },
    /// Linked, `Confirmed` was already reported, and now the wait is for the module's own final
    /// state, unbounded this time, and, once that succeeds, for the mint to finish issuing the
    /// claimed notes.
    Waiting {
        link: Link,
    },
    Done,
}

/// A fresh stream over a walletv2 deposit, running the three-step algorithm
/// [`current_receive`] runs once, this time continuing past every step that only stops a
/// point-in-time read: an unlinked address is watched via [`Client::log_event_added_rx`] rather
/// than answered once, an `Aborted` claim goes back to looking for a fresh link instead of
/// stopping, and a linked operation's final state is awaited without a bound.
pub(super) async fn subscribe_receive(
    federation: &FederationInner,
    id: OperationId,
) -> Result<BoxStream<'static, Result<OnchainReceiveState>>> {
    let client = federation.client(false).await?;
    // Obtained once, here, rather than freshly from the client on every visit to the `Linking`
    // arm below. `Client::log_event_added_rx` only ever clones the receiver it stores internally,
    // and nothing in `Client` ever marks that stored receiver as seen, so a fresh clone reports a
    // change the instant any event at all has ever been logged, past or future: waited on
    // straight away, that busy-scans the log instead of waiting for a new one. Keeping this one
    // receiver for the stream's whole life, and calling `mark_unchanged` on it immediately before
    // each scan (see `receive_step`'s `Linking` arm), is what turns `changed().await` into an
    // actual wait for something logged after the scan started.
    let added = client.log_event_added_rx();
    drop(client);
    let ctx = ReceiveCtx {
        sdk: federation.sdk.clone(),
        federation_id: federation.id,
        db: federation.db(),
        id,
        added,
    };
    let stream = futures::stream::unfold(ReceiveCursor::Start, move |cursor| {
        receive_step(ctx.clone(), cursor)
    });
    Ok(until_final(stream))
}

/// One step of the subscription's own state machine: internally loops through every transition
/// the algorithm does not hand out (finding a link, a bounded check that already resolved) and
/// returns only once there is a state to report, together with where the next call resumes.
async fn receive_step(
    mut ctx: ReceiveCtx,
    mut cursor: ReceiveCursor,
) -> Option<(Result<OnchainReceiveState>, ReceiveCursor)> {
    loop {
        cursor = match cursor {
            ReceiveCursor::Done => return None,
            ReceiveCursor::Start => {
                let details = match read_wire(&ctx.db, ctx.id).await {
                    Ok(Some(details)) => details,
                    Ok(None) => {
                        let err =
                            internal(format!("no record for operation {}", ctx.id.fmt_full()));
                        return Some((Err(err), ReceiveCursor::Done));
                    }
                    Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                };
                if details.net_credit_msats.is_some() {
                    return Some((claimed_from(details), ReceiveCursor::Done));
                }
                match wire_link(&details) {
                    Ok(Some(found)) => ReceiveCursor::Linked {
                        link: found,
                        announced: false,
                    },
                    Ok(None) => {
                        let address = match OnchainReceiveDetails::try_from(details.clone()) {
                            Ok(public) => public.address,
                            Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                        };
                        ReceiveCursor::Linking {
                            address,
                            cursor: details.event_cursor.unwrap_or(0),
                            announced: false,
                        }
                    }
                    Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                }
            }
            ReceiveCursor::Linking {
                address,
                cursor: from,
                announced,
            } => {
                let Some(federation) = live_federation(&ctx) else {
                    return Some((Err(federation_closed()), ReceiveCursor::Done));
                };
                let client = match federation.client(false).await {
                    Ok(client) => client,
                    Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                };
                let handle = client.handle();
                drop(client);
                let stop = handle.task_group().make_handle().make_shutdown_rx();
                // Marked unchanged right before the scan, not after: an event logged while
                // `find_link` is still paging through the log must still register once this call
                // reaches the `None` arm below, or it would sit unnoticed until some later,
                // unrelated event happened to wake the wait. `ctx.added` is the one receiver
                // `subscribe_receive` obtained for the stream's whole life; see there for why it
                // must not be re-fetched from the client here instead.
                ctx.added.mark_unchanged();
                // Run through `wait_holding_client`, like every other client-owning read in this
                // stream: the engine keeps this step's own future parked, handle and all, across
                // a dropped `next()`, so nothing here may hold the handle in its own frame across
                // an `.await` (see `wait_holding_client`'s own documentation). That is what keeps
                // the handle out of the `tokio::select!` below too: it is on the spawned task by
                // the time that runs, not in this future's own state.
                let scan_address = address.clone();
                let found = wait_holding_client(handle, stop, move |client| async move {
                    Ok(find_link(&client, &scan_address, from).await)
                })
                .await;
                match found {
                    Ok(Some(found)) => {
                        if let Err(err) = link(&federation, ctx.id, &found).await {
                            return Some((Err(err), ReceiveCursor::Done));
                        }
                        ReceiveCursor::Linked {
                            link: found,
                            announced: false,
                        }
                    }
                    Ok(None) if !announced => {
                        return Some((
                            Ok(OnchainReceiveState::WaitingForTransaction),
                            ReceiveCursor::Linking {
                                address,
                                cursor: from,
                                announced: true,
                            },
                        ));
                    }
                    Ok(None) => {
                        let mut closed = federation.closed();
                        tokio::select! {
                            _ = ctx.added.changed() => {}
                            _ = closed.changed() => {
                                if closed.has_changed().is_err() || *closed.borrow_and_update() {
                                    return Some((Err(federation_closed()), ReceiveCursor::Done));
                                }
                            }
                        }
                        ReceiveCursor::Linking {
                            address,
                            cursor: from,
                            announced: true,
                        }
                    }
                    Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                }
            }
            // A fresh link always earns an immediate `Confirmed`, reported before the on-chain
            // outcome is ever checked: the transaction has already been seen by the scanner, and
            // that is what `Confirmed` means on walletv2. Checking straight away and reporting
            // `Claimed` directly when the claim already settled would let a continuous
            // subscriber jump from `WaitingForTransaction` past `Confirmed`, which loses
            // information the engine's own deduplication makes this extra step free to keep.
            ReceiveCursor::Linked {
                link: found,
                announced: false,
            } => {
                return Some((
                    Ok(OnchainReceiveState::Confirmed {
                        txid: found.txid.clone(),
                        gross_deposited: found.gross,
                    }),
                    ReceiveCursor::Linked {
                        link: found,
                        announced: true,
                    },
                ));
            }
            ReceiveCursor::Linked {
                link: found,
                announced: true,
            } => {
                let Some(federation) = live_federation(&ctx) else {
                    return Some((Err(federation_closed()), ReceiveCursor::Done));
                };
                let client = match federation.client(false).await {
                    Ok(client) => client,
                    Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                };
                // Every other arm drops its guard before awaiting anything else: a close must
                // always be able to take the client write lock. `observe_link` and, on the
                // `Aborted` branch, `read_address` run on a cloned handle instead, so the guard
                // is gone before either one is awaited; `observe_link` itself runs through
                // `wait_holding_client`, so that handle is never in this future's own frame
                // either, the same as every other client-owning read in this stream.
                let handle = client.handle();
                drop(client);
                let stop = handle.task_group().make_handle().make_shutdown_rx();
                let db = ctx.db.clone();
                let id = ctx.id;
                let observed = found.clone();
                let outcome = wait_holding_client(handle, stop, move |client| async move {
                    observe_link(&client, &db, id, &observed).await
                })
                .await;
                match outcome {
                    Ok(ClaimProgress::StillFunding) => {
                        return Some((
                            Ok(OnchainReceiveState::Confirmed {
                                txid: found.txid.clone(),
                                gross_deposited: found.gross,
                            }),
                            ReceiveCursor::Waiting { link: found },
                        ));
                    }
                    Ok(ClaimProgress::Aborted) => {
                        let address = match read_address(&ctx.db, ctx.id).await {
                            Ok(address) => address,
                            Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                        };
                        return Some((
                            Ok(OnchainReceiveState::Confirmed {
                                txid: found.txid.clone(),
                                gross_deposited: found.gross,
                            }),
                            ReceiveCursor::Linking {
                                address,
                                cursor: found.cursor,
                                announced: true,
                            },
                        ));
                    }
                    Ok(ClaimProgress::Final(state)) => {
                        return Some((Ok(state), ReceiveCursor::Done));
                    }
                    Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                }
            }
            ReceiveCursor::Waiting { link: found } => {
                let Some(federation) = live_federation(&ctx) else {
                    return Some((Err(federation_closed()), ReceiveCursor::Done));
                };
                let client = match federation.client(false).await {
                    Ok(client) => client,
                    Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                };
                let handle = client.handle();
                drop(client);
                let stop = handle.task_group().make_handle().make_shutdown_rx();
                let upstream = found.upstream;
                let outcome = wait_holding_client(handle, stop, move |client| async move {
                    let module = module_of(&client)?;
                    module
                        .await_final_receive_operation_state(upstream)
                        .await
                        .map_err(subscribe_error)
                })
                .await;
                match outcome {
                    Ok(FinalReceiveOperationState::Aborted) => {
                        let address = match read_address(&ctx.db, ctx.id).await {
                            Ok(address) => address,
                            Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                        };
                        return Some((
                            Ok(OnchainReceiveState::Confirmed {
                                txid: found.txid.clone(),
                                gross_deposited: found.gross,
                            }),
                            ReceiveCursor::Linking {
                                address,
                                cursor: found.cursor,
                                announced: true,
                            },
                        ));
                    }
                    Ok(FinalReceiveOperationState::Success) => {
                        let Some(federation) = live_federation(&ctx) else {
                            return Some((Err(federation_closed()), ReceiveCursor::Done));
                        };
                        let client = match federation.client(false).await {
                            Ok(client) => client,
                            Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                        };
                        let handle = client.handle();
                        drop(client);
                        let stop = handle.task_group().make_handle().make_shutdown_rx();
                        let db = ctx.db.clone();
                        let id = ctx.id;
                        let claim_link = found.clone();
                        // `claim_from_upstream` now waits out the mint's issuance of the claimed
                        // notes, which can take a while: run it through `wait_holding_client` so
                        // a subscriber that stops polling here does not leave the handle stuck
                        // for that whole wait.
                        let claimed = wait_holding_client(handle, stop, move |client| async move {
                            claim_from_upstream(&client, &db, id, &claim_link).await
                        })
                        .await;
                        return Some((claimed, ReceiveCursor::Done));
                    }
                    Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                }
            }
        };
    }
}

/// Decodes a walletv2 `WalletOperationMeta` and returns the address a `Receive` variant is
/// waiting on, for `federation.rs`'s deposit-adoption reconciler to match against an
/// `ONCHAIN_RECEIVE` record it already owns.
pub(crate) fn deposit_address_of(meta: &serde_json::Value) -> Option<String> {
    let meta: WalletOperationMeta = serde_json::from_value(meta.clone()).ok()?;
    match meta {
        WalletOperationMeta::Receive(ReceiveMeta {
            address: Some(address),
            ..
        }) => Some(address.assume_checked_ref().to_string()),
        WalletOperationMeta::Receive(_) | WalletOperationMeta::Send(_) => None,
    }
}

/// Rebuilds a record from a walletv2 log entry: exact for a `Send` this SDK created, whose
/// custom metadata carries the quoted terms verbatim; an estimate for one it did not create.
/// A `Receive` entry backfills only once it names both an address and the outpoint that funded
/// it; walletv2 mints no operation of its own at `receive` time, so this SDK's own record is
/// what an entry missing either is waiting to be adopted by instead (see `deposit_address_of`),
/// never something to estimate from the log alone.
pub(super) fn backfill(
    id: OperationId,
    meta: &serde_json::Value,
    created_at: u64,
) -> Option<Backfilled> {
    let meta: WalletOperationMeta = serde_json::from_value(meta.clone()).ok()?;
    match meta {
        WalletOperationMeta::Send(SendMeta {
            address,
            value,
            fee,
            custom_meta,
            ..
        }) => {
            let wire = match from_custom_meta::<wire::OnchainSendDetailsWire>(&custom_meta) {
                Some(wire) => wire,
                None => {
                    let fee_amount = from_upstream(fedimint_core::Amount::from(fee));
                    let total_btc = value.checked_add(fee)?;
                    let total_amount = from_upstream(fedimint_core::Amount::from(total_btc));
                    wire::OnchainSendDetailsWire {
                        address: address.assume_checked_ref().to_string(),
                        amount_sats: value.to_sat(),
                        fee_msats: fee_amount.msats(),
                        total_msats: total_amount.msats(),
                        created_at,
                    }
                }
            };
            Some(Backfilled {
                kind: kinds::ONCHAIN_SEND,
                details: wire::encode_send_wire(&wire).ok()?,
                phase: None,
                final_state: None,
            })
        }
        WalletOperationMeta::Receive(ReceiveMeta {
            address: Some(address),
            value,
            outpoint: Some(outpoint),
            ..
        }) => {
            let wire = wire::OnchainReceiveDetailsWire {
                address: address.assume_checked_ref().to_string(),
                txid: Some(Txid::from_upstream(outpoint.txid).to_string()),
                gross_deposited_sats: Some(value.to_sat()),
                fee_msats: None,
                fee_breakdown: None,
                net_credit_msats: None,
                created_at,
                upstream_operation_id: Some(id.fmt_full().to_string()),
                event_cursor: None,
            };
            Some(Backfilled {
                kind: kinds::ONCHAIN_RECEIVE,
                details: wire::encode_receive_wire(&wire).ok()?,
                phase: Some(wire::PHASE_SEEN),
                final_state: None,
            })
        }
        // Neither an address nor an outpoint is knowable yet, so there is nothing to backfill:
        // this SDK's own record (adopted by address, see `deposit_address_of`) is what carries
        // the deposit until the scanner resolves both.
        WalletOperationMeta::Receive(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use fedimint_core::BitcoinHash;
    use fedimint_core::bitcoin::address::NetworkUnchecked;

    use super::*;
    use crate::{Amount, Timestamp};

    fn a_bitcoin_txid() -> bitcoin::Txid {
        "0000000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .expect("a well-formed transaction id")
    }

    fn upstream_address() -> bitcoin::Address<NetworkUnchecked> {
        "bcrt1q2nfxmhd4n3c8834pj72xagvyr9gl57n5r94fsl"
            .parse()
            .expect("a valid regtest address")
    }

    fn an_address() -> Address {
        "bcrt1q2nfxmhd4n3c8834pj72xagvyr9gl57n5r94fsl"
            .parse()
            .expect("a valid regtest address")
    }

    fn an_operation_id() -> OperationId {
        OperationId::new_random()
    }

    fn a_change_range() -> fedimint_core::OutPointRange {
        fedimint_core::OutPointRange::new_single(fedimint_core::TransactionId::all_zeros(), 0)
            .expect("a range")
    }

    fn send_meta(
        value_sats: u64,
        fee_sats: u64,
        custom_meta: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::to_value(WalletOperationMeta::Send(SendMeta {
            change_outpoint_range: a_change_range(),
            address: upstream_address(),
            value: bitcoin::Amount::from_sat(value_sats),
            fee: bitcoin::Amount::from_sat(fee_sats),
            custom_meta,
        }))
        .expect("serialises")
    }

    fn receive_meta(
        address: Option<bitcoin::Address<NetworkUnchecked>>,
        value_sats: u64,
        fee_sats: u64,
        outpoint: Option<bitcoin::OutPoint>,
    ) -> serde_json::Value {
        serde_json::to_value(WalletOperationMeta::Receive(ReceiveMeta {
            change_outpoint_range: a_change_range(),
            value: bitcoin::Amount::from_sat(value_sats),
            fee: bitcoin::Amount::from_sat(fee_sats),
            address,
            outpoint,
        }))
        .expect("serialises")
    }

    #[test]
    fn map_final_send_states_fold_onto_the_send_lifecycle() {
        let txid = a_bitcoin_txid();
        let cases = [
            (
                FinalSendOperationState::Success(txid),
                OnchainSendState::Succeeded {
                    txid: Txid::from_upstream(txid),
                },
            ),
            (
                FinalSendOperationState::Aborted,
                OnchainSendState::Refunded {
                    reason: "the federation rejected the funding transaction".to_owned(),
                },
            ),
            (
                FinalSendOperationState::Failure,
                OnchainSendState::Failed {
                    reason: "the funding was accepted but no transaction came of it".to_owned(),
                },
            ),
        ];
        for (upstream, expected) in cases {
            assert_eq!(map_final_send(&upstream), expected, "{upstream:?}");
        }
    }

    #[test]
    fn a_send_log_entry_without_our_wire_backfills_an_estimate() {
        let meta = send_meta(25_000, 500, serde_json::Value::Null);
        let backfilled = backfill(an_operation_id(), &meta, 1_700_000_000_000).expect("recognised");
        assert_eq!(backfilled.kind, kinds::ONCHAIN_SEND);
        assert_eq!(backfilled.phase, None);
        assert_eq!(backfilled.final_state, None);
        let details = wire::decode_send_details(&backfilled.details).expect("decode");
        assert_eq!(details.amount, Sats::from_sats(25_000));
        assert_eq!(details.fee, Amount::from_msats(500_000));
        assert_eq!(details.total, Amount::from_msats(25_500_000));
    }

    #[test]
    fn a_send_log_entry_with_our_wire_backfills_exactly() {
        let exact = OnchainSendDetails {
            address: an_address(),
            amount: Sats::from_sats(25_000),
            fee: Amount::from_msats(1_234),
            total: Amount::from_msats(25_001_234),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        };
        let our_wire = wire::OnchainSendDetailsWire::from(&exact);
        let meta = send_meta(25_000, 500, custom_meta(&our_wire).expect("encode"));
        // The upstream meta's own fee, if it were used, would imply a fee of 500 000 msat
        // rather than the wire's 1 234: this proves our own copy wins over the estimate.
        let backfilled = backfill(an_operation_id(), &meta, 999).expect("recognised");
        assert_eq!(backfilled.kind, kinds::ONCHAIN_SEND);
        let recovered = wire::decode_send_details(&backfilled.details).expect("decode");
        assert_eq!(recovered, exact);
    }

    #[test]
    fn a_receive_log_entry_backfills_a_receive_record() {
        let txid = a_bitcoin_txid();
        let outpoint = bitcoin::OutPoint { txid, vout: 0 };
        let id = an_operation_id();
        let meta = receive_meta(Some(upstream_address()), 100_000, 500, Some(outpoint));
        let backfilled = backfill(id, &meta, 1_700_000_000_000).expect("recognised");
        assert_eq!(backfilled.kind, kinds::ONCHAIN_RECEIVE);
        assert_eq!(backfilled.phase, Some(wire::PHASE_SEEN));
        assert_eq!(backfilled.final_state, None);
        let raw = wire::decode_receive_wire(&backfilled.details).expect("decode");
        assert_eq!(
            raw.address,
            upstream_address().assume_checked_ref().to_string()
        );
        assert_eq!(raw.txid, Some(Txid::from_upstream(txid).to_string()));
        assert_eq!(raw.gross_deposited_sats, Some(100_000));
        assert_eq!(raw.upstream_operation_id, Some(id.fmt_full().to_string()));
        assert_eq!(raw.created_at, 1_700_000_000_000);
    }

    #[test]
    fn a_receive_log_entry_with_no_address_backfills_nothing() {
        let outpoint = bitcoin::OutPoint {
            txid: a_bitcoin_txid(),
            vout: 0,
        };
        let meta = receive_meta(None, 100_000, 500, Some(outpoint));
        assert!(backfill(an_operation_id(), &meta, 0).is_none());
    }

    #[test]
    fn a_receive_log_entry_with_no_outpoint_backfills_nothing() {
        let meta = receive_meta(Some(upstream_address()), 100_000, 500, None);
        assert!(backfill(an_operation_id(), &meta, 0).is_none());
    }
}
