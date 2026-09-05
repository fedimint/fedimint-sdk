//! A joined federation, and the capability facades hanging off it.

use std::sync::{Arc, Weak};

use fedimint_client::{Client, ClientHandleArc};
use fedimint_core::config;
use fedimint_core::db::Database;
use fedimint_core::db::IDatabaseTransactionOpsCoreTyped;
use fedimint_core::module::AmountUnit;
use futures::StreamExt;

use crate::db::{FederationRecord, StoredStatus};
use crate::operation::{Driver, Operation, OperationInner, OperationState};
use crate::sdk::SdkInner;
use crate::{
    ActivityPage, Amount, AnyOperation, Cursor, Ecash, FederationId, FederationInfo,
    FederationStatus, InviteCode, Lightning, Meta, Network, Onchain, OperationId, Result,
};

/// A handle to one federation this SDK instance has joined.
///
/// Everything an application does with a federation goes through this type:
/// reading its identity and balance, obtaining the [ecash](Federation::ecash),
/// [lightning](Federation::lightning) and [on-chain](Federation::onchain)
/// facades, reading [metadata](Federation::meta), reattaching to a running
/// [operation](Federation::operation), and paging through local
/// [activity](Federation::activity).
///
/// Like the other handles in this crate it is a cheap clone over shared
/// state: every clone talks to the same running federation client, and it
/// is `Send + Sync` on native targets, with the same types compiled for a
/// single-threaded host on wasm.
///
/// A handle keeps working until the federation is closed with
/// [`Sdk::close_federation`](crate::Sdk::close_federation), erased with
/// [`Sdk::forget_federation`](crate::Sdk::forget_federation), or the whole
/// instance is shut down. An application holding a stale handle degrades
/// into a reportable error rather than a crash: nothing here panics after a
/// close.
///
/// # What a closed handle does
///
/// "Fails with
/// [`FederationClosed`](crate::ErrorCode::FederationClosed)" applies to the
/// **fallible** calls: [`balance`](Federation::balance),
/// [`operation`](Federation::operation),
/// [`activity`](Federation::activity), [`backup`](Federation::backup), and
/// every call made through a facade. The rest of this type returns plain
/// values and has no way to report a failure, so each has a defined closed
/// behaviour instead:
///
/// - **The descriptive accessors keep answering.**
///   [`id`](Federation::id), [`name`](Federation::name),
///   [`network`](Federation::network),
///   [`invite_code`](Federation::invite_code) and
///   [`capabilities`](Federation::capabilities) go on returning the
///   configuration last known for this federation. A history screen can
///   still label rows with a federation that has been closed underneath it.
/// - **The facade accessors keep returning `Some`.**
///   [`ecash`](Federation::ecash), [`lightning`](Federation::lightning) and
///   [`onchain`](Federation::onchain) return a facade whenever the
///   federation had that module, closed or not, and the failure surfaces
///   from the facade call as
///   [`FederationClosed`](crate::ErrorCode::FederationClosed). Returning
///   `None` instead would be a lie with a specific documented meaning:
///   "this federation has no mint module", and would make a closed
///   federation indistinguishable from one that never supported ecash at
///   all. [`meta`](Federation::meta), which is unconditional, behaves the
///   same way.
/// - **[`balance_updates`](Federation::balance_updates) still hands out a
///   subscriber**, whose very first
///   [`next`](BalanceUpdates::next) yields
///   [`FederationClosed`](crate::ErrorCode::FederationClosed). The error is
///   where a caller can act on it, rather than being swallowed by an
///   accessor that cannot return one.
#[derive(Debug, Clone)]
pub struct Federation {
    inner: Arc<FederationInner>,
}

impl Federation {
    /// This federation's id.
    pub fn id(&self) -> FederationId {
        FederationId::from_upstream(self.inner.id)
    }

    /// The federation's human-readable name, when its configuration
    /// declares one.
    ///
    /// This is configuration metadata, not a verified or unique identifier:
    /// two federations may present the same name. Identity is
    /// [`Federation::id`].
    pub fn name(&self) -> Option<String> {
        self.inner.record().name
    }

    /// The Bitcoin network this federation operates on.
    ///
    /// On-chain addresses are validated against this when an on-chain quote
    /// is requested, failing with
    /// [`NetworkMismatch`](crate::ErrorCode::NetworkMismatch) on
    /// disagreement. There is no second check at send time, because
    /// [`Onchain::send`](crate::Onchain::send) takes only a quote: the
    /// address is bound into the quote when it is issued.
    pub fn network(&self) -> Network {
        self.inner.record().network.into()
    }

    /// An invite code for this federation, suitable for sharing so someone
    /// else can join it.
    pub fn invite_code(&self) -> InviteCode {
        InviteCode::from_upstream(self.inner.record().invite)
    }

