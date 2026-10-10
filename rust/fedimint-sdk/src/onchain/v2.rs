//! The walletv2 wallet module (`walletv2`): the withdrawal plan and send, the deposit address,
//! the record a deposit gets when the module claims it and its subscription, and the backfill of
//! this module's own operation log.

use std::sync::{Arc, Weak};

use fedimint_client::Client;
use fedimint_client_module::ClientModuleInstance;
use fedimint_client_module::TransactionSubmitError;
use fedimint_client_module::transaction::FeeQuote;
use fedimint_core::bitcoin;
use fedimint_core::core::{ModuleInstanceId, OperationId};
use fedimint_core::db::{Database, IDatabaseTransactionOpsCoreTyped};
use fedimint_core::util::{BoxStream, FmtCompact};
use fedimint_eventlog::{DBTransactionEventLogExt as _, Event as _, EventLogId, PersistedLogEntry};
use fedimint_walletv2_client::events::ReceivePaymentEvent;
use fedimint_walletv2_client::{
    FinalReceiveOperationState, FinalSendOperationState, ReceiveMeta, SendError, SendMeta,
    WalletClientModule, WalletOperationMeta,
};
use fedimint_walletv2_common::KIND;
use fedimint_walletv2_common::config::WalletClientConfig;
use futures::StreamExt as _;

use super::driver::{SendStep, through_settle};
use super::{
    OnchainQuoteInner, Plan, Terms, add, balance_of, bitcoin_to_sats, check_amount,
    check_covers_amount, claim_figures, fee_quote_failure, fee_quote_refusal, from_upstream,
    insufficient, internal, now, plan_of, quote_changed, sats_to_amount, sats_to_bitcoin,
    subscribe_error, timeout, unreachable, wire,
};
use crate::federation::{FederationInner, wait_holding_client};
use crate::operation::{
    Backfilled, CURRENT_STATE_SETTLE, Driver, custom_meta, from_custom_meta, kinds, until_final,
};
use crate::sdk::{CONTACT_TIMEOUT, SdkInner};
use crate::{
    Address, Amount, Error, ErrorCode, OnchainReceiveDetails, OnchainReceiveState,
    OnchainSendDetails, OnchainSendState, Operation, Result, Sats, Txid,
};

/// The walletv2 module's own configuration, cast out of the client's decoded config exactly as
/// the ecash facade reads `MintClientConfig`: this module keeps no public accessor for its
/// `dust_limit` or `fee_consensus` the way the v1 wallet module does.
pub(super) async fn config(client: &Client, id: ModuleInstanceId) -> Result<WalletClientConfig> {
    let client_config = client.config().await;
    let module_config = client_config
        .get_module_cfg(id)
        .map_err(|err| internal(err.fmt_compact()))?;
    module_config
        .cast::<WalletClientConfig>()
        .cloned()
        .map_err(|err| internal(err.fmt_compact()))
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
/// (`modules/fedimint-walletv2-client/src/lib.rs:291-304`).
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
// | `Federation`                    | `FederationUnreachable`  |
// | `UnsupportedAddress`            | `InvalidInput`           |
// | `Failed`                        | `Internal`               |
// | any other                       | `Internal`               |
//
// `InsufficientFunds` names no amounts, so it carries none into the error either.
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
        SendError::NoConsensusFeerateAvailable | SendError::Federation(_) => {
            unreachable(err.fmt_compact())
        }
        SendError::UnsupportedAddress => Error::new(
            ErrorCode::InvalidInput,
            "this address type is not supported",
        ),
        _ => internal(err.fmt_compact()),
    }
}

