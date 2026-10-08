//! The v1 wallet module (`wallet`): the withdrawal plan and send, the deposit address, the
//! record a deposit gets when the module announces it and its subscription, and the backfill of
//! this module's own operation log.

use std::sync::{Arc, Weak};

use fedimint_client::Client;
use fedimint_client_module::ClientModuleInstance;
use fedimint_client_module::TransactionSubmitError;
use fedimint_client_module::transaction::FeeQuote;
use fedimint_core::core::OperationId;
use fedimint_core::db::{Database, IDatabaseTransactionOpsCoreTyped};
use fedimint_core::util::{BoxStream, FmtCompact};
use fedimint_eventlog::{Event as _, PersistedLogEntry};
use fedimint_wallet_client::events::ReceivePaymentEvent;
use fedimint_wallet_client::{
    DepositAddressError, DepositStateV2, WalletClientModule, WalletOperationMeta,
    WalletOperationMetaVariant, WithdrawState,
};
use fedimint_wallet_common::PegOutFees;
use futures::StreamExt;

use super::driver::{SendStep, through_settle};
use super::{
    OnchainQuoteInner, Plan, Terms, add, balance_of, bitcoin_to_sats, check_amount,
    check_covers_amount, claim_figures, fee_quote_failure, fee_quote_refusal, from_upstream,
    insufficient, internal, now, plan_of, quote_changed, sats_to_amount, sats_to_bitcoin, short_of,
    subscribe_error, timeout, unreachable, wire,
};
use crate::federation::{FederationInner, write_record_in};
use crate::operation::{
    Backfilled, Driver, READABLE_STATE_SCHEMA, custom_meta, from_custom_meta, kinds,
    record_phase_in, until_final, write_details_in,
};
use crate::sdk::{CONTACT_TIMEOUT, SdkInner};
use crate::{
    Address, Amount, Error, ErrorCode, OnchainReceiveState, OnchainSendDetails, OnchainSendState,
    Operation, Result, Sats, Txid,
};

/// The v1 module on a live client, or `NotSupported` when the federation dropped it.
pub(super) fn module_of(client: &Client) -> Result<ClientModuleInstance<'_, WalletClientModule>> {
    client
        .get_first_module::<WalletClientModule>()
        .map_err(|_| {
            Error::new(
                ErrorCode::NotSupported,
                "this federation no longer has a v1 wallet module",
            )
        })
}

/// Plans a v1 withdrawal: prices the destination output, then the transaction that funds it.
pub(super) async fn plan(
    client: &Client,
    module: &WalletClientModule,
    address: &Address,
    amount: Sats,
    available: Amount,
) -> Result<Plan> {
    // The network was already checked by the caller (`Onchain::quote`) against this exact
    // federation, so a failure here is a bug in that ordering, not a bad address.
    let checked = address
        .inner()
        .clone()
        .require_network(module.get_network())
        .map_err(|err| internal(format!("the address's network was already checked: {err}")))?;
    check_amount(amount, checked.script_pubkey().minimal_non_dust())?;
    check_covers_amount(amount, available)?;
    let fees = fedimint_core::runtime::timeout(
        CONTACT_TIMEOUT,
        module.get_withdraw_fees(&checked, sats_to_bitcoin(amount)),
    )
    .await
    .map_err(|_| timeout())?
    .map_err(|err| {
        unreachable(format!(
            "could not quote the withdrawal's on-chain fee: {}",
            err.fmt_compact()
        ))
    })?;
    let output_value = sats_to_bitcoin(amount)
        .checked_add(fees.amount())
        .ok_or_else(|| internal("the withdrawal amount plus its on-chain fee overflowed"))?;
    // A Bitcoin amount is consensus-bounded to 21 million BTC, well inside a millisatoshi `u64`,
    // so this never overflows: the same conversion upstream's own `From<bitcoin::Amount> for
    // fedimint_core::Amount` uses.
    let chain_fee = from_upstream(fedimint_core::Amount::from(fees.amount()));
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
    let module_fee = from_upstream(module.get_fee_consensus().peg_out_abs);
    plan_of(
        chain_fee,
        module_fee,
        &quote,
        amount,
        Terms::V1 {
            fees,
            quote: quote.clone(),
        },
    )
}