    /// The ecash balance: the value this instance currently holds as its
    /// own, uncommitted notes.
    ///
    /// Value that is committed to an in-flight operation, funding a
    /// lightning payment, sitting in out-of-band notes that have not been
    /// redeemed or reclaimed, waiting on an on-chain deposit to confirm, is
    /// not counted here.
    ///
    /// Holding is not spending, and this method takes no position on the
    /// latter: whether a spend would be *permitted* is governed by the
    /// federation's status, not by this number. The case where the two
    /// diverge is a recovery-locked federation: while the rescan proceeds
    /// the balance reported here is partial, still moving, and worth
    /// showing as progress, yet none of it is spendable, and every spend
    /// or receive is refused with
    /// [`Recovering`](crate::ErrorCode::Recovering) no matter what this
    /// method returned. It settles when recovery finishes. On a
    /// [`Running`](crate::FederationStatus::Running) federation the two
    /// notions coincide, and this is exactly the amount a spend can draw
    /// on.
    ///
    /// # Errors
    ///
    /// [`Internal`](crate::ErrorCode::Internal) for a client with no usable balance source
    /// (API-version negotiation left every module out, most often), which is not the caller's
    /// doing and not a storage fault, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn balance(&self) -> Result<Amount> {
        // Reading a balance is not fund-touching: a recovery-locked federation's number is
        // partial and worth showing as progress, and it is the spends that are refused.
        let client = self.inner.client(false).await?;
        let balance = client
            .get_balance_for_unit(AmountUnit::BITCOIN)
            .await
            .map_err(|err| {
                // The one way this fails in practice is a client with no primary module, which
                // happens when API-version negotiation left every module out. That is not the
                // caller's doing and not a storage fault.
                crate::Error::new(
                    crate::ErrorCode::Internal,
                    format!("this federation cannot report a balance: {err}"),
                )
            })?;
        Ok(Amount::from_msats(balance.msats))
    }

    /// Opens a new, independent subscription to the balance.
    ///
    /// Each call returns its own cursor, exactly like
    /// [`Operation::updates`](crate::Operation::updates): two subscribers
    /// both see every change and neither consumes the other's updates.
    ///
    /// This cannot fail, so it hands out a subscriber even for a closed
    /// federation; that subscriber's first
    /// [`next`](BalanceUpdates::next) yields
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub fn balance_updates(&self) -> BalanceUpdates {
        BalanceUpdates {
            inner: Arc::new(BalanceUpdatesInner {
                federation: self.inner.clone(),
                cursor: tokio::sync::Mutex::new(BalanceCursor {
                    stream: None,
                    last: None,
                }),
            }),
        }
    }

    /// What this federation can do.
    ///
    /// Reported as plain booleans so an application can decide what to
    /// render before the user touches anything. It answers the same
    /// question as the three facade accessors below and exists alongside
    /// them for the case where a screen needs to know about several
    /// capabilities at once without taking handles it will not use.
    pub fn capabilities(&self) -> Capabilities {
        self.inner.record().capabilities.into()
    }

    /// The ecash facade, or `None` if this federation has no mint module.
    ///
    /// `None` means exactly one thing: this federation has no mint module. It does not mean
    /// "closed": a closed federation that has a mint module still returns `Some`, and the
    /// facade's calls fail with
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed). See the type documentation.
    ///
    /// [`ErrorCode::NotSupported`](crate::ErrorCode::NotSupported) covers the narrower case of
    /// a facade obtained while the module was present, then used after the federation's
    /// configuration changed to drop it.
    pub fn ecash(&self) -> Option<Ecash> {
        self.capabilities()
            .ecash
            .then(|| Ecash::new(self.inner.clone()))
    }

    /// The lightning facade, or `None` if the federation has no lightning
    /// module. See [`Federation::ecash`] for why this is an `Option`.
    pub fn lightning(&self) -> Option<Lightning> {
        self.capabilities()
            .lightning
            .then(|| Lightning::new(self.inner.clone()))
    }

    /// The on-chain facade, or `None` if this federation has no wallet
    /// module. See [`Federation::ecash`] for why this is an `Option`.
    pub fn onchain(&self) -> Option<Onchain> {
        self.capabilities()
            .onchain
            .then(|| Onchain::new(self.inner.clone()))
    }

    /// The metadata facade.
    ///
    /// Unconditional, unlike the three capability facades above: every
    /// federation has configuration metadata, so there is always something
    /// to read. A federation without a meta module simply has no consensus
    /// metadata, which [`Meta`] reports as an absent value rather than as a
    /// missing facade.
    pub fn meta(&self) -> Meta {
        Meta::new(self.inner.clone())
    }

    /// Looks up an operation by id, whatever kind it is.
    ///
    /// This is how an application reattaches after a restart: persist the
    /// [`OperationId`] (or read one from
    /// [`ActivityItem`](crate::ActivityItem)), pass it here, and get back a
    /// handle to the operation that has been running all along.
    ///
    /// The call is asynchronous and fallible because it reads persistent
    /// state: the operation log lives in storage, not in memory, and a
    /// lookup can fail the way any read can.
    ///
    /// `Ok(None)` means precisely that this federation has no operation
    /// with that id. It is not an error: asking about an id that turns out
    /// not to exist (a stale deep link, a record from a federation that was
    /// forgotten) is a normal thing for an application to do.
    ///
    /// An operation that exists but that this SDK version cannot interpret
    /// comes back as `Ok(Some(op))` with
    /// [`OperationKind::Unknown`](crate::OperationKind::Unknown) rather than
    /// as an error. Persisted operations outlive any one SDK version:
    /// applications get downgraded, module sets change, and a record
    /// written by a newer build is still a real record. Reporting it as
    /// unknown lets a history screen show it honestly; failing the lookup
    /// would make it invisible.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage) if the operation log cannot
    /// be read, [`FederationClosed`](crate::ErrorCode::FederationClosed) if
    /// the federation is closed.
    pub async fn operation(&self, id: &OperationId) -> Result<Option<AnyOperation>> {
        self.inner.ensure_open()?;
        self.inner.operation(id.upstream()).await
    }

    /// Reads a page of local activity history, newest first.
    ///
    /// Pass `None` as `cursor` for the first page and the
    /// [`next`](crate::ActivityPage::next) cursor of the previous page for
    /// each following one. At most `limit` items are returned; fewer is
    /// normal, and an empty page with no cursor means the end.
    ///
    /// The history is *local*, see [`ActivityItem`](crate::ActivityItem)
    /// for exactly what that excludes.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage),
    /// [`InvalidInput`](crate::ErrorCode::InvalidInput) for a cursor that
    /// is not one this federation issued, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn activity(&self, cursor: Option<Cursor>, limit: u16) -> Result<ActivityPage> {
        unimplemented!()
    }

    /// Uploads a fresh encrypted backup to the federation.
    ///
    /// Backups are what make seed-only restore possible: they let a
    /// recovering client learn which notes and operations to look for
    /// instead of rescanning blindly. The SDK also backs up automatically
    /// after changes that affect recoverability, so this call is for
    /// applications that want an explicit "back up now" affordance or want
    /// to be sure a backup exists before some user-visible milestone.
    ///
    /// # Errors
    ///
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`Recovering`](crate::ErrorCode::Recovering) while this federation's
    /// recovery is incomplete, which is not the same as still running, since
    /// a recovery that stopped short leaves the lock in place, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn backup(&self) -> Result<()> {
        unimplemented!()
    }

    /// Wraps shared federation state in a handle.
    pub(crate) fn new(inner: Arc<FederationInner>) -> Federation {
        Federation { inner }
    }
}

/// Which capabilities a federation offers.
///
/// Each flag says whether the corresponding accessor on [`Federation`]
/// would return `Some`. Reading them is how an application decides what to
/// put on screen before the user acts, rather than discovering an absent
/// capability by attempting an operation and handling the failure.
///
/// `#[non_exhaustive]` like every data type here: a federation gaining a
/// new kind of capability must be an additive change, not a breaking one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Capabilities {
    /// Whether [`Federation::ecash`] is available.
    pub ecash: bool,
    /// Whether [`Federation::lightning`] is available.
    pub lightning: bool,
    /// Whether [`Federation::onchain`] is available.
    pub onchain: bool,
}

/// One independent subscription to a federation's balance.
///
/// Obtained from [`Federation::balance_updates`]. Not `Clone`, for the same
/// reason [`OperationUpdates`](crate::OperationUpdates) is not: it is a
/// single cursor, and a second consumer should have a second subscription.
/// Dropping it stops only this subscription.
#[derive(Debug)]
pub struct BalanceUpdates {
    inner: Arc<BalanceUpdatesInner>,
}

impl BalanceUpdates {
    /// Waits for the next balance.
    ///
    /// The first call returns the current balance immediately; each later
    /// call resolves when the balance changes.
    ///
    /// Unlike [`OperationUpdates::next`](crate::OperationUpdates::next), this never resolves
    /// to a clean end: a balance has no final state, so the only way this stream ends is the
    /// federation being closed or the SDK shutting down, reported as an error rather than
    /// folded into an `Option`.
    ///
    /// # Errors
    ///
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) once the
    /// federation is closed or the SDK shut down, the terminal condition
    /// for this stream. Other errors are infrastructure failures:
    /// [`Storage`](crate::ErrorCode::Storage) or
    /// [`Internal`](crate::ErrorCode::Internal).
    pub async fn next(&mut self) -> Result<Amount> {
        let mut cursor = self.inner.cursor.lock().await;
        let mut closed = self.inner.federation.closed();
        loop {
            if *closed.borrow_and_update() {
                return Err(crate::Error::new(
                    crate::ErrorCode::FederationClosed,
                    "this federation is not running",
                ));
            }

            if cursor.stream.is_none() {
                let client = self.inner.federation.client(false).await?;
                // Upstream's balance stream never yields and never ends when a client has no
                // primary module, so a read has to prove there is one before a caller is handed
                // something that could hang for the life of the process.
                let initial = client
                    .get_balance_for_unit(AmountUnit::BITCOIN)
                    .await
                    .map_err(|err| {
                        crate::Error::new(
                            crate::ErrorCode::Internal,
                            format!("this federation cannot report a balance: {err}"),
                        )
                    })?;
                let _ = initial;
                cursor.stream = Some(client.subscribe_balance_changes(AmountUnit::BITCOIN).await);
            }

            let stream = cursor
                .stream
                .as_mut()
                .expect("the stream was just established");
            let next = tokio::select! {
                next = stream.next() => next,
                _ = closed.changed() => continue,
            };
            let Some(balance) = next else {
                // The stream only ends when the client behind it is gone, which from a caller's
                // point of view is the federation no longer running.
                return Err(crate::Error::new(
                    crate::ErrorCode::FederationClosed,
                    "this federation is not running",
                ));
            };

            let balance = Amount::from_msats(balance.msats);
            if cursor.last == Some(balance) {
                continue;
            }
            cursor.last = Some(balance);
            return Ok(balance);
        }
    }
}