/// Plans a walletv2 withdrawal: prices the destination output, then the transaction that funds
/// it.
pub(super) async fn plan(
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
            // `required` is the amount plus the on-chain fee already quoted above; the dry run
            // that would have priced the funding side is exactly what failed.
            let required = add(sats_to_amount(amount)?, chain_fee)?;
            return Err(fee_quote_refusal(
                &err,
                required,
                Some(available),
                "could not quote the withdrawal's funding fee",
            ));
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
            // `required` is the amount plus the on-chain fee already re-quoted above; the dry
            // run that would have priced the funding side is exactly what failed.
            let required = add(sats_to_amount(quote.amount)?, fresh_chain_fee_amount)?;
            return Err(fee_quote_failure(
                client,
                federation.status(),
                &err,
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
// step and no `Created` to map here: `subscribe_send` reports that state itself before this
// ever runs. Upstream documents `Failure` itself as "a programming
// error has occurred or the federation is malicious"
// (`modules/fedimint-walletv2-client/src/lib.rs:106`), which is why it maps to `Failed` rather
// than the ordinary `Refunded` ending `Aborted` gets.
//
// `Aborted` is the funding rejection, so it is not an ending here: it is the step that sends the
// withdrawal to the settle gate, which chooses `Refunded` or `Failed` from what the recovery of
// the notes it removed establishes. `Failure` is unrelated to that recovery and stays an ending
// of its own.
//
// | upstream    | here                                                          |
// | ----------- | ------------------------------------------------------------- |
// | `Success`   | `Succeeded { txid }`                                            |
// | `Aborted`   | `FundingRejected { reason: "the federation rejected ..." }`      |
// | `Failure`   | `Failed { reason: "the funding was accepted but ..." }`          |
pub(super) fn map_final_send(state: &FinalSendOperationState) -> SendStep {
    match state {
        FinalSendOperationState::Success(txid) => SendStep::State(OnchainSendState::Succeeded {
            txid: Txid::from_upstream(*txid),
        }),
        FinalSendOperationState::Aborted => SendStep::FundingRejected {
            reason: "the federation rejected the funding transaction".to_owned(),
        },
        FinalSendOperationState::Failure => SendStep::State(OnchainSendState::Failed {
            reason: "the funding was accepted but no transaction came of it".to_owned(),
        }),
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
    let created = futures::stream::once(async { Ok(SendStep::State(OnchainSendState::Created)) });
    let finished = futures::stream::once(async move {
        let state = wait_holding_client(handle, stop, move |client| async move {
            let module = module_of(&client)?;
            module
                .await_final_send_operation_state(id)
                .await
                .map_err(|err| subscribe_error(err.fmt_compact()))
        })
        .await?;
        Ok(map_final_send(&state))
    });
    // The stream is `'static` and outlives this call, so it carries the way back to the
    // federation rather than the federation itself; see `through_settle`.
    let stream = through_settle(
        created.chain(finished),
        federation.sdk.clone(),
        federation.id,
        id,
    );
    Ok(until_final(stream))
}

/// The walletv2 module's current deposit address.
///
/// The module hands back the highest address its scanner has derived, which stays the same one
/// until the scanner has seen it paid, and waits for the scanner when it has derived none yet
/// (`modules/fedimint-walletv2-client/src/lib.rs:581-601`). Deriving one is a hash-prefix search
/// that can take well over half a minute on constrained hardware (fedimint/fedimint-sdk#418),
/// and the caller holds the client throughout, so the wait is bounded.
pub(super) async fn receive(module: &WalletClientModule) -> Result<Address> {
    let address = fedimint_core::runtime::timeout(ADDRESS_WAIT_TIMEOUT, module.receive())
        .await
        .map_err(|_| {
            Error::new(
                ErrorCode::Timeout,
                "the wallet has not derived a deposit address yet; try again shortly",
            )
        })?;
    Ok(Address::from_upstream(address.into_unchecked()))
}

/// How long [`receive`] waits for the module's scanner to derive its first address.
const ADDRESS_WAIT_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(120);

/// Reads the walletv2 module's announcement that it found a payment and started claiming it
/// out of an event log entry, or `None` for any other entry: the operation the claim runs
/// under.
pub(super) fn deposit_found(entry: &PersistedLogEntry) -> Option<OperationId> {
    if entry.module_kind() != Some(&KIND) || entry.kind != ReceivePaymentEvent::KIND {
        return None;
    }
    Some(entry.to_event::<ReceivePaymentEvent>()?.operation_id)
}

/// Gives the deposit claimed under `id` its record, and returns the operation that stands for
/// it: `Some` exactly when `id` itself has a deposit record.
///
/// The record is rebuilt from the claim's own log entry (see [`backfill`]). A claim of a deposit
/// that is already recorded under another claim gets none, and neither does one whose entry
/// cannot be read as a deposit, so both are answered with `None`.
///
/// A record under `id` that does not read as a deposit's is left as it is and answered with
/// `None` too, so one unreadable record cannot hold up every deposit announced after it.
///
/// # Errors
///
/// [`Storage`](ErrorCode::Storage).
pub(super) async fn adopt(
    federation: &Arc<FederationInner>,
    id: OperationId,
) -> Result<Option<OperationId>> {
    let Some(record) = federation.record_of(id).await? else {
        return Ok(None);
    };
    if record.kind != kinds::ONCHAIN_RECEIVE || record.module != "walletv2" {
        return Ok(None);
    }
    if let Err(err) = wire::decode_receive_wire(&record.details) {
        tracing::warn!(
            target: "fedimint_sdk",
            federation = %federation.id,
            operation = %id.fmt_full(),
            error = %err,
            "a deposit was announced for an operation whose record cannot be read",
        );
        return Ok(None);
    }
    Ok(Some(id))
}

/// What identifies a deposit across the claims the module makes of it: the output that paid it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Paid(bitcoin::OutPoint);

/// The output a walletv2 `Receive` log entry claims, or `None` for any other entry. A `Receive`
/// entry that names no address or no output is one of those: [`backfill`] records no deposit for
/// it.
pub(super) fn paid_by(meta: &serde_json::Value) -> Option<Paid> {
    let meta: WalletOperationMeta = serde_json::from_value(meta.clone()).ok()?;
    let WalletOperationMeta::Receive(ReceiveMeta {
        address: Some(_),
        outpoint: Some(outpoint),
        ..
    }) = meta
    else {
        return None;
    };
    Some(Paid(outpoint))
}

/// Whether a stored deposit record is of the output `paid`.
pub(super) fn names(details: &str, paid: &Paid) -> bool {
    wire::decode_receive_wire(details)
        .ok()
        .and_then(|details| paid_of(&details))
        .is_some_and(|recorded| recorded == *paid)
}

/// The output a deposit record was written for, or `None` for a record that names none: it
/// keeps no output index, or its transaction id does not parse. Every record of a walletv2
/// deposit names one.
fn paid_of(details: &wire::OnchainReceiveDetailsWire) -> Option<Paid> {
    Some(Paid(bitcoin::OutPoint {
        txid: details.txid.parse().ok()?,
        vout: details.vout?,
    }))
}

/// One claim of a deposit: the upstream operation it runs under, and the event-log position a
/// search for another claim of the deposit starts from. For a record that has not searched yet
/// that is the beginning of the log, because the claim it was written for need not be the first
/// one made of its output. The module's scanner comes back to an output until a claim of it was
/// accepted (`modules/fedimint-walletv2-client/src/lib.rs:864-930`), so a wallet that restarts
/// while a claim is pending can claim an output that is still unspent a second time. After a
/// search the position is just past the announcement of the claim that was found, or as far as
/// a search that found none got.
///
/// The claim and the position are stored in one write, and every pair stored holds the same
/// promise: a claim of the output announced before the position is the one the record follows
/// or one it has moved on from. Readers of one record do not wait for each other, so the pair
/// stored last wins and may be the older of two. That costs the next search a stretch of the
/// log it reads again, and cannot make it miss a claim.
#[derive(Debug, Clone)]
pub(super) struct Link {
    pub(super) upstream: OperationId,
    pub(super) txid: Txid,
    pub(super) gross: Sats,
    pub(super) cursor: u64,
}

/// Scans the event log in `db`, the client's own database, from `from` for the next claim of
/// `paid` other than `rejected`. It is one pass over the log as it stands now and does not wait
/// for new entries, so a caller that wants to keep watching calls this again after the next
/// [`Client::log_event_added_rx`] tick.
///
/// Returns the claim, if there is one, and otherwise the position the pass reached, which is
/// where the next one starts.
pub(super) async fn find_claim(
    db: &Database,
    paid: &Paid,
    rejected: OperationId,
    from: u64,
) -> std::result::Result<Link, u64> {
    const PAGE: u64 = 64;
    let mut pos = EventLogId::LOG_START.saturating_add(from);
    loop {
        let page = db
            .begin_transaction_nc()
            .await
            .get_event_log(Some(pos), PAGE)
            .await;
        if page.is_empty() {
            return Err(u64::from(pos));
        }
        for entry in &page {
            let cursor = u64::from(entry.id().saturating_add(1));
            if entry.module_kind() != Some(&KIND) || entry.kind != ReceivePaymentEvent::KIND {
                continue;
            }
            let Some(event) = entry.to_event::<ReceivePaymentEvent>() else {
                continue;
            };
            // The module only ever writes `None` for an output it could not resolve, and a
            // claim that names no output cannot be told to be of this deposit.
            let Some(outpoint) = event.outpoint else {
                continue;
            };
            if event.operation_id == rejected || Paid(outpoint) != *paid {
                continue;
            }
            return Ok(Link {
                upstream: event.operation_id,
                txid: Txid::from_upstream(outpoint.txid),
                gross: bitcoin_to_sats(event.value),
                cursor,
            });
        }
        pos = page
            .last()
            .expect("checked non-empty above")
            .id()
            .saturating_add(1);
    }
}

/// Records the claim a deposit's record follows and where a search for another one starts: on
/// moving to another claim, the upstream operation it runs under and the end of its
/// announcement, and after a search that found none, how far that search got. The transaction
/// and the amount are the deposit's own and stay as they are.
pub(super) async fn link(
    federation: &FederationInner,
    id: OperationId,
    found: &Link,
) -> Result<()> {
    let upstream = found.upstream.fmt_full().to_string();
    let cursor = found.cursor;
    update_wire(&federation.db(), id, move |details| {
        details.upstream_operation_id = Some(upstream.clone());
        details.event_cursor = Some(cursor);
    })
    .await?;
    Ok(())
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
        Ok(Err(err)) => Err(subscribe_error(err.fmt_compact())),
        Err(_) => Ok(None),
    }
}

// walletv2 has no state machine for a deposit to follow the way v1's `DepositStateV2` is one:
// it has one per claim. The phases `OnchainReceiveState` reports here are this SDK's own
// observation of the deposit across its claims, and the module's own claim machine lands on them
// as `Funding` -> `Confirmed`, `Success` (once the mint has issued the claimed notes) ->
// `Claimed`, or `Failed` if the mint could not issue one of them, `Aborted` (another claim of
// the output, made already or still to come, is followed under the same operation id) -> stays
// `Confirmed`.
/// What the bounded (or, once already `Confirmed`, unbounded) check of a linked upstream
/// operation found.
enum ClaimProgress {
    /// The claim has not settled, or has settled but the mint has not finished issuing the
    /// claimed notes yet; still `Confirmed` either way.
    StillFunding,
    /// The federation rejected the claim; still `Confirmed`, and another `ReceivePaymentEvent`
    /// for the same output moves the record on to the upstream operation it names.
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
    // ($FM/modules/fedimint-walletv2-client/src/lib.rs:630-649): it returns once every note the
    // claim minted is spendable, and fails if the mint's own state machine for one of them
    // ended in failure instead. That failure arrives as `TransactionSubmitError::PrimaryModule`
    // (`$FM/fedimint-client/src/client.rs:1320`), the only variant the wait itself produces besides
    // a missing primary module. Mint issuance cannot be delayed or failed from a test at this
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
                    "the claim was accepted but its notes could not be issued: {}",
                    cause.fmt_compact()
                ),
            });
        }
        Err(err) => {
            return Err(internal(format!(
                "could not wait for the claimed notes to be issued: {}",
                err.fmt_compact()
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

    let fee_msats = fee_total.msats();
    let fee_breakdown = wire::ReceiveFeeBreakdownWire::from(&breakdown);
    let net_credit_msats = net_credit.msats();
    let stored = update_wire(db, id, move |details| {
        details.fee_msats = Some(fee_msats);
        details.fee_breakdown = Some(fee_breakdown.clone());
        details.net_credit_msats = Some(net_credit_msats);
    })
    .await?;
    if stored.is_none() {
        return Err(internal(format!(
            "no record for operation {}",
            id.fmt_full()
        )));
    }

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

/// Changes the stored details of the deposit record `id` in one transaction, so a change worked
/// out from a copy read earlier cannot undo one committed in between. Returns the details as
/// they are stored afterwards, or `None` when there is no record.
///
/// # Errors
///
/// [`Storage`](ErrorCode::Storage), and [`Internal`](ErrorCode::Internal) for a record that does
/// not read as a deposit's.
async fn update_wire<F>(
    db: &Database,
    id: OperationId,
    change: F,
) -> Result<Option<wire::OnchainReceiveDetailsWire>>
where
    F: Fn(&mut wire::OnchainReceiveDetailsWire) + Clone + Send + Sync + 'static,
{
    db.autocommit(
        |dbtx, _| {
            let change = change.clone();
            Box::pin(async move {
                let key = crate::db::OperationRecordKey(id);
                let Some(mut record) = dbtx.get_value(&key).await else {
                    return Ok(Ok(None));
                };
                let mut details = match wire::decode_receive_wire(&record.details) {
                    Ok(details) => details,
                    Err(err) => return Ok(Err(err)),
                };
                change(&mut details);
                let encoded = match wire::encode_receive_wire(&details) {
                    Ok(encoded) => encoded,
                    Err(err) => return Ok(Err(err)),
                };
                if record.details != encoded {
                    record.details = encoded;
                    dbtx.insert_entry(&key, &record).await;
                }
                Ok::<_, core::convert::Infallible>(Ok(Some(details)))
            })
        },
        Some(100),
    )
    .await
    .map_err(crate::db::storage_error)?
}

/// The output the deposit recorded as `id` was paid by.
async fn read_paid(db: &Database, id: OperationId) -> Result<Paid> {
    let Some(details) = read_wire(db, id).await? else {
        return Err(internal(format!(
            "no record for operation {}",
            id.fmt_full()
        )));
    };
    paid_of(&details).ok_or_else(|| {
        internal(format!(
            "the record of operation {} names no output",
            id.fmt_full()
        ))
    })
}

fn claimed_from(details: wire::OnchainReceiveDetailsWire) -> Result<OnchainReceiveState> {
    let public = OnchainReceiveDetails::try_from(details)?;
    Ok(OnchainReceiveState::Claimed {
        txid: public.txid,
        gross_deposited: public.gross_deposited,
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

/// The claim a deposit's record follows.
///
/// # Errors
///
/// [`Internal`](ErrorCode::Internal) for a record that names no claim, or whose claim or
/// transaction id does not parse. Every record of a walletv2 deposit is written with the claim
/// it follows.
fn wire_link(details: &wire::OnchainReceiveDetailsWire) -> Result<Link> {
    let Some(upstream) = details.upstream_operation_id.as_deref() else {
        return Err(internal("a walletv2 deposit record names no claim"));
    };
    Ok(Link {
        upstream: parse_upstream_id(upstream)?,
        txid: details
            .txid
            .parse::<Txid>()
            .map_err(|_| internal("a stored transaction id does not parse"))?,
        gross: Sats::from_sats(details.gross_deposited_sats),
        cursor: details.event_cursor.unwrap_or(0),
    })
}

fn federation_closed() -> Error {
    Error::new(
        ErrorCode::FederationClosed,
        "this federation stopped running",
    )
}

/// The current state of a walletv2 deposit, following the same steps `subscribe_receive` runs
/// continuously: a claimed record answers from storage, and otherwise the claim the record
/// follows gets one bounded check, moving on past a rejected claim to another claim of the
/// deposit the way the subscription does.
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
    let mut found = wire_link(&details)?;
    let client = federation.client(false).await?;
    let mut progress = observe_link(&client, &db, id, &found).await?;
    // A deposit whose claim the federation rejected has, or soon gets, another claim under an
    // upstream operation of its own, announced like the first. `subscribe_receive` moves the
    // record on to it; this bounded read has to do the same, or a caller that only ever polls
    // `state()` would stay pinned to the rejected claim and read `Confirmed` long after another
    // one credited the deposit. Each rejected claim answers its final state at once, so the
    // loop only ever spends the bound on the live claim at its end.
    while let ClaimProgress::Aborted = progress {
        let paid = read_paid(&db, id).await?;
        let next = match find_claim(client.db(), &paid, found.upstream, found.cursor).await {
            Ok(next) => next,
            Err(reached) => {
                // Still the rejected claim, searched up to here: the next read picks the search
                // up from this position instead of walking the same stretch of the log again.
                found.cursor = reached;
                link(federation, id, &found).await?;
                break;
            }
        };
        link(federation, id, &next).await?;
        progress = observe_link(&client, &db, id, &next).await?;
        found = next;
    }
    match progress {
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
    /// The claim the record follows. `announced` says whether the `Confirmed` a deposit earns by
    /// existing has already been reported: the claim's own outcome is only checked once that
    /// has happened, so a continuous subscriber can never see a deposit jump straight to
    /// `Claimed`.
    Linked {
        link: Link,
        announced: bool,
    },
    /// `Confirmed` was already reported, and now the wait is for the module's own final state,
    /// unbounded this time, and, once that succeeds, for the mint to finish issuing the claimed
    /// notes.
    Waiting {
        link: Link,
    },
    /// The federation rejected the claim `link`: another claim of the deposit is searched for,
    /// and waited for while there is none. `link.cursor` is how far the search has got.
    Retrying {
        link: Link,
    },
    Done,
}

/// A fresh stream over a walletv2 deposit, running the steps [`current_receive`] runs once, this
/// time continuing past every one that only stops a point-in-time read: a claim's final state is
/// awaited without a bound, and after a rejected claim the module's next one is watched for via
/// [`Client::log_event_added_rx`] rather than looked for once.
pub(super) async fn subscribe_receive(
    federation: &FederationInner,
    id: OperationId,
) -> Result<BoxStream<'static, Result<OnchainReceiveState>>> {
    let client = federation.client(false).await?;
    // Obtained once, here, rather than freshly from the client on every visit to the `Retrying`
    // arm below. `Client::log_event_added_rx` only ever clones the receiver it stores internally,
    // and nothing in `Client` ever marks that stored receiver as seen, so a fresh clone reports a
    // change the instant any event at all has ever been logged, past or future: waited on
    // straight away, that busy-scans the log instead of waiting for a new one. Keeping this one
    // receiver for the stream's whole life, and calling `mark_unchanged` on it immediately before
    // each scan (see `receive_step`'s `Retrying` arm), is what turns `changed().await` into an
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
/// the algorithm does not hand out (a rejected claim, the search for the next one) and returns
/// only once there is a state to report, together with where the next call resumes.
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
                    Ok(found) => ReceiveCursor::Linked {
                        link: found,
                        announced: false,
                    },
                    Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                }
            }
            // A deposit always earns an immediate `Confirmed`, reported before the claim's own
            // outcome is ever checked: the module's scanner has already seen the transaction,
            // and that is what `Confirmed` means on walletv2. Checking straight away and
            // reporting `Claimed` directly when the claim already settled would let a
            // continuous subscriber never see `Confirmed` at all, which loses information the
            // engine's own deduplication makes this extra step free to keep.
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
                // Every arm drops its guard before awaiting anything else: a close must always
                // be able to take the client write lock. `observe_link` runs on a cloned handle
                // instead, through `wait_holding_client`, so that handle is never in this
                // future's own frame either: the engine keeps this step's future parked, handle
                // and all, across a dropped `next()` (see `wait_holding_client`'s own
                // documentation).
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
                    Ok(ClaimProgress::StillFunding) => ReceiveCursor::Waiting { link: found },
                    Ok(ClaimProgress::Aborted) => ReceiveCursor::Retrying { link: found },
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
                        .map_err(|err| subscribe_error(err.fmt_compact()))
                })
                .await;
                match outcome {
                    Ok(FinalReceiveOperationState::Aborted) => {
                        ReceiveCursor::Retrying { link: found }
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
                        // `claim_from_upstream` waits out the mint's issuance of the claimed
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
            // Nothing is reported while this waits: the deposit stays `Confirmed`, which a
            // subscriber has already been told.
            ReceiveCursor::Retrying { link: rejected } => {
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
                // `find_claim` is still paging through the log must still register once the
                // scan has come back with nothing, or it would sit unnoticed until some later,
                // unrelated event happened to wake the wait. `ctx.added` is the one receiver
                // `subscribe_receive` obtained for the stream's whole life; see there for why it
                // must not be re-fetched from the client here instead.
                ctx.added.mark_unchanged();
                // Run through `wait_holding_client`, like every other client-owning read in this
                // stream, which also keeps the handle out of the `tokio::select!` below: it is
                // on the spawned task by the time that runs, not in this future's own state.
                let db = ctx.db.clone();
                let id = ctx.id;
                let from = rejected.clone();
                let found = wait_holding_client(handle, stop, move |client| async move {
                    let paid = read_paid(&db, id).await?;
                    Ok(find_claim(client.db(), &paid, from.upstream, from.cursor).await)
                })
                .await;
                match found {
                    Ok(Ok(next)) => {
                        if let Err(err) = link(&federation, ctx.id, &next).await {
                            return Some((Err(err), ReceiveCursor::Done));
                        }
                        ReceiveCursor::Linked {
                            link: next,
                            announced: true,
                        }
                    }
                    Ok(Err(reached)) => {
                        // Kept in memory only: every event the client logs wakes this wait, and
                        // none of them should cost a write.
                        let rejected = Link {
                            cursor: reached,
                            ..rejected
                        };
                        let mut closed = federation.closed();
                        tokio::select! {
                            changed = ctx.added.changed() => {
                                // The client this receiver came from is gone. Ending the
                                // stream without a final state has the engine subscribe again,
                                // to whichever client is in place by then.
                                if changed.is_err() {
                                    return None;
                                }
                            }
                            _ = closed.changed() => {
                                if closed.has_changed().is_err() || *closed.borrow_and_update() {
                                    return Some((Err(federation_closed()), ReceiveCursor::Done));
                                }
                            }
                        }
                        ReceiveCursor::Retrying { link: rejected }
                    }
                    Err(err) => return Some((Err(err), ReceiveCursor::Done)),
                }
            }
        };
    }
}

/// Rebuilds a record from a walletv2 log entry: exact for a `Send` this SDK created, whose
/// custom metadata carries the quoted terms verbatim; an estimate for one it did not create.
/// A `Receive` entry is one claim of a deposit, and backfills the deposit's record when it names
/// both the address and the outpoint that paid it: the record follows that claim, under the
/// claim's own operation id.
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
                txid: Txid::from_upstream(outpoint.txid).to_string(),
                gross_deposited_sats: value.to_sat(),
                fee_msats: None,
                fee_breakdown: None,
                net_credit_msats: None,
                created_at,
                upstream_operation_id: Some(id.fmt_full().to_string()),
                event_cursor: None,
                vout: Some(outpoint.vout),
            };
            Some(Backfilled {
                kind: kinds::ONCHAIN_RECEIVE,
                details: wire::encode_receive_wire(&wire).ok()?,
                phase: None,
                final_state: None,
            })
        }
        // A claim that names no address or no outpoint cannot be reported as a deposit: its
        // record has to name both the address that was paid and the output that paid it.
        WalletOperationMeta::Receive(_) => None,
    }
}

