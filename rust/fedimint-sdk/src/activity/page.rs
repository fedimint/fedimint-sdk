//! The page walk behind `Federation::activity`: the index, the records, the rows.

use std::sync::Arc;

use fedimint_core::core::OperationId as UpstreamOperationId;
use fedimint_core::db::IDatabaseTransactionOpsCoreTyped;
use futures::StreamExt;

use super::rows::{Figures, Row};
use crate::db::{OperationIndexKey, OperationIndexKeyPrefix, OperationRecord, OperationRecordKey};
use crate::federation::FederationInner;
use crate::operation::{AnyOperation, Driver, ErasedDriver, Operation, OperationInner, driver_for};
use crate::{
    ActivityItem, ActivityPage, ActivityStatus, Cursor, Error, ErrorCode, FederationId,
    OperationId, OperationKind, OperationSupport, Result, Timestamp,
};

/// One page of a federation's activity, newest first.
///
/// # Errors
///
/// [`FederationClosed`](ErrorCode::FederationClosed), [`InvalidInput`](ErrorCode::InvalidInput)
/// for a zero `limit` or a cursor another federation issued, and
/// [`Storage`](ErrorCode::Storage).
pub(crate) async fn page(
    federation: &Arc<FederationInner>,
    cursor: Option<Cursor>,
    limit: u16,
) -> Result<ActivityPage> {
    federation.ensure_open()?;
    if limit == 0 {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "limit must be at least 1",
        ));
    }
    if let Some(cursor) = &cursor {
        let issuer = FederationId::from_upstream(federation.id);
        if cursor.federation() != &issuer {
            return Err(Error::new(
                ErrorCode::InvalidInput,
                "this cursor was issued by another federation",
            ));
        }
    }
    // The boundary the walk skips past: everything at or before it belongs to a page already
    // served.
    let boundary = cursor
        .as_ref()
        .map(|cursor| (cursor.created_at(), cursor.id().upstream()));

    let db = federation.db();
    let mut dbtx = db.begin_transaction_nc().await;
    let mut entries = dbtx
        .find_by_prefix_sorted_descending(&OperationIndexKeyPrefix)
        .await;
    let mut keys: Vec<OperationIndexKey> = Vec::new();
    while let Some((key, ())) = entries.next().await {
        if boundary.is_some_and(|boundary| (key.created_at, key.id) >= boundary) {
            continue;
        }
        keys.push(key);
        // One key beyond `limit`, if the walk gets that far, only says a next page exists; it
        // is never read as a row.
        if keys.len() == usize::from(limit) + 1 {
            break;
        }
    }
    drop(entries);
    drop(dbtx);

    let has_more = keys.len() > usize::from(limit);
    keys.truncate(usize::from(limit));

    let mut dbtx = db.begin_transaction_nc().await;
    let mut records = Vec::with_capacity(keys.len());
    for key in keys {
        match dbtx.get_value(&OperationRecordKey(key.id)).await {
            Some(record) => records.push((key.id, record)),
            // The index and its record are written together (`FederationInner::write_record`),
            // so this never happens; a missing row is a better failure than a panic.
            None => tracing::warn!(
                target: "fedimint_sdk",
                federation = %federation.id,
                operation = %key.id.fmt_full(),
                "an activity index entry has no record",
            ),
        }
    }
    drop(dbtx);

    // The rows of one page are built concurrently rather than one at a time: a row still in
    // flight costs its driver's current-state read, half a second on the lightning drivers, and
    // a page pays that once rather than once per row.
    let items = futures::future::join_all(
        records
            .iter()
            .map(|(id, record)| row(federation, *id, record.clone())),
    )
    .await
    .into_iter()
    .collect::<Result<Vec<_>>>()?;

    let next = has_more
        .then(|| records.last())
        .flatten()
        .map(|(id, record)| {
            Cursor::new(
                FederationId::from_upstream(federation.id),
                record.created_at,
                OperationId::from_upstream(*id),
            )
        });

    Ok(ActivityPage { items, next })
}