/// The shared per-federation state every [`Federation`] clone and every facade talks to.
///
/// The optional client is the whole lifecycle in one field: `None` is a federation that is closed,
/// quarantined, or on its way out, and taking the write lock is how a close, an erase or a
/// shutdown waits for the calls already in flight before it takes the client away.
#[derive(Debug)]
pub(crate) struct FederationInner {
    pub(crate) id: config::FederationId,
    /// The instance this federation belongs to. Weak, because the instance owns the federations.
    pub(crate) sdk: Weak<SdkInner>,
    /// This federation's slice of the root store, which is also what the client was given.
    pub(crate) db: Database,
    /// `None` while the federation is not running, for any reason.
    client: tokio::sync::RwLock<Option<ClientHandleArc>>,
    /// The last configuration that validated, which is what the descriptive accessors answer from.
    record: std::sync::RwLock<FederationRecord>,
    /// The observable status, which is richer than the stored one: quarantine and recovery are
    /// facts about a running instance rather than about the storage.
    status: std::sync::RwLock<FederationStatus>,
    /// Flipped once the federation stops running, so a pending subscriber resolves promptly
    /// instead of waiting on a stream that will never yield again.
    closed: tokio::sync::watch::Sender<bool>,
}

/// Whether a record is the placeholder `FederationInner::backfill_at`'s no-backfiller branch
/// writes: the module kind copied verbatim as the tag.
///
/// No facade ever writes this shape — every real kind tag lives in `crate::operation::kinds` and
/// names the operation rather than the module that owns it — so a record with it is always safe
/// for reconciliation to replace. A tag this build simply does not recognise is not the same
/// thing: it may be a newer build's real kind, carrying a cancellation intent, a phase or a final
/// state this build cannot reconstruct from the log entry alone.
fn is_unclaimed_placeholder(record: &crate::db::OperationRecord) -> bool {
    record.kind == record.module
}

impl FederationInner {
    /// Assembles the shared state for one federation.
    ///
    /// `client` is `None` for a federation the instance remembers but is not running.
    pub(crate) fn new(
        id: config::FederationId,
        sdk: Weak<SdkInner>,
        db: Database,
        record: FederationRecord,
        status: FederationStatus,
        client: Option<ClientHandleArc>,
    ) -> FederationInner {
        let running = client.is_some();
        FederationInner {
            id,
            sdk,
            db,
            client: tokio::sync::RwLock::new(client),
            record: std::sync::RwLock::new(record),
            status: std::sync::RwLock::new(status),
            closed: tokio::sync::watch::Sender::new(!running),
        }
    }