/// Executes a v1 withdrawal: re-derives the terms and refuses on drift, then funds and records.
pub(super) async fn send(
    federation: &Arc<FederationInner>,
    client: &Client,
    module: &WalletClientModule,
    quote: &OnchainQuoteInner,
    fees: PegOutFees,
    quoted: &FeeQuote,
    driver: Arc<dyn Driver<OnchainSendState>>,
) -> Result<Operation<OnchainSendState>> {
    let checked = quote
        .address
        .inner()
        .clone()
        .require_network(module.get_network())
        .map_err(|err| {
            internal(format!(
                "the withdrawal address no longer checks out: {err}"
            ))
        })?;
    let fresh_fees = fedimint_core::runtime::timeout(
        CONTACT_TIMEOUT,
        module.get_withdraw_fees(&checked, sats_to_bitcoin(quote.amount)),
    )
    .await
    .map_err(|_| timeout())?
    .map_err(|err| {
        unreachable(format!(
            "could not re-quote the withdrawal's on-chain fee: {}",
            err.fmt_compact()
        ))
    })?;
    let output_value = sats_to_bitcoin(quote.amount)
        .checked_add(fresh_fees.amount())
        .ok_or_else(|| internal("the withdrawal amount plus its on-chain fee overflowed"))?;
    let fresh_chain_fee = from_upstream(fedimint_core::Amount::from(fresh_fees.amount()));
    let fresh_quote = match module.send_fee_quote(output_value).await {
        Ok(fee_quote) => fee_quote,
        Err(err) => {
            // `required` is the amount plus the on-chain fee already re-quoted above; the dry
            // run that would have priced the funding side is exactly what failed.
            let required = add(sats_to_amount(quote.amount)?, fresh_chain_fee)?;
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
    // Both `get_withdraw_fees` and `send_fee_quote` must return exactly what they returned when
    // this quote was built; the federation rebuilds the peg-out from `fees` at submission time
    // and rejects a `total_weight` it disagrees with, so a mismatch caught here first is a
    // `QuoteChanged` refusal instead of a submission failure.
    if fees != fresh_fees || quoted != &fresh_quote {
        let module_fee = from_upstream(module.get_fee_consensus().peg_out_abs);
        let current = plan_of(
            fresh_chain_fee,
            module_fee,
            &fresh_quote,
            quote.amount,
            Terms::V1 {
                fees: fresh_fees,
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
    let submitted = module
        .withdraw(
            &checked,
            sats_to_bitcoin(quote.amount),
            fees,
            custom_meta(&wire)?,
        )
        .await;
    let id = match submitted {
        Ok(id) => id,
        Err(err) => {
            // The balance covered the quoted total when it was checked above, so a funding
            // shortfall here means it moved since: it is read again for that one refusal, and
            // a read that fails leaves the refusal without figures rather than hiding it.
            let available = if err.is_insufficient_funds() {
                balance_of(client, federation.status()).await.ok()
            } else {
                None
            };
            return Err(map_withdraw_error(&err, quote.plan.total, available));
        }
    };
    federation
        .create_operation(id, kinds::ONCHAIN_SEND, "wallet", &wire, driver)
        .await
}

// `withdraw` fails inside `finalize_and_submit_transaction`, which reports a balance too low to
// fund the transaction as `TransactionSubmitError::InsufficientFunds`. The amounts that error
// carries are the mint's remainder, not the withdrawal's (see `fee_quote_refusal`), so the
// figures are the quoted `total` and the balance read after the refusal, when it could be read.
// Any other failure is reported with its whole chain.
fn map_withdraw_error(
    err: &TransactionSubmitError,
    total: Amount,
    available: Option<Amount>,
) -> Error {
    if !err.is_insufficient_funds() {
        return internal(format!(
            "the withdrawal could not be submitted: {}",
            err.fmt_compact()
        ));
    }
    short_of(total, available)
}

// `safe_allocate_deposit_address` refuses with `SafeDepositUnverified` when the client has never
// been online to confirm that the federation's wallet module handles every deposit safely; the
// federation is then not usable for deposits yet, which is what `NotSupported` says. Every other
// variant is a failure of the client's own database, bitcoin backend or operation log.
fn map_deposit_address_error(err: &DepositAddressError) -> Error {
    match err {
        DepositAddressError::SafeDepositUnverified => {
            Error::new(ErrorCode::NotSupported, err.fmt_compact().to_string())
        }
        _ => internal(format!(
            "could not allocate a deposit address: {}",
            err.fmt_compact()
        )),
    }
}

// Upstream v1 `WithdrawState` onto this SDK's own send lifecycle. `Failed` is only ever the
// funding transaction being rejected before anything left the balance
// (`modules/fedimint-wallet-client/src/withdraw.rs`), so it is not an ending here: it is the
// rejection that sends the withdrawal to the settle gate, which chooses `Refunded` or `Failed`
// from what the recovery of the notes it removed establishes. No upstream state maps straight
// onto either of those two.
//
// | upstream           | here                       |
// | ------------------ | -------------------------- |
// | `Created`          | `Created`                   |
// | `Succeeded(txid)`  | `Succeeded { txid }`        |
// | `Failed(reason)`   | `FundingRejected { reason }` |
pub(super) fn map_withdraw(state: &WithdrawState) -> SendStep {
    match state {
        WithdrawState::Created => SendStep::State(OnchainSendState::Created),
        WithdrawState::Succeeded(txid) => SendStep::State(OnchainSendState::Succeeded {
            txid: Txid::from_upstream(*txid),
        }),
        WithdrawState::Failed(reason) => SendStep::FundingRejected {
            reason: reason.clone(),
        },
    }
}

/// A fresh stream over a v1 withdrawal, holding the client only for the bounded upstream read
/// that produces the stream; see `lightning::v1::subscribe_send` for the same shape.
pub(super) async fn subscribe_withdraw(
    federation: &FederationInner,
    id: OperationId,
) -> Result<BoxStream<'static, Result<OnchainSendState>>> {
    let client = federation.client(false).await?;
    let module = module_of(&client)?;
    let upstream = module
        .subscribe_withdraw_updates(id)
        .await
        .map_err(|err| subscribe_error(err.fmt_compact()))?
        .into_stream();
    // The stream is `'static` and outlives this call, so it carries the way back to the
    // federation rather than the federation itself; see `through_settle`.
    let stream = through_settle(
        upstream.map(|state| Ok(map_withdraw(&state))),
        federation.sdk.clone(),
        federation.id,
        id,
    );
    Ok(until_final(stream))
}

/// A newly allocated v1 deposit address.
///
/// The module logs the allocation as an operation of its own
/// (`modules/fedimint-wallet-client/src/lib.rs:1190-1249`). That operation is an address, not a
/// deposit, and gets no record here: [`adopt`] writes one when the module announces a payment
/// to the address.
pub(super) async fn receive(module: &WalletClientModule) -> Result<Address> {
    let info = module
        .safe_allocate_deposit_address(serde_json::Value::Null)
        .await
        .map_err(|err| map_deposit_address_error(&err))?;
    Ok(Address::from_upstream(info.address.into_unchecked()))
}

/// The v1 module's announcement that it found a payment to one of its addresses and started
/// claiming it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Found {
    /// The operation the paid address was allocated under, which is also the one the module
    /// claims every payment to that address under.
    pub(super) operation: OperationId,
    pub(super) txid: Txid,
    pub(super) gross: Sats,
    /// When the module logged it, in milliseconds since the Unix epoch.
    pub(super) found_at: u64,
}

/// The payment a deposit record stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Payment {
    pub(super) txid: Txid,
    pub(super) gross: Sats,
}

/// Reads the v1 module's announcement out of an event log entry, or `None` for any other entry.
pub(super) fn deposit_found(entry: &PersistedLogEntry) -> Option<Found> {
    if entry.module_kind() != Some(&fedimint_wallet_common::KIND)
        || entry.kind != ReceivePaymentEvent::KIND
    {
        return None;
    }
    let event = entry.to_event::<ReceivePaymentEvent>()?;
    Some(Found {
        operation: event.operation_id,
        txid: Txid::from_upstream(event.txid),
        // The module logs the funding output's value converted to millisatoshis
        // (`modules/fedimint-wallet-client/src/pegin_monitor.rs:478`), so the division is exact.
        gross: Sats::from_sats(event.amount.msats / 1000),
        found_at: entry.ts_usecs / 1000,
    })
}

/// Whether a v1 log entry is a deposit address the module handed out.
pub(super) fn is_address_allocation(meta: &serde_json::Value) -> bool {
    serde_json::from_value::<WalletOperationMeta>(meta.clone())
        .is_ok_and(|meta| matches!(meta.variant, WalletOperationMetaVariant::Deposit { .. }))
}

/// Gives the deposit `found` announces its record, and returns the operation that stands for
/// it: `Some` exactly when that operation's record is of this very payment.
///
/// The module claims every payment to an address under the operation the address was allocated
/// with, so a second payment to one address is announced under an operation that already has a
/// record. That record goes on describing the first payment and the second is answered with
/// `None`: the module has one operation per address, not per payment (fedimint/fedimint#8123).
///
/// A record under the operation that does not read as a deposit's is left as it is and answered
/// with `None` too, so one unreadable record cannot hold up every deposit announced after it.
///
/// # Errors
///
/// [`Storage`](ErrorCode::Storage).
pub(super) async fn adopt(
    federation: &FederationInner,
    found: &Found,
) -> Result<Option<OperationId>> {
    let db = federation.db();
    let id = found.operation;
    let stored = db
        .begin_transaction_nc()
        .await
        .get_value(&crate::db::OperationRecordKey(id))
        .await;
    let record = match stored {
        Some(record) if record.kind == kinds::ONCHAIN_RECEIVE => record,
        // No record yet, or one that only says reconciliation could not place the entry.
        _ => {
            // An address the module derived again while recovering a wallet has no log entry
            // (`modules/fedimint-wallet-client/src/lib.rs:246-266`), and the module has no call
            // that names the address of an operation, so a payment to one is claimed into the
            // balance without a record to show for it.
            let Some(address) = allocated_address(&db, id).await else {
                return Ok(None);
            };
            let details = wire::OnchainReceiveDetailsWire {
                address,
                txid: Some(found.txid.to_string()),
                gross_deposited_sats: Some(found.gross.sats()),
                fee_msats: None,
                fee_breakdown: None,
                net_credit_msats: None,
                created_at: found.found_at,
                upstream_operation_id: None,
                event_cursor: None,
                vout: None,
            };
            let record = crate::db::OperationRecord {
                schema_version: READABLE_STATE_SCHEMA,
                kind: kinds::ONCHAIN_RECEIVE.to_owned(),
                module: "wallet".to_owned(),
                created_at: found.found_at,
                details: wire::encode_receive_wire(&details)?,
                phase: Some(wire::PHASE_SEEN),
                cancel_requested_at: None,
                final_state: None,
            };
            write_record_in(&db, id, record, true).await?
        }
    };
    if record.kind != kinds::ONCHAIN_RECEIVE {
        return Ok(None);
    }
    let details = match wire::decode_receive_wire(&record.details) {
        Ok(details) => details,
        Err(err) => {
            tracing::warn!(
                target: "fedimint_sdk",
                federation = %federation.id,
                operation = %id.fmt_full(),
                error = %err,
                "a deposit was announced for an operation whose record cannot be read",
            );
            return Ok(None);
        }
    };
    match details.txid {
        Some(txid) if txid != found.txid.to_string() => Ok(None),
        // The record of this very payment. One can name its payment and still lack its phase: a
        // version of this crate that recorded an address when it was handed out wrote the two
        // separately, the payment first, and could be interrupted in between.
        Some(_) => {
            if record.phase.is_none() {
                record_phase_in(&db, id, wire::PHASE_SEEN).await?;
            }
            Ok(Some(id))
        }
        // A record written by a version of this crate that recorded an address when it was
        // handed out: it names no payment until this one is filled in.
        None => {
            fill_seen(&db, id, found).await?;
            Ok(Some(id))
        }
    }
}

/// The address the v1 operation `id` was allocated for, or `None` if `id` is not an address
/// allocation.
async fn allocated_address(db: &Database, id: OperationId) -> Option<String> {
    let entry = fedimint_client::oplog::OperationLog::new(db.clone())
        .get_operation(id)
        .await?;
    if entry.operation_module_kind() != "wallet" {
        return None;
    }
    let meta: WalletOperationMeta = entry.try_meta().ok()?;
    match meta.variant {
        WalletOperationMetaVariant::Deposit { address, .. } => {
            Some(address.assume_checked_ref().to_string())
        }
        WalletOperationMetaVariant::Withdraw { .. }
        | WalletOperationMetaVariant::RbfWithdraw { .. } => None,
    }
}

/// How far the claim of a deposit has got, as the module reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Progress {
    /// The claim was made and what it mints is not spendable yet.
    Claiming,
    /// What the claim minted is spendable.
    Claimed,
    /// What the claim minted could not be issued.
    Failed(String),
}

// Upstream v1 `DepositStateV2` onto the progress of the record's own payment:
//
// | upstream                 | here             | reported as                      |
// | ------------------------ | ---------------- | -------------------------------- |
// | `WaitingForTransaction`  | `Claiming`       | `Confirmed`                      |
// | `WaitingForConfirmation` | `Claiming`       | `Confirmed`                      |
// | `Confirmed`              | `Claiming`       | `Confirmed`                      |
// | `Claimed`                | `Claimed`        | `Claimed`, with the net credit   |
// | `Failed(reason)`         | `Failed(reason)` | `Failed { reason }`              |
//
// A record exists only for a payment the module has already started claiming (see `adopt`), and
// `subscribe_deposit` replays its states from the first one on every call
// (`modules/fedimint-wallet-client/src/lib.rs:1567-1620`). The two it yields before `Confirmed`
// are therefore that replay catching up, the second of them behind a round trip to the bitcoin
// backend, and not where the deposit is: the first state is there at once, and none is reported
// that is behind the claim the record stands for.
//
// The one record that can be ahead of its claim was written by a version of this crate that
// recorded a deposit when it first saw the transaction. Until that transaction has the
// federation's confirmations it reads as `Confirmed` too: no earlier state is left to report.
//
// Only the progress is taken from upstream. The transaction and the amount every state reports
// are the record's own, the ones the module announced, because upstream's stream follows the
// first payment in the address's history
// (`modules/fedimint-wallet-client/src/lib.rs:1575-1596`), and for an address paid more than once
// that need not be the payment the record stands for. Its progress is then another payment's
// too: the module has one operation per address and no way to follow one payment to it
// (fedimint/fedimint#8123), which is why an address is for one payment.
pub(super) fn progress_of(state: &DepositStateV2) -> Progress {
    match state {
        DepositStateV2::WaitingForTransaction
        | DepositStateV2::WaitingForConfirmation { .. }
        | DepositStateV2::Confirmed { .. } => Progress::Claiming,
        DepositStateV2::Claimed { .. } => Progress::Claimed,
        DepositStateV2::Failed(reason) => Progress::Failed(reason.clone()),
    }
}

/// A fresh stream over a v1 deposit: follows the module's progress and, on the claim, reads back
/// what the claim transaction itself minted (or a figure a previous subscription already
/// computed) before yielding `Claimed`.
///
/// The stream cannot hold the client guard the way `subscribe_withdraw` does, because the claim
/// may arrive long after this call returns: it captures a weak handle to the SDK and re-derives
/// a live client only when the claim actually happens, the same way
/// `lightning::v1::subscribe_receive`'s reclaim retry re-derives the federation it needs from a
/// weak handle rather than holding one.
pub(super) async fn subscribe_deposit(
    federation: &FederationInner,
    id: OperationId,
) -> Result<BoxStream<'static, Result<OnchainReceiveState>>> {
    let db = federation.db();
    let payment = payment_of(&db, id).await?;
    let client = federation.client(false).await?;
    let module = module_of(&client)?;
    let peg_in_abs = from_upstream(module.get_fee_consensus().peg_in_abs);
    let upstream = module
        .subscribe_deposit(id)
        .await
        .map_err(|err| subscribe_error(err.fmt_compact()))?
        .into_stream();
    drop(client);

    let sdk = federation.sdk.clone();
    let federation_id = federation.id;
    let stream = upstream.then(move |state| {
        let sdk = sdk.clone();
        let db = db.clone();
        let Payment { txid, gross } = payment.clone();
        async move {
            match progress_of(&state) {
                Progress::Claiming => Ok(OnchainReceiveState::Confirmed {
                    txid,
                    gross_deposited: gross,
                }),
                Progress::Claimed => {
                    let net_credit =
                        claim_net_credit(&db, id, sdk, federation_id, peg_in_abs, gross).await?;
                    Ok(OnchainReceiveState::Claimed {
                        txid,
                        gross_deposited: gross,
                        net_credit,
                    })
                }
                Progress::Failed(reason) => Ok(OnchainReceiveState::Failed { reason }),
            }
        }
    });
    Ok(until_final(stream))
}

