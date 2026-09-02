//! A joined federation, and the capability facades hanging off it.

use std::sync::Arc;

use crate::{
    ActivityPage, Amount, AnyOperation, Cursor, Ecash, FederationId, InviteCode, Lightning, Meta,
    Network, Onchain, OperationId, Result,
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
/// state — every clone talks to the same running federation client — and it
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
/// **fallible** calls — [`balance`](Federation::balance),
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
///   `None` instead would be a lie with a specific documented meaning —
///   "this federation has no mint module" — and would make a closed
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
        unimplemented!()
    }

    /// The federation's human-readable name, when its configuration
    /// declares one.
    ///
    /// This is configuration metadata, not a verified or unique identifier:
    /// two federations may present the same name. Identity is
    /// [`Federation::id`].
    pub fn name(&self) -> Option<String> {
        unimplemented!()
    }

    /// The Bitcoin network this federation operates on.
    ///
    /// On-chain addresses are validated against this when an on-chain quote
    /// is requested, failing with
    /// [`NetworkMismatch`](crate::ErrorCode::NetworkMismatch) on
    /// disagreement. There is no second check at send time, because
    /// [`Onchain::send`](crate::Onchain::send) takes only a quote — the
    /// address is bound into the quote when it is issued.
    pub fn network(&self) -> Network {
        unimplemented!()
    }

    /// An invite code for this federation, suitable for sharing so someone
    /// else can join it.
    pub fn invite_code(&self) -> InviteCode {
        unimplemented!()
    }

    /// The ecash balance: the value this instance currently holds as its
    /// own, uncommitted notes.
    ///
    /// Value that is committed to an in-flight operation — funding a
    /// lightning payment, sitting in out-of-band notes that have not been
    /// redeemed or reclaimed, waiting on an on-chain deposit to confirm — is
    /// not counted here.
    ///
    /// Holding is not spending, and this method takes no position on the
    /// latter: whether a spend would be *permitted* is governed by the
    /// federation's status, not by this number. The case where the two
    /// diverge is a recovery-locked federation: while the rescan proceeds
    /// the balance reported here is partial, still moving, and worth
    /// showing as progress — yet none of it is spendable, and every spend
    /// or receive is refused with
    /// [`Recovering`](crate::ErrorCode::Recovering) no matter what this
    /// method returned. It settles when recovery finishes. On a
    /// [`Running`](crate::FederationStatus::Running) federation the two
    /// notions coincide, and this is exactly the amount a spend can draw
    /// on.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage),
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn balance(&self) -> Result<Amount> {
        unimplemented!()
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
        unimplemented!()
    }

    /// What this federation can do.
    ///
    /// Reported as plain booleans so an application can decide what to
    /// render before the user touches anything. It answers the same
    /// question as the three facade accessors below and exists alongside
    /// them for the case where a screen needs to know about several
    /// capabilities at once without taking handles it will not use.
    pub fn capabilities(&self) -> Capabilities {
        unimplemented!()
    }

    /// The ecash facade, or `None` if this federation has no mint module.
    ///
    /// # Why `Option` and not an error
    ///
    /// Capability discovery must not be error-driven. An application needs
    /// to know whether to draw a "send ecash" button *before* the user
    /// presses it; a design where the only way to find out is to try the
    /// operation and catch a failure forces every UI to either attempt
    /// operations speculatively or hard-code assumptions about federation
    /// configuration. Returning `None` makes the absent capability an
    /// ordinary value to branch on.
    ///
    /// [`ErrorCode::NotSupported`](crate::ErrorCode::NotSupported) still
    /// exists, but only for the narrow residual case: a facade that was
    /// obtained while the module was present, then used after the
    /// federation's configuration changed to drop it.
    ///
    /// `None` therefore means one thing only, and it is not "closed": a
    /// closed federation that has a mint module still returns `Some`, and
    /// the facade's calls fail with
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed). See the
    /// type documentation.
    pub fn ecash(&self) -> Option<Ecash> {
        unimplemented!()
    }

    /// The lightning facade, or `None` if the federation has no lightning
    /// module. See [`Federation::ecash`] for why this is an `Option`.
    pub fn lightning(&self) -> Option<Lightning> {
        unimplemented!()
    }

    /// The on-chain facade, or `None` if this federation has no wallet
    /// module. See [`Federation::ecash`] for why this is an `Option`.
    pub fn onchain(&self) -> Option<Onchain> {
        unimplemented!()
    }

    /// The metadata facade.
    ///
    /// Unconditional, unlike the three capability facades above: every
    /// federation has configuration metadata, so there is always something
    /// to read. A federation without a meta module simply has no consensus
    /// metadata, which [`Meta`] reports as an absent value rather than as a
    /// missing facade.
    pub fn meta(&self) -> Meta {
        unimplemented!()
    }

    /// Looks up an operation by id, whatever kind it is.
    ///
    /// This is how an application reattaches after a restart: persist the
    /// [`OperationId`] (or read one from
    /// [`ActivityItem`](crate::ActivityItem)), pass it here, and get back a
    /// handle to the operation that has been running all along.
    ///
    /// The call is asynchronous and fallible because it reads persistent
    /// state — the operation log lives in storage, not in memory, and a
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
    /// The history is *local* — see [`ActivityItem`](crate::ActivityItem)
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
    /// recovery is incomplete — which is not the same as still running, since
    /// a recovery that stopped short leaves the lock in place — and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn backup(&self) -> Result<()> {
        unimplemented!()
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
    /// # Why this is not `Option`-shaped
    ///
    /// [`OperationUpdates::next`](crate::OperationUpdates::next) returns
    /// `Result<Option<_>>`, where `Ok(None)` means "a final state was
    /// observed and the subscription closed cleanly". A balance has no
    /// final state — a federation's balance can always change again — so
    /// that case simply cannot arise here, and an `Option` would be a
    /// permanently-`Some` wrapper that every caller has to unwrap for no
    /// reason. The only way this stream ends is the federation being closed
    /// or the SDK shutting down, and that is a condition callers must
    /// notice, so it surfaces as
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed). The
    /// asymmetry with `OperationUpdates` is deliberate and reflects a real
    /// difference between the two streams.
    ///
    /// # Errors
    ///
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) once the
    /// federation is closed or the SDK shut down — the terminal condition
    /// for this stream. Other errors are infrastructure failures:
    /// [`Storage`](crate::ErrorCode::Storage) or
    /// [`Internal`](crate::ErrorCode::Internal).
    pub async fn next(&mut self) -> Result<Amount> {
        unimplemented!()
    }
}

/// Placeholder for the shared per-federation client state.
#[derive(Debug)]
struct FederationInner;

/// Placeholder for one balance subscription's state.
#[derive(Debug)]
struct BalanceUpdatesInner;