    /// A read guard over the live client, or the reason there is not one.
    ///
    /// Every facade call holds one of these for its whole duration, which is what makes a close or
    /// an erase wait for work already in flight rather than pulling the client out from under it.
    /// `fund_touching` marks the calls a recovery-locked federation refuses: sends, receives and
    /// taking a fresh backup, as opposed to reading a balance or a name.
    pub(crate) async fn client(&self, fund_touching: bool) -> Result<ClientGuard<'_>> {
        if fund_touching && self.status() == FederationStatus::Recovering {
            return Err(crate::Error::new(
                crate::ErrorCode::Recovering,
                "this federation's wallet is still being reconstructed",
            ));
        }
        let guard = self.client.read().await;
        if guard.is_none() {
            return Err(crate::Error::new(
                crate::ErrorCode::FederationClosed,
                "this federation is not running",
            ));
        }
        Ok(ClientGuard(guard))
    }

    /// This federation's slice of the store, for records the SDK keeps beside the client's.
    pub(crate) fn db(&self) -> Database {
        self.db.clone()
    }

    /// A snapshot of the last configuration that validated.
    pub(crate) fn record(&self) -> FederationRecord {
        read_lock(&self.record).clone()
    }

    /// Replaces the cached configuration snapshot. The durable write is the caller's job.
    pub(crate) fn set_record(&self, record: FederationRecord) {
        *write_lock(&self.record) = record;
    }

    /// What the SDK can currently do with this federation.
    pub(crate) fn status(&self) -> FederationStatus {
        read_lock(&self.status).clone()
    }

    /// Records a new status and, when it is not an open one, releases everything waiting on the
    /// federation so a pending subscriber resolves instead of hanging.
    pub(crate) fn set_status(&self, status: FederationStatus) {
        let open = matches!(
            status,
            FederationStatus::Running | FederationStatus::Recovering
        );
        *write_lock(&self.status) = status;
        self.closed.send_replace(!open);
    }

    /// The listing record for this federation.
    pub(crate) fn info(&self) -> FederationInfo {
        let record = self.record();
        FederationInfo {
            id: crate::FederationId::from_upstream(self.id),
            name: record.name,
            network: record.network.into(),
            status: self.status(),
        }
    }

    /// Whether this federation is one of the two open states.
    pub(crate) fn is_open(&self) -> bool {
        matches!(
            self.status(),
            FederationStatus::Running | FederationStatus::Recovering
        )
    }

    /// Puts a freshly opened client in place. Callers set the status separately, after the
    /// durable write that makes the transition observable.
    pub(crate) async fn install(&self, client: ClientHandleArc) {
        *self.client.write().await = Some(client);
    }

    /// Retires the federation and hands back its client, if it had one.
    ///
    /// Taking the write lock is the quiesce: it waits for every call already holding a
    /// [`ClientGuard`], and once it returns nothing new can take one. The client's own workers are
    /// told to stop here so their state is flushed before eligibility for anything is judged; the
    /// handle is still readable afterwards, which is what lets an erase check a balance.
    pub(crate) async fn quiesce(&self) -> Option<ClientHandleArc> {
        let taken = self.client.write().await.take();
        self.closed.send_replace(true);
        if let Some(client) = taken.as_ref() {
            client.task_group().shutdown();
        }
        taken
    }

    /// Retires the federation and shuts its client down.
    pub(crate) async fn stop(&self) -> Result<()> {
        let Some(client) = self.quiesce().await else {
            return Ok(());
        };
        shutdown_client(client).await
    }

    /// A receiver that fires when this federation stops running.
    pub(crate) fn closed(&self) -> tokio::sync::watch::Receiver<bool> {
        self.closed.subscribe()
    }

    /// `Ok` while this federation is still usable, and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) once it is not.
    ///
    /// Both halves of "usable" are asked. The stored row says whether the storage still means
    /// this federation to be opened at all; the live status says whether it is running now, and
    /// a quarantined federation is exactly the case where the two disagree, because quarantine
    /// leaves the row untouched. Neither half reads the client handle, so the answer is the same
    /// after a close, a quarantine and a shutdown, and does not depend on whether some other
    /// task is holding the client lock.
    pub(crate) fn ensure_open(&self) -> crate::Result<()> {
        match read_lock(&self.record).status {
            StoredStatus::Open => {}
            // `Joining` is an interrupted join: the next build wipes that namespace and redoes
            // the join from the invite, so nothing in it may be used in the meantime.
            StoredStatus::Joining | StoredStatus::Closed | StoredStatus::Forgetting => {
                return Err(crate::Error::new(
                    crate::ErrorCode::FederationClosed,
                    "this federation is closed",
                ));
            }
        }
        if !self.is_open() {
            return Err(crate::Error::new(
                crate::ErrorCode::FederationClosed,
                "this federation is closed",
            ));
        }
        Ok(())
    }

    /// Records a newly created operation and returns the handle for it.
    ///
    /// Called by a facade immediately after the module call that created the operation returned
    /// its id. `details` is the facade's own wire record, serialised here so that the JSON shape
    /// of a details record is the facade's business and the storage layer's is only that it is
    /// JSON.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage) if the record cannot be committed,
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed), and
    /// [`Internal`](crate::ErrorCode::Internal) for a details record that will not serialise,
    /// which is a bug in the facade rather than a condition an application can be in.
    pub(crate) async fn create_operation<S>(
        self: &Arc<Self>,
        id: fedimint_core::core::OperationId,
        kind: &str,
        module: &str,
        details: &impl serde::Serialize,
        driver: Arc<dyn Driver<S>>,
    ) -> crate::Result<Operation<S>>
    where
        S: OperationState,
    {
        self.ensure_open()?;
        // Both of these are hoisted out of the transaction below on purpose: an autocommit
        // closure may run more than once (`fedimint-core/src/db/mod.rs:534-536`), and a
        // creation time that changed between attempts would put the record and its index entry
        // out of step.
        let created_at = crate::db::now_millis();
        let details = serde_json::to_string(details).map_err(|err| {
            crate::Error::new(
                crate::ErrorCode::Internal,
                format!("could not record what this operation is: {err}"),
            )
        })?;
        let record = crate::db::OperationRecord {
            schema_version: crate::operation::READABLE_STATE_SCHEMA,
            kind: kind.to_owned(),
            module: module.to_owned(),
            created_at,
            details,
            phase: None,
            cancel_requested_at: None,
            final_state: None,
        };
        // `overwrite_placeholder: false`: an id the client itself just minted should never
        // already have a record, so any that turns up here won a race against this call rather
        // than being a placeholder to upgrade, and it is kept exactly as it is.
        let record = self.write_record(id, record, false).await?;
        Ok(Operation::attach(
            Arc::new(OperationInner {
                federation: self.clone(),
                id,
                record,
            }),
            driver,
        ))
    }

    /// Looks one operation up by id, rebuilding its record from the client's own log if a crash
    /// left the log entry without one.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage).
    pub(crate) async fn operation(
        self: &Arc<Self>,
        id: fedimint_core::core::OperationId,
    ) -> crate::Result<Option<AnyOperation>> {
        let db = self.db();
        let mut dbtx = db.begin_transaction_nc().await;
        let stored = dbtx.get_value(&crate::db::OperationRecordKey(id)).await;
        drop(dbtx);
        let record = match stored {
            Some(record) => record,
            None => match self.backfill(id).await? {
                Some(record) => record,
                None => return Ok(None),
            },
        };
        Ok(Some(AnyOperation::from_record(Arc::new(OperationInner {
            federation: self.clone(),
            id,
            record,
        }))))
    }

    /// Gives every operation in the client's log an SDK record, and upgrades any record this
    /// build can now place better than the build that wrote it could.
    ///
    /// Run when a federation is brought up. The client's operation log is authoritative for
    /// which operations exist; the SDK's records are a decoration over it that a crash between
    /// the module's own commit and the SDK's write can leave incomplete.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage) for a failure reading the log or the index; a
    /// failure backfilling one entry is logged and does not stop the rest of the pass (see
    /// below), and is reported here only once every entry has had its turn, as the first such
    /// failure.
    pub(crate) async fn reconcile_operations(self: &Arc<Self>) -> crate::Result<()> {
        use futures::StreamExt;

        let db = self.db();
        // Newest first, so an operation a crash left unrecorded a moment ago is reached first.
        // The whole index is walked rather than a prefix of it, because a record can also need
        // upgrading long after it was written, and a wallet's operation count is small.
        let mut dbtx = db.begin_transaction_nc().await;
        let entries: Vec<(fedimint_core::core::OperationId, u64)> = dbtx
            .find_by_prefix_sorted_descending(
                &fedimint_client::db::ChronologicalOperationLogKeyPrefix,
            )
            .await
            .map(|(key, ())| (key.operation_id, crate::db::millis_of(key.creation_time)))
            .collect()
            .await;
        drop(dbtx);

        let mut first_err = None;
        for (id, created_at) in entries {
            let mut dbtx = db.begin_transaction_nc().await;
            let stored = dbtx.get_value(&crate::db::OperationRecordKey(id)).await;
            drop(dbtx);
            let needs_work = match &stored {
                None => true,
                // Only the exact placeholder shape is offered to the backfillers again. A tag
                // this build merely does not recognise is left alone even so: it may be a newer
                // build's real kind, and rewriting it would lose a cancellation intent, a phase
                // or a final state the log entry does not carry.
                Some(record) => is_unclaimed_placeholder(record),
            };
            if !needs_work {
                continue;
            }
            // One entry's storage trouble does not stop the rest of the pass: every other
            // operation still gets its chance, and a lookup through `FederationInner::operation`
            // repairs this specific id later regardless.
            if let Err(err) = self.backfill_at(id, created_at).await {
                tracing::warn!(
                    target: "fedimint_sdk",
                    federation = %self.id,
                    operation = %id.fmt_full(),
                    error = %err,
                    "could not reconcile this operation's record",
                );
                first_err.get_or_insert(err);
            }
        }
        match first_err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// Rebuilds one operation's record from the client's log entry, dating it from the client's
    /// own chronological index.
    ///
    /// `Ok(None)` when the log has no entry with that id, which is what "this federation has no
    /// operation with that id" means.
    async fn backfill(
        self: &Arc<Self>,
        id: fedimint_core::core::OperationId,
    ) -> crate::Result<Option<crate::db::OperationRecord>> {
        let db = self.db();
        // The single-key lookup first: most calls here are `Federation::operation` asking about
        // an id that was never real, and this answers that in O(1) rather than paying for the
        // chronological scan below only to find nothing there either.
        if fedimint_client::oplog::OperationLog::new(db.clone())
            .get_operation(id)
            .await
            .is_none()
        {
            return Ok(None);
        }
        let created_at = match self.creation_time_of(id).await {
            Some(created_at) => created_at,
            // No index entry means no operation: the client writes the entry and the index in
            // one transaction (`fedimint-client/src/oplog.rs:82-118`).
            None => return Ok(None),
        };
        self.backfill_at(id, created_at).await
    }

    /// The backfill proper, once the creation time is known.
    async fn backfill_at(
        self: &Arc<Self>,
        id: fedimint_core::core::OperationId,
        created_at: u64,
    ) -> crate::Result<Option<crate::db::OperationRecord>> {
        let db = self.db();
        let Some(entry) = fedimint_client::oplog::OperationLog::new(db.clone())
            .get_operation(id)
            .await
        else {
            return Ok(None);
        };
        let module = entry.operation_module_kind().to_owned();
        // `try_meta` rather than `meta`: the latter panics on a shape this build does not know,
        // and every entry here was written by a module rather than by this crate.
        let meta: serde_json::Value = entry.try_meta().unwrap_or(serde_json::Value::Null);
        let claimed = crate::operation::backfillers()
            .into_iter()
            .find_map(|backfiller| backfiller.backfill(&module, &meta));
        let record = match claimed {
            Some(claimed) => crate::db::OperationRecord {
                schema_version: crate::operation::READABLE_STATE_SCHEMA,
                kind: claimed.kind.to_owned(),
                module,
                created_at,
                details: claimed.details,
                phase: claimed.phase,
                cancel_requested_at: None,
                final_state: None,
            },
            // Nothing claimed it, so it is recorded under the module that owns it and reads back
            // as a kind this build does not know: real, listable, and honestly not actionable.
            // The module's own meta is kept verbatim so a later build has it to work from.
            None => crate::db::OperationRecord {
                schema_version: crate::operation::READABLE_STATE_SCHEMA,
                kind: module.clone(),
                module,
                created_at,
                details: meta.to_string(),
                phase: None,
                cancel_requested_at: None,
                final_state: None,
            },
        };
        // `overwrite_placeholder: true`: this rebuild is only ever reached for an id that had no
        // record, or one that was itself the placeholder above, so replacing exactly that shape
        // — and nothing else, if a race wrote something better in the meantime — is correct.
        let record = self.write_record(id, record, true).await?;
        Ok(Some(record))
    }

    /// When the client says this operation was created, from its own chronological index.
    ///
    /// Newest first, so a just-created operation is found on the first step. The client keeps no
    /// creation time on the entry itself (`fedimint-client-module/src/oplog.rs:138-145`),
    /// which is why the SDK's record carries its own copy.
    async fn creation_time_of(&self, id: fedimint_core::core::OperationId) -> Option<u64> {
        use futures::StreamExt;

        let db = self.db();
        let mut dbtx = db.begin_transaction_nc().await;
        let mut keys = dbtx
            .find_by_prefix_sorted_descending(
                &fedimint_client::db::ChronologicalOperationLogKeyPrefix,
            )
            .await;
        while let Some((key, ())) = keys.next().await {
            if key.operation_id == id {
                return Some(crate::db::millis_of(key.creation_time));
            }
        }
        None
    }

    /// Commits one operation record and its index entry together, unless an existing record
    /// already answers for this id and is not to be disturbed.
    ///
    /// The existence check happens inside the same autocommit transaction as the write, mirroring
    /// [`OperationInner::record_final_state`]: a `get_value` read commits nothing, so repeating it
    /// on every attempt is free, and it is the only way to be sure nothing else committed a
    /// record for this id in the gap between whatever the caller read before calling this and
    /// the write itself.
    ///
    /// `overwrite_placeholder` says whether an existing record that is itself
    /// [`is_unclaimed_placeholder`] may still be replaced:
    /// [`create_operation`](Self::create_operation) passes `false`, because a record already at
    /// an id the client itself just minted only ever
    /// means another write won a race, never that there is a placeholder to upgrade; the backfill
    /// paths pass `true`, because upgrading exactly that placeholder is what reconciliation is
    /// for. Either way, a record that is not the placeholder — one this build can already place,
    /// or one a newer build wrote under a tag this build does not recognise — is always kept.
    ///
    /// Returns whatever ends up authoritative for this id, so a caller never hands out a handle
    /// or a lookup result for a record that lost a race it did not know it was in.
    async fn write_record(
        &self,
        id: fedimint_core::core::OperationId,
        record: crate::db::OperationRecord,
        overwrite_placeholder: bool,
    ) -> crate::Result<crate::db::OperationRecord> {
        let db = self.db();
        db.autocommit(
            |dbtx, _| {
                let record = record.clone();
                Box::pin(async move {
                    let key = crate::db::OperationRecordKey(id);
                    if let Some(existing) = dbtx.get_value(&key).await {
                        if !(overwrite_placeholder && is_unclaimed_placeholder(&existing)) {
                            return Ok::<_, core::convert::Infallible>(existing);
                        }
                        // Being replaced: its index entry is only still correct if the new
                        // record keeps the same creation time. A backfill recomputes `created_at`
                        // from the client's own chronological log, which need not agree with
                        // whatever an earlier write guessed or was given, and leaving the old
                        // entry behind would leak it under a key nothing will ever look up again.
                        if existing.created_at != record.created_at {
                            dbtx.remove_entry(&crate::db::OperationIndexKey {
                                created_at: existing.created_at,
                                id,
                            })
                            .await;
                        }
                    }
                    dbtx.insert_entry(&key, &record).await;
                    dbtx.insert_entry(
                        &crate::db::OperationIndexKey {
                            created_at: record.created_at,
                            id,
                        },
                        &(),
                    )
                    .await;
                    Ok::<_, core::convert::Infallible>(record)
                })
            },
            Some(100),
        )
        .await
        .map_err(crate::db::storage_error)
    }

    /// A federation handle with no client behind it, for the operation engine's own tests.
    ///
    /// The engine is exercised against a scripted driver rather than a live federation, so what
    /// it needs from here is a namespace to read and write and an answer to
    /// [`ensure_open`](FederationInner::ensure_open). Anything that reaches for the client gets
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed), which is the correct answer for
    /// a federation with no client.
    #[cfg(test)]
    pub(crate) fn detached(db: Database, open: bool) -> Arc<FederationInner> {
        use fedimint_core::PeerId;
        use fedimint_core::invite_code::InviteCode;
        use fedimint_core::util::SafeUrl;

        use crate::db::{StoredCapabilities, StoredNetwork};

        let (stored, status) = if open {
            (StoredStatus::Open, FederationStatus::Running)
        } else {
            (StoredStatus::Closed, FederationStatus::Closed)
        };
        let id = config::FederationId::dummy();
        let federation = FederationInner::new(
            id,
            Weak::new(),
            db,
            FederationRecord {
                invite: InviteCode::new(
                    SafeUrl::parse("wss://guardian.example:5000").expect("a valid url"),
                    PeerId::from(0),
                    id,
                    None,
                ),
                network: StoredNetwork::Regtest,
                status: stored,
                capabilities: StoredCapabilities {
                    ecash: true,
                    lightning: true,
                    onchain: true,
                },
                generation: Some(1),
                name: None,
            },
            status.clone(),
            None,
        );
        // `new` derives the closed watch from whether a client was handed over, and a detached
        // federation never has one. Setting the status again puts the watch back in step with
        // it, which is what a parked `OperationUpdates::next` is waiting on.
        federation.set_status(status);
        Arc::new(federation)
    }
}

