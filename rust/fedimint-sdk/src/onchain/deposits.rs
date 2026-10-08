//! How an on-chain deposit becomes an operation.
//!
//! A wallet module announces a payment it has found and started claiming with an entry in the
//! client's event log. The pass in this file reads those entries and sees to it that each
//! deposit announced has its record, and it is what a deposit subscription waits on. For the v1
//! module the announcement is the only thing a record is ever written from. A walletv2 claim
//! also has a log entry of its own that names the payment, so reconciliation can write its
//! record from that as well. A deposit address that was only handed out is announced by
//! nothing, so it never has a record.

use std::collections::HashSet;
use std::sync::Arc;

use fedimint_client::Client;
use fedimint_core::core::OperationId;
use fedimint_core::db::IDatabaseTransactionOpsCoreTyped;
use fedimint_eventlog::{DBTransactionEventLogExt as _, EventLogId};
use futures::StreamExt as _;

use super::{internal, v1, v2};
use crate::db::{DepositCursorKey, OperationRecordKeyPrefix};
use crate::federation::FederationInner;
use crate::operation::kinds;
use crate::{Error, ErrorCode, OnchainReceiveState, Operation, Result};

/// Brings this federation's deposit records up to date with what its wallet has found.
///
/// Reads the event log from where the previous pass stopped and writes a record for every
/// deposit announced since. It runs when a federation comes up and before activity is listed, so
/// a deposit found while nothing was subscribed has its record by the time anything looks for
/// it. A pass that finds nothing new costs one read.
///
/// # Errors
///
/// [`Storage`](crate::ErrorCode::Storage) if a record or the position cannot be committed.
pub(crate) async fn pick_up_deposits(federation: &Arc<FederationInner>) -> Result<()> {
    let db = federation.db();
    let from = db
        .begin_transaction_nc()
        .await
        .get_value(&DepositCursorKey)
        .await
        .unwrap_or(0);
    let (_, reached) = scan(federation, from, |_| false).await?;
    if reached == from {
        return Ok(());
    }
    db.autocommit(
        |dbtx, _| {
            Box::pin(async move {
                // Never backwards: a pass that started earlier and finished later must not undo
                // the progress of one that overtook it.
                let stored = dbtx.get_value(&DepositCursorKey).await.unwrap_or(0);
                if stored < reached {
                    dbtx.insert_entry(&DepositCursorKey, &reached).await;
                }
                Ok::<_, core::convert::Infallible>(())
            })
        },
        Some(100),
    )
    .await
    .map_err(crate::db::storage_error)
}