// At file scope rather than inside `mod tests`, because a `mod tests` is private to its own
// file and the tests of the pass that reads these (`src/onchain/deposits.rs`) need them too.
#[cfg(test)]
pub(super) mod fixtures {
    use fedimint_core::core::OperationId;
    use fedimint_core::{BitcoinHash as _, bitcoin};
    use fedimint_eventlog::{Event as _, EventLogEntry, EventLogModule};
    use fedimint_walletv2_client::events::ReceivePaymentEvent;
    use fedimint_walletv2_client::{ReceiveMeta, WalletOperationMeta};
    use fedimint_walletv2_common::KIND;

    /// What every claim in these fixtures pays the federation's wallet to be made.
    const CLAIM_FEE: bitcoin::Amount = bitcoin::Amount::from_sat(500);

    /// The log entry meta the module writes for a claim of `outpoint`, which paid `sats` to
    /// `address`.
    pub(crate) fn claim(
        address: &str,
        sats: u64,
        outpoint: bitcoin::OutPoint,
    ) -> serde_json::Value {
        let change =
            fedimint_core::OutPointRange::new_single(fedimint_core::TransactionId::all_zeros(), 0)
                .expect("a range");
        serde_json::to_value(WalletOperationMeta::Receive(ReceiveMeta {
            change_outpoint_range: change,
            value: bitcoin::Amount::from_sat(sats),
            fee: CLAIM_FEE,
            address: Some(address.parse().expect("a valid address")),
            outpoint: Some(outpoint),
        }))
        .expect("serialises")
    }