/// The payment the record of `id` stands for.
///
/// # Errors
///
/// [`Internal`](ErrorCode::Internal) for a record that names no payment. Only a version of this
/// crate that recorded an address when it was handed out wrote one, and it is a deposit address
/// nobody has paid, which has no state to report.
async fn payment_of(db: &Database, id: OperationId) -> Result<Payment> {
    let Some(record) = db
        .begin_transaction_nc()
        .await
        .get_value(&crate::db::OperationRecordKey(id))
        .await
    else {
        return Err(internal(format!(
            "no record for operation {}",
            id.fmt_full()
        )));
    };
    let details = wire::decode_receive_wire(&record.details)?;
    let (Some(txid), Some(gross)) = (details.txid, details.gross_deposited_sats) else {
        return Err(internal(format!(
            "operation {} is a deposit address that has not been paid",
            id.fmt_full()
        )));
    };
    Ok(Payment {
        txid: txid
            .parse()
            .map_err(|_| internal("a stored transaction id does not parse"))?,
        gross: Sats::from_sats(gross),
    })
}

/// Turns a record that names no payment into the record of the deposit `found` announces: the
/// payment is filled in, the record is dated from when the deposit was found, which moves it in
/// the chronological index too, and it is marked as a deposit.
///
/// Only a record written by a version of this crate that recorded an address when it was handed
/// out needs it. It is one write, so the record is an unpaid address until it is the deposit in
/// full. A record that has gone, or that names a payment by now, is left as it is.
async fn fill_seen(db: &Database, id: OperationId, found: &Found) -> Result<()> {
    db.autocommit(
        |dbtx, _| {
            let found = found.clone();
            Box::pin(async move {
                let key = crate::db::OperationRecordKey(id);
                let Some(mut record) = dbtx.get_value(&key).await else {
                    return Ok(Ok(()));
                };
                let mut details = match wire::decode_receive_wire(&record.details) {
                    Ok(details) => details,
                    Err(err) => return Ok(Err(err)),
                };
                if details.txid.is_some() {
                    return Ok(Ok(()));
                }
                details.txid = Some(found.txid.to_string());
                details.gross_deposited_sats = Some(found.gross.sats());
                details.created_at = found.found_at;
                record.details = match wire::encode_receive_wire(&details) {
                    Ok(details) => details,
                    Err(err) => return Ok(Err(err)),
                };
                // The index entry repeats the record's own time, so it moves with it.
                dbtx.remove_entry(&crate::db::OperationIndexKey {
                    created_at: record.created_at,
                    id,
                })
                .await;
                record.created_at = found.found_at;
                record.phase = Some(wire::PHASE_SEEN);
                dbtx.insert_entry(&key, &record).await;
                dbtx.insert_entry(
                    &crate::db::OperationIndexKey {
                        created_at: record.created_at,
                        id,
                    },
                    &(),
                )
                .await;
                Ok::<_, core::convert::Infallible>(Ok(()))
            })
        },
        Some(100),
    )
    .await
    .map_err(crate::db::storage_error)?
}

