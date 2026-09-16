//! The v1 wallet module (`wallet`): the withdrawal plan and send, the deposit address
//! allocation and its subscription, and the backfill of this module's own operation log.

use std::sync::{Arc, Weak};

use fedimint_client::Client;
use fedimint_client_module::ClientModuleInstance;
use fedimint_client_module::transaction::FeeQuote;
use fedimint_core::core::OperationId;
use fedimint_core::db::{Database, IDatabaseTransactionOpsCoreTyped};
use fedimint_core::util::BoxStream;
use fedimint_wallet_client::{
    DepositStateV2, WalletClientModule, WalletOperationMeta, WalletOperationMetaVariant,
    WithdrawState,
};
use fedimint_wallet_common::PegOutFees;
use futures::StreamExt;

use super::driver::{SendStep, through_settle};
use super::{
    OnchainQuoteInner, Plan, Terms, add, balance_of, bitcoin_to_sats, check_amount,
    check_covers_amount, claim_figures, fee_quote_failure, from_upstream, insufficient, internal,
    now, plan_of, quote_changed, sats_to_amount, sats_to_bitcoin, subscribe_error, timeout,
    unreachable, wire,
};
use crate::federation::FederationInner;
use crate::operation::{
    Backfilled, Driver, custom_meta, from_custom_meta, kinds, record_phase_in, until_final,
    write_details_in,
};
use crate::sdk::{CONTACT_TIMEOUT, SdkInner};
use crate::{
    Address, Amount, Error, ErrorCode, OnchainReceive, OnchainReceiveState, OnchainSendDetails,
    OnchainSendState, Operation, Result, Sats, Txid,
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
    federation: &Arc<FederationInner>,
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
            "could not quote the withdrawal's on-chain fee: {err}"
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
            let text = format!("{err:#}");
            let short = err.downcast_ref::<fedimint_mint_client::InsufficientBalanceError>();
            // `required` is the amount plus the on-chain fee already quoted above; the
            // dry run that would have priced the funding side is exactly what failed.
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
            "could not re-quote the withdrawal's on-chain fee: {err}"
        ))
    })?;
    let output_value = sats_to_bitcoin(quote.amount)
        .checked_add(fresh_fees.amount())
        .ok_or_else(|| internal("the withdrawal amount plus its on-chain fee overflowed"))?;
    let fresh_chain_fee = from_upstream(fedimint_core::Amount::from(fresh_fees.amount()));
    let fresh_quote = match module.send_fee_quote(output_value).await {
        Ok(fee_quote) => fee_quote,
        Err(err) => {
            let text = format!("{err:#}");
            let short = err.downcast_ref::<fedimint_mint_client::InsufficientBalanceError>();
            // `required` is the amount plus the on-chain fee already re-quoted above; the
            // dry run that would have priced the funding side is exactly what failed.
            let required = add(sats_to_amount(quote.amount)?, fresh_chain_fee)?;
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
    let id = module
        .withdraw(
            &checked,
            sats_to_bitcoin(quote.amount),
            fees,
            custom_meta(&wire)?,
        )
        .await
        .map_err(|err| {
            // `withdraw` fails inside `finalize_and_submit_transaction`, which has no dedicated
            // insufficient-balance variant of its own (unlike walletv2's typed
            // `SendError::InsufficientFunds`): the mint's own `InsufficientBalanceError`, if
            // that is the cause, sits behind a generic `TransactionSubmitError::PrimaryModule`
            // wrapper, so its text is read off the whole cause chain rather than matched by a
            // concrete downcast.
            let text = format!("{err:#}");
            if text.to_lowercase().contains("insufficient") {
                return Error::new(
                    ErrorCode::InsufficientBalance,
                    format!("the withdrawal could not be funded: {text}"),
                );
            }
            internal(format!("the withdrawal could not be submitted: {text}"))
        })?;
    federation
        .create_operation(id, kinds::ONCHAIN_SEND, "wallet", &wire, driver)
        .await
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
        .map_err(subscribe_error)?
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

/// Allocates a fresh v1 deposit address and records it.
pub(super) async fn receive(
    federation: &Arc<FederationInner>,
    client: &Client,
    module: &WalletClientModule,
    driver: Arc<dyn Driver<OnchainReceiveState>>,
) -> Result<OnchainReceive> {
    let created_at = now();
    // The address is not known until the call below returns it, so there is nothing this SDK
    // could put in `extra_meta` yet that upstream's own meta does not already carry once the
    // call commits: `WalletOperationMetaVariant::Deposit`'s own `address` field. Passing `Null`
    // rather than a placeholder wire means `backfill`, reconstructing a record from a crash
    // between upstream's commit and the write below, always reads the real address from that
    // field rather than risking a placeholder this SDK wrote earlier.
    let info = module
        .safe_allocate_deposit_address(serde_json::Value::Null)
        .await
        .map_err(|err| {
            let text = err.to_string();
            if text.contains("consensus version") {
                Error::new(ErrorCode::NotSupported, text)
            } else {
                internal(format!("could not allocate a deposit address: {text}"))
            }
        })?;
    let address = Address::from_upstream(info.address.clone().into_unchecked());
    let wire = wire::OnchainReceiveDetailsWire {
        address: info.address.to_string(),
        txid: None,
        gross_deposited_sats: None,
        fee_msats: None,
        fee_breakdown: None,
        net_credit_msats: None,
        created_at: created_at.epoch_millis(),
        upstream_operation_id: None,
        event_cursor: None,
    };
    let operation = federation
        .create_operation(
            info.operation_id,
            kinds::ONCHAIN_RECEIVE,
            "wallet",
            &wire,
            driver,
        )
        .await?;
    Ok(OnchainReceive { address, operation })
}

/// What the original deposit's next upstream state means: a state to hand out, or the claim
/// sentinel the below-fee rule turns into `Failed` instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DepositStep {
    State(OnchainReceiveState),
    Claimed { txid: Txid, gross: Sats },
}

// Upstream v1 `DepositStateV2` onto `OnchainReceiveState`, variant for variant but not payload
// for payload, plus the below-fee rule: the peg-in monitor refuses to claim a deposit at or
// below the federation's deposit fee but still writes the sentinel that makes
// `subscribe_deposit` report `Claimed`
// (`modules/fedimint-wallet-client/src/pegin_monitor.rs:494-497,552-561`), so a `Claimed` this
// small is handed back as `Failed` instead, with no ecash ever having been minted for it.
//
// Only the transaction half of upstream's `btc_out_point` is carried; the vout is nothing this
// API needs. `Claimed` additionally reports a net credit this SDK computes itself; upstream's
// own `Claimed` reports only the gross figure it deposited.
pub(super) fn map_deposit(state: &DepositStateV2, peg_in_abs: Amount) -> DepositStep {
    match state {
        DepositStateV2::WaitingForTransaction => {
            DepositStep::State(OnchainReceiveState::WaitingForTransaction)
        }
        DepositStateV2::WaitingForConfirmation {
            btc_deposited,
            btc_out_point,
        } => DepositStep::State(OnchainReceiveState::WaitingForConfirmation {
            txid: Txid::from_upstream(btc_out_point.txid),
            gross_deposited: bitcoin_to_sats(*btc_deposited),
        }),
        DepositStateV2::Confirmed {
            btc_deposited,
            btc_out_point,
        } => DepositStep::State(OnchainReceiveState::Confirmed {
            txid: Txid::from_upstream(btc_out_point.txid),
            gross_deposited: bitcoin_to_sats(*btc_deposited),
        }),
        DepositStateV2::Claimed {
            btc_deposited,
            btc_out_point,
        } => {
            // A Bitcoin amount is consensus-bounded to 21 million BTC, well inside a
            // millisatoshi `u64`, so this never overflows: the same conversion upstream's own
            // `From<bitcoin::Amount> for fedimint_core::Amount` uses. `map_deposit` cannot
            // return a `Result`, since a mapping is not itself a fallible operation, so this is
            // the one place in this file that leans on that bound rather than threading one
            // through.
            let gross_msats = from_upstream(fedimint_core::Amount::from(*btc_deposited));
            if gross_msats <= peg_in_abs {
                return DepositStep::State(OnchainReceiveState::Failed {
                    reason: "the deposit does not exceed the federation's deposit fee and was \
                              not claimed"
                        .to_owned(),
                });
            }
            DepositStep::Claimed {
                txid: Txid::from_upstream(btc_out_point.txid),
                gross: bitcoin_to_sats(*btc_deposited),
            }
        }
        DepositStateV2::Failed(reason) => DepositStep::State(OnchainReceiveState::Failed {
            reason: reason.clone(),
        }),
    }
}

/// A fresh stream over a v1 deposit: maps every upstream state, fills the wire record's
/// address-side fields the first time a transaction is seen, and on the claim reads back what the
/// claim transaction itself minted (or reads back a figure a previous subscription already
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
    let client = federation.client(false).await?;
    let module = module_of(&client)?;
    let peg_in_abs = from_upstream(module.get_fee_consensus().peg_in_abs);
    let upstream = module
        .subscribe_deposit(id)
        .await
        .map_err(subscribe_error)?
        .into_stream();
    drop(client);

    let sdk = federation.sdk.clone();
    let federation_id = federation.id;
    let db = federation.db();
    let stream = upstream.then(move |state| {
        let sdk = sdk.clone();
        let db = db.clone();
        async move {
            match map_deposit(&state, peg_in_abs) {
                DepositStep::State(mapped) => {
                    if let OnchainReceiveState::WaitingForConfirmation {
                        txid,
                        gross_deposited,
                    }
                    | OnchainReceiveState::Confirmed {
                        txid,
                        gross_deposited,
                    } = &mapped
                    {
                        fill_seen(&db, id, txid.clone(), *gross_deposited).await?;
                    }
                    Ok(mapped)
                }
                DepositStep::Claimed { txid, gross } => {
                    fill_seen(&db, id, txid.clone(), gross).await?;
                    let net_credit =
                        claim_net_credit(&db, id, sdk, federation_id, peg_in_abs, gross).await?;
                    Ok(OnchainReceiveState::Claimed {
                        txid,
                        gross_deposited: gross,
                        net_credit,
                    })
                }
            }
        }
    });
    Ok(until_final(stream))
}

