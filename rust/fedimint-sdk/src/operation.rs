//! The operation model: typed state machines observed from the outside.
//!
//! Everything the SDK does that takes longer than a single call — paying an
//! invoice, waiting for a deposit to confirm, redeeming ecash — is an
//! *operation*. An operation is created by a facade call, runs in the
//! background from that moment, is persisted as it goes, and reports its
//! progress as a sequence of states. This module defines the vocabulary
//! shared by all of them: the [`OperationState`] trait each state enum
//! implements, the [`Operation`] handle used to observe one, the
//! [`OperationUpdates`] subscriber that streams its transitions, and the
//! type-erased [`AnyOperation`] returned when an operation is looked up by
//! id after a restart.

use std::marker::PhantomData;
use std::sync::Arc;

use crate::{
    EcashReceiveState, EcashSendState, LnReceiveState, LnSendState, OnchainReceiveState,
    OnchainSendState, OperationId, Result,
};

/// The sealing module for [`OperationState`].
///
/// It is `pub(crate)` rather than private so that the facade modules can
/// name [`sealed::Sealed`] to implement it next to each state enum they
/// define. Because the module is not reachable from outside this crate,
/// downstream code still cannot name the trait and therefore cannot
/// implement [`OperationState`].
pub(crate) mod sealed {
    /// Marker that only this crate can implement, sealing
    /// [`OperationState`](super::OperationState).
    pub trait Sealed {}
}

/// The progress of one operation, expressed as a flat state enum.
///
/// Each kind of operation has its own state type — [`EcashSendState`],
/// [`LnSendState`], [`OnchainReceiveState`], and so on — and this trait is
/// what they have in common: a state can say whether it is terminal, and
/// the generic machinery ([`Operation`], [`OperationUpdates`]) is written
/// once against that.
///
/// # Sealed
///
/// The trait is sealed: the set of operation kinds is defined by this SDK
/// and cannot be extended from outside it. That is not gatekeeping for its
/// own sake — each state type is monomorphised into a concrete type in
/// every generated binding, so a state type the SDK does not know about
/// could not cross a foreign-function boundary at all. Adding a kind is an
/// additive change to this crate.
///
/// # Supertraits
///
/// `Clone` because states are values handed out to every subscriber
/// independently; `Send + Sync + 'static` because operations are driven by
/// background tasks on native targets and their states cross task and
/// thread boundaries. Note that `Debug` is deliberately *not* required
/// here even though every concrete state enum implements it, so that the
/// bound stays minimal; the generic types in this module still print,
/// because their `Debug` impls apply whenever the state type happens to be
/// `Debug`.
pub trait OperationState: sealed::Sealed + Clone + Send + Sync + 'static {
    /// Whether this state is terminal, meaning the operation has finished
    /// and will never transition again.
    ///
    /// A terminal state is not necessarily a *successful* one: a refunded
    /// lightning payment and an expired invoice are both final. This is the
    /// predicate [`Operation::await_final`] waits for and the point at
    /// which an [`OperationUpdates`] subscription closes.
    fn is_final(&self) -> bool;
}

/// A handle for observing one background operation.
///
/// # Operations are detached, not owned
///
/// An operation starts running the moment the facade call that created it
/// returns, and it keeps running whether or not anyone is watching. This
/// handle observes; it does not own. Dropping it does not cancel, pause, or
/// abort anything, and neither does dropping an [`OperationUpdates`]
/// obtained from it — the only thing that ends an operation is the
/// operation reaching a final state. The same is true across restarts: an
/// operation is persisted as it progresses, resumes when the SDK is built
/// again over the same storage, and can be picked up again with
/// [`Federation::operation`](crate::Federation::operation).
///
/// That is a deliberate answer to "is this cancellable?": for most
/// operations there is nothing to cancel, because the money has already
/// moved into a protocol that will resolve one way or the other. Where a
/// cancellation genuinely exists, it is a named request on that specific
/// operation — see
/// [`Operation::<EcashSendState>::request_cancel`](crate::Operation::request_cancel) —
/// and its outcome arrives as a state, not as the return value of the
/// cancel call.
///
/// # Failures are states, errors are not
///
/// A payment that fails, an invoice that expires, a deposit the federation
/// rejects: all of those are ordinary final *states*, reported as `Ok`. An
/// `Err` from any method here means something else went wrong — storage
/// could not be read, the federation could not be reached, the handle
/// belongs to a closed federation. Applications render states; they log
/// errors.
///
/// The handle is a cheap clone over shared state, like the other handles in
/// this crate.
#[derive(Debug, Clone)]
pub struct Operation<S: OperationState> {
    inner: Arc<OperationInner>,
    _state: PhantomData<S>,
}