/// The row for one record, brought up to date if its ending is not recorded yet.
async fn row(
    federation: &Arc<FederationInner>,
    id: UpstreamOperationId,
    record: OperationRecord,
) -> Result<ActivityItem> {
    let inner = Arc::new(OperationInner {
        federation: federation.clone(),
        id,
        record,
    });
    let any = AnyOperation::from_record(inner.clone());
    let kind = any.kind();

    let driver = (any.support() == OperationSupport::Observable)
        .then(|| driver_for(&inner.record.kind))
        .flatten();
    let Some(driver) = driver else {
        return Ok(unreadable(id, &inner.record, kind));
    };

    match driver {
        ErasedDriver::EcashSend(driver) => typed(inner, driver, kind).await,
        ErasedDriver::EcashReceive(driver) => typed(inner, driver, kind).await,
        ErasedDriver::LnSend(driver) => typed(inner, driver, kind).await,
        ErasedDriver::LnReceive(driver) => typed(inner, driver, kind).await,
        ErasedDriver::OnchainSend(driver) => typed(inner, driver, kind).await,
        ErasedDriver::OnchainReceive(driver) => typed(inner, driver, kind).await,
        ErasedDriver::Recovery(driver) => typed(inner, driver, kind).await,
    }
}

/// The row for a record this build cannot observe as a typed state: an unrecognised kind, a
/// state schema newer than this build reads, or a kind with no driver yet.
fn unreadable(
    id: UpstreamOperationId,
    record: &OperationRecord,
    kind: OperationKind,
) -> ActivityItem {
    ActivityItem {
        operation_id: OperationId::from_upstream(id),
        kind,
        time: Timestamp::from_epoch_millis(record.created_at),
        amount: None,
        fee: None,
        direction: None,
        status: ActivityStatus::Unknown,
        is_final: record.final_state.is_some(),
    }
}