    /// The event the module logs when it makes that claim under `operation`.
    pub(crate) fn announcement(
        operation: OperationId,
        address: &str,
        sats: u64,
        outpoint: bitcoin::OutPoint,
    ) -> EventLogEntry {
        let event = ReceivePaymentEvent {
            operation_id: operation,
            value: bitcoin::Amount::from_sat(sats),
            fee: CLAIM_FEE,
            address: address.parse().expect("a valid address"),
            outpoint: Some(outpoint),
        };
        EventLogEntry {
            kind: ReceivePaymentEvent::KIND,
            module: Some(EventLogModule { kind: KIND, id: 2 }),
            ts_usecs: 1_700_000_000_000_000,
            payload: serde_json::to_vec(&event).expect("serialises"),
        }
    }
}

#[cfg(test)]
mod tests {
    use fedimint_client_module::ClientModuleError;
    use fedimint_core::BitcoinHash;
    use fedimint_core::bitcoin::address::NetworkUnchecked;

    use super::*;
    use crate::{Amount, Timestamp};

    #[test]
    fn a_send_error_maps_to_the_code_that_names_its_cause() {
        assert_eq!(
            map_send_error(SendError::WrongNetwork).code,
            ErrorCode::NetworkMismatch
        );
        assert_eq!(
            map_send_error(SendError::DustValue).code,
            ErrorCode::InvalidInput
        );
        assert_eq!(
            map_send_error(SendError::InsufficientFunds).code,
            ErrorCode::InsufficientBalance
        );
        assert_eq!(
            map_send_error(SendError::NoConsensusFeerateAvailable).code,
            ErrorCode::FederationUnreachable
        );
        assert_eq!(
            map_send_error(SendError::UnsupportedAddress).code,
            ErrorCode::InvalidInput
        );
        // A fee request that no guardian answered.
        let no_answer = fedimint_api_client::api::FederationError {
            method: "send_fee".to_owned(),
            params: serde_json::Value::Null,
            general: None,
            peer_errors: Default::default(),
        };
        assert_eq!(
            map_send_error(SendError::Federation(Box::new(no_answer))).code,
            ErrorCode::FederationUnreachable
        );
    }