/// The net credit for a claimed deposit: read back if an earlier subscription already computed
/// it, since a details record's fee fields fill in at most once, or read the claim transaction's
/// own mint outputs and persist the result.
///
/// The read runs on a client this function re-derives at the moment it is needed rather than one
/// held since the subscription started, and the wait itself is handed to `wait_holding_client`
/// rather than simply awaited here: that function's own documentation is why a stream may not
/// keep a client handle in its own frame across a wait an idle subscriber could park on
/// indefinitely.
async fn claim_net_credit(
    db: &Database,
    id: OperationId,
    sdk: Weak<SdkInner>,
    federation_id: fedimint_core::config::FederationId,
    peg_in_abs: Amount,
    gross: Sats,
) -> Result<Amount> {
    let Some(record) = db
        .begin_transaction_nc()
        .await
        .get_value(&crate::db::OperationRecordKey(id))
        .await
    else {
        return Err(internal(format!(
            "no record for operation {}",
            id.fmt_full()
        )));
    };
    let mut details = wire::decode_receive_wire(&record.details)?;
    if let Some(net_credit_msats) = details.net_credit_msats {
        return Ok(Amount::from_msats(net_credit_msats));
    }

    let closed = || {
        Error::new(
            ErrorCode::FederationClosed,
            "this federation stopped running",
        )
    };
    let sdk = sdk.upgrade().ok_or_else(closed)?;
    let federation = sdk.federation_inner(&federation_id).ok_or_else(closed)?;
    let client = federation.client(false).await?;
    let handle = client.handle();
    drop(client);
    let stop = handle.task_group().make_handle().make_shutdown_rx();
    let (fee, breakdown, net_credit) =
        crate::federation::wait_holding_client(handle, stop, move |client| async move {
            let wallet_instance = module_of(&client)?.id;
            claim_figures(
                &client,
                id,
                wallet_instance,
                sats_to_amount(gross)?,
                peg_in_abs,
                Amount::from_msats(0),
                gross,
            )
            .await
        })
        .await?;

    details.fee_msats = Some(fee.msats());
    details.fee_breakdown = Some(wire::ReceiveFeeBreakdownWire::from(&breakdown));
    details.net_credit_msats = Some(net_credit.msats());
    write_details_in(db, id, wire::encode_receive_wire(&details)?).await?;

    Ok(net_credit)
}