impl<S: OperationState> Operation<S> {
    /// This operation's id, stable for its whole lifetime including across
    /// restarts.
    ///
    /// Persist it to find the operation again with
    /// [`Federation::operation`](crate::Federation::operation), or to
    /// correlate an [`ActivityItem`](crate::ActivityItem) with a live
    /// handle.
    pub fn id(&self) -> OperationId {
        unimplemented!()
    }

    /// Reads the current state.
    ///
    /// This is a point-in-time snapshot: by the time the caller looks at
    /// it, the operation may have moved on. Use [`Operation::updates`] to
    /// follow it, or [`Operation::await_final`] to wait for the end.
    ///
    /// # Errors
    ///
    /// Only for infrastructure failures —
    /// [`Storage`](crate::ErrorCode::Storage) if the state cannot be read,
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) if the
    /// federation was closed or the SDK shut down. A failed operation is
    /// `Ok` with a failure state, never an `Err`.
    pub async fn state(&self) -> Result<S> {
        unimplemented!()
    }

    /// Opens a new, independent subscription to this operation's states.
    ///
    /// The subscription yields the **current state first**, immediately,
    /// and then every subsequent transition. Two properties of that are
    /// worth stating exactly, because both are easy to assume otherwise:
    ///
    /// - **It is not a replay of history.** The first value is where the
    ///   operation is *now*, not where it started. An application that
    ///   subscribes to an operation which has already funded and settled
    ///   sees the settled state and then a clean close; it does not
    ///   receive the intermediate states it missed. Anything that needs the
    ///   full trail must record it as it happens, or read
    ///   [activity history](crate::Federation::activity).
    /// - **Each call is its own cursor.** Two subscriptions to the same
    ///   operation both see every transition from the moment they were
    ///   created; they do not share a position and cannot steal updates
    ///   from one another. A screen and a background sync task can each
    ///   subscribe without coordinating.
    ///
    /// Dropping the returned subscriber ends only that subscription.
    pub fn updates(&self) -> OperationUpdates<S> {
        unimplemented!()
    }

    /// Waits until the operation reaches a final state and returns it.
    ///
    /// Equivalent to subscribing and reading until
    /// [`OperationState::is_final`] holds, which means it also returns
    /// immediately if the operation has already finished. The returned
    /// state may be a failure state; that is a normal, successful result of
    /// this call.
    ///
    /// # Errors
    ///
    /// Only for infrastructure failures, as for [`Operation::state`]. In
    /// particular a payment that fails yields `Ok(final state)`, and
    /// closing the federation while this is pending yields
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn await_final(&self) -> Result<S> {
        unimplemented!()
    }
}

/// One independent subscription to an operation's states.
///
/// Obtained from [`Operation::updates`]. Deliberately not `Clone`: a
/// subscriber is a single cursor, and handing out copies of a cursor is
/// exactly the shared-position confusion that
/// [`Operation::updates`] exists to avoid — call it again for a second,
/// independent subscription instead.
///
/// Dropping a subscriber, or dropping a future returned by
/// [`OperationUpdates::next`] before it resolves, cancels only this
/// subscription. The operation itself is unaffected.
#[derive(Debug)]
pub struct OperationUpdates<S: OperationState> {
    inner: Arc<OperationInner>,
    _state: PhantomData<S>,
}

impl<S: OperationState> OperationUpdates<S> {
    /// Waits for the next state.
    ///
    /// The three possible answers each mean exactly one thing:
    ///
    /// - `Ok(Some(state))` — the operation is in this state now. The very
    ///   first call returns the current state without waiting; later calls
    ///   resolve when the operation transitions.
    /// - `Ok(None)` — a final state was already yielded and the
    ///   subscription closed cleanly. Nothing was lost and nothing failed;
    ///   this is the normal end of the stream, and further calls keep
    ///   returning `Ok(None)`.
    /// - `Err(_)` — an infrastructure failure. Storage could not be read,
    ///   the federation went away, the SDK was shut down. The subscription
    ///   may not be resumable afterwards; obtain a fresh one from
    ///   [`Operation::updates`] and, if the error was
    ///   [`FederationClosed`](crate::ErrorCode::FederationClosed), a fresh
    ///   [`Operation`] handle first.
    ///
    /// The distinction that matters: an operation that *failed* ends with
    /// `Ok(Some(failure state))` followed by `Ok(None)`. `Err` never
    /// carries the outcome of an operation, only the failure of observing
    /// it.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage),
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed), or
    /// [`Internal`](crate::ErrorCode::Internal).
    pub async fn next(&mut self) -> Result<Option<S>> {
        unimplemented!()
    }
}

/// An operation whose kind is known only at runtime.
///
/// Returned by [`Federation::operation`](crate::Federation::operation),
/// which looks an operation up by id and therefore cannot know statically
/// what kind it is — the id may have come from persisted state, from an
/// [`ActivityItem`](crate::ActivityItem), or from another process's
/// notification. Read [`AnyOperation::kind`] to find out, then use the
/// matching accessor to recover a typed [`Operation`].
///
/// Like [`Operation`], this is an observation handle over a detached,
/// persisted operation, and it is a cheap clone.
#[derive(Debug, Clone)]
pub struct AnyOperation {
    inner: Arc<AnyOperationInner>,
}