    #[test]
    fn a_failed_send_keeps_its_whole_cause_chain() {
        let failed = SendError::Failed(TransactionSubmitError::PrimaryModule(
            ClientModuleError::other("the notes are locked"),
        ));
        let err = map_send_error(failed);
        assert_eq!(err.code, ErrorCode::Internal);
        assert!(
            err.message
                .contains("The send transaction could not be submitted"),
            "{}",
            err.message
        );
        assert!(
            err.message.contains("The primary module failed"),
            "{}",
            err.message
        );
        assert!(
            err.message.contains("the notes are locked"),
            "{}",
            err.message
        );
    }

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

    /// A deposit record's stored details, as [`backfill`] writes them for a claim of `outpoint`.
    fn details_of_a_claim(outpoint: bitcoin::OutPoint) -> String {
        let meta = receive_meta(Some(upstream_address()), 100_000, 500, Some(outpoint));
        backfill(an_operation_id(), &meta, 1_700_000_000_000)
            .expect("recognised")
            .details
    }

    #[test]
    fn a_claim_names_the_output_it_claims() {
        let outpoint = bitcoin::OutPoint {
            txid: a_bitcoin_txid(),
            vout: 3,
        };
        let meta = receive_meta(Some(upstream_address()), 100_000, 500, Some(outpoint));
        assert_eq!(paid_by(&meta), Some(Paid(outpoint)));

        // Only a claim that names both the address and the output is one of a deposit.
        assert!(paid_by(&receive_meta(None, 100_000, 500, Some(outpoint))).is_none());
        assert!(paid_by(&receive_meta(Some(upstream_address()), 100_000, 500, None)).is_none());
        assert!(paid_by(&send_meta(100_000, 500, serde_json::Value::Null)).is_none());
        assert!(paid_by(&serde_json::Value::Null).is_none());
    }