/// Whether a wallet module's log entry is one that must be left without a record of its own.
///
/// Two kinds are. A v1 `Deposit` entry is written when an address is handed out, before
/// anything is paid: the deposit it may become gets its record from the wallet's announcement
/// instead (see [`pick_up_deposits`]), which names the transaction the entry does not. And a
/// walletv2 `Receive` entry for an output another deposit record already names is another claim
/// of that same deposit, made because the federation rejected one: the deposit keeps its one
/// record, which follows whichever claim is live.
pub(crate) async fn log_entry_is_not_an_operation(
    federation: &FederationInner,
    id: OperationId,
    module: &str,
    meta: &serde_json::Value,
) -> bool {
    match module {
        "wallet" => v1::is_address_allocation(meta),
        "walletv2" => {
            let Some(paid) = v2::paid_by(meta) else {
                return false;
            };
            let db = federation.db();
            // Read first and judged afterwards: telling whether a record is of this output can
            // take a read of its own, which is kept off this walk's transaction.
            let others: Vec<String> = db
                .begin_transaction_nc()
                .await
                .find_by_prefix(&OperationRecordKeyPrefix)
                .await
                .filter_map(|(key, record)| {
                    let other = key.0 != id
                        && record.kind == kinds::ONCHAIN_RECEIVE
                        && record.module == module;
                    core::future::ready(other.then_some(record.details))
                })
                .collect()
                .await;
            for details in others {
                if v2::names(&db, &details, &paid).await {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

/// The state behind an [`OnchainDeposits`](crate::OnchainDeposits).
pub(super) struct Subscription {
    federation: Arc<FederationInner>,
    cursor: tokio::sync::Mutex<Cursor>,
}

/// Where a deposit subscription has got to.
struct Cursor {
    /// The event-log position the next walk starts from.
    position: u64,
    /// Ticks whenever the client logs an event.
    added: tokio::sync::watch::Receiver<()>,
    /// The deposits already handed out. A v1 address paid by two outputs of one transaction is
    /// announced once per output under the one operation, and this is what hands it out once.
    announced: HashSet<OperationId>,
}

impl core::fmt::Debug for Subscription {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OnchainDeposits")
            .field("federation", &self.federation.id)
            .finish()
    }
}

impl Subscription {
    /// A subscription that starts at the end of `client`'s event log as it stands now.
    pub(super) async fn open(federation: Arc<FederationInner>, client: &Client) -> Subscription {
        let added = client.log_event_added_rx();
        let position = u64::from(client.get_next_event_log_id().await);
        Subscription::starting_at(federation, position, added)
    }

    /// The next deposit announced at or after this subscription's position.
    ///
    /// # Errors
    ///
    /// [`FederationClosed`](ErrorCode::FederationClosed) once the federation stops running,
    /// [`Storage`](ErrorCode::Storage), and [`Internal`](ErrorCode::Internal).
    pub(super) async fn next(&self) -> Result<Operation<OnchainReceiveState>> {
        let mut cursor = self.cursor.lock().await;
        let mut closed = self.federation.closed();
        loop {
            if *closed.borrow_and_update() {
                return Err(not_running());
            }
            self.federation.ensure_open()?;
            // Marked unchanged before the walk, not after: an event logged while the walk is
            // still reading has to wake the wait below, or it would sit unnoticed until some
            // later, unrelated event did.
            cursor.added.mark_unchanged();
            let from = cursor.position;
            let announced = &cursor.announced;
            let (found, reached) =
                scan(&self.federation, from, |id| !announced.contains(&id)).await?;
            if let Some(id) = found {
                // The handle is built before the cursor moves: if this fails, or this future is
                // dropped while it runs, the next call finds the same deposit again.
                let operation = operation_of(&self.federation, id).await?;
                cursor.position = reached;
                cursor.announced.insert(id);
                return Ok(operation);
            }
            cursor.position = reached;
            tokio::select! {
                changed = cursor.added.changed() => {
                    if changed.is_err() {
                        // The client the receiver came from is gone. A federation still running
                        // has had its client replaced, and the successor's receiver takes over;
                        // one that is not answers `FederationClosed` here.
                        let client = self.federation.client(false).await?;
                        cursor.added = client.log_event_added_rx();
                    }
                }
                _ = closed.changed() => {}
            }
        }
    }

    /// A subscription that starts at `position` and is woken by `added`.
    fn starting_at(
        federation: Arc<FederationInner>,
        position: u64,
        added: tokio::sync::watch::Receiver<()>,
    ) -> Subscription {
        Subscription {
            federation,
            cursor: tokio::sync::Mutex::new(Cursor {
                position,
                added,
                announced: HashSet::new(),
            }),
        }
    }
}

/// Walks the event log from `from`, writing a record for every deposit it announces, and stops
/// at the first one `wanted` accepts.
///
/// Returns that deposit, if there was one, and the position the next walk starts from: just past
/// the deposit's own entry, or the end of the log.
async fn scan(
    federation: &Arc<FederationInner>,
    from: u64,
    mut wanted: impl FnMut(OperationId) -> bool,
) -> Result<(Option<OperationId>, u64)> {
    let db = federation.db();
    let mut position = from;
    loop {
        let page = db
            .begin_transaction_nc()
            .await
            .get_event_log(Some(EventLogId::LOG_START.saturating_add(position)), PAGE)
            .await;
        if page.is_empty() {
            return Ok((None, position));
        }
        for entry in &page {
            position = u64::from(entry.id().saturating_add(1));
            let deposit = if let Some(found) = v2::deposit_found(entry) {
                v2::adopt(federation, found, position).await?
            } else if let Some(found) = v1::deposit_found(entry) {
                v1::adopt(federation, &found).await?
            } else {
                None
            };
            if let Some(id) = deposit
                && wanted(id)
            {
                return Ok((Some(id), position));
            }
        }
    }
}

/// How many event log entries one read takes.
const PAGE: u64 = 64;

/// The typed handle for a deposit whose record was just written or found.
async fn operation_of(
    federation: &Arc<FederationInner>,
    id: OperationId,
) -> Result<Operation<OnchainReceiveState>> {
    federation
        .operation(id)
        .await?
        .and_then(|operation| operation.as_onchain_receive())
        .ok_or_else(|| {
            internal(format!(
                "the deposit recorded as operation {} could not be read back",
                id.fmt_full()
            ))
        })
}

fn not_running() -> Error {
    Error::new(
        ErrorCode::FederationClosed,
        "this federation is not running",
    )
}

// At file scope rather than inside `mod tests`, because a `mod tests` is private to its own
// file and the erase guard's own tests (`src/sdk.rs`) plant a deposit too.
#[cfg(all(test, not(target_family = "wasm")))]
pub(crate) mod fixtures {
    use fedimint_client::oplog::OperationLog;
    use fedimint_core::bitcoin;
    use fedimint_core::core::OperationId;
    use fedimint_core::db::{Database, IDatabaseTransactionOpsCoreTyped as _};
    use fedimint_eventlog::{EventLogEntry, EventLogId};

    use super::{v1, v2};

    /// A real regtest address, taken from `bitcoin`'s own test suite.
    pub(crate) const ADDRESS: &str = "bcrt1q2nfxmhd4n3c8834pj72xagvyr9gl57n5r94fsl";

    /// When the announcements these fixtures write were logged, in microseconds since the Unix
    /// epoch.
    pub(crate) const LOGGED_AT: u64 = 1_700_000_000_000_000;

    pub(crate) fn txid(byte: u8) -> bitcoin::Txid {
        format!("{byte:02x}")
            .repeat(32)
            .parse()
            .expect("a well-formed transaction id")
    }

    /// Writes an operation log entry the way the client itself does.
    pub(crate) async fn write_log_entry(
        db: &Database,
        id: OperationId,
        module: &str,
        meta: serde_json::Value,
    ) {
        let mut dbtx = db.begin_transaction().await;
        OperationLog::new(db.clone())
            .add_operation_log_entry_dbtx(&mut dbtx.to_ref_nc(), id, module, meta)
            .await;
        dbtx.commit_tx().await;
    }

    /// Puts `entry` at `position` of the event log, where the client's own ordering task would
    /// have put it.
    pub(crate) async fn log(db: &Database, position: u64, entry: EventLogEntry) {
        let mut dbtx = db.begin_transaction().await;
        dbtx.insert_new_entry(&EventLogId::LOG_START.saturating_add(position), &entry)
            .await;
        dbtx.commit_tx().await;
    }

    /// Hands a v1 address out under `id` and announces a payment of 70 000 sat to it, made by
    /// `paid_by`, at `position` of the event log.
    pub(crate) async fn a_paid_v1_address(
        db: &Database,
        id: OperationId,
        paid_by: bitcoin::Txid,
        position: u64,
    ) {
        write_log_entry(db, id, "wallet", v1::fixtures::allocation(ADDRESS)).await;
        log(
            db,
            position,
            v1::fixtures::announcement(id, paid_by, 70_000, LOGGED_AT),
        )
        .await;
    }

    /// Logs a walletv2 claim of `paid`, an output worth 100 000 sat, under `id`, and announces
    /// it at `position` of the event log.
    pub(crate) async fn a_walletv2_claim(
        db: &Database,
        id: OperationId,
        paid: bitcoin::OutPoint,
        position: u64,
    ) {
        write_log_entry(
            db,
            id,
            "walletv2",
            v2::fixtures::claim(ADDRESS, 100_000, paid),
        )
        .await;
        log(
            db,
            position,
            v2::fixtures::announcement(id, ADDRESS, 100_000, paid),
        )
        .await;
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use fedimint_core::bitcoin;
    use fedimint_core::db::Database;

    use super::fixtures::{
        ADDRESS, LOGGED_AT, a_paid_v1_address, a_walletv2_claim, log, txid, write_log_entry,
    };
    use super::*;
    use crate::db::{OperationRecord, OperationRecordKey, federation_namespace, in_memory_root};
    use crate::onchain::wire;
    use crate::{FederationStatus, OperationKind};

    fn a_federation() -> (Database, Arc<FederationInner>) {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        (db.clone(), FederationInner::detached(db, true))
    }

    fn operation(byte: u8) -> OperationId {
        OperationId([byte; 32])
    }

    fn outpoint(byte: u8, vout: u32) -> bitcoin::OutPoint {
        bitcoin::OutPoint {
            txid: txid(byte),
            vout,
        }
    }

    async fn record(db: &Database, id: OperationId) -> Option<OperationRecord> {
        db.begin_transaction_nc()
            .await
            .get_value(&OperationRecordKey(id))
            .await
    }

    async fn position(db: &Database) -> Option<u64> {
        db.begin_transaction_nc()
            .await
            .get_value(&DepositCursorKey)
            .await
    }

    /// Waits until `subscription` has a `next` in flight: the call takes the cursor's lock
    /// before anything else and holds it for as long as it runs.
    async fn until_waiting(subscription: &Subscription) {
        while subscription.cursor.try_lock().is_ok() {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_v1_address_is_an_operation_only_once_its_payment_is_announced() {
        let (db, federation) = a_federation();
        let id = operation(1);
        write_log_entry(&db, id, "wallet", v1::fixtures::allocation(ADDRESS)).await;

        // An address that was handed out is not an operation: not when it is looked up, and
        // not when the client's log is reconciled.
        assert!(federation.operation(id).await.expect("lookup").is_none());
        federation.reconcile_operations().await.expect("reconcile");
        pick_up_deposits(&federation).await.expect("pick up");
        assert_eq!(record(&db, id).await, None);

        log(
            &db,
            0,
            v1::fixtures::announcement(id, txid(1), 70_000, LOGGED_AT),
        )
        .await;
        pick_up_deposits(&federation).await.expect("pick up");

        let stored = record(&db, id).await.expect("the deposit has its record");
        assert_eq!(stored.kind, kinds::ONCHAIN_RECEIVE);
        assert_eq!(stored.module, "wallet");
        assert_eq!(stored.phase, Some(wire::PHASE_SEEN));
        assert_eq!(stored.final_state, None);
        // Dated from the announcement, not from when the address was handed out.
        assert_eq!(stored.created_at, LOGGED_AT / 1000);
        let details = wire::decode_receive_details(&stored.details).expect("a deposit's details");
        assert_eq!(details.address.to_string(), ADDRESS);
        assert_eq!(details.txid.to_string(), txid(1).to_string());
        assert_eq!(details.gross_deposited, crate::Sats::from_sats(70_000));
        assert_eq!(details.net_credit, None);

        let found = federation
            .operation(id)
            .await
            .expect("lookup")
            .expect("the deposit is an operation");
        assert_eq!(found.kind(), OperationKind::OnchainReceive);
        assert!(found.as_onchain_receive().is_some());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_second_payment_to_a_v1_address_is_not_a_second_deposit() {
        let (db, federation) = a_federation();
        let id = operation(1);
        a_paid_v1_address(&db, id, txid(1), 0).await;
        // The module claims every payment to an address under the address's own operation.
        log(
            &db,
            1,
            v1::fixtures::announcement(id, txid(2), 30_000, LOGGED_AT + 1),
        )
        .await;

        let (first, reached) = scan(&federation, 0, |_| true).await.expect("scan");
        assert_eq!((first, reached), (Some(id), 1));
        let (second, reached) = scan(&federation, reached, |_| true).await.expect("scan");
        assert_eq!((second, reached), (None, 2));

        // The record goes on describing the payment it was written for.
        let stored = record(&db, id).await.expect("the deposit has its record");
        let details = wire::decode_receive_details(&stored.details).expect("a deposit's details");
        assert_eq!(details.txid.to_string(), txid(1).to_string());
        assert_eq!(details.gross_deposited, crate::Sats::from_sats(70_000));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_announcement_of_something_that_is_not_an_address_records_nothing() {
        let (db, federation) = a_federation();
        // No log entry at all, and one that is a withdrawal.
        let unknown = operation(1);
        let withdrawal = operation(2);
        write_log_entry(
            &db,
            withdrawal,
            "wallet",
            serde_json::json!({
                "variant": {
                    "withdraw": {
                        "address": ADDRESS,
                        "amount": 25_000,
                        "fee": { "fee_rate": { "sats_per_kvb": 10_000 }, "total_weight": 4_000 },
                        "change": [],
                    },
                },
                "extra_meta": {},
            }),
        )
        .await;
        log(
            &db,
            0,
            v1::fixtures::announcement(unknown, txid(1), 70_000, LOGGED_AT),
        )
        .await;
        log(
            &db,
            1,
            v1::fixtures::announcement(withdrawal, txid(2), 70_000, LOGGED_AT),
        )
        .await;

        assert_eq!(
            scan(&federation, 0, |_| true).await.expect("scan"),
            (None, 2)
        );
        assert_eq!(record(&db, unknown).await, None);
        assert_eq!(record(&db, withdrawal).await, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_pass_resumes_where_the_last_one_stopped_and_changes_nothing_it_already_wrote() {
        let (db, federation) = a_federation();
        assert_eq!(position(&db).await, None);
        pick_up_deposits(&federation).await.expect("pick up");
        // Nothing to read, so nothing to remember.
        assert_eq!(position(&db).await, None);

        let first = operation(1);
        a_paid_v1_address(&db, first, txid(1), 0).await;
        pick_up_deposits(&federation).await.expect("pick up");
        assert_eq!(position(&db).await, Some(1));
        let written = record(&db, first)
            .await
            .expect("the deposit has its record");

        pick_up_deposits(&federation).await.expect("pick up");
        assert_eq!(position(&db).await, Some(1));
        assert_eq!(record(&db, first).await, Some(written.clone()));

        let second = operation(2);
        a_paid_v1_address(&db, second, txid(2), 1).await;
        pick_up_deposits(&federation).await.expect("pick up");
        assert_eq!(position(&db).await, Some(2));
        assert!(record(&db, second).await.is_some());
        assert_eq!(record(&db, first).await, Some(written));

        // Reading every announcement again, as a subscription opened at the start of the log
        // does, changes nothing either.
        let before = [record(&db, first).await, record(&db, second).await];
        assert_eq!(
            scan(&federation, 0, |_| false).await.expect("scan"),
            (None, 2)
        );
        assert_eq!(
            [record(&db, first).await, record(&db, second).await],
            before
        );
    }

    /// A record that names no payment was written by a version of this crate that recorded an
    /// address when it handed the address out. The announcement fills the payment in.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_address_recorded_when_it_was_handed_out_becomes_a_deposit_once_paid() {
        let (db, federation) = a_federation();
        let id = operation(1);
        write_log_entry(&db, id, "wallet", v1::fixtures::allocation(ADDRESS)).await;
        let unpaid = serde_json::json!({
            "address": ADDRESS,
            "txid": null,
            "gross_deposited_sats": null,
            "fee_msats": null,
            "fee_breakdown": null,
            "net_credit_msats": null,
            "created_at": 1_600_000_000_000u64,
        });
        federation
            .create_operation(
                id,
                kinds::ONCHAIN_RECEIVE,
                "wallet",
                &unpaid,
                Arc::new(crate::onchain::OnchainReceiveDriver)
                    as Arc<dyn crate::operation::Driver<OnchainReceiveState>>,
            )
            .await
            .expect("create");
        assert_eq!(record(&db, id).await.expect("written").phase, None);

        log(
            &db,
            0,
            v1::fixtures::announcement(id, txid(1), 70_000, LOGGED_AT),
        )
        .await;
        assert_eq!(
            scan(&federation, 0, |_| true).await.expect("scan"),
            (Some(id), 1)
        );

        let stored = record(&db, id).await.expect("still there");
        assert_eq!(stored.phase, Some(wire::PHASE_SEEN));
        let details = wire::decode_receive_details(&stored.details).expect("a deposit's details");
        assert_eq!(details.txid.to_string(), txid(1).to_string());
        assert_eq!(details.gross_deposited, crate::Sats::from_sats(70_000));
        // The deposit is dated from when it was found, not from when the address was handed out.
        assert_eq!(
            details.created_at,
            crate::Timestamp::from_epoch_millis(LOGGED_AT / 1000)
        );
    }

    /// The fill-in above is two writes, the payment and then the phase. One interrupted between
    /// them leaves a record that names its payment and is still marked as an unpaid address,
    /// and reading the announcement again finishes it.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_interrupted_fill_in_is_finished_when_the_announcement_is_read_again() {
        let (db, federation) = a_federation();
        let id = operation(1);
        write_log_entry(&db, id, "wallet", v1::fixtures::allocation(ADDRESS)).await;
        let half_filled = serde_json::json!({
            "address": ADDRESS,
            "txid": txid(1).to_string(),
            "gross_deposited_sats": 70_000,
            "fee_msats": null,
            "fee_breakdown": null,
            "net_credit_msats": null,
            "created_at": 1_600_000_000_000u64,
        });
        federation
            .create_operation(
                id,
                kinds::ONCHAIN_RECEIVE,
                "wallet",
                &half_filled,
                Arc::new(crate::onchain::OnchainReceiveDriver)
                    as Arc<dyn crate::operation::Driver<OnchainReceiveState>>,
            )
            .await
            .expect("create");
        assert_eq!(record(&db, id).await.expect("written").phase, None);
        log(
            &db,
            0,
            v1::fixtures::announcement(id, txid(1), 70_000, LOGGED_AT),
        )
        .await;

        pick_up_deposits(&federation).await.expect("pick up");

        assert_eq!(
            record(&db, id).await.expect("still there").phase,
            Some(wire::PHASE_SEEN)
        );
    }

    /// A record that cannot be read is left alone, and the deposits announced after it still
    /// get theirs.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_unreadable_record_does_not_hold_up_the_deposits_after_it() {
        let (db, federation) = a_federation();
        let unreadable = operation(1);
        write_log_entry(&db, unreadable, "wallet", v1::fixtures::allocation(ADDRESS)).await;
        federation
            .create_operation(
                unreadable,
                kinds::ONCHAIN_RECEIVE,
                "wallet",
                &serde_json::json!({ "not": "a deposit record" }),
                Arc::new(crate::onchain::OnchainReceiveDriver)
                    as Arc<dyn crate::operation::Driver<OnchainReceiveState>>,
            )
            .await
            .expect("create");
        let written = record(&db, unreadable).await;
        log(
            &db,
            0,
            v1::fixtures::announcement(unreadable, txid(1), 70_000, LOGGED_AT),
        )
        .await;
        let later = operation(2);
        a_paid_v1_address(&db, later, txid(2), 1).await;

        pick_up_deposits(&federation).await.expect("pick up");

        assert_eq!(record(&db, unreadable).await, written);
        assert!(record(&db, later).await.is_some());
        assert_eq!(position(&db).await, Some(2));
    }

    /// Coming up is when a deposit the wallet found in an earlier run, with nothing reading its
    /// announcements, gets its record.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_federation_coming_up_records_the_deposits_found_before() {
        let (db, federation) = a_federation();
        let on_v1 = operation(1);
        a_paid_v1_address(&db, on_v1, txid(1), 0).await;
        let on_v2 = operation(2);
        a_walletv2_claim(&db, on_v2, outpoint(3, 1), 1).await;

        crate::federation::reconcile_on_open(&federation).await;

        let v1_record = record(&db, on_v1).await.expect("the v1 deposit");
        assert_eq!(v1_record.kind, kinds::ONCHAIN_RECEIVE);
        assert_eq!(v1_record.phase, Some(wire::PHASE_SEEN));
        let v2_record = record(&db, on_v2).await.expect("the walletv2 deposit");
        assert_eq!(v2_record.kind, kinds::ONCHAIN_RECEIVE);
        // Reconciling the log wrote the walletv2 record, and the pass after it told the record
        // where its claim's announcement ends.
        let stored = wire::decode_receive_wire(&v2_record.details).expect("decode");
        assert_eq!(stored.event_cursor, Some(2));
        assert_eq!(position(&db).await, Some(2));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_walletv2_claim_is_an_operation_under_its_own_id() {
        let (db, federation) = a_federation();
        let id = operation(1);
        let paid = outpoint(3, 1);
        write_log_entry(
            &db,
            id,
            "walletv2",
            v2::fixtures::claim(ADDRESS, 100_000, paid),
        )
        .await;
        log(
            &db,
            0,
            v2::fixtures::announcement(id, ADDRESS, 100_000, paid),
        )
        .await;

        pick_up_deposits(&federation).await.expect("pick up");

        let stored = record(&db, id).await.expect("the deposit has its record");
        assert_eq!(stored.kind, kinds::ONCHAIN_RECEIVE);
        assert_eq!(stored.module, "walletv2");
        assert_eq!(stored.phase, Some(wire::PHASE_SEEN));
        let details = wire::decode_receive_details(&stored.details).expect("a deposit's details");
        assert_eq!(details.address.to_string(), ADDRESS);
        assert_eq!(details.txid.to_string(), txid(3).to_string());
        assert_eq!(details.gross_deposited, crate::Sats::from_sats(100_000));
        // The record knows where its claim's announcement ends, which is where a search for a
        // later claim of the same deposit would start.
        let stored = wire::decode_receive_wire(&stored.details).expect("decode");
        assert_eq!(stored.event_cursor, Some(1));
        assert_eq!(position(&db).await, Some(1));
    }

    /// The module logs a claim's entry before it announces the claim, so a federation can come
    /// up with the entry and no announcement yet. Reconciling the log is enough for the record.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_walletv2_claim_is_recorded_from_its_log_entry_alone() {
        let (db, federation) = a_federation();
        let id = operation(1);
        write_log_entry(
            &db,
            id,
            "walletv2",
            v2::fixtures::claim(ADDRESS, 100_000, outpoint(3, 1)),
        )
        .await;

        federation.reconcile_operations().await.expect("reconcile");

        let stored = record(&db, id).await.expect("the deposit has its record");
        assert_eq!(stored.kind, kinds::ONCHAIN_RECEIVE);
        assert_eq!(stored.phase, Some(wire::PHASE_SEEN));
    }

    /// The federation rejected a claim and the module made another of the same output: one
    /// deposit, so one record, under the claim that was recorded first.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_later_claim_of_a_walletv2_deposit_gets_no_record_of_its_own() {
        let (db, federation) = a_federation();
        let rejected = operation(1);
        let retried = operation(2);
        let paid = outpoint(3, 1);
        for (at, id) in [(0, rejected), (1, retried)] {
            write_log_entry(
                &db,
                id,
                "walletv2",
                v2::fixtures::claim(ADDRESS, 100_000, paid),
            )
            .await;
            log(
                &db,
                at,
                v2::fixtures::announcement(id, ADDRESS, 100_000, paid),
            )
            .await;
        }

        let (first, reached) = scan(&federation, 0, |_| true).await.expect("scan");
        assert_eq!((first, reached), (Some(rejected), 1));
        let (second, reached) = scan(&federation, reached, |_| true).await.expect("scan");
        assert_eq!((second, reached), (None, 2));

        assert!(record(&db, rejected).await.is_some());
        assert_eq!(record(&db, retried).await, None);
        assert!(
            federation
                .operation(retried)
                .await
                .expect("lookup")
                .is_none()
        );
        let meta = v2::fixtures::claim(ADDRESS, 100_000, paid);
        assert!(log_entry_is_not_an_operation(&federation, retried, "walletv2", &meta).await);
        // The record's own entry is still an operation, as far as this question goes.
        assert!(!log_entry_is_not_an_operation(&federation, rejected, "walletv2", &meta).await);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn two_outputs_of_one_transaction_are_two_walletv2_deposits() {
        let (db, federation) = a_federation();
        for (at, id, vout) in [(0, operation(1), 0), (1, operation(2), 1)] {
            let paid = outpoint(3, vout);
            write_log_entry(
                &db,
                id,
                "walletv2",
                v2::fixtures::claim(ADDRESS, 100_000, paid),
            )
            .await;
            log(
                &db,
                at,
                v2::fixtures::announcement(id, ADDRESS, 100_000, paid),
            )
            .await;
        }

        pick_up_deposits(&federation).await.expect("pick up");

        assert!(record(&db, operation(1)).await.is_some());
        assert!(record(&db, operation(2)).await.is_some());
    }

    /// A record that keeps no output index, as an earlier version of this crate wrote them, is
    /// of the output its claim's own log entry names: another claim of that output joins it, and
    /// a claim of another output of the same transaction, paid to the same address, does not.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_record_without_an_output_index_is_matched_by_the_output_its_claim_names() {
        let (db, federation) = a_federation();
        let followed = operation(1);
        write_log_entry(
            &db,
            followed,
            "walletv2",
            v2::fixtures::claim(ADDRESS, 100_000, outpoint(3, 0)),
        )
        .await;
        let old = operation(9);
        let details = serde_json::json!({
            "address": ADDRESS,
            "txid": txid(3).to_string(),
            "gross_deposited_sats": 100_000,
            "fee_msats": null,
            "fee_breakdown": null,
            "net_credit_msats": null,
            "created_at": 1_600_000_000_000u64,
            "upstream_operation_id": followed.fmt_full().to_string(),
            "event_cursor": 0,
        });
        federation
            .create_operation(
                old,
                kinds::ONCHAIN_RECEIVE,
                "walletv2",
                &details,
                Arc::new(crate::onchain::OnchainReceiveDriver)
                    as Arc<dyn crate::operation::Driver<OnchainReceiveState>>,
            )
            .await
            .expect("create");

        let sibling = operation(2);
        a_walletv2_claim(&db, sibling, outpoint(3, 1), 0).await;
        let retried = operation(3);
        a_walletv2_claim(&db, retried, outpoint(3, 0), 1).await;

        pick_up_deposits(&federation).await.expect("pick up");

        assert!(
            record(&db, sibling).await.is_some(),
            "another output of the same transaction is a deposit of its own"
        );
        assert_eq!(
            record(&db, retried).await,
            None,
            "another claim of the recorded output joins the record it already has"
        );
        assert_eq!(record(&db, followed).await, None);
    }

    /// Reconciling the log walks it newest first and knows nothing of the announcements, and it
    /// still never gives one deposit two records.
    #[tokio::test(flavor = "multi_thread")]
    async fn reconciliation_never_records_one_walletv2_deposit_twice() {
        let (db, federation) = a_federation();
        let paid = outpoint(3, 1);
        for id in [operation(1), operation(2)] {
            write_log_entry(
                &db,
                id,
                "walletv2",
                v2::fixtures::claim(ADDRESS, 100_000, paid),
            )
            .await;
        }

        federation.reconcile_operations().await.expect("reconcile");

        let recorded = [operation(1), operation(2)];
        let mut count = 0;
        for id in recorded {
            if record(&db, id).await.is_some() {
                count += 1;
            }
        }
        assert_eq!(count, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_subscription_hands_out_only_what_is_announced_after_it_was_opened() {
        let (db, federation) = a_federation();
        let earlier = operation(1);
        a_paid_v1_address(&db, earlier, txid(1), 0).await;

        let (_logged, added) = tokio::sync::watch::channel(());
        let subscription = Subscription::starting_at(federation.clone(), 1, added);
        let waited =
            tokio::time::timeout(std::time::Duration::from_millis(200), subscription.next()).await;
        assert!(waited.is_err(), "nothing was announced since it was opened");
        // The deposit announced before the subscription was opened was never its to hand out,
        // and has no record until something lists what the wallet found.
        assert_eq!(record(&db, earlier).await, None);

        let later = operation(2);
        a_paid_v1_address(&db, later, txid(2), 1).await;
        let deposit = subscription.next().await.expect("a deposit");
        assert_eq!(deposit.id(), crate::OperationId::from_upstream(later));
        assert_eq!(record(&db, earlier).await, None);
    }

    /// A call already waiting is woken by the announcement, not only answered by the next call.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_waiting_subscription_is_woken_by_the_announcement() {
        let (db, federation) = a_federation();
        let (logged, added) = tokio::sync::watch::channel(());
        let subscription = Arc::new(Subscription::starting_at(federation.clone(), 0, added));

        let waiting = tokio::spawn({
            let subscription = subscription.clone();
            async move { subscription.next().await }
        });
        until_waiting(&subscription).await;
        // Long enough for the call to have walked the empty log and parked.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!waiting.is_finished());

        let id = operation(1);
        a_paid_v1_address(&db, id, txid(1), 0).await;
        logged.send_replace(());

        let deposit = tokio::time::timeout(std::time::Duration::from_secs(10), waiting)
            .await
            .expect("the announcement wakes the wait")
            .expect("the task does not panic")
            .expect("a deposit");
        assert_eq!(deposit.id(), crate::OperationId::from_upstream(id));
    }

    /// A v1 address paid by two outputs of one transaction is announced once per output, under
    /// the one operation and the one transaction. It is one deposit, handed out once.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_deposit_announced_twice_is_handed_out_once() {
        let (db, federation) = a_federation();
        let id = operation(1);
        a_paid_v1_address(&db, id, txid(1), 0).await;
        log(
            &db,
            1,
            v1::fixtures::announcement(id, txid(1), 30_000, LOGGED_AT + 1),
        )
        .await;
        let after = operation(2);
        a_paid_v1_address(&db, after, txid(2), 2).await;

        let (_logged, added) = tokio::sync::watch::channel(());
        let subscription = Subscription::starting_at(federation.clone(), 0, added);
        let first = subscription.next().await.expect("a deposit");
        assert_eq!(first.id(), crate::OperationId::from_upstream(id));
        // The second announcement of the same deposit is passed over for the one after it.
        let second = subscription.next().await.expect("a deposit");
        assert_eq!(second.id(), crate::OperationId::from_upstream(after));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_waiting_subscription_ends_when_the_federation_stops_running() {
        let (_db, federation) = a_federation();
        let (_logged, added) = tokio::sync::watch::channel(());
        let subscription = Arc::new(Subscription::starting_at(federation.clone(), 0, added));

        let waiting = tokio::spawn({
            let subscription = subscription.clone();
            async move { subscription.next().await }
        });
        until_waiting(&subscription).await;

        federation.set_status(FederationStatus::Closed);
        let err = tokio::time::timeout(std::time::Duration::from_secs(10), waiting)
            .await
            .expect("the wait ends")
            .expect("the task does not panic")
            .expect_err("a closed federation has no next deposit");
        assert_eq!(err.code, ErrorCode::FederationClosed);
        // And it stays that way.
        let err = subscription.next().await.expect_err("still closed");
        assert_eq!(err.code, ErrorCode::FederationClosed);
    }
}