/// The row for one observable record, through the driver its kind registered.
async fn typed<S>(
    inner: Arc<OperationInner>,
    driver: Arc<dyn Driver<S>>,
    kind: OperationKind,
) -> Result<ActivityItem>
where
    S: Row,
{
    // A record whose details cannot be read still has a time, a kind and an outcome.
    let figures = S::figures(driver.as_ref(), &inner.record.details).unwrap_or_else(|err| {
        tracing::warn!(
            target: "fedimint_sdk",
            operation = %inner.id.fmt_full(),
            error = %err,
            "could not read this operation's details",
        );
        Figures::NONE
    });

    let (status, is_final) = match &inner.record.final_state {
        Some(encoded) => match driver.decode_state(encoded) {
            Ok(state) if state.is_final() => (state.bucket(), true),
            // Either the decode failed, or it produced a state that is not final, which is a
            // corrupt record either way: this build cannot say how it turned out, but the
            // record does say it is finished.
            _ => (ActivityStatus::Unknown, true),
        },
        None => match Operation::attach(inner.clone(), driver.clone())
            .state()
            .await
        {
            Ok(state) => (state.bucket(), state.is_final()),
            Err(err) if matches!(err.code, ErrorCode::FederationClosed | ErrorCode::Storage) => {
                return Err(err);
            }
            // An upstream state this build's driver cannot decode (fedimint/fedimint#8969 on
            // the v1 module today) must not make the whole list unreadable: the record still
            // says what it knows.
            Err(err) => {
                tracing::warn!(
                    target: "fedimint_sdk",
                    operation = %inner.id.fmt_full(),
                    error = %err,
                    "could not refresh this operation's current state",
                );
                (ActivityStatus::Pending, false)
            }
        },
    };

    // `Unknown` implies no figures, enforced here once rather than by every branch above
    // remembering to clear them.
    let figures = if status == ActivityStatus::Unknown {
        Figures::NONE
    } else {
        figures
    };

    Ok(ActivityItem {
        operation_id: OperationId::from_upstream(inner.id),
        kind,
        time: Timestamp::from_epoch_millis(inner.record.created_at),
        amount: figures.amount,
        fee: figures.fee,
        direction: figures.direction,
        status,
        is_final,
    })
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use fedimint_core::db::Database;
    use fedimint_core::util::{BoxFuture, BoxStream};

    use super::*;
    use crate::db::{federation_namespace, in_memory_root};
    use crate::lightning::fixtures::{
        receive_details, receive_details_json, send_details, send_details_json,
    };
    use crate::lightning::{LnReceiveDriver, LnSendDriver};
    use crate::operation::kinds;
    use crate::{Amount, Direction, EcashSendState, LnReceiveState, LnSendState};

    /// An operation id with the given last byte, the rest zero, so a test can build several
    /// distinct ids by a small number alone.
    fn id_bytes(n: u8) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[31] = n;
        bytes
    }

    /// The public [`OperationId`] for [`id_bytes`], to compare against a row's
    /// [`ActivityItem::operation_id`].
    fn opid(n: u8) -> OperationId {
        OperationId::from_upstream(UpstreamOperationId(id_bytes(n)))
    }

    /// Writes one operation's record and its index entry together, mirroring
    /// `FederationInner::write_record`.
    #[expect(clippy::too_many_arguments, reason = "a test fixture, not API")]
    async fn write(
        db: &Database,
        id: [u8; 32],
        kind: &str,
        module: &str,
        created_at: u64,
        details: String,
        final_state: Option<String>,
        schema_version: u32,
    ) {
        let id = UpstreamOperationId(id);
        let record = OperationRecord {
            schema_version,
            kind: kind.to_owned(),
            module: module.to_owned(),
            created_at,
            details,
            phase: None,
            cancel_requested_at: None,
            final_state,
        };
        let mut dbtx = db.begin_transaction().await;
        dbtx.insert_entry(&OperationRecordKey(id), &record).await;
        dbtx.insert_entry(&OperationIndexKey { created_at, id }, &())
            .await;
        dbtx.commit_tx().await;
    }

    fn ids(activity: &ActivityPage) -> Vec<OperationId> {
        activity
            .items
            .iter()
            .map(|item| item.operation_id.clone())
            .collect()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pages_run_newest_first_and_continue_through_the_cursor() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        let details = receive_details_json(&receive_details());
        let claimed = LnReceiveDriver
            .encode_state(&LnReceiveState::Claimed)
            .expect("encode");
        for n in 1..=5u8 {
            write(
                &db,
                id_bytes(n),
                kinds::LN_RECEIVE,
                "lnv2",
                u64::from(n),
                details.clone(),
                Some(claimed.clone()),
                1,
            )
            .await;
        }

        let first = page(&federation, None, 2).await.expect("first page");
        assert_eq!(ids(&first), vec![opid(5), opid(4)]);
        let cursor = first.next.expect("three rows remain");

        let second = page(&federation, Some(cursor), 2)
            .await
            .expect("second page");
        assert_eq!(ids(&second), vec![opid(3), opid(2)]);
        let cursor = second.next.expect("one row remains");

        let third = page(&federation, Some(cursor), 2).await.expect("last page");
        assert_eq!(ids(&third), vec![opid(1)]);
        assert!(third.next.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn two_rows_created_in_the_same_millisecond_are_ordered_by_id_and_never_repeated() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        let details = receive_details_json(&receive_details());
        let claimed = LnReceiveDriver
            .encode_state(&LnReceiveState::Claimed)
            .expect("encode");
        for n in 1..=3u8 {
            write(
                &db,
                id_bytes(n),
                kinds::LN_RECEIVE,
                "lnv2",
                1_000,
                details.clone(),
                Some(claimed.clone()),
                1,
            )
            .await;
        }

        let mut seen = Vec::new();
        let mut cursor = None;
        for _ in 0..3 {
            let one_page = page(&federation, cursor, 1).await.expect("a page");
            assert_eq!(one_page.items.len(), 1);
            seen.push(one_page.items[0].operation_id.clone());
            cursor = one_page.next;
        }
        assert!(cursor.is_none());
        // The index orders a tie on `created_at` by descending id, the same as the walk orders
        // everything else.
        assert_eq!(seen, vec![opid(3), opid(2), opid(1)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_zero_limit_is_refused() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db, true);
        let err = page(&federation, None, 0)
            .await
            .expect_err("zero is not a valid limit");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_cursor_from_another_federation_is_refused() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db, true);
        let other = "00".repeat(32).parse::<FederationId>().expect("valid");
        let cursor = Cursor::new(other, 0, opid(1));

        let err = page(&federation, Some(cursor), 1)
            .await
            .expect_err("a cursor from another federation is refused");
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_closed_federation_says_so() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db, false);
        let err = page(&federation, None, 1)
            .await
            .expect_err("a closed federation refuses to page");
        assert_eq!(err.code, ErrorCode::FederationClosed);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_recorded_ending_is_read_from_the_record() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        let details = send_details();
        let success = LnSendState::Success {
            preimage: "11".repeat(32).parse().expect("a preimage"),
            fee: Amount::from_msats(1_000),
            route: details.route.clone(),
        };
        let encoded = LnSendDriver.encode_state(&success).expect("encode");
        write(
            &db,
            id_bytes(1),
            kinds::LN_SEND,
            "lnv2",
            1_700_000_000_000,
            send_details_json(&details),
            Some(encoded),
            1,
        )
        .await;

        // No client exists on a detached federation, so a successful call here also proves no
        // refresh was attempted: reading the current state would have failed with
        // `FederationClosed`.
        let activity = page(&federation, None, 10)
            .await
            .expect("no refresh needed");
        assert_eq!(activity.items.len(), 1);
        let row = &activity.items[0];
        assert_eq!(row.status, ActivityStatus::Success);
        assert!(row.is_final);
        assert_eq!(row.amount, Some(Amount::from_msats(100_000)));
        assert_eq!(row.fee, Some(Amount::from_msats(1_000)));
        assert_eq!(row.direction, Some(Direction::Outgoing));
        assert_eq!(row.kind, OperationKind::LnSend);
        assert_eq!(row.time, Timestamp::from_epoch_millis(1_700_000_000_000));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn every_lightning_ending_lands_in_its_bucket() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        let send_details = send_details_json(&send_details());
        let receive_details = receive_details_json(&receive_details());

        let refunded = LnSendDriver
            .encode_state(&LnSendState::Refunded)
            .expect("encode");
        write(
            &db,
            id_bytes(1),
            kinds::LN_SEND,
            "lnv2",
            1,
            send_details.clone(),
            Some(refunded),
            1,
        )
        .await;

        let failed = LnSendDriver
            .encode_state(&LnSendState::Failed {
                reason: "gone".to_owned(),
            })
            .expect("encode");
        write(
            &db,
            id_bytes(2),
            kinds::LN_SEND,
            "lnv2",
            2,
            send_details,
            Some(failed),
            1,
        )
        .await;

        let claimed = LnReceiveDriver
            .encode_state(&LnReceiveState::Claimed)
            .expect("encode");
        write(
            &db,
            id_bytes(3),
            kinds::LN_RECEIVE,
            "lnv2",
            3,
            receive_details.clone(),
            Some(claimed),
            1,
        )
        .await;

        let canceled = LnReceiveDriver
            .encode_state(&LnReceiveState::Canceled {
                reason: "withdrawn".to_owned(),
            })
            .expect("encode");
        write(
            &db,
            id_bytes(4),
            kinds::LN_RECEIVE,
            "lnv2",
            4,
            receive_details.clone(),
            Some(canceled),
            1,
        )
        .await;

        let expired = LnReceiveDriver
            .encode_state(&LnReceiveState::Expired)
            .expect("encode");
        write(
            &db,
            id_bytes(5),
            kinds::LN_RECEIVE,
            "lnv2",
            5,
            receive_details.clone(),
            Some(expired),
            1,
        )
        .await;

        let failed_receive = LnReceiveDriver
            .encode_state(&LnReceiveState::Failed)
            .expect("encode");
        write(
            &db,
            id_bytes(6),
            kinds::LN_RECEIVE,
            "lnv2",
            6,
            receive_details,
            Some(failed_receive),
            1,
        )
        .await;

        let activity = page(&federation, None, 10).await.expect("a page");
        let by_id = |n: u8| {
            activity
                .items
                .iter()
                .find(|item| item.operation_id == opid(n))
                .unwrap_or_else(|| panic!("operation {n} was not listed"))
        };
        assert_eq!(by_id(1).status, ActivityStatus::Refunded);
        assert_eq!(by_id(2).status, ActivityStatus::Failed);
        let claimed_row = by_id(3);
        assert_eq!(claimed_row.status, ActivityStatus::Success);
        assert_eq!(claimed_row.direction, Some(Direction::Incoming));
        assert_eq!(claimed_row.amount, Some(Amount::from_msats(100_000)));
        assert_eq!(claimed_row.fee, Some(Amount::from_msats(500)));
        assert_eq!(by_id(4).status, ActivityStatus::Canceled);
        assert_eq!(by_id(5).status, ActivityStatus::Canceled);
        assert_eq!(by_id(6).status, ActivityStatus::Failed);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_ending_this_build_cannot_read_is_unknown_and_final() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        write(
            &db,
            id_bytes(1),
            kinds::LN_SEND,
            "lnv2",
            1,
            send_details_json(&send_details()),
            Some(r#"{"Settled":{}}"#.to_owned()),
            1,
        )
        .await;

        let activity = page(&federation, None, 10).await.expect("a page");
        assert_eq!(activity.items.len(), 1);
        let row = &activity.items[0];
        assert_eq!(row.status, ActivityStatus::Unknown);
        assert!(row.is_final);
        assert_eq!(row.amount, None);
        assert_eq!(row.fee, None);
        assert_eq!(row.direction, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_record_of_an_unknown_kind_is_listed_without_numbers() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        write(
            &db,
            id_bytes(1),
            "widget",
            "widget",
            1,
            "{}".to_owned(),
            None,
            1,
        )
        .await;
        write(
            &db,
            id_bytes(2),
            "widget",
            "widget",
            2,
            "{}".to_owned(),
            Some("done".to_owned()),
            1,
        )
        .await;

        let activity = page(&federation, None, 10).await.expect("a page");
        let running = activity
            .items
            .iter()
            .find(|item| item.operation_id == opid(1))
            .expect("listed");
        let finished = activity
            .items
            .iter()
            .find(|item| item.operation_id == opid(2))
            .expect("listed");

        for row in [running, finished] {
            assert_eq!(row.kind, OperationKind::Unknown);
            assert_eq!(row.status, ActivityStatus::Unknown);
            assert_eq!(row.amount, None);
            assert_eq!(row.fee, None);
            assert_eq!(row.direction, None);
        }
        assert!(!running.is_final);
        assert!(finished.is_final);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_state_schema_newer_than_this_build_is_unknown() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        let details = send_details();
        let success = LnSendState::Success {
            preimage: "11".repeat(32).parse().expect("a preimage"),
            fee: Amount::from_msats(1_000),
            route: details.route.clone(),
        };
        let encoded = LnSendDriver.encode_state(&success).expect("encode");
        write(
            &db,
            id_bytes(1),
            kinds::LN_SEND,
            "lnv2",
            1,
            send_details_json(&details),
            Some(encoded),
            2,
        )
        .await;

        let activity = page(&federation, None, 10).await.expect("a page");
        assert_eq!(activity.items.len(), 1);
        let row = &activity.items[0];
        assert_eq!(row.kind, OperationKind::LnSend);
        assert_eq!(row.status, ActivityStatus::Unknown);
        assert!(row.is_final);
        assert_eq!(row.amount, None);
        assert_eq!(row.fee, None);
        assert_eq!(row.direction, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_row_with_no_recorded_ending_is_refreshed_through_its_driver() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        write(
            &db,
            id_bytes(1),
            kinds::ECASH_SEND,
            "mint",
            1,
            String::new(),
            None,
            1,
        )
        .await;

        let activity = page(&federation, None, 10).await.expect("a page");
        assert_eq!(activity.items.len(), 1);
        let row = &activity.items[0];
        assert_eq!(row.status, ActivityStatus::Success);
        assert!(row.is_final);
        // The probe driver has no details to decode, so the outcome is known but the figures
        // are not: this is the details-unreadable path, not the `Unknown`-status one.
        assert_eq!(row.amount, None);
        assert_eq!(row.fee, None);
        assert_eq!(row.direction, None);

        let mut dbtx = db.begin_transaction_nc().await;
        let record = dbtx
            .get_value(&OperationRecordKey(UpstreamOperationId(id_bytes(1))))
            .await
            .expect("the record still exists");
        assert!(
            record.final_state.is_some(),
            "the refresh recorded the final state it observed"
        );
    }

    /// A driver whose `current` answers a chosen error, so a refresh can be made to fail in a
    /// chosen way without a real client.
    struct FailingDriver {
        error: ErrorCode,
    }

    impl Driver<EcashSendState> for FailingDriver {
        fn current<'a>(
            &'a self,
            _federation: &'a FederationInner,
            _id: UpstreamOperationId,
            _record: &'a OperationRecord,
        ) -> BoxFuture<'a, Result<EcashSendState>> {
            Box::pin(async move { Err(Error::new(self.error, "chosen by the test")) })
        }

        fn subscribe<'a>(
            &'a self,
            _federation: &'a FederationInner,
            _id: UpstreamOperationId,
            _record: &'a OperationRecord,
        ) -> BoxFuture<'a, Result<BoxStream<'static, Result<EcashSendState>>>> {
            Box::pin(async move { Err(Error::new(self.error, "chosen by the test")) })
        }

        fn same_state(&self, previous: &EcashSendState, next: &EcashSendState) -> bool {
            previous == next
        }

        fn encode_state(&self, _state: &EcashSendState) -> Result<String> {
            Err(Error::new(ErrorCode::Internal, "not exercised"))
        }

        fn decode_state(&self, _encoded: &str) -> Result<EcashSendState> {
            Err(Error::new(ErrorCode::Internal, "not exercised"))
        }

        fn decode_details(&self, _json: &str) -> Result<Box<dyn Any + Send + Sync>> {
            Err(Error::new(
                ErrorCode::Internal,
                "the failing driver has no details",
            ))
        }
    }

    fn a_pending_ecash_send(federation: &Arc<FederationInner>) -> Arc<OperationInner> {
        Arc::new(OperationInner {
            federation: federation.clone(),
            id: UpstreamOperationId(id_bytes(1)),
            record: OperationRecord {
                schema_version: 1,
                kind: kinds::ECASH_SEND.to_owned(),
                module: "mint".to_owned(),
                created_at: 1,
                details: String::new(),
                phase: None,
                cancel_requested_at: None,
                final_state: None,
            },
        })
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_refresh_that_fails_leaves_the_row_at_what_its_record_says() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        // `Operation::state` reloads the record before calling the driver, so the id it is
        // called for must actually have one.
        write(
            &db,
            id_bytes(1),
            kinds::ECASH_SEND,
            "mint",
            1,
            String::new(),
            None,
            1,
        )
        .await;
        let inner = a_pending_ecash_send(&federation);

        let item = typed(
            inner.clone(),
            Arc::new(FailingDriver {
                error: ErrorCode::Internal,
            }) as Arc<dyn Driver<EcashSendState>>,
            OperationKind::EcashSend,
        )
        .await
        .expect("an unrecognised refresh error still yields a row");
        assert_eq!(item.status, ActivityStatus::Pending);
        assert!(!item.is_final);

        let err = typed(
            inner.clone(),
            Arc::new(FailingDriver {
                error: ErrorCode::Storage,
            }) as Arc<dyn Driver<EcashSendState>>,
            OperationKind::EcashSend,
        )
        .await
        .expect_err("a storage failure propagates");
        assert_eq!(err.code, ErrorCode::Storage);

        let err = typed(
            inner,
            Arc::new(FailingDriver {
                error: ErrorCode::FederationClosed,
            }) as Arc<dyn Driver<EcashSendState>>,
            OperationKind::EcashSend,
        )
        .await
        .expect_err("a federation-closed failure propagates");
        assert_eq!(err.code, ErrorCode::FederationClosed);
    }
}