impl AnyOperation {
    /// This operation's id.
    pub fn id(&self) -> OperationId {
        unimplemented!()
    }

    /// What kind of operation this is.
    ///
    /// May be [`OperationKind::Unknown`] for an operation this SDK version
    /// cannot interpret; see that variant.
    pub fn kind(&self) -> OperationKind {
        unimplemented!()
    }

    /// Recovers a typed handle if this is an out-of-band ecash send, and
    /// `None` otherwise.
    pub fn as_ecash_send(&self) -> Option<Operation<EcashSendState>> {
        unimplemented!()
    }

    /// Recovers a typed handle if this is an ecash redemption, and `None`
    /// otherwise.
    pub fn as_ecash_receive(&self) -> Option<Operation<EcashReceiveState>> {
        unimplemented!()
    }

    /// Recovers a typed handle if this is an outgoing lightning payment,
    /// and `None` otherwise.
    pub fn as_ln_send(&self) -> Option<Operation<LnSendState>> {
        unimplemented!()
    }

    /// Recovers a typed handle if this is an incoming lightning payment,
    /// and `None` otherwise.
    pub fn as_ln_receive(&self) -> Option<Operation<LnReceiveState>> {
        unimplemented!()
    }

    /// Recovers a typed handle if this is an on-chain withdrawal, and
    /// `None` otherwise.
    pub fn as_onchain_send(&self) -> Option<Operation<OnchainSendState>> {
        unimplemented!()
    }

    /// Recovers a typed handle if this is an on-chain deposit, and `None`
    /// otherwise.
    pub fn as_onchain_receive(&self) -> Option<Operation<OnchainReceiveState>> {
        unimplemented!()
    }

    // The seventh accessor, `as_recovery`, is feature-gated: it returns
    // `Operation<RecoveryState>`, and `RecoveryState` exists only behind the
    // off-by-default `experimental` feature. It is therefore defined in
    // `recovery.rs`, in an `impl AnyOperation` block alongside that module's
    // `impl Sdk`, so the default build's `AnyOperation` is exactly the six
    // accessors above. `kind()` reports `OperationKind::Recovery` in either
    // build, which is what an activity list needs. Fold the accessor in here
    // when recovery stabilises.
}

/// What kind of work an operation is doing.
///
/// Reported by [`AnyOperation::kind`] and carried on
/// [`ActivityItem`](crate::ActivityItem), so that a history screen can
/// label and group rows without having to resolve each one to a typed
/// handle first.
///
/// `#[non_exhaustive]`: new kinds arrive with new modules. Rust callers
/// must include a wildcard arm; bindings map an unrecognised kind to their
/// own unknown case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OperationKind {
    /// Ecash spent out of band, tracked by
    /// [`EcashSendState`](crate::EcashSendState).
    EcashSend,
    /// Ecash notes redeemed into the balance, tracked by
    /// [`EcashReceiveState`](crate::EcashReceiveState).
    EcashReceive,
    /// An outgoing lightning payment, tracked by
    /// [`LnSendState`](crate::LnSendState).
    LnSend,
    /// An incoming lightning payment, tracked by
    /// [`LnReceiveState`](crate::LnReceiveState).
    LnReceive,
    /// An on-chain withdrawal, tracked by
    /// [`OnchainSendState`](crate::OnchainSendState).
    OnchainSend,
    /// An on-chain deposit, tracked by
    /// [`OnchainReceiveState`](crate::OnchainReceiveState).
    OnchainReceive,
    /// Restoring a wallet from its seed.
    ///
    /// This variant is **not** gated behind the crate's experimental
    /// feature even though the recovery API is: a recovery operation
    /// persisted by a build that had the feature enabled must still be
    /// listable and nameable by a build that does not, rather than
    /// degrading to [`OperationKind::Unknown`].
    Recovery,
    /// An operation this SDK version cannot interpret.
    ///
    /// Persisted operations outlive the version that created them: an
    /// application may be downgraded, or a federation may have been used
    /// with a build that supported a module this one does not. Such an
    /// operation is still real, still recorded, and still identifiable by
    /// id, so reporting it as `Unknown` is strictly better than failing the
    /// lookup — an application can list it as "an operation from a newer
    /// version" instead of pretending the record does not exist. None of
    /// the typed accessors on [`AnyOperation`] match it, and acting on it
    /// is what
    /// [`ErrorCode::UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation)
    /// reports.
    Unknown,
}

/// Placeholder for the shared per-operation state a typed handle and its
/// subscribers observe.
#[derive(Debug)]
struct OperationInner;

/// Placeholder for the shared state behind a type-erased operation handle.
#[derive(Debug)]
struct AnyOperationInner;