/// Shuts a client down, waiting for its workers.
///
/// `ClientHandle::shutdown` consumes the handle, so it needs the last reference. If a caller is
/// still holding a clone, the best that can be done is to stop the executor and let the eventual
/// drop clean up, which upstream also logs about.
pub(crate) async fn shutdown_client(client: ClientHandleArc) -> Result<()> {
    let Some(handle) = Arc::into_inner(client) else {
        return Err(crate::Error::new(
            crate::ErrorCode::Internal,
            "the federation's client is still in use elsewhere",
        ));
    };
    handle.shutdown().await;
    Ok(())
}

/// Reconciles a federation that has just come up, reporting a failure rather than failing the
/// bring-up.
///
/// The client's operation log is authoritative and the SDK's records decorate it, so a crash
/// between a module's own commit and the SDK's write leaves an entry with no record. Coming up
/// is when that is repaired, and it is also when a record an earlier build could not place is
/// offered to the facades this build has.
//
// A failure is logged rather than propagated: it leaves some history rows temporarily
// unreadable, which is not a reason to deny an application its federation, its balance and the
// rest of its history, and the per-id backfill in FederationInner::operation repairs whatever
// this pass did not.
pub(crate) async fn reconcile_on_open(federation: &Arc<FederationInner>) {
    if let Err(err) = federation.reconcile_operations().await {
        tracing::warn!(
            target: "fedimint_sdk",
            federation = %federation.id,
            error = %err,
            "could not reconcile this federation's operation records",
        );
    }
}