    #[test]
    fn map_final_send_states_fold_onto_the_send_lifecycle() {
        let txid = a_bitcoin_txid();
        let cases = [
            (
                FinalSendOperationState::Success(txid),
                SendStep::State(OnchainSendState::Succeeded {
                    txid: Txid::from_upstream(txid),
                }),
            ),
            // Not `Refunded`: a rejected funding is a step towards an ending, not one. Which
            // ending it becomes is the settle gate's to say.
            (
                FinalSendOperationState::Aborted,
                SendStep::FundingRejected {
                    reason: "the federation rejected the funding transaction".to_owned(),
                },
            ),
            // `Failure` is not a funding rejection and has no recovery to wait for, so it stays
            // an ending of its own.
            (
                FinalSendOperationState::Failure,
                SendStep::State(OnchainSendState::Failed {
                    reason: "the funding was accepted but no transaction came of it".to_owned(),
                }),
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
        assert_eq!(backfilled.phase, None);
        assert_eq!(backfilled.final_state, None);
        let raw = wire::decode_receive_wire(&backfilled.details).expect("decode");
        assert_eq!(
            raw.address,
            upstream_address().assume_checked_ref().to_string()
        );
        assert_eq!(raw.txid, Txid::from_upstream(txid).to_string());
        assert_eq!(raw.gross_deposited_sats, 100_000);
        // The claim the record follows is the entry's own, and the output is kept whole, which
        // is what tells a later claim of the same deposit from a claim of another.
        assert_eq!(raw.upstream_operation_id, Some(id.fmt_full().to_string()));
        assert_eq!(raw.vout, Some(0));
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

    /// What the claim-following code does with nothing but storage: none of these needs a
    /// client, and a federation rejecting a claim cannot be provoked from a test.
    #[cfg(not(target_family = "wasm"))]
    mod stored {
        use super::*;
        use crate::db::{federation_namespace, in_memory_root};
        use crate::onchain::deposit_fixtures::{a_walletv2_claim, txid};
        use crate::operation::write_details_in;

        fn operation(byte: u8) -> OperationId {
            OperationId([byte; 32])
        }

        /// A federation with one walletv2 deposit, claimed under `operation(1)` and announced
        /// at the start of the event log, and the deposit's record.
        async fn a_recorded_deposit() -> (Database, Arc<FederationInner>, OperationId) {
            let db = federation_namespace(&in_memory_root(), [1u8; 32]);
            let federation = FederationInner::detached(db.clone(), true);
            let id = operation(1);
            let paid = bitcoin::OutPoint {
                txid: txid(3),
                vout: 1,
            };
            a_walletv2_claim(&db, id, paid, 0).await;
            crate::onchain::pick_up_deposits(&federation)
                .await
                .expect("pick up");
            (db, federation, id)
        }

        async fn stored(db: &Database, id: OperationId) -> wire::OnchainReceiveDetailsWire {
            read_wire(db, id)
                .await
                .expect("reads")
                .expect("the deposit has its record")
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_record_follows_the_claim_it_was_written_for() {
            let (db, _federation, id) = a_recorded_deposit().await;

            let found =
                wire_link(&stored(&db, id).await).expect("a deposit record follows a claim");
            assert_eq!(found.upstream, id);
            assert_eq!(found.txid.to_string(), txid(3).to_string());
            assert_eq!(found.gross, Sats::from_sats(100_000));
            // No search has been made for another claim, so one would start at the beginning.
            assert_eq!(found.cursor, 0);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn moving_on_to_a_later_claim_keeps_the_deposit_s_own_facts() {
            let (db, federation, id) = a_recorded_deposit().await;
            let before = stored(&db, id).await;
            let later = Link {
                upstream: operation(2),
                txid: before.txid.parse().expect("a txid"),
                gross: Sats::from_sats(100_000),
                cursor: 5,
            };

            link(&federation, id, &later).await.expect("link");

            let after = stored(&db, id).await;
            assert_eq!(
                after.upstream_operation_id,
                Some(operation(2).fmt_full().to_string())
            );
            assert_eq!(after.event_cursor, Some(5));
            assert_eq!(after.address, before.address);
            assert_eq!(after.txid, before.txid);
            assert_eq!(after.vout, before.vout);
            assert_eq!(after.gross_deposited_sats, before.gross_deposited_sats);
            assert_eq!(after.created_at, before.created_at);
            let found = wire_link(&after).expect("still follows");
            assert_eq!(found.upstream, operation(2));
            assert_eq!(found.cursor, 5);

            // Reading the first claim's announcement again does not move the record back.
            assert_eq!(adopt(&federation, id).await.expect("adopt"), Some(id));
            assert_eq!(stored(&db, id).await, after);
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_claimed_deposit_answers_from_its_record() {
            let (db, federation, id) = a_recorded_deposit().await;
            let mut details = stored(&db, id).await;
            details.fee_msats = Some(1_500);
            details.net_credit_msats = Some(99_998_500);
            write_details_in(
                &db,
                id,
                wire::encode_receive_wire(&details).expect("encode"),
            )
            .await
            .expect("write");

            // No client exists on a detached federation, so an answer proves none was asked.
            let state = current_receive(&federation, id).await.expect("a state");
            assert_eq!(
                state,
                OnchainReceiveState::Claimed {
                    txid: txid(3).to_string().parse().expect("a txid"),
                    gross_deposited: Sats::from_sats(100_000),
                    net_credit: Amount::from_msats(99_998_500),
                }
            );
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn an_unclaimed_deposit_needs_the_federation_running() {
            let (_db, federation, id) = a_recorded_deposit().await;
            let err = current_receive(&federation, id)
                .await
                .expect_err("the claim cannot be checked without a client");
            assert_eq!(err.code, ErrorCode::FederationClosed);
        }

        /// The search for a later claim of a deposit passes over the rejected claim itself and
        /// over claims of other outputs, and says how far it got when it finds none.
        #[tokio::test(flavor = "multi_thread")]
        async fn a_search_finds_the_next_claim_of_the_output_or_says_how_far_it_got() {
            let (db, _federation, rejected) = a_recorded_deposit().await;
            let paid = read_paid(&db, rejected).await.expect("the output");

            // Nothing but the rejected claim's own announcement, read or not.
            assert!(matches!(find_claim(&db, &paid, rejected, 0).await, Err(1)));
            assert!(matches!(find_claim(&db, &paid, rejected, 1).await, Err(1)));

            // A claim of another output of the same transaction is another deposit.
            let sibling = bitcoin::OutPoint {
                txid: txid(3),
                vout: 0,
            };
            a_walletv2_claim(&db, operation(2), sibling, 1).await;
            assert!(matches!(find_claim(&db, &paid, rejected, 1).await, Err(2)));

            // The module's next claim of the same output.
            let retried = operation(3);
            let same = bitcoin::OutPoint {
                txid: txid(3),
                vout: 1,
            };
            a_walletv2_claim(&db, retried, same, 2).await;
            for from in [0, 2] {
                let found = find_claim(&db, &paid, rejected, from)
                    .await
                    .expect("the later claim");
                assert_eq!(found.upstream, retried);
                assert_eq!(found.txid.to_string(), txid(3).to_string());
                assert_eq!(found.gross, Sats::from_sats(100_000));
                // Just past the later claim's announcement.
                assert_eq!(found.cursor, 3);
            }
            // And nothing after it.
            assert!(matches!(find_claim(&db, &paid, rejected, 3).await, Err(3)));
        }

        /// Two claims of one output can be in flight at once: a wallet that restarts while a
        /// claim is pending can claim the output again, and the federation accepts only one of
        /// the two. A record written for the later claim still finds the earlier one.
        #[tokio::test(flavor = "multi_thread")]
        async fn a_search_finds_a_claim_announced_before_the_one_the_record_follows() {
            let db = federation_namespace(&in_memory_root(), [1u8; 32]);
            let federation = FederationInner::detached(db.clone(), true);
            let paid = bitcoin::OutPoint {
                txid: txid(3),
                vout: 1,
            };
            let (earlier, later) = (operation(1), operation(2));
            a_walletv2_claim(&db, earlier, paid, 0).await;
            a_walletv2_claim(&db, later, paid, 1).await;
            // Looked up before anything read the announcements, the later claim gets the
            // deposit's record, and the earlier one then joins it.
            assert!(federation.operation(later).await.expect("lookup").is_some());
            crate::onchain::pick_up_deposits(&federation)
                .await
                .expect("pick up");
            assert!(read_wire(&db, earlier).await.expect("reads").is_none());
            let followed = wire_link(&stored(&db, later).await).expect("follows a claim");
            assert_eq!(followed.upstream, later);

            let paid = read_paid(&db, later).await.expect("the output");
            let found = find_claim(&db, &paid, later, followed.cursor)
                .await
                .expect("the earlier claim");
            assert_eq!(found.upstream, earlier);
        }

        /// What a claim credited is written onto the record as it is stored at that moment, so
        /// it keeps what the record learnt in the meantime.
        #[tokio::test(flavor = "multi_thread")]
        async fn a_change_to_a_record_keeps_what_was_stored_in_the_meantime() {
            let (db, federation, id) = a_recorded_deposit().await;
            let before = stored(&db, id).await;
            let later = Link {
                upstream: operation(2),
                txid: txid(3).to_string().parse().expect("a txid"),
                gross: Sats::from_sats(100_000),
                cursor: 5,
            };
            link(&federation, id, &later).await.expect("link");

            let after = update_wire(&db, id, |details| {
                details.net_credit_msats = Some(99_998_500);
            })
            .await
            .expect("update")
            .expect("the deposit has its record");

            assert_eq!(after, stored(&db, id).await);
            assert_eq!(after.net_credit_msats, Some(99_998_500));
            assert_eq!(
                after.upstream_operation_id,
                Some(operation(2).fmt_full().to_string())
            );
            assert_eq!(after.event_cursor, Some(5));
            assert_eq!(after.txid, before.txid);
            // A record that is not there is not written.
            let missing = update_wire(&db, operation(9), |details| {
                details.event_cursor = Some(1);
            })
            .await
            .expect("update");
            assert_eq!(missing, None);
        }

        fn a_claim_of(outpoint: bitcoin::OutPoint) -> Paid {
            paid_by(&receive_meta(
                Some(upstream_address()),
                100_000,
                500,
                Some(outpoint),
            ))
            .expect("a claim of an output")
        }

        #[test]
        fn a_record_names_the_output_it_was_written_for_and_no_other() {
            let outpoint = bitcoin::OutPoint {
                txid: a_bitcoin_txid(),
                vout: 3,
            };
            let details = details_of_a_claim(outpoint);

            assert!(names(&details, &a_claim_of(outpoint)));
            // Another output of the same transaction is another deposit, even to the same
            // address.
            let sibling = bitcoin::OutPoint {
                txid: outpoint.txid,
                vout: 4,
            };
            assert!(!names(&details, &a_claim_of(sibling)));
            let elsewhere = bitcoin::OutPoint {
                txid: bitcoin::Txid::from_byte_array([7; 32]),
                vout: 3,
            };
            assert!(!names(&details, &a_claim_of(elsewhere)));
            assert!(!names("not a record", &a_claim_of(outpoint)));

            // A record that names no output is of none.
            let recorded = wire::decode_receive_wire(&details).expect("decode");
            let without_an_index = wire::OnchainReceiveDetailsWire {
                vout: None,
                ..recorded.clone()
            };
            let without_a_transaction = wire::OnchainReceiveDetailsWire {
                txid: "not a transaction id".to_owned(),
                ..recorded
            };
            for unnamed in [without_an_index, without_a_transaction] {
                let unnamed = wire::encode_receive_wire(&unnamed).expect("encode");
                assert!(!names(&unnamed, &a_claim_of(outpoint)));
            }
        }
    }
}