/// Rebuilds a record from a v1 log entry. A withdrawal is exact when this SDK made it, whose
/// metadata carries the quoted terms verbatim, and an estimate otherwise, with no mint-side
/// funding cost, since that is unknowable from the operation log alone. A deposit entry rebuilds
/// nothing: it is the address the module handed out, and a payment to it gets its record from
/// the module's own announcement (see [`adopt`]).
pub(super) fn backfill(
    id: OperationId,
    meta: &serde_json::Value,
    created_at: u64,
) -> Option<Backfilled> {
    let meta: WalletOperationMeta = serde_json::from_value(meta.clone()).ok()?;
    match meta.variant {
        // Reconciliation never offers one of these (see
        // `crate::onchain::log_entry_is_not_an_operation`); answering `None` keeps this function
        // honest about it all the same.
        WalletOperationMetaVariant::Deposit { .. } => None,
        WalletOperationMetaVariant::Withdraw {
            address,
            amount,
            fee,
            ..
        } => {
            let wire = match from_custom_meta::<wire::OnchainSendDetailsWire>(&meta.extra_meta) {
                Some(wire) => wire,
                None => {
                    // The mint-side cost of funding a withdrawal made by another install, or an
                    // older build of this one that predates this SDK's own copy, is unknowable
                    // from the operation log alone: only the on-chain component (`fee`) and the
                    // amount survive, so the total below is an estimate, not the true debit.
                    let fee_amount = from_upstream(fedimint_core::Amount::from(fee.amount()));
                    let total =
                        add(sats_to_amount(bitcoin_to_sats(amount)).ok()?, fee_amount).ok()?;
                    wire::OnchainSendDetailsWire {
                        address: address.assume_checked_ref().to_string(),
                        amount_sats: amount.to_sat(),
                        fee_msats: fee_amount.msats(),
                        total_msats: total.msats(),
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
        // Neither an operation this SDK creates: RBF is not offered by this facade at all.
        WalletOperationMetaVariant::RbfWithdraw { .. } => None,
    }
}

// At file scope rather than inside `mod tests`, because a `mod tests` is private to its own
// file and the tests of the pass that reads these (`src/onchain/deposits.rs`) need them too.
#[cfg(test)]
pub(super) mod fixtures {
    use fedimint_core::bitcoin;
    use fedimint_core::core::OperationId;
    use fedimint_eventlog::{Event as _, EventLogEntry, EventLogModule};
    use fedimint_wallet_client::client_db::TweakIdx;
    use fedimint_wallet_client::events::ReceivePaymentEvent;
    use fedimint_wallet_client::{WalletOperationMeta, WalletOperationMetaVariant};

    /// The log entry meta the module writes for the operation it allocates `address` under,
    /// when asked for an address the way this crate asks.
    pub(crate) fn allocation(address: &str) -> serde_json::Value {
        serde_json::to_value(WalletOperationMeta {
            variant: WalletOperationMetaVariant::Deposit {
                address: address.parse().expect("a valid address"),
                tweak_idx: Some(TweakIdx(0)),
                expires_at: None,
            },
            extra_meta: serde_json::Value::Null,
        })
        .expect("serialises")
    }

    /// The event the module logs when it starts claiming a payment of `sats`, made by `txid`,
    /// to the address allocated under `operation`.
    pub(crate) fn announcement(
        operation: OperationId,
        txid: bitcoin::Txid,
        sats: u64,
        ts_usecs: u64,
    ) -> EventLogEntry {
        let event = ReceivePaymentEvent {
            operation_id: operation,
            amount: fedimint_core::Amount::from_sats(sats),
            txid,
        };
        EventLogEntry {
            kind: ReceivePaymentEvent::KIND,
            module: Some(EventLogModule {
                kind: fedimint_wallet_common::KIND,
                id: 2,
            }),
            ts_usecs,
            payload: serde_json::to_vec(&event).expect("serialises"),
        }
    }
}

#[cfg(test)]
mod tests {
    use fedimint_client_module::error::{
        ClientModuleError, InsufficientBalanceError, OperationAlreadyExistsError,
    };
    use fedimint_core::bitcoin;

    use super::*;
    use crate::Timestamp;

    #[test]
    fn a_withdrawal_shortfall_reports_the_quoted_total_and_the_balance() {
        // What the mint reports after setting notes aside to consolidate: what was left to
        // fund and what the remaining notes covered, neither of them the withdrawal or the
        // balance.
        let short = TransactionSubmitError::InsufficientFunds(InsufficientBalanceError {
            requested_amount: fedimint_core::Amount::from_msats(524_640),
            total_amount: fedimint_core::Amount::from_msats(524_288),
        });
        let total = Amount::from_msats(1_180_000);
        let balance = Amount::from_msats(1_179_648);

        let err = map_withdraw_error(&short, total, Some(balance));
        assert_eq!(err.code, ErrorCode::InsufficientBalance);
        match err.detail() {
            Some(crate::ErrorDetails::InsufficientBalance {
                required,
                available,
            }) => {
                assert_eq!(*required, total);
                assert_eq!(*available, balance);
            }
            other => panic!("expected InsufficientBalance, got {other:?}"),
        }

        // A balance that could not be read leaves the refusal without figures.
        let err = map_withdraw_error(&short, total, None);
        assert_eq!(err.code, ErrorCode::InsufficientBalance);
        assert!(err.detail().is_none(), "{:?}", err.detail());
    }

    #[test]
    fn any_other_withdrawal_failure_keeps_its_whole_cause_chain() {
        let unrelated =
            TransactionSubmitError::PrimaryModule(ClientModuleError::other("the notes are locked"));
        let err = map_withdraw_error(&unrelated, Amount::from_msats(10), None);
        assert_eq!(err.code, ErrorCode::Internal);
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

    #[test]
    fn an_unverified_federation_cannot_hand_out_a_deposit_address() {
        let err = map_deposit_address_error(&DepositAddressError::SafeDepositUnverified);
        assert_eq!(err.code, ErrorCode::NotSupported);
        let other = map_deposit_address_error(&DepositAddressError::OperationAlreadyExists(
            OperationAlreadyExistsError {
                operation_id: OperationId::new_random(),
            },
        ));
        assert_eq!(other.code, ErrorCode::Internal);
    }

    fn a_bitcoin_txid() -> bitcoin::Txid {
        "0000000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .expect("a well-formed transaction id")
    }

    fn an_address() -> String {
        "bcrt1q2nfxmhd4n3c8834pj72xagvyr9gl57n5r94fsl".to_owned()
    }

    fn an_operation_id() -> OperationId {
        OperationId::new_random()
    }

    #[test]
    fn withdraw_states_fold_onto_the_send_lifecycle() {
        let txid = a_bitcoin_txid();
        let cases = [
            (
                WithdrawState::Created,
                SendStep::State(OnchainSendState::Created),
            ),
            (
                WithdrawState::Succeeded(txid),
                SendStep::State(OnchainSendState::Succeeded {
                    txid: Txid::from_upstream(txid),
                }),
            ),
            // Not `Refunded`: a rejected funding is a step towards an ending, not one. Which
            // ending it becomes is the settle gate's to say.
            (
                WithdrawState::Failed("the funding transaction was rejected".to_owned()),
                SendStep::FundingRejected {
                    reason: "the funding transaction was rejected".to_owned(),
                },
            ),
        ];
        for (upstream, expected) in cases {
            assert_eq!(map_withdraw(&upstream), expected, "{upstream:?}");
        }
    }

    #[test]
    fn deposit_states_fold_onto_the_progress_of_the_claim() {
        let out_point = bitcoin::OutPoint {
            txid: a_bitcoin_txid(),
            vout: 0,
        };
        let paid = bitcoin::Amount::from_sat(50_000);

        // Everything up to and including upstream's own `Confirmed` is a claim in progress: a
        // record exists only for a payment that is already being claimed.
        for claiming in [
            DepositStateV2::WaitingForTransaction,
            DepositStateV2::WaitingForConfirmation {
                btc_deposited: paid,
                btc_out_point: out_point,
            },
            DepositStateV2::Confirmed {
                btc_deposited: paid,
                btc_out_point: out_point,
            },
        ] {
            assert_eq!(progress_of(&claiming), Progress::Claiming, "{claiming:?}");
        }
        assert_eq!(
            progress_of(&DepositStateV2::Claimed {
                btc_deposited: paid,
                btc_out_point: out_point,
            }),
            Progress::Claimed
        );
        assert_eq!(
            progress_of(&DepositStateV2::Failed("boom".to_owned())),
            Progress::Failed("boom".to_owned())
        );
    }

    /// The entry the module writes when it hands an address out is not a deposit: it backfills
    /// nothing, and it is recognised as the allocation reconciliation leaves alone.
    #[test]
    fn a_deposit_address_log_entry_backfills_nothing() {
        let meta = serde_json::json!({
            "variant": {
                "deposit": {
                    "address": an_address(),
                },
            },
            "extra_meta": {},
        });
        assert!(backfill(an_operation_id(), &meta, 1_700_000_000_000).is_none());
        assert!(is_address_allocation(&meta));
    }

    #[test]
    fn a_withdraw_log_entry_is_not_an_address_allocation() {
        let meta = serde_json::json!({
            "variant": {
                "withdraw": {
                    "address": an_address(),
                    "amount": 25_000,
                    "fee": { "fee_rate": { "sats_per_kvb": 10_000 }, "total_weight": 4_000 },
                    "change": [],
                },
            },
            "extra_meta": {},
        });
        assert!(!is_address_allocation(&meta));
        assert!(!is_address_allocation(&serde_json::Value::Null));
    }

    #[test]
    fn a_withdraw_log_entry_without_our_wire_backfills_an_estimate() {
        let meta = serde_json::json!({
            "variant": {
                "withdraw": {
                    "address": an_address(),
                    "amount": 25_000,
                    "fee": { "fee_rate": { "sats_per_kvb": 10_000 }, "total_weight": 4_000 },
                    "change": [],
                },
            },
            "extra_meta": {},
        });
        let backfilled = backfill(an_operation_id(), &meta, 1_700_000_000_000).expect("recognised");
        assert_eq!(backfilled.kind, kinds::ONCHAIN_SEND);
        let details = wire::decode_send_details(&backfilled.details).expect("decode");
        assert_eq!(details.amount, Sats::from_sats(25_000));
        assert_eq!(details.fee, Amount::from_msats(10_000_000));
        assert_eq!(details.total, Amount::from_msats(35_000_000));
    }

    #[test]
    fn a_withdraw_log_entry_with_our_wire_backfills_exactly() {
        let exact = OnchainSendDetails {
            address: an_address().parse().expect("a valid address"),
            amount: Sats::from_sats(25_000),
            fee: Amount::from_msats(1_234),
            total: Amount::from_msats(25_001_234),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        };
        let our_wire = wire::OnchainSendDetailsWire::from(&exact);
        let meta = serde_json::json!({
            "variant": {
                "withdraw": {
                    "address": an_address(),
                    "amount": 25_000,
                    "fee": { "fee_rate": { "sats_per_kvb": 10_000 }, "total_weight": 4_000 },
                    "change": [],
                },
            },
            "extra_meta": custom_meta(&our_wire).expect("encode"),
        });
        // The upstream meta's own fee, if it were used, would imply a fee of 10 000 000 msat
        // rather than the wire's 1 234: this proves our own copy wins rather than the estimate.
        let backfilled = backfill(an_operation_id(), &meta, 999).expect("recognised");
        assert_eq!(backfilled.kind, kinds::ONCHAIN_SEND);
        let recovered = wire::decode_send_details(&backfilled.details).expect("decode");
        assert_eq!(recovered, exact);
    }

    #[test]
    fn an_rbf_withdraw_log_entry_backfills_nothing() {
        let meta = serde_json::json!({
            "variant": {
                "rbf_withdraw": {
                    "rbf": {
                        "fees": { "fee_rate": { "sats_per_kvb": 1_000 }, "total_weight": 400 },
                        "txid": "0".repeat(64),
                    },
                    "change": [],
                },
            },
            "extra_meta": {},
        });
        assert!(backfill(an_operation_id(), &meta, 0).is_none());
    }

    /// What the deposit code does with nothing but storage.
    #[cfg(not(target_family = "wasm"))]
    mod stored {
        use super::*;
        use crate::db::{federation_namespace, in_memory_root};
        use crate::onchain::deposit_fixtures::{ADDRESS, txid};

        /// A v1 receive record with `txid` and `gross_deposited_sats` as its payment.
        async fn a_record(
            txid: serde_json::Value,
            gross_deposited_sats: serde_json::Value,
        ) -> (Database, OperationId) {
            let db = federation_namespace(&in_memory_root(), [1u8; 32]);
            let federation = FederationInner::detached(db.clone(), true);
            let id = OperationId([1; 32]);
            let details = serde_json::json!({
                "address": ADDRESS,
                "txid": txid,
                "gross_deposited_sats": gross_deposited_sats,
                "fee_msats": null,
                "fee_breakdown": null,
                "net_credit_msats": null,
                "created_at": 1_700_000_000_000u64,
            });
            federation
                .create_operation(
                    id,
                    kinds::ONCHAIN_RECEIVE,
                    "wallet",
                    &details,
                    Arc::new(crate::onchain::OnchainReceiveDriver)
                        as Arc<dyn Driver<OnchainReceiveState>>,
                )
                .await
                .expect("create");
            (db, id)
        }

        /// Every state of a deposit reports the payment its record was written for.
        #[tokio::test(flavor = "multi_thread")]
        async fn a_deposit_reports_the_payment_on_its_record() {
            let (db, id) = a_record(txid(1).to_string().into(), 70_000.into()).await;
            assert_eq!(
                payment_of(&db, id).await.expect("a payment"),
                Payment {
                    txid: Txid::from_upstream(txid(1)),
                    gross: Sats::from_sats(70_000),
                }
            );
        }

        /// A record that names no payment is a deposit address an earlier version of this
        /// crate recorded when it handed the address out. Nobody paid it, so it has no state.
        #[tokio::test(flavor = "multi_thread")]
        async fn an_address_nobody_paid_has_no_payment_to_report() {
            let (db, id) = a_record(serde_json::Value::Null, serde_json::Value::Null).await;
            let err = payment_of(&db, id)
                .await
                .expect_err("an unpaid address has no payment");
            assert_eq!(err.code, ErrorCode::Internal);

            let err = payment_of(&db, OperationId([2; 32]))
                .await
                .expect_err("no record, no payment");
            assert_eq!(err.code, ErrorCode::Internal);
        }
    }
}