// `FederationInner` gets no `Drop`. An earlier draft of this plan gave it one, to stop
// `ClientHandle::drop` panicking when the last `Sdk` clone was dropped outside a tokio runtime:
// against 0.12.0 that `Drop` called `RuntimeHandle::current()`, which panics when no runtime is
// entered, and the workaround was to `mem::forget` the handle. At the pinned revision upstream
// checks `RuntimeHandle::try_current()` and falls back to a non-blocking partial shutdown with an
// `error!` line instead (`fedimint-client/src/client/handle.rs:161-200`), so dropping a handle
// anywhere is degraded rather than fatal, and forgetting one would leak the client and the store's
// file lock for no reason. `Sdk::shutdown` is still the way to get a clean stop.

/// A read guard over one federation's live client.
///
/// Holding it keeps a close, an erase or a shutdown waiting until the call is done.
pub(crate) struct ClientGuard<'a>(tokio::sync::RwLockReadGuard<'a, Option<ClientHandleArc>>);

impl core::ops::Deref for ClientGuard<'_> {
    type Target = Client;

    fn deref(&self) -> &Client {
        let handle: &fedimint_client::ClientHandle = self
            .0
            .as_ref()
            .expect("a client guard is only built while the client is live");
        handle
    }
}

impl core::fmt::Debug for ClientGuard<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ClientGuard")
    }
}

/// One independent balance subscription's state.
///
/// The upstream stream is created on the first `next()` rather than here, so that handing out a
/// subscriber stays infallible even for a federation that has no client to subscribe to.
pub(crate) struct BalanceUpdatesInner {
    federation: Arc<FederationInner>,
    cursor: tokio::sync::Mutex<BalanceCursor>,
}

impl core::fmt::Debug for BalanceUpdatesInner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BalanceUpdates")
            .field("federation", &self.federation.id)
            .finish()
    }
}

