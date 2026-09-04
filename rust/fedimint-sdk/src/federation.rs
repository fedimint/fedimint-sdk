//! A joined federation, and the capability facades hanging off it.

use std::sync::{Arc, Weak};

use fedimint_client::{Client, ClientHandleArc};
use fedimint_core::config;
use fedimint_core::db::Database;
use fedimint_core::module::AmountUnit;
use futures::StreamExt;

use crate::db::FederationRecord;
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
        unimplemented!()
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
    use std::sync::Weak;

    use fedimint_core::PeerId;
    use fedimint_core::config::FederationId as UpstreamFederationId;
    use fedimint_core::db::Database;
    use fedimint_core::db::mem_impl::MemDatabase;
    use fedimint_core::module::registry::ModuleDecoderRegistry;
    use fedimint_core::util::SafeUrl;

    use crate::db::{FederationRecord, StoredCapabilities, StoredNetwork, StoredStatus};
    use crate::{ErrorCode, FederationStatus};

    use super::*;

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
}