/// Fills the funding transaction the first time it is seen and marks the operation as past a
/// pure address watch.
///
/// Idempotent both by construction (`write_details_in`/`record_phase_in` are themselves no-ops
/// once the stored value already matches) and because a re-subscription replays every earlier
/// state on every call, so this runs again for a transaction it already recorded.
async fn fill_seen(db: &Database, id: OperationId, txid: Txid, gross: Sats) -> Result<()> {
    let Some(record) = db
        .begin_transaction_nc()
        .await
        .get_value(&crate::db::OperationRecordKey(id))
        .await
    else {
        return Ok(());
    };
    let mut details = wire::decode_receive_wire(&record.details)?;
    if details.txid.is_none() {
        details.txid = Some(txid.to_string());
        details.gross_deposited_sats = Some(gross.sats());
        write_details_in(db, id, wire::encode_receive_wire(&details)?).await?;
    }
    record_phase_in(db, id, wire::PHASE_SEEN).await
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

/// Rebuilds a record from a v1 log entry. A deposit is always rebuilt from upstream's own meta
/// alone, address and nothing else, because `receive` never puts a wire of its own on a deposit's
/// entry; a withdrawal is exact when this SDK made it, whose metadata carries the quoted terms
/// verbatim, and an estimate otherwise, with no mint-side funding cost, since that is unknowable
/// from the operation log alone.
pub(super) fn backfill(
    id: OperationId,
    meta: &serde_json::Value,
    created_at: u64,
) -> Option<Backfilled> {
    let meta: WalletOperationMeta = serde_json::from_value(meta.clone()).ok()?;
    match meta.variant {
        WalletOperationMetaVariant::Deposit { address, .. } => {
            // `receive` never puts a wire of its own in a deposit's `extra_meta` (the address is
            // not known until after the call that would carry it), so this is always rebuilt
            // from upstream's own meta: the address it names, and this entry's `created_at`.
            let wire = wire::OnchainReceiveDetailsWire {
                address: address.assume_checked_ref().to_string(),
                txid: None,
                gross_deposited_sats: None,
                fee_msats: None,
                fee_breakdown: None,
                net_credit_msats: None,
                created_at,
                upstream_operation_id: None,
                event_cursor: None,
            };
            Some(Backfilled {
                kind: kinds::ONCHAIN_RECEIVE,
                details: wire::encode_receive_wire(&wire).ok()?,
                phase: None,
                final_state: None,
            })
        }
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

#[cfg(test)]
mod tests {
    use fedimint_core::bitcoin;

    use super::*;
    use crate::Timestamp;

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
    fn deposit_states_fold_onto_the_receive_lifecycle() {
        let peg_in_abs = Amount::from_msats(1_000_000);
        let out_point = bitcoin::OutPoint {
            txid: a_bitcoin_txid(),
            vout: 0,
        };
        let above_fee = bitcoin::Amount::from_sat(50_000);

        assert_eq!(
            map_deposit(&DepositStateV2::WaitingForTransaction, peg_in_abs),
            DepositStep::State(OnchainReceiveState::WaitingForTransaction)
        );
        assert_eq!(
            map_deposit(
                &DepositStateV2::WaitingForConfirmation {
                    btc_deposited: above_fee,
                    btc_out_point: out_point,
                },
                peg_in_abs
            ),
            DepositStep::State(OnchainReceiveState::WaitingForConfirmation {
                txid: Txid::from_upstream(out_point.txid),
                gross_deposited: Sats::from_sats(50_000),
            })
        );
        assert_eq!(
            map_deposit(
                &DepositStateV2::Confirmed {
                    btc_deposited: above_fee,
                    btc_out_point: out_point,
                },
                peg_in_abs
            ),
            DepositStep::State(OnchainReceiveState::Confirmed {
                txid: Txid::from_upstream(out_point.txid),
                gross_deposited: Sats::from_sats(50_000),
            })
        );
        assert_eq!(
            map_deposit(
                &DepositStateV2::Claimed {
                    btc_deposited: above_fee,
                    btc_out_point: out_point,
                },
                peg_in_abs
            ),
            DepositStep::Claimed {
                txid: Txid::from_upstream(out_point.txid),
                gross: Sats::from_sats(50_000),
            }
        );
        assert_eq!(
            map_deposit(&DepositStateV2::Failed("boom".to_owned()), peg_in_abs),
            DepositStep::State(OnchainReceiveState::Failed {
                reason: "boom".to_owned(),
            })
        );
    }

    #[test]
    fn a_claim_at_or_below_the_deposit_fee_is_failed() {
        let peg_in_abs = Amount::from_msats(1_000_000);
        let out_point = bitcoin::OutPoint {
            txid: a_bitcoin_txid(),
            vout: 0,
        };
        for at_or_below in [
            bitcoin::Amount::from_sat(1_000),
            bitcoin::Amount::from_sat(500),
        ] {
            assert_eq!(
                map_deposit(
                    &DepositStateV2::Claimed {
                        btc_deposited: at_or_below,
                        btc_out_point: out_point,
                    },
                    peg_in_abs
                ),
                DepositStep::State(OnchainReceiveState::Failed {
                    reason: "the deposit does not exceed the federation's deposit fee and was \
                              not claimed"
                        .to_owned(),
                })
            );
        }
    }

    #[test]
    fn a_deposit_log_entry_backfills_a_receive_record() {
        let meta = serde_json::json!({
            "variant": {
                "deposit": {
                    "address": an_address(),
                },
            },
            "extra_meta": {},
        });
        let backfilled = backfill(an_operation_id(), &meta, 1_700_000_000_000).expect("recognised");
        assert_eq!(backfilled.kind, kinds::ONCHAIN_RECEIVE);
        assert_eq!(backfilled.phase, None);
        assert_eq!(backfilled.final_state, None);
        let details = wire::decode_receive_details(&backfilled.details).expect("decode");
        assert_eq!(details.address.to_string(), an_address());
        assert_eq!(details.txid, None);
        assert_eq!(
            details.created_at,
            Timestamp::from_epoch_millis(1_700_000_000_000)
        );
    }

    /// The exact payload a crash between `safe_allocate_deposit_address`'s commit and
    /// `create_operation`'s own write would have left behind, if `receive` still put a wire of
    /// its own in a deposit's `extra_meta`: the placeholder's empty address, because the real one
    /// was not known until after the call it rode inside of. `backfill` never reads a deposit's
    /// `extra_meta` at all, so upstream's own address wins regardless of what is there.
    #[test]
    fn a_deposit_log_entry_with_a_stale_placeholder_backfills_upstream_s_address() {
        let placeholder = wire::OnchainReceiveDetailsWire {
            address: String::new(),
            txid: None,
            gross_deposited_sats: None,
            fee_msats: None,
            fee_breakdown: None,
            net_credit_msats: None,
            created_at: 1_700_000_000_000,
            upstream_operation_id: None,
            event_cursor: None,
        };
        let meta = serde_json::json!({
            "variant": {
                "deposit": {
                    "address": an_address(),
                },
            },
            "extra_meta": custom_meta(&placeholder).expect("encode"),
        });
        let backfilled = backfill(an_operation_id(), &meta, 1_700_000_000_000).expect("recognised");
        let details = wire::decode_receive_details(&backfilled.details).expect("decode");
        assert_eq!(details.address.to_string(), an_address());
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
}