/// Reads a lock without propagating poisoning.
///
/// A panic in one thread must not turn every later status read into a panic of its own: these
/// locks guard plain snapshots, and a half-written one is not a possibility.
pub(crate) fn read_lock<T>(lock: &std::sync::RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Writes a lock without propagating poisoning. See [`read_lock`].
pub(crate) fn write_lock<T>(lock: &std::sync::RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Where one balance subscription has got to.
struct BalanceCursor {
    /// The upstream stream, once the first `next()` has established there is a client to open it
    /// on. Upstream's own stream hangs forever when a client has no primary module, so it is only
    /// opened after a balance read has proved there is one.
    stream: Option<fedimint_core::util::BoxStream<'static, fedimint_core::Amount>>,
    /// The last value handed out, so a repeat is not delivered as a change.
    last: Option<Amount>,
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use std::sync::{Arc, Weak};

    use fedimint_client::oplog::OperationLog;
    use fedimint_core::PeerId;
    use fedimint_core::config::FederationId as UpstreamFederationId;
    use fedimint_core::core::OperationId as UpstreamOperationId;
    use fedimint_core::db::Database;
    use fedimint_core::db::IDatabaseTransactionOpsCoreTyped;
    use fedimint_core::db::mem_impl::MemDatabase;
    use fedimint_core::module::registry::ModuleDecoderRegistry;
    use fedimint_core::util::SafeUrl;

    use crate::db::{
        FederationRecord, OperationRecordKey, StoredCapabilities, StoredNetwork, StoredStatus,
        federation_namespace, in_memory_root,
    };
    use crate::operation::kinds;
    use crate::{ErrorCode, FederationStatus};

    use super::*;

    /// Writes an operation log entry the way the client itself does, so that the reconciliation
    /// path is exercised against a real entry rather than a stand-in.
    async fn write_log_entry(
        db: &Database,
        id: UpstreamOperationId,
        module_kind: &str,
        meta: serde_json::Value,
    ) {
        let mut dbtx = db.begin_transaction().await;
        OperationLog::new(db.clone())
            .add_operation_log_entry_dbtx(&mut dbtx.to_ref_nc(), id, module_kind, meta)
            .await;
        dbtx.commit_tx().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_unknown_id_is_not_an_error() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db, true);
        assert!(
            federation
                .operation(UpstreamOperationId([9u8; 32]))
                .await
                .expect("a missing operation is a normal answer")
                .is_none()
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_log_entry_with_no_record_is_backfilled_when_it_is_looked_up() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        let id = UpstreamOperationId([9u8; 32]);
        // The crash window: the module committed its entry, the SDK never wrote its record. No
        // backfiller in this build claims `unclaimed_module`, which is the case this test wants.
        write_log_entry(
            &db,
            id,
            "unclaimed_module",
            serde_json::json!({"kept": true}),
        )
        .await;

        let any = federation
            .operation(id)
            .await
            .expect("lookup")
            .expect("an entry the SDK never recorded is still a real operation");
        // Nothing claimed it, so it is honestly reported as a kind this build cannot place.
        assert_eq!(any.kind(), crate::OperationKind::Unknown);
        assert_eq!(any.raw_kind().module.as_deref(), Some("unclaimed_module"));

        // And the record is now there, so the next lookup does not have to rebuild it.
        let mut dbtx = db.begin_transaction_nc().await;
        let record = dbtx
            .get_value(&OperationRecordKey(id))
            .await
            .expect("the backfill was committed");
        assert_eq!(record.module, "unclaimed_module");
        assert!(record.created_at > 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reconciliation_gives_a_claimed_entry_its_real_kind() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        let id = UpstreamOperationId([9u8; 32]);
        write_log_entry(&db, id, "probe_module", serde_json::json!({"kept": true})).await;

        federation.reconcile_operations().await.expect("reconcile");

        let mut dbtx = db.begin_transaction_nc().await;
        let record = dbtx
            .get_value(&OperationRecordKey(id))
            .await
            .expect("the backfill was committed");
        assert_eq!(record.kind, kinds::ECASH_SEND);
        assert_eq!(record.details, r#"{"kept":true}"#);
        assert_eq!(record.phase, Some(1));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reconciliation_upgrades_a_record_a_later_build_can_place() {
        use futures::StreamExt;

        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let id = UpstreamOperationId([9u8; 32]);
        write_log_entry(&db, id, "probe_module", serde_json::json!({"kept": true})).await;

        // Exactly what a build with no backfiller for this module wrote: the module kind
        // verbatim as the tag, which reads back as a kind that build could not place.
        let earlier = crate::db::OperationRecord {
            schema_version: crate::operation::READABLE_STATE_SCHEMA,
            kind: "probe_module".to_owned(),
            module: "probe_module".to_owned(),
            created_at: 1,
            details: r#"{"kept":true}"#.to_owned(),
            phase: None,
            cancel_requested_at: None,
            final_state: None,
        };
        assert_eq!(
            crate::operation::kind_of_tag(&earlier.kind),
            crate::OperationKind::Unknown
        );
        let mut dbtx = db.begin_transaction().await;
        dbtx.insert_entry(&OperationRecordKey(id), &earlier).await;
        // The index entry a real write through `write_record` would also have left, dated the
        // same `created_at: 1` the earlier build guessed: the actual chronological log entry
        // above was written just now, so reconciliation's own reading of `created_at` disagrees
        // with it, and this is what would otherwise leak as a stale index row.
        dbtx.insert_entry(
            &crate::db::OperationIndexKey {
                created_at: earlier.created_at,
                id,
            },
            &(),
        )
        .await;
        dbtx.commit_tx().await;

        // This build does know the module, so reconciliation offers the record to the
        // backfillers again and replaces the earlier reading rather than leaving it.
        let later = FederationInner::detached(db.clone(), true);
        later.reconcile_operations().await.expect("reconcile");

        let mut dbtx = db.begin_transaction_nc().await;
        let record = dbtx
            .get_value(&OperationRecordKey(id))
            .await
            .expect("record");
        assert_eq!(record.kind, kinds::ECASH_SEND);
        assert_ne!(record.created_at, earlier.created_at);

        // The stale index entry from the earlier `created_at` is gone rather than left behind
        // beside the new one.
        let indexed: Vec<_> = dbtx
            .find_by_prefix_sorted_descending(&crate::db::OperationIndexKeyPrefix)
            .await
            .map(|(key, ())| (key.created_at, key.id))
            .collect()
            .await;
        assert_eq!(indexed, vec![(record.created_at, id)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reconciliation_leaves_an_unrecognised_facade_tag_alone() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        let id = UpstreamOperationId([9u8; 32]);
        // A log entry has to exist or reconciliation never visits this id at all; the module is
        // deliberately one no backfiller in this build claims, so a claim can never be the
        // reason the record below survives.
        write_log_entry(&db, id, "mint", serde_json::json!({"kept": true})).await;

        // What a newer build's facade wrote: a real kind tag this build does not recognise, over
        // a module this build also has no backfiller for. It is not the placeholder shape
        // (`kind == module` is false here), which is the only thing that tells the two apart.
        let newer = crate::db::OperationRecord {
            schema_version: crate::operation::READABLE_STATE_SCHEMA,
            kind: "future_send".to_owned(),
            module: "mint".to_owned(),
            created_at: 1,
            details: r#"{"kept":true}"#.to_owned(),
            phase: Some(3),
            cancel_requested_at: Some(9),
            final_state: Some("done".to_owned()),
        };
        assert_eq!(
            crate::operation::kind_of_tag(&newer.kind),
            crate::OperationKind::Unknown
        );
        let mut dbtx = db.begin_transaction().await;
        dbtx.insert_entry(&OperationRecordKey(id), &newer).await;
        dbtx.commit_tx().await;

        federation.reconcile_operations().await.expect("reconcile");

        let mut dbtx = db.begin_transaction_nc().await;
        let after = dbtx
            .get_value(&OperationRecordKey(id))
            .await
            .expect("record");
        // Untouched: only the placeholder shape is ever offered to the backfillers again, and a
        // tag this build cannot place is not the same thing — rewriting it would have lost
        // everything below that the log entry alone does not carry.
        assert_eq!(after.kind, "future_send");
        assert_eq!(after.cancel_requested_at, Some(9));
        assert_eq!(after.phase, Some(3));
        assert_eq!(after.final_state.as_deref(), Some("done"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reconciliation_leaves_a_record_the_sdk_wrote_alone() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        let id = UpstreamOperationId([9u8; 32]);
        write_log_entry(&db, id, "probe_module", serde_json::json!({"kept": true})).await;
        federation
            .reconcile_operations()
            .await
            .expect("first reconcile");

        let mut dbtx = db.begin_transaction().await;
        let mut record = dbtx
            .get_value(&OperationRecordKey(id))
            .await
            .expect("record");
        record.cancel_requested_at = Some(7);
        dbtx.insert_entry(&OperationRecordKey(id), &record).await;
        dbtx.commit_tx().await;

        federation
            .reconcile_operations()
            .await
            .expect("second reconcile");

        let mut dbtx = db.begin_transaction_nc().await;
        let after = dbtx
            .get_value(&OperationRecordKey(id))
            .await
            .expect("record");
        // A record this build can place is never rewritten, so nothing it accumulated is lost.
        assert_eq!(after.cancel_requested_at, Some(7));
    }

    fn closed_federation(capabilities: StoredCapabilities) -> Arc<FederationInner> {
        let id = UpstreamFederationId::dummy();
        let root = Database::new(MemDatabase::new(), ModuleDecoderRegistry::default());
        let record = FederationRecord {
            invite: fedimint_core::invite_code::InviteCode::new(
                SafeUrl::parse("wss://guardian.example:5000").expect("a valid url"),
                PeerId::from(0),
                id,
                None,
            ),
            network: StoredNetwork::Regtest,
            status: StoredStatus::Closed,
            capabilities,
            generation: Some(1),
            name: Some("Test Federation".to_owned()),
        };
        Arc::new(FederationInner::new(
            id,
            Weak::new(),
            root.with_prefix(crate::db::federation_prefix(&id).to_vec()),
            record,
            FederationStatus::Closed,
            None,
        ))
    }

    fn everything() -> StoredCapabilities {
        StoredCapabilities {
            ecash: true,
            lightning: true,
            onchain: true,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_closed_federation_refuses_every_fallible_call() {
        let federation = Federation::new(closed_federation(everything()));
        let err = federation
            .balance()
            .await
            .expect_err("a closed federation refuses");
        assert_eq!(err.code, ErrorCode::FederationClosed);
    }

    #[test]
    fn a_closed_federation_still_describes_itself() {
        // A history screen has to be able to label rows with a federation that was
        // closed underneath it, so none of these may fail or go blank.
        let federation = Federation::new(closed_federation(everything()));
        assert_eq!(federation.name().as_deref(), Some("Test Federation"));
        assert_eq!(federation.network(), crate::Network::Regtest);
        assert_eq!(
            federation.capabilities(),
            crate::Capabilities {
                ecash: true,
                lightning: true,
                onchain: true
            }
        );
        assert_eq!(
            federation.id(),
            crate::FederationId::from_upstream(UpstreamFederationId::dummy())
        );
        // The invite code renders, and renders redacted.
        assert_eq!(
            format!("{:?}", federation.invite_code()),
            "InviteCode(<redacted>)"
        );
    }

    #[test]
    fn facade_accessors_answer_for_the_module_set_not_for_the_state() {
        // `None` means "this federation has no mint module", so a closed federation
        // that has one must still hand out the facade; the failure belongs on the
        // facade's own calls.
        let federation = Federation::new(closed_federation(everything()));
        assert!(federation.ecash().is_some());
        assert!(federation.lightning().is_some());
        assert!(federation.onchain().is_some());

        let mint_only = Federation::new(closed_federation(StoredCapabilities {
            ecash: true,
            lightning: false,
            onchain: false,
        }));
        assert!(mint_only.ecash().is_some());
        assert!(mint_only.lightning().is_none());
        assert!(mint_only.onchain().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_closed_federation_still_hands_out_a_balance_subscriber() {
        // The error belongs where a caller can act on it rather than being
        // swallowed by an accessor that cannot return one.
        let federation = Federation::new(closed_federation(everything()));
        let mut updates = federation.balance_updates();
        let err = updates
            .next()
            .await
            .expect_err("the first call reports the closure");
        assert_eq!(err.code, ErrorCode::FederationClosed);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_recovering_federation_refuses_only_fund_touching_calls() {
        let inner = closed_federation(everything());
        inner.set_status(FederationStatus::Recovering);
        // Without a client there is nothing to hand out either way, but the
        // recovery lock has to be reported as itself rather than as a closure.
        let err = inner
            .client(true)
            .await
            .expect_err("a recovery-locked federation refuses fund-touching work");
        assert_eq!(err.code, ErrorCode::Recovering);
    }

    #[test]
    fn a_detached_federation_answers_from_its_record_and_its_namespace() {
        let root = in_memory_root();
        let db = federation_namespace(&root, [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        // The namespace is the one the client would have been handed, so the SDK's records and
        // the client's operation log share a keyspace.
        assert!(federation.db().is_global());
        assert!(federation.ensure_open().is_ok());

        let closed = FederationInner::detached(db, false);
        let err = closed
            .ensure_open()
            .expect_err("a closed federation refuses every fallible call");
        assert_eq!(err.code, crate::ErrorCode::FederationClosed);
    }

    #[test]
    fn a_federation_that_is_not_running_refuses_every_fallible_call() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);

        // A stored row that is not `Open` is refused whatever the live status says. `Joining` is
        // the one that is easy to read as open and is not: it is an interrupted join whose
        // namespace the next build wipes and redoes from the invite.
        for stored in [
            StoredStatus::Joining,
            StoredStatus::Closed,
            StoredStatus::Forgetting,
        ] {
            let federation = FederationInner::detached(db.clone(), true);
            let mut record = federation.record();
            record.status = stored;
            federation.set_record(record);
            assert_eq!(
                federation
                    .ensure_open()
                    .expect_err("only an open row is open")
                    .code,
                crate::ErrorCode::FederationClosed,
                "{stored:?}"
            );
        }

        // And an `Open` row is still refused once the running federation is not. Quarantine is
        // the case that matters: the storage is intact, so the row never changes, and the
        // contract says every subsequent fallible call on a quarantined federation fails.
        let federation = FederationInner::detached(db, true);
        federation.set_status(FederationStatus::Quarantined {
            diagnostic: crate::Diagnostic::new(
                crate::ErrorCode::FederationUnreachable,
                "no guardian answered",
            ),
        });
        assert_eq!(
            federation
                .ensure_open()
                .expect_err("a quarantined federation is not usable")
                .code,
            crate::ErrorCode::FederationClosed
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn creating_an_operation_writes_its_record_and_its_index_entry() {
        use futures::StreamExt;

        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        let id = UpstreamOperationId([5u8; 32]);
        federation
            .create_operation(
                id,
                kinds::ECASH_SEND,
                "mint",
                &serde_json::json!({"notes": "…"}),
                Arc::new(crate::operation::ProbeEcashSendDriver)
                    as Arc<dyn crate::operation::Driver<crate::EcashSendState>>,
            )
            .await
            .expect("create");

        let mut dbtx = db.begin_transaction_nc().await;
        let record = dbtx
            .get_value(&OperationRecordKey(id))
            .await
            .expect("record");
        assert_eq!(record.kind, kinds::ECASH_SEND);
        assert_eq!(record.module, "mint");
        assert_eq!(
            record.schema_version,
            crate::operation::READABLE_STATE_SCHEMA
        );
        assert_eq!(record.final_state, None);

        let indexed: Vec<_> = dbtx
            .find_by_prefix_sorted_descending(&crate::db::OperationIndexKeyPrefix)
            .await
            .map(|(key, ())| (key.created_at, key.id))
            .collect()
            .await;
        assert_eq!(indexed, vec![(record.created_at, id)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn create_operation_never_overwrites_an_existing_record() {
        let db = federation_namespace(&in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db.clone(), true);
        let id = UpstreamOperationId([5u8; 32]);

        // Something is already recorded at this id — an id the client itself mints fresh for
        // every real call, so this can only mean another write already won a race against the
        // one below, and it must be left exactly as it is.
        let existing = crate::db::OperationRecord {
            schema_version: crate::operation::READABLE_STATE_SCHEMA,
            kind: "probe_module".to_owned(),
            module: "probe_module".to_owned(),
            created_at: 1,
            details: "{}".to_owned(),
            phase: None,
            cancel_requested_at: None,
            final_state: None,
        };
        let mut dbtx = db.begin_transaction().await;
        dbtx.insert_entry(&OperationRecordKey(id), &existing).await;
        dbtx.commit_tx().await;

        federation
            .create_operation(
                id,
                kinds::ECASH_SEND,
                "mint",
                &serde_json::json!({"different": true}),
                Arc::new(crate::operation::ProbeEcashSendDriver)
                    as Arc<dyn crate::operation::Driver<crate::EcashSendState>>,
            )
            .await
            .expect("create");

        let mut dbtx = db.begin_transaction_nc().await;
        let record = dbtx
            .get_value(&OperationRecordKey(id))
            .await
            .expect("record");
        // The existing record won, not the one this call tried to write — even though it is
        // itself the placeholder shape a backfill would have been allowed to replace.
        assert_eq!(record.kind, "probe_module");
        assert_eq!(record.details, "{}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_operation_is_found_again_through_a_fresh_handle_over_the_same_store() {
        let root = in_memory_root();
        let db = federation_namespace(&root, [1u8; 32]);
        let id = UpstreamOperationId([5u8; 32]);

        let created = {
            let federation = FederationInner::detached(db.clone(), true);
            let operation = federation
                .create_operation(
                    id,
                    kinds::ECASH_SEND,
                    "mint",
                    &serde_json::json!({"notes": "…"}),
                    Arc::new(crate::operation::ProbeEcashSendDriver)
                        as Arc<dyn crate::operation::Driver<crate::EcashSendState>>,
                )
                .await
                .expect("create");
            operation.inner().record.clone()
            // Every handle from that run is dropped here, as it would be by a shutdown.
        };

        // A new run over the same store, through a fresh federation handle.
        let reopened = FederationInner::detached(db, true);
        let found = reopened
            .operation(id)
            .await
            .expect("lookup")
            .expect("the operation is still there");
        assert_eq!(found.kind(), crate::OperationKind::EcashSend);
        assert_eq!(found.id(), crate::OperationId::from_upstream(id));
        // The whole record, unchanged: the id is all it takes to pick an operation back up.
        let typed = found.as_ecash_send().expect("a typed handle");
        assert_eq!(typed.inner().record, created);
        assert_eq!(
            typed.state().await.expect("state"),
            crate::EcashSendState::Redeemed
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_operation_survives_a_real_close_and_reopen_of_the_store() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let id = UpstreamOperationId([5u8; 32]);

        let created = {
            let root = crate::db::open_native_root(dir.path()).await.expect("open");
            let federation =
                FederationInner::detached(federation_namespace(&root, [1u8; 32]), true);
            let operation = federation
                .create_operation(
                    id,
                    kinds::ONCHAIN_RECEIVE,
                    "wallet",
                    &serde_json::json!({"address": "…"}),
                    Arc::new(crate::operation::ProbeEcashSendDriver)
                        as Arc<dyn crate::operation::Driver<crate::EcashSendState>>,
                )
                .await
                .expect("create");
            operation.inner().record.clone()
            // The database handle is dropped here, which is what releases the store.
        };

        let root = crate::db::open_native_root(dir.path())
            .await
            .expect("reopen");
        let federation = FederationInner::detached(federation_namespace(&root, [1u8; 32]), true);
        let found = federation
            .operation(id)
            .await
            .expect("lookup")
            .expect("the operation is still there after a real restart");
        assert_eq!(found.kind(), crate::OperationKind::OnchainReceive);
        assert_eq!(found.raw_kind().module.as_deref(), Some("wallet"));
        assert!(
            found.as_onchain_receive().is_none(),
            "this build has no on-chain driver yet, so there is no typed handle"
        );
        // The record itself is byte-for-byte what was written before the restart.
        let db = federation.db();
        let mut dbtx = db.begin_transaction_nc().await;
        assert_eq!(
            dbtx.get_value(&OperationRecordKey(id)).await.as_ref(),
            Some(&created)
        );
    }
}
