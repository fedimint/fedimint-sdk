//! The operation model: background work observed from the outside.
//!
//! Everything the SDK does that takes longer than a single call, paying an
//! invoice, waiting for a deposit to confirm, redeeming ecash, is an
//! *operation*. An operation is created by a facade call, runs in the
//! background from that moment, is persisted as it goes, and reports its
//! progress as a sequence of states. This module defines the vocabulary
//! shared by all of them: the [`OperationState`] trait each state enum
//! implements, the [`Operation`] handle used to observe one, the
//! [`OperationUpdates`] subscriber that streams its transitions, the
//! type-erased [`AnyOperation`] returned when an operation is looked up by
//! id after a restart, and the [`OperationSupport`] answer that says how far
//! this build can go with such a record.
//!
//! It also defines the half of an operation that is not a state: the
//! persisted [`OperationDetails`] record each kind keeps, the notes handed
//! out, the invoice issued, the address allocated, the fee and route of the
//! quote that was executed, read back through [`Operation::details`]. States
//! say where an operation has got to; details say what it is. Both are needed
//! to make good on the promise that an operation id is all it takes to pick
//! an operation back up, because a subscription yields the current state and
//! never replays the ones before it.

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use fedimint_core::core::OperationId as UpstreamOperationId;
use fedimint_core::task::{MaybeSend, MaybeSync};
use fedimint_core::util::{BoxFuture, BoxStream};

use crate::db::OperationRecord;
use crate::federation::FederationInner;
use crate::{
    EcashReceiveState, EcashSendState, Error, ErrorCode, LnReceiveState, LnSendState,
    OnchainReceiveState, OnchainSendState, OperationId, RecoveryState, Result,
};

// The sealing module for `OperationState` and `OperationDetails`. `pub(crate)` so the facade
// modules can implement `Sealed` next to each state enum and details record they define, while
// staying unreachable from outside the crate.
pub(crate) mod sealed {
    /// Marker that only this crate can implement, sealing
    /// [`OperationState`](super::OperationState) and
    /// [`OperationDetails`](super::OperationDetails).
    pub trait Sealed {}
}

/// The progress of one operation, expressed as a flat state enum.
///
/// Each kind of operation has its own state type, [`EcashSendState`],
/// [`LnSendState`], [`OnchainReceiveState`], and so on, and this trait is
/// what they have in common: a state can say whether it is terminal.
///
/// The set of operation kinds is defined by this SDK and cannot be extended
/// from outside it.
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

/// The persisted, per-kind record of what an operation *is*, as opposed to
/// where it has got to.
///
/// Each kind of operation has its own record, `EcashSendDetails`,
/// `LnReceiveDetails`, `OnchainReceiveDetails`, and so on, each defined
/// beside the facade that creates that operation, and this trait is what
/// they have in common. It has no methods: a details record is plain data,
/// read field by field. Read one with [`Operation::details`].
///
/// An [`OperationId`] is all it takes to pick an operation back up, and this
/// record is half of what makes that true: the notes a sender must hand to a
/// receiver, the invoice a payee must show as a QR code, the deposit address
/// a depositor must display, and the terms an operation was executed on (a
/// lightning fee and route, for instance) all live here rather than only in
/// the value the original facade call returned, because
/// [`Operation::updates`] is not a replay and would not hand them back.
///
/// # The placement rule
///
/// Every value an operation exposes belongs in exactly one of three places:
///
/// 1. **Fixed when the operation is created, or when its quote was
///    executed: the details record, and only there.** The notes, the
///    invoice, the address, the destination, the resolved amounts, the fee
///    and route the quote committed to, the moment it started. None of it
///    ever changes.
/// 2. **Set by a transition and carried by every state from then on, final
///    states included: the state, and only there.** The preimage of a
///    successful lightning payment is the example: it exists exactly when
///    [`LnSendState::Success`](crate::LnSendState::Success) does.
/// 3. **Set by a transition but not carried by every later state: both.**
///    The state announces it; the record keeps it. The funding transaction a
///    caller learns from
///    [`WaitingForConfirmation`](crate::OnchainReceiveState::WaitingForConfirmation)
///    is gone by [`Claimed`](crate::OnchainReceiveState::Claimed), and a
///    lightning send's fee and route appear only on success; these are
///    exactly the values an `Option` field on a record is for, absent until
///    the fact is established, then set once and never changed again.
///
/// A caller never needs to have seen an earlier state: whatever it takes to
/// render or complete a reattached operation, [`Operation::details`] and the
/// current state supply between them.
// The record must be committed in the same storage transaction that creates the operation,
// before the creating call returns.
// - Fields fill in at most once and never move: set at creation, or set in the same write that
//   records the transition establishing them. `None` becomes `Some` at most once and a value
//   never changes to a different value or reverts.
// - Nothing is derived at read time from a state that may have been missed; persist a value as
//   soon as it is observed.
// - No secrets beyond what the caller already holds: bearer artifacts the caller owns are fine,
//   seed material never belongs here.
// - `Debug` is a supertrait bound (unlike on `OperationState`) because every record derives it
//   under this crate's `missing_debug_implementations` lint; types that must not appear in a log
//   (see `Notes`) redact their own `Debug` instead of relying on a container to omit them.
//
// Fedimint does not let an embedder join the transaction the module commits its own operation
// log entry in: no module's creation method takes a `dbtx`, and the `_dbtx` variants that would
// allow it hang off `ClientContext`, which only a module implementation holds
// (fedimint-client-module/src/module/mod.rs:400, :662, :865). So the record is written
// immediately after the module call returns and before the creating call hands back a handle,
// which is what the crate's durability contract actually promises (see the durability section on
// `Sdk`), and a crash in the window between the two commits is repaired by
// `FederationInner::reconcile_operations`, which rebuilds a record from the log entry the module
// did commit. See fedimint#TBD (filed with this pull request) for the upstream change that would
// close the window; the note is deleted once it lands.
pub trait OperationDetails:
    sealed::Sealed + Clone + core::fmt::Debug + Send + Sync + 'static
{
}

/// An [`OperationState`] whose kind persists an [`OperationDetails`] record.
///
/// This links the two halves of the pattern: it names, for one operation
/// kind, the record that kind persists, so [`Operation::details`] can be
/// written once for every kind that implements it. Not every kind does: a
/// recovery, for instance, has no fixed facts worth persisting, so
/// [`RecoveryState`](crate::RecoveryState) does not implement this trait.
pub trait DetailedOperationState: OperationState {
    /// The record [`Operation::details`] returns for this kind.
    type Details: OperationDetails;
}

/// A handle for observing one background operation.
///
/// An operation starts running the moment the facade call that created it
/// returns, and it keeps running whether or not anyone is watching. This
/// handle observes; it does not own. Dropping it, or an [`OperationUpdates`]
/// obtained from it, does not cancel, pause, or abort anything: the only
/// thing that ends an operation is reaching a final state. This holds across
/// restarts too: an operation is persisted as it progresses, resumes when the
/// SDK is built again over the same storage, and can be picked up again with
/// [`Federation::operation`](crate::Federation::operation). Most operations
/// have nothing to cancel, because the money has already moved into a
/// protocol that will resolve one way or the other; where a cancellation
/// genuinely exists it is a named request on that specific operation, see
/// [`Operation::<EcashSendState>::request_cancel`](crate::Operation::request_cancel),
/// and its outcome arrives as a state, not as the return value of the cancel
/// call.
///
/// Reattaching after a restart needs two things:
/// [`state`](Operation::state) or [`updates`](Operation::updates) for where
/// the operation has got to, which is not a history and does not replay
/// earlier states, and [`details`](Operation::details) for the facts fixed
/// when the operation was created that no state carries. The full path is
/// [`Federation::operation`](crate::Federation::operation) by id, the
/// matching accessor on [`AnyOperation`] for a typed handle, then those two
/// calls, nothing else.
///
/// A payment that fails, an invoice that expires, a deposit the federation
/// rejects: all of those are ordinary final states, reported as `Ok`. An
/// `Err` from any method here means something else went wrong, storage could
/// not be read, the federation could not be reached, the handle belongs to a
/// closed federation.
///
/// The handle is a cheap clone over shared state, like the other handles in
/// this crate.
pub struct Operation<S: OperationState> {
    inner: Arc<OperationInner>,
    driver: Arc<dyn Driver<S>>,
}

impl<S: OperationState> Clone for Operation<S> {
    /// A clone observes the same operation through the same driver; both are behind an `Arc`, so
    /// this costs two refcount bumps.
    fn clone(&self) -> Operation<S> {
        Operation {
            inner: self.inner.clone(),
            driver: self.driver.clone(),
        }
    }
}

impl<S: OperationState> fmt::Debug for Operation<S> {
    /// Hand-written rather than derived: a driver is a trait object with no `Debug`, and the
    /// state type has none either ([`OperationState`] does not require one).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Operation")
            .field("id", &self.inner.id)
            .finish_non_exhaustive()
    }
}

impl<S: OperationState> Operation<S> {
    /// Builds a typed handle over a loaded record and the driver for its kind.
    pub(crate) fn attach(inner: Arc<OperationInner>, driver: Arc<dyn Driver<S>>) -> Operation<S> {
        Operation { inner, driver }
    }

    /// The shared state behind this handle.
    ///
    /// For the facades: a method that lives on a concrete monomorphisation of [`Operation`]
    /// (there is one, [`request_cancel`](Operation::<crate::EcashSendState>::request_cancel)) is
    /// written in its own module and cannot reach a private field of this one.
    pub(crate) fn inner(&self) -> &Arc<OperationInner> {
        &self.inner
    }

    /// This operation's id, stable for its whole lifetime including across
    /// restarts.
    ///
    /// Persist it to find the operation again with
    /// [`Federation::operation`](crate::Federation::operation), or to
    /// correlate an [`ActivityItem`](crate::ActivityItem) with a live
    /// handle.
    pub fn id(&self) -> OperationId {
        OperationId::from_upstream(self.inner.id)
    }

    /// Reads the current state.
    ///
    /// This is a point-in-time snapshot: by the time the caller looks at
    /// it, the operation may have moved on. Use [`Operation::updates`] to
    /// follow it, or [`Operation::await_final`] to wait for the end.
    ///
    /// # Errors
    ///
    /// Only for infrastructure failures:
    /// [`Storage`](crate::ErrorCode::Storage) if the state cannot be read,
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) if the
    /// federation was closed or the SDK shut down, and
    /// [`Internal`](crate::ErrorCode::Internal) for a state this build cannot
    /// decode. A failed operation is `Ok` with a failure state, never an
    /// `Err`. Never
    /// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation): a
    /// typed handle exists only where [`AnyOperation::support`] was
    /// [`Observable`](OperationSupport::Observable), so a record this build
    /// cannot read is refused before a handle for it exists.
    pub async fn state(&self) -> Result<S> {
        self.inner.federation.ensure_open()?;
        let record = self.inner.reload().await?;
        let state = self
            .driver
            .current(&self.inner.federation, self.inner.id, &record)
            .await?;
        if state.is_final() {
            // "Persist a value as soon as it is observed": a history row must be able to say an
            // operation finished without decoding its state again, and this is one of the two
            // places a final state is first seen.
            self.inner
                .record_final_state(self.driver.encode_state(&state)?)
                .await?;
        }
        Ok(state)
    }

    /// Opens a new, independent subscription to this operation's states.
    ///
    /// The subscription yields the **current state first**, immediately,
    /// and then every subsequent transition. Two properties follow:
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
        OperationUpdates {
            inner: self.inner.clone(),
            driver: self.driver.clone(),
            stream: None,
            last: None,
            resubscribed: false,
            current_fallback_used: false,
            finished: false,
            handoff: None,
            closed: self.inner.federation.closed(),
        }
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
        let mut updates = self.updates();
        let mut last = None;
        while let Some(state) = updates.next().await? {
            let reached_the_end = state.is_final();
            last = Some(state);
            if reached_the_end {
                break;
            }
        }
        last.ok_or_else(|| {
            // Unreachable: `next` only answers `None` once it has handed out a final state, and
            // that hand-off is what fills `last`.
            Error::new(
                ErrorCode::Internal,
                "this operation's subscription closed without a final state",
            )
        })
    }
}

impl<S: DetailedOperationState> Operation<S> {
    /// Reads this operation's persisted details: the one-shot artifacts it
    /// produced and the terms it was executed on.
    ///
    /// This is the other half of observing an operation, beside
    /// [`state`](Operation::state). The state says where the operation has
    /// got to; this says what it is, the notes to hand over, the invoice to
    /// show, the address to display, the amounts, the fee and route that were
    /// committed to. See [`OperationDetails`]'s placement rule for which
    /// values appear here rather than on a state.
    ///
    /// Calling this twice returns the same values, with one exception: a
    /// field documented as filling in later goes from `None` to `Some` at
    /// most once and then never changes, and stays `None` if the fact it
    /// records never comes to exist. There is no ordering a caller has to get
    /// right between this call and [`state`](Operation::state).
    ///
    /// # Errors
    ///
    /// Only infrastructure failures,
    /// [`Storage`](crate::ErrorCode::Storage) if the record cannot be read,
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) if the
    /// federation was closed or the SDK shut down, and
    /// [`Internal`](crate::ErrorCode::Internal). Never
    /// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation): a
    /// typed handle exists only for an operation this build can observe, and
    /// that is checked once, earlier, by [`AnyOperation::supported_kind`].
    pub async fn details(&self) -> Result<S::Details> {
        self.inner.federation.ensure_open()?;
        let record = self.inner.reload().await?;
        let decoded = self.driver.decode_details(&record.details)?;
        match decoded.downcast::<S::Details>() {
            Ok(details) => Ok(*details),
            // Unreachable through the public API: a typed handle is only ever built with the
            // driver its own kind registered. It is an `Internal` rather than a panic because
            // the binding layer is built with `panic = "abort"`.
            Err(_) => Err(Error::new(
                ErrorCode::Internal,
                "this operation's details record does not match its kind",
            )),
        }
    }
}

/// One independent subscription to an operation's states.
///
/// Obtained from [`Operation::updates`]. Deliberately not `Clone`: a
/// subscriber is a single cursor, so call [`Operation::updates`] again for a
/// second, independent subscription instead of copying one.
///
/// Dropping a pending [`next`](OperationUpdates::next) future cancels only
/// that wait; the subscriber survives and a later `next()` resumes from the
/// same position with no transition lost. Dropping the subscriber itself ends
/// that subscription and nothing else: other subscribers keep their own
/// cursors, and the operation keeps running either way.
pub struct OperationUpdates<S: OperationState> {
    inner: Arc<OperationInner>,
    driver: Arc<dyn Driver<S>>,
    /// The stream this subscriber is reading, opened lazily on the first `next`.
    stream: Option<BoxStream<'static, Result<S>>>,
    /// The last state handed to the caller: this subscriber's cursor.
    last: Option<S>,
    /// Whether the stream has already been re-established once since the last hand-off.
    resubscribed: bool,
    /// Whether the direct-read fallback below has already been tried once.
    current_fallback_used: bool,
    /// Whether a final state has been handed out, after which there is nothing left to say.
    finished: bool,
    /// A final state that has been reached but not yet durably persisted and returned.
    ///
    /// Set before the persist that follows a final state, and cleared only once that persist
    /// has actually succeeded and the state is on its way back to the caller. This is what makes
    /// that stretch cancellation-safe despite containing an `await`: dropping `next` while this
    /// is `Some` loses nothing, because the next call retries the persist from here instead of
    /// re-reading the stream, and `last`/`finished` do not move until it succeeds.
    handoff: Option<S>,
    /// Fires when the federation stops running, for any reason.
    ///
    /// Raced against the stream on every `next`, because a stream whose federation has gone will
    /// never yield again and an outstanding `next` has to resolve rather than wait for a
    /// transition that is not coming.
    closed: tokio::sync::watch::Receiver<bool>,
}

impl<S: OperationState> fmt::Debug for OperationUpdates<S> {
    /// Hand-written for the same reason [`Operation`]'s is: a boxed stream and a driver have no
    /// `Debug`, and neither does the state type.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OperationUpdates")
            .field("id", &self.inner.id)
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

impl<S: OperationState> OperationUpdates<S> {
    /// Waits for the next state.
    ///
    /// The three possible answers each mean exactly one thing:
    ///
    /// - `Ok(Some(state))`: the operation is in this state now. The very
    ///   first call returns the current state without waiting; later calls
    ///   resolve when the operation transitions.
    /// - `Ok(None)`: a final state was already yielded and the subscription
    ///   closed cleanly. This is the normal end of the stream, and further
    ///   calls keep returning `Ok(None)`.
    /// - `Err(_)`: an infrastructure failure. The subscription may not be
    ///   resumable afterwards; obtain a fresh one from
    ///   [`Operation::updates`] and, if the error was
    ///   [`FederationClosed`](crate::ErrorCode::FederationClosed), a fresh
    ///   [`Operation`] handle first.
    ///
    /// An operation that failed ends with `Ok(Some(failure state))` followed
    /// by `Ok(None)`. `Err` never carries the outcome of an operation, only
    /// the failure of observing it.
    ///
    /// This call is cancellation-safe: dropping the future it returns before
    /// it resolves cancels only that one wait. The subscriber remains usable,
    /// the cursor does not move, and no transition is lost, a state the
    /// operation reached while no future was pending is still delivered by
    /// the following `next()`. That is what makes it safe to race against a
    /// timeout, put in a `select!`, or abandon when a screen closes. Dropping
    /// the subscriber itself is the different event that ends the
    /// subscription; either way the operation itself keeps running.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage),
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed), or
    /// [`Internal`](crate::ErrorCode::Internal).
    pub async fn next(&mut self) -> Result<Option<S>> {
        // The cursor is `self.last`, `self.resubscribed`, `self.current_fallback_used`,
        // `self.finished` and `self.handoff`, and every one of them lives on the subscriber
        // rather than inside this future. For a
        // non-final state that is enough on its own: it is taken from the stream and returned in
        // the same poll, with no `await` in between, so dropping this future either way leaves
        // nothing half-done. A final state has one `await` on the way out, the persist below,
        // and `self.handoff` is what makes that stretch safe too: it is set before the persist
        // and only cleared once the persist has actually succeeded, so `last`/`finished` never
        // move ahead of a write that might not have happened, and a dropped or failed persist is
        // retried by the next call instead of losing the state it was about to hand out.
        loop {
            if let Some(state) = self.handoff.clone() {
                let encoded = self.driver.encode_state(&state)?;
                self.inner.record_final_state(encoded).await?;
                self.handoff = None;
                self.last = Some(state.clone());
                self.finished = true;
                return Ok(Some(state));
            }
            if self.finished {
                return Ok(None);
            }
            self.inner.federation.ensure_open()?;
            if self.stream.is_none() {
                // Reloaded rather than the cached `self.inner.record`: a resubscribe can follow a
                // `phase` write the first subscription's driver made after this handle was
                // created, and a driver that folds its starting point from `phase` needs that
                // write to be visible or it resumes from the wrong place.
                let record = self.inner.reload().await?;
                let stream = self
                    .driver
                    .subscribe(&self.inner.federation, self.inner.id, &record)
                    .await?;
                self.stream = Some(stream);
            }
            // The check above only catches a federation that had already stopped when this call
            // started. Racing the stream against the federation's `closed` watch is what catches
            // one that stops while this call is parked: after that the stream will never yield
            // again, and the contract is that every outstanding `next` resolves promptly rather
            // than waiting for a transition that is not coming.
            let mut watch_fired = false;
            let item = {
                let OperationUpdates { stream, closed, .. } = self;
                let Some(stream) = stream.as_mut() else {
                    // Unreachable: the block above just established it. An `Internal` rather
                    // than an `expect`, because the binding layer is built with
                    // `panic = "abort"`.
                    return Err(Error::new(
                        ErrorCode::Internal,
                        "this operation's subscription went missing",
                    ));
                };
                tokio::select! {
                    item = futures::StreamExt::next(stream) => item,
                    _ = closed.changed() => {
                        watch_fired = true;
                        None
                    }
                }
            };
            if watch_fired {
                // `changed` also resolves once the sender is gone, which cannot happen while
                // this subscriber holds the federation through an `Arc`; reading that as a stop
                // is both the safe answer and what keeps this loop from spinning. The value
                // itself is read back rather than assumed, because a federation that comes back
                // to life flips the watch the other way and this subscriber simply carries on.
                if self.closed.has_changed().is_err() || *self.closed.borrow_and_update() {
                    return Err(Error::new(
                        ErrorCode::FederationClosed,
                        "this federation stopped running",
                    ));
                }
                continue;
            }
            match item {
                Some(Ok(state)) => {
                    // Taken from the stream and either returned or handed to `self.handoff` in
                    // this same poll, with no `await` in between, so the caller either receives
                    // the state (a non-final one, directly; a final one, once the handoff above
                    // has persisted it) or this state never left the stream at all.
                    if let Some(previous) = &self.last
                        && self.driver.same_state(previous, &state)
                    {
                        continue;
                    }
                    self.resubscribed = false;
                    if state.is_final() {
                        self.handoff = Some(state);
                        continue;
                    }
                    self.last = Some(state.clone());
                    return Ok(Some(state));
                }
                Some(Err(err)) => return Err(err),
                None => {
                    // Belt-and-braces: `self.last` and `self.finished` are only ever set
                    // together, by the handoff retry above, so a final `self.last` implies
                    // `self.finished` already returned `Ok(None)` at the top of this loop before
                    // the stream was ever polled again. Kept as a guard rather than removed,
                    // since nothing prevents a future change from setting one without the other.
                    if self.last.as_ref().is_some_and(OperationState::is_final) {
                        self.finished = true;
                        return Ok(None);
                    }
                    // This subscription has not handed out anything yet, so an empty stream here
                    // is not necessarily lag: a driver's stream is contractually supposed to
                    // yield the current state first, but a driver whose current state is already
                    // final can have nothing left to stream for an operation that settled before
                    // this subscription started. A direct read settles which one it was, once,
                    // before the lag-recovery path below has a chance to declare the subscription
                    // cut off over what may just be a driver with nothing to add to a state it
                    // already reported through `current`.
                    if self.last.is_none() && !self.current_fallback_used {
                        self.current_fallback_used = true;
                        self.stream = None;
                        let record = self.inner.reload().await?;
                        let state = self
                            .driver
                            .current(&self.inner.federation, self.inner.id, &record)
                            .await?;
                        if state.is_final() {
                            self.handoff = Some(state);
                            continue;
                        }
                        self.last = Some(state.clone());
                        return Ok(Some(state));
                    }
                    // A stream that ended without a final state did not finish, it was cut off:
                    // the client's notifier ends a subscriber's stream when it falls more than
                    // ten thousand transitions behind, with only a log line to say so
                    // (`fedimint-client-module/src/sm/notifier.rs:146-154`, and the lag
                    // error it reacts to, in
                    // `fedimint-core/src/util/broadcaststream.rs:61-67`).
                    // Subscribing again re-reads the current state, which the dedupe above drops,
                    // and carries on. Once per hand-off, so a stream that cannot be established
                    // is reported rather than spun on.
                    if self.resubscribed {
                        return Err(Error::new(
                            ErrorCode::Internal,
                            "this operation's updates could not be followed: the subscription \
                             ended twice without reaching a final state",
                        ));
                    }
                    self.resubscribed = true;
                    self.stream = None;
                }
            }
        }
    }
}

/// An operation whose kind is known only at runtime.
///
/// Returned by [`Federation::operation`](crate::Federation::operation),
/// which looks an operation up by id and therefore cannot know statically
/// what kind it is: the id may have come from persisted state, from an
/// [`ActivityItem`](crate::ActivityItem), or from another process's
/// notification. Read [`AnyOperation::kind`] to find out, then use the
/// matching accessor to recover a typed [`Operation`].
///
/// Like [`Operation`], this is an observation handle over a detached,
/// persisted operation, and it is a cheap clone.
///
/// # What "supported" means here
///
/// An operation is supported when this build can actually observe its typed
/// state, which takes two things: the persisted discriminator maps onto a
/// known [`OperationKind`] other than [`Unknown`](OperationKind::Unknown),
/// and the persisted state schema is one this build reads (a record written
/// by a newer SDK can name a kind this build knows while using a state schema
/// it has never seen).
///
/// Four calls answer four different questions about a record:
/// [`kind`](AnyOperation::kind) says what it is and never
/// fails; [`support`](AnyOperation::support) says how far this build can go
/// with it and why no further, as a plain value for logs and bug reports;
/// [`supported_kind`](AnyOperation::supported_kind) is the gate to pass
/// before acting on the operation, returning
/// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation) when it
/// is not supported; and [`raw_kind`](AnyOperation::raw_kind) gives the
/// persisted discriminator verbatim, for logs and bug reports.
///
/// The seven `as_*` accessors return `Some` only when the kind matches and
/// the operation is supported, and `None` otherwise, without distinguishing
/// a plain kind mismatch from an unsupported record of the matching kind:
/// use [`support`](AnyOperation::support) or
/// [`supported_kind`](AnyOperation::supported_kind) first when the answer
/// matters, for example before showing an error to the user. This keeps a
/// caller from being handed a typed [`Operation`] whose state later fails to
/// decode from [`Operation::state`] instead of from this gate.
///
/// This determination is made before any state is read, so it is not a
/// promise that reading the state will succeed. A record whose schema is
/// unrecorded is treated as not ruled out. The residue, a record that passed
/// both checks and still cannot be decoded, surfaces as
/// [`Internal`](crate::ErrorCode::Internal) from [`Operation::state`], not as
/// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation).
#[derive(Debug, Clone)]
pub struct AnyOperation {
    inner: Arc<AnyOperationInner>,
}

impl AnyOperation {
    /// This operation's id.
    pub fn id(&self) -> OperationId {
        OperationId::from_upstream(self.inner.operation.id)
    }

    /// What kind of operation this is.
    ///
    /// Never fails: an operation recorded by a build that understood
    /// something this one does not still has an id, still has a row, and
    /// reports [`OperationKind::Unknown`] here, see that variant. Use
    /// [`raw_kind`](AnyOperation::raw_kind) to find out what it was recorded
    /// as, and [`supported_kind`](AnyOperation::supported_kind) instead of
    /// this when the next thing you do is act on the operation rather than
    /// label it.
    ///
    /// This is the reading of the discriminator and nothing more, so it
    /// answers with a real kind even when nothing can be done with the
    /// operation: a record whose state was written at a schema this build
    /// cannot read still says it is a lightning send, reported as unsupported
    /// by [`support`](AnyOperation::support).
    pub fn kind(&self) -> OperationKind {
        self.inner.kind
    }

    /// How far this build can go with this operation, and why no further.
    ///
    /// The reason behind [`supported_kind`](AnyOperation::supported_kind), as
    /// an ordinary value instead of an error, for a log line, a bug report,
    /// or a message to a user. [`Observable`](OperationSupport::Observable)
    /// means supported: the matching `as_*` accessor will hand back a typed
    /// handle. Every other variant names the reason it will not, pair it with
    /// [`raw_kind`](AnyOperation::raw_kind) to say which record it was about.
    ///
    /// Infallible and cheap: it reads no storage, touches no network, and
    /// does not read the operation's state, so it is not a promise that
    /// reading the state will succeed.
    // "`Observable` means supported: the matching `as_*` accessor will hand back a typed handle"
    // is accurate once every kind in `kinds` has a driver arm in `driver_for` below, filled in by
    // T7, T8, T9 and T12. Until then, `support_of` still answers `Observable` for the six kinds
    // whose arm is an unconditional `None` (every kind but `ECASH_SEND`, which has its own test-
    // only probe arm and note): the record's kind and schema version are all `support_of` looks
    // at, and neither says whether a driver has been written yet. So the matching `as_*` accessor
    // on those six returns `None` regardless. This is not a bug in the accessor, which is honest
    // about what it can do, but a temporary gap between what `support` promises and what a build
    // this incomplete can deliver; it closes as each task above lands its arm.
    pub fn support(&self) -> OperationSupport {
        self.inner.support
    }

    /// This operation's kind if this build can observe its typed state, and
    /// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation) if it
    /// cannot.
    ///
    /// The fallible twin of [`kind`](AnyOperation::kind) and the gate to pass
    /// before acting on an operation. `Ok(kind)` means the accessor for that
    /// kind will hand back a typed handle: it is never
    /// `Ok(OperationKind::Unknown)`, and never `Ok` for a record whose state
    /// was written at a schema version newer than this build reads, even when
    /// the kind itself is one this build knows.
    /// [`support`](AnyOperation::support) answers the same question and says
    /// which condition failed, without an error.
    ///
    /// # Errors
    ///
    /// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation), and
    /// nothing else, for an unrecognised discriminator or a state schema
    /// newer than this build reads. This reads no storage and touches no
    /// network: the record was already read to produce this handle.
    pub fn supported_kind(&self) -> Result<OperationKind> {
        supported_kind_of(self.kind(), &self.raw_kind())
    }

    /// The discriminator this operation was persisted under, verbatim.
    ///
    /// [`kind`](AnyOperation::kind) is this SDK's reading of the record; this
    /// is what the record actually says. The difference matters when the
    /// reading is [`OperationKind::Unknown`]: an application that reports the
    /// module and tag it did not recognise gives a user something to show
    /// and a maintainer something to fix. Available for every kind, not only
    /// the unknown one.
    ///
    /// For humans, never for control flow: use [`kind`](AnyOperation::kind)
    /// and [`supported_kind`](AnyOperation::supported_kind) to branch on.
    /// Infallible, like [`kind`](AnyOperation::kind): the record was read
    /// when this handle was created.
    pub fn raw_kind(&self) -> RawOperationKind {
        self.inner.raw.clone()
    }

    /// Recovers a typed handle if this is an out-of-band ecash send.
    ///
    /// `None` for any other kind, for a record this build cannot interpret,
    /// and for a record of *this* kind whose typed state this build cannot
    /// observe; see the type documentation for how to tell those apart.
    pub fn as_ecash_send(&self) -> Option<Operation<EcashSendState>> {
        match driver_for(&self.inner.raw.kind)? {
            ErasedDriver::EcashSend(driver) => self.typed(OperationKind::EcashSend, driver),
            // A tag whose driver observes another state type, which `typed`'s own kind check
            // would refuse in any case.
            _ => None,
        }
    }

    /// Recovers a typed handle if this is an ecash redemption.
    ///
    /// `None` for any other kind, for a record this build cannot interpret,
    /// and for a record of *this* kind whose typed state this build cannot
    /// observe; see the type documentation for how to tell those apart.
    pub fn as_ecash_receive(&self) -> Option<Operation<EcashReceiveState>> {
        match driver_for(&self.inner.raw.kind)? {
            ErasedDriver::EcashReceive(driver) => self.typed(OperationKind::EcashReceive, driver),
            _ => None,
        }
    }

    /// Recovers a typed handle if this is an outgoing lightning payment.
    ///
    /// `None` for any other kind, for a record this build cannot interpret,
    /// and for a record of *this* kind whose typed state this build cannot
    /// observe; see the type documentation for how to tell those apart.
    pub fn as_ln_send(&self) -> Option<Operation<LnSendState>> {
        match driver_for(&self.inner.raw.kind)? {
            ErasedDriver::LnSend(driver) => self.typed(OperationKind::LnSend, driver),
            _ => None,
        }
    }

    /// Recovers a typed handle if this is an incoming lightning payment.
    ///
    /// `None` for any other kind, for a record this build cannot interpret,
    /// and for a record of *this* kind whose typed state this build cannot
    /// observe; see the type documentation for how to tell those apart.
    pub fn as_ln_receive(&self) -> Option<Operation<LnReceiveState>> {
        match driver_for(&self.inner.raw.kind)? {
            ErasedDriver::LnReceive(driver) => self.typed(OperationKind::LnReceive, driver),
            _ => None,
        }
    }

    /// Recovers a typed handle if this is an on-chain withdrawal.
    ///
    /// `None` for any other kind, for a record this build cannot interpret,
    /// and for a record of *this* kind whose typed state this build cannot
    /// observe; see the type documentation for how to tell those apart.
    pub fn as_onchain_send(&self) -> Option<Operation<OnchainSendState>> {
        match driver_for(&self.inner.raw.kind)? {
            ErasedDriver::OnchainSend(driver) => self.typed(OperationKind::OnchainSend, driver),
            _ => None,
        }
    }

    /// Recovers a typed handle if this is an on-chain deposit.
    ///
    /// `None` for any other kind, for a record this build cannot interpret,
    /// and for a record of *this* kind whose typed state this build cannot
    /// observe; see the type documentation for how to tell those apart.
    pub fn as_onchain_receive(&self) -> Option<Operation<OnchainReceiveState>> {
        match driver_for(&self.inner.raw.kind)? {
            ErasedDriver::OnchainReceive(driver) => {
                self.typed(OperationKind::OnchainReceive, driver)
            }
            _ => None,
        }
    }

    /// Recovers a typed handle if this is a seed recovery.
    ///
    /// `None` for any other kind, for a record this build cannot interpret,
    /// and for a record of *this* kind whose typed state this build cannot
    /// observe; see the type documentation for how to tell those apart.
    ///
    /// A process that dies mid-rescan leaves a persisted recovery running; on
    /// the next build, [`Federation::operation`](crate::Federation::operation)
    /// finds it and [`kind`](AnyOperation::kind) reports
    /// [`OperationKind::Recovery`], and this is how its progress is then
    /// observed.
    ///
    /// This path needs the operation id, so it is the one to use when the
    /// application kept it. When it did not,
    /// [`Sdk::recovery_status`](crate::Sdk::recovery_status) and
    /// [`Sdk::resume_recovery`](crate::Sdk::resume_recovery) reach the same
    /// recovery from the [`FederationId`](crate::FederationId) alone; the
    /// [recovery module](crate::Recovery) lays out all three routes.
    pub fn as_recovery(&self) -> Option<Operation<RecoveryState>> {
        match driver_for(&self.inner.raw.kind)? {
            ErasedDriver::Recovery(driver) => self.typed(OperationKind::Recovery, driver),
            _ => None,
        }
    }

    /// Builds a type-erased handle over a record that has already been read.
    ///
    /// The whole support decision is made here, once, so that the four questions the type
    /// answers cost nothing afterwards.
    pub(crate) fn from_record(operation: Arc<OperationInner>) -> AnyOperation {
        let raw = RawOperationKind {
            kind: operation.record.kind.clone(),
            // An empty module is a record that names none, not a module called "".
            module: (!operation.record.module.is_empty()).then(|| operation.record.module.clone()),
            schema_version: Some(operation.record.schema_version),
        };
        let kind = kind_of_tag(&raw.kind);
        let support = support_of(kind, &raw);
        AnyOperation {
            inner: Arc::new(AnyOperationInner {
                operation,
                kind,
                raw,
                support,
            }),
        }
    }

    /// The typed handle for one kind, when this record is of that kind and this build can
    /// observe it.
    ///
    /// The caller has already found the driver, so the two conditions left are the ones about
    /// the record. Together with "this build has a driver for the kind" they are deliberately
    /// not distinguished: the accessors answer `None` for all three, and a caller who needs to
    /// know which uses [`support`](AnyOperation::support) first.
    fn typed<S>(&self, kind: OperationKind, driver: Arc<dyn Driver<S>>) -> Option<Operation<S>>
    where
        S: OperationState,
    {
        if self.inner.kind != kind {
            return None;
        }
        if !matches!(self.inner.support, OperationSupport::Observable) {
            return None;
        }
        Some(Operation::attach(self.inner.operation.clone(), driver))
    }
}

/// The discriminator an operation was persisted under, as it was written.
///
/// Returned by [`AnyOperation::raw_kind`]. This is the record's own account
/// of itself, kept readable so that a build which cannot interpret an
/// operation can still say what it could not interpret, and so that a build
/// which can is not left unable to report the schema it read.
///
/// These fields are diagnostics: log them, show them in a bug report, put
/// them behind a "details" disclosure. Do not branch on them; use
/// [`OperationKind`], [`OperationSupport`] and
/// [`AnyOperation::supported_kind`] for control flow instead.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct RawOperationKind {
    /// The operation-kind tag as persisted, verbatim and unnormalised.
    ///
    /// For an operation this SDK wrote, the tag it wrote for it; for a
    /// record written by another build or another module set, whatever that
    /// build recorded. Always present: a record with no readable
    /// discriminator at all is not a record this crate can produce a handle
    /// for.
    pub kind: String,
    /// The federation module the record belongs to, when the record names
    /// one separately from [`kind`](RawOperationKind::kind), for example
    /// `"mint"`, `"ln"`, `"wallet"`, or a module this build has never heard
    /// of.
    ///
    /// `None` when the persisted form carries no separate module marker,
    /// which is not a failure to read one: some records simply do not have
    /// it.
    pub module: Option<String>,
    /// The schema version the record was written with, when one was
    /// recorded.
    ///
    /// `None` when the record predates versioning or does not carry a
    /// version; that is not the same as this build knowing the version is
    /// safe to read. Use [`AnyOperation::support`] for the verdict rather
    /// than comparing this value directly.
    pub schema_version: Option<u32>,
}

/// What kind of work an operation is doing.
///
/// Reported by [`AnyOperation::kind`] and carried on
/// [`ActivityItem`](crate::ActivityItem), so that a history screen can
/// label and group rows without having to resolve each one to a typed
/// handle first.
///
/// `#[non_exhaustive]`: new kinds arrive with new modules, and Rust callers
/// must include a wildcard arm. [`Unknown`](OperationKind::Unknown) is a real
/// variant every binding already has, so a kind added later is reported
/// through it rather than left undecodable.
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
    /// Restoring a wallet from its seed, tracked by
    /// [`RecoveryState`](crate::RecoveryState).
    Recovery,
    /// An operation this SDK version cannot interpret.
    ///
    /// Persisted operations outlive the version that created them: an
    /// application may be downgraded, or a federation may have been used
    /// with a build that supported a module this one does not. Such an
    /// operation is still real, still recorded, and still identifiable by
    /// id. What was actually persisted stays readable through
    /// [`AnyOperation::raw_kind`]: the tag, the module it belonged to, and
    /// the schema version it was written at.
    ///
    /// None of the typed accessors on [`AnyOperation`] match it; they return
    /// `None`, as they do for a mismatched kind.
    ///
    /// In [activity history](crate::ActivityItem) such a row reports
    /// [`ActivityStatus::Unknown`](crate::ActivityStatus::Unknown) rather
    /// than a guessed outcome, with
    /// [`ActivityItem::is_final`](crate::ActivityItem::is_final) still
    /// answering whether it has finished.
    Unknown,
}

/// How far this build can go with one persisted operation, decided before any
/// of its state is read.
///
/// Returned by [`AnyOperation::support`].
/// [`Observable`](OperationSupport::Observable) is the one answer that means
/// supported; every other variant names the condition that failed and means
/// there is no typed handle to be had in this build. They are different
/// things to say and to do: "written by a newer version than this one" is an
/// application that needs updating, "this build does not recognise it at
/// all" is one to put in a bug report verbatim.
/// [`AnyOperation::supported_kind`] flattens both into
/// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation) for
/// control flow, but a log line, a support ticket, or a message shown to a
/// user should use this type instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OperationSupport {
    /// This build can observe the operation's typed state: the kind is one it
    /// knows, and the recorded state schema is one it reads.
    ///
    /// The matching `as_*` accessor on [`AnyOperation`] returns `Some`, and
    /// [`AnyOperation::supported_kind`] returns `Ok`. This is everything
    /// establishable before the state is read, which is not the same as a
    /// promise that reading it will succeed.
    Observable,
    /// The persisted discriminator is not one this build maps onto a kind, so
    /// there is nothing to interpret it as.
    ///
    /// [`AnyOperation::kind`] reports [`OperationKind::Unknown`] for the same
    /// record, and [`AnyOperation::raw_kind`] says what was actually written.
    UnknownKind,
    /// The kind is known, but the record's state was written at a schema
    /// version newer than this build reads.
    ///
    /// This describes a gap between the record and this build, not a defect
    /// in the record: the operation is real, the state is intact, and a
    /// build of the version that wrote it reads it fine.
    StateSchemaTooNew,
}

/// The newest operation-state schema version this build can read, for every
/// kind it knows.
///
/// One number today, because every kind's state schema is at its first
/// version and they were introduced together. It is reached through
/// `OperationKind::readable_state_schema` rather than compared directly, so
/// that a kind whose state schema is later revised on its own becomes a
/// one-line divergence there instead of a redesign here.
pub(crate) const READABLE_STATE_SCHEMA: u32 = 1;

impl OperationKind {
    /// The newest state schema version this build can read for this kind.
    ///
    /// Zero for [`Unknown`](OperationKind::Unknown): this build reads no
    /// version of an unknown kind's state. The value is never consulted for
    /// one anyway, because `support_of` refuses such a record on the first of
    /// the two conditions, before any version is compared.
    ///
    /// The match is exhaustive rather than defaulting, even though
    /// [`OperationKind`] is `#[non_exhaustive]` (which binds only outside this
    /// crate): a kind added later must state which schema this build reads
    /// for it, and a compile error is the right way to be asked.
    const fn readable_state_schema(self) -> u32 {
        match self {
            OperationKind::EcashSend
            | OperationKind::EcashReceive
            | OperationKind::LnSend
            | OperationKind::LnReceive
            | OperationKind::OnchainSend
            | OperationKind::OnchainReceive
            | OperationKind::Recovery => READABLE_STATE_SCHEMA,
            OperationKind::Unknown => 0,
        }
    }
}

/// The whole support decision, as a pure function of what the record says.
///
/// Kept apart from [`AnyOperation::support`] because this is the part worth
/// testing: it takes the two values that method reads off the handle and
/// nothing else, so the whole answer is checkable without a live operation.
///
/// The order of the checks is the order of the two conditions on
/// [`AnyOperation`], and the first is a precondition of the second: an
/// unrecognised discriminator has no kind whose state schema could be
/// compared.
fn support_of(kind: OperationKind, raw: &RawOperationKind) -> OperationSupport {
    if matches!(kind, OperationKind::Unknown) {
        return OperationSupport::UnknownKind;
    }
    match raw.schema_version {
        Some(recorded) if recorded > kind.readable_state_schema() => {
            OperationSupport::StateSchemaTooNew
        }
        // Either a version this build reads, or a record that carries none,
        // which is not a failure to read one, so nothing known here rules the
        // record out. That is as far as a check made before the state is read
        // can honestly go.
        _ => OperationSupport::Observable,
    }
}

/// [`AnyOperation::supported_kind`]'s answer, as a pure function of what the
/// record says, so that the gate and the reason behind it cannot drift apart.
fn supported_kind_of(kind: OperationKind, raw: &RawOperationKind) -> Result<OperationKind> {
    // Exhaustive, with no wildcard: a reason added later must have its
    // sentence written here rather than silently reaching a caller as
    // "cannot interpret this".
    let because = match support_of(kind, raw) {
        OperationSupport::Observable => return Ok(kind),
        OperationSupport::UnknownKind => {
            "this build does not recognise that kind of operation".to_owned()
        }
        OperationSupport::StateSchemaTooNew => format!(
            "its state was written at a newer schema version than this build reads (up to {})",
            kind.readable_state_schema(),
        ),
    };
    Err(Error::new(
        ErrorCode::UnsupportedOperation,
        format!(
            "cannot act on the operation recorded as {}: {because}",
            describe_record(raw)
        ),
    ))
}

/// Renders a persisted discriminator for the one error message that carries
/// it, so that a log line says which record was refused.
///
/// For humans only, like [`RawOperationKind`] itself: the fields are quoted
/// and labelled rather than formatted for parsing.
fn describe_record(raw: &RawOperationKind) -> String {
    let mut described = format!("{:?}", raw.kind);
    if let Some(module) = &raw.module {
        described.push_str(&format!(" in module {module:?}"));
    }
    if let Some(version) = raw.schema_version {
        described.push_str(&format!(" at schema version {version}"));
    }
    described
}

/// The SDK's own operation-kind tags, the vocabulary [`OperationRecord::kind`] is written in.
///
/// SDK-owned rather than the module kind the client records, because the two do not line up in
/// either direction: one module produces several kinds (the mint produces both an out-of-band
/// send and a redemption) and one kind spans two module generations (a lightning send is `ln` on
/// one federation and `lnv2` on another). A tag is written once when an operation is created and
/// read for the life of the record, so these strings are storage format: change one and every
/// record written before the change reads back as [`OperationKind::Unknown`].
pub(crate) mod kinds {
    /// Ecash spent out of band.
    pub(crate) const ECASH_SEND: &str = "ecash_send";
    /// Ecash notes redeemed into the balance.
    pub(crate) const ECASH_RECEIVE: &str = "ecash_receive";
    /// An outgoing lightning payment.
    pub(crate) const LN_SEND: &str = "ln_send";
    /// An incoming lightning payment.
    pub(crate) const LN_RECEIVE: &str = "ln_receive";
    /// An on-chain withdrawal.
    pub(crate) const ONCHAIN_SEND: &str = "onchain_send";
    /// An on-chain deposit.
    pub(crate) const ONCHAIN_RECEIVE: &str = "onchain_receive";
    /// Restoring a wallet from its seed.
    pub(crate) const RECOVERY: &str = "recovery";
}

/// This build's reading of a persisted kind tag.
///
/// A tag this build does not know is [`OperationKind::Unknown`], which is a real answer rather
/// than a failure: the operation is still recorded, still has an id, and
/// [`AnyOperation::raw_kind`] still says what it was written as.
pub(crate) fn kind_of_tag(tag: &str) -> OperationKind {
    match tag {
        kinds::ECASH_SEND => OperationKind::EcashSend,
        kinds::ECASH_RECEIVE => OperationKind::EcashReceive,
        kinds::LN_SEND => OperationKind::LnSend,
        kinds::LN_RECEIVE => OperationKind::LnReceive,
        kinds::ONCHAIN_SEND => OperationKind::OnchainSend,
        kinds::ONCHAIN_RECEIVE => OperationKind::OnchainReceive,
        kinds::RECOVERY => OperationKind::Recovery,
        _ => OperationKind::Unknown,
    }
}

/// How one kind of operation is observed.
///
/// One implementation per state enum, written beside the facade that creates that kind. The
/// engine in this module owns everything the API promises about *observing* an operation, and a
/// driver owns everything specific to one kind: which client call to make, how to fold the
/// module's own state machine onto the SDK's state enum, and how the details record is spelled
/// as JSON.
///
/// Object-safe on purpose: [`driver_for`] answers with one of these per kind, and
/// `AnyOperation`'s accessors hand the matching one to a typed [`Operation`].
//
// `MaybeSend`/`MaybeSync` and `BoxFuture`/`BoxStream` rather than `Send`/`Sync` and
// `Box<dyn Future + Send>`: on `target_family = "wasm"` these expand to no bound at all
// (`fedimint-core/src/task.rs:504-536`, `fedimint-core/src/util/mod.rs:31-35`),
// which is what lets one set of types compile for both a threaded host and a browser.
//
// Every method takes the federation rather than a `&Client`, so that a driver decides for itself
// whether it needs a live client and with which `fund_touching` value, and so the engine's own
// unit tests can drive it with no client at all. The lifetimes are named rather than elided:
// with `&self` and a second reference argument, an elided output lifetime binds to `&self` and no
// implementation that touches the other argument compiles.
pub(crate) trait Driver<S>: MaybeSend + MaybeSync + 'static
where
    S: OperationState,
{
    /// The state the operation is in now, read without subscribing.
    fn current<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: UpstreamOperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<S>>;

    /// A fresh, independent stream that yields the current state first and then every
    /// transition, ending once a final state has been yielded.
    ///
    /// The stream may also end without a final state, which is not the operation finishing: the
    /// client's notifier ends a subscriber's stream when it falls behind, and the engine treats
    /// that as a signal to subscribe again.
    fn subscribe<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: UpstreamOperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<S>>>>;

    /// Whether `next` says nothing `previous` did not already say.
    ///
    /// Every real state enum answers `previous == next`. It is a method rather than a
    /// `PartialEq` bound because [`OperationState`] deliberately does not require one, and
    /// adding a supertrait to a public trait to serve a private engine would be the wrong way
    /// round. The engine needs it because the mappings are many-to-one: two different upstream
    /// events can be the same state here, and a subscriber must not see the same state twice.
    fn same_state(&self, previous: &S, next: &S) -> bool;

    /// The state as it is persisted on [`OperationRecord::final_state`].
    ///
    /// Called only for a final state, so that a history row can report a finished operation as
    /// finished without decoding it.
    fn encode_state(&self, state: &S) -> Result<String>;

    /// The details record this kind persisted at creation, decoded from its JSON.
    ///
    /// Type-erased because [`Operation::details`] is one generic body over every kind: the
    /// concrete type is `S::Details`, which exists only for an `S` that implements
    /// [`DetailedOperationState`], and a trait object cannot name it. The caller downcasts
    /// straight back, so the erasure never escapes this module.
    ///
    /// A kind with no details record ([`RecoveryState`](crate::RecoveryState)) returns
    /// [`Internal`](crate::ErrorCode::Internal); no caller can reach it, because
    /// [`Operation::details`] does not exist for such a kind.
    fn decode_details(&self, json: &str) -> Result<Box<dyn Any + Send + Sync>>;
}

/// Rebuilds an SDK operation record from the client's own operation log entry.
///
/// The operation log is authoritative and the SDK's record is a decoration over it, so a crash
/// between the module's own commit and the SDK's write leaves an entry with no record. A facade
/// registers one of these for each module kind it owns and reconciliation asks each in turn.
pub(crate) trait Backfiller: MaybeSend + MaybeSync + 'static {
    /// What the SDK would have written for this entry, or `None` if this backfiller does not
    /// recognise it.
    ///
    /// `module_kind` is [`OperationLogEntry::operation_module_kind`], and `meta` is the entry's
    /// own JSON, read with `try_meta` so that a shape this build does not know is a `None` here
    /// rather than a panic.
    fn backfill(&self, module_kind: &str, meta: &serde_json::Value) -> Option<Backfilled>;
}

/// What a [`Backfiller`] recovered from a log entry.
#[derive(Debug, Clone)]
pub(crate) struct Backfilled {
    /// The SDK kind tag, from [`kinds`].
    pub(crate) kind: &'static str,
    /// The details record as JSON, rebuilt from the module's own meta.
    pub(crate) details: String,
    /// The phase the entry proves was reached, if the meta says.
    pub(crate) phase: Option<u32>,
}

/// A driver for one of the seven kinds, with its state type recovered by matching.
///
/// [`driver_for`] is one function over the whole kind vocabulary, so it has one return type,
/// while a [`Driver<S>`](Driver) is generic in the state enum it observes and the seven state
/// enums share nothing a trait object could name. This enum is the join: the caller knows which
/// variant the kind it asked about must be, and a variant that does not match is the same `None`
/// as no driver at all.
pub(crate) enum ErasedDriver {
    /// Observes [`EcashSendState`](crate::EcashSendState).
    EcashSend(Arc<dyn Driver<EcashSendState>>),
    /// Observes [`EcashReceiveState`](crate::EcashReceiveState).
    EcashReceive(Arc<dyn Driver<EcashReceiveState>>),
    /// Observes [`LnSendState`](crate::LnSendState).
    LnSend(Arc<dyn Driver<LnSendState>>),
    /// Observes [`LnReceiveState`](crate::LnReceiveState).
    LnReceive(Arc<dyn Driver<LnReceiveState>>),
    /// Observes [`OnchainSendState`](crate::OnchainSendState).
    OnchainSend(Arc<dyn Driver<OnchainSendState>>),
    /// Observes [`OnchainReceiveState`](crate::OnchainReceiveState).
    OnchainReceive(Arc<dyn Driver<OnchainReceiveState>>),
    /// Observes [`RecoveryState`](crate::RecoveryState).
    Recovery(Arc<dyn Driver<RecoveryState>>),
}

/// The driver this build observes `kind` with, if it has one.
///
/// A build-wide lookup rather than per-federation state, because which kinds can be observed is
/// a property of the build. A record only ever carries a tag the federation that wrote it could
/// produce, so a federation with no mint module has no `ecash_send` record to look up in the
/// first place, and a driver that does need a live client asks the federation for one itself and
/// reports honestly when there is none.
///
/// No driver is not an error anywhere: [`AnyOperation`]'s accessor for that kind answers `None`,
/// exactly as it does for a kind mismatch, and the record stays findable, listable and correctly
/// labelled.
//
// A driver holds nothing — every method takes the federation it should act on — so building one
// per lookup costs an `Arc` allocation and no state, which is what lets the arms below be plain
// expressions rather than a table of cached singletons.
pub(crate) fn driver_for(kind: &str) -> Option<ErasedDriver> {
    match kind {
        // One arm per tag in `kinds`, filled in by the task that writes the facade owning that
        // kind: ecash and lightning in T7-T8, on-chain in T9, recovery in T12. Until an arm is
        // filled in this build cannot observe that kind, which is a real answer rather than a
        // gap: the record is still found, still listed, and still says what it is.
        //
        // The probe stands in for the ecash-send driver so that the type-erased accessors are
        // exercised end to end before any facade exists; T7 replaces the pair of arms below with
        // a single unconditional one.
        #[cfg(test)]
        kinds::ECASH_SEND => Some(ErasedDriver::EcashSend(Arc::new(ProbeEcashSendDriver))),
        #[cfg(not(test))]
        kinds::ECASH_SEND => None,
        kinds::ECASH_RECEIVE => None,
        kinds::LN_SEND => None,
        kinds::LN_RECEIVE => None,
        kinds::ONCHAIN_SEND => None,
        kinds::ONCHAIN_RECEIVE => None,
        kinds::RECOVERY => None,
        // A tag this build does not know, which `kind_of_tag` already reads as
        // `OperationKind::Unknown`.
        _ => None,
    }
}

/// The backfillers this build has, in the order reconciliation offers a log entry to them.
///
/// A list rather than a lookup keyed by kind, because a backfiller is asked about an upstream
/// *module* kind and one module produces several of the SDK's kinds: the facade that owns the
/// module is the only thing that can tell them apart.
pub(crate) fn backfillers() -> Vec<Arc<dyn Backfiller>> {
    // One entry per facade that owns a module kind, added by the task that writes the facade:
    // ecash and lightning in T7-T8, on-chain in T9. The probe entry is the engine's own fixture
    // and exists only in a test build.
    #[cfg(test)]
    {
        vec![Arc::new(ProbeBackfiller) as Arc<dyn Backfiller>]
    }
    #[cfg(not(test))]
    {
        Vec::new()
    }
}

/// A driver for a real kind, so the type-erased accessors can be exercised end to end.
///
/// The engine's own behaviour is covered against `ProbeState`; this exists to check the wiring
/// from a persisted tag to a typed handle, which needs one of the seven public state enums.
//
// At file scope rather than inside `mod tests`, because `driver_for` above returns it and
// `federation.rs`'s tests use it too, and a `mod tests` is private to its own file.
#[cfg(test)]
pub(crate) struct ProbeEcashSendDriver;

#[cfg(test)]
impl Driver<EcashSendState> for ProbeEcashSendDriver {
    fn current<'a>(
        &'a self,
        _federation: &'a FederationInner,
        _id: UpstreamOperationId,
        _record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<EcashSendState>> {
        Box::pin(async { Ok(EcashSendState::Redeemed) })
    }

    fn subscribe<'a>(
        &'a self,
        _federation: &'a FederationInner,
        _id: UpstreamOperationId,
        _record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<EcashSendState>>>> {
        Box::pin(async {
            Ok(
                Box::pin(futures::stream::iter(vec![Ok(EcashSendState::Redeemed)]))
                    as BoxStream<'static, _>,
            )
        })
    }

    fn same_state(&self, previous: &EcashSendState, next: &EcashSendState) -> bool {
        previous == next
    }

    fn encode_state(&self, state: &EcashSendState) -> Result<String> {
        Ok(format!("{state:?}"))
    }

    fn decode_details(&self, _json: &str) -> Result<Box<dyn Any + Send + Sync>> {
        Err(Error::new(
            ErrorCode::Internal,
            "the ecash probe has no details",
        ))
    }
}

/// A backfiller that claims the probe module's entries, for the reconciliation tests.
///
/// The only one this build has, so the reconciliation tests distinguish a claimed entry from an
/// unclaimed one by the module kind they write the log entry under rather than by which
/// backfillers are installed.
#[cfg(test)]
pub(crate) struct ProbeBackfiller;

#[cfg(test)]
impl Backfiller for ProbeBackfiller {
    fn backfill(&self, module_kind: &str, meta: &serde_json::Value) -> Option<Backfilled> {
        (module_kind == "probe_module").then(|| Backfilled {
            kind: kinds::ECASH_SEND,
            details: meta.to_string(),
            phase: Some(1),
        })
    }
}

/// The shared per-operation state a typed handle and every subscriber of it observe.
///
/// Holds the federation the operation belongs to, its id, and the record as it read when the
/// handle was made. The cached record is what the cheap, infallible accessors answer from
/// ([`AnyOperation::kind`] and [`AnyOperation::raw_kind`] promise to read no storage); anything
/// that must see a field filled in since then reloads it.
#[derive(Debug)]
pub(crate) struct OperationInner {
    /// The federation this operation belongs to, which owns the namespace it is recorded in and
    /// the drivers that observe it.
    pub(crate) federation: Arc<FederationInner>,
    /// The client's own id for the operation, which is also the key of its record.
    pub(crate) id: UpstreamOperationId,
    /// The record as it read when this handle was made.
    pub(crate) record: OperationRecord,
}

impl OperationInner {
    /// Reads this operation's record again.
    ///
    /// The cached copy is a snapshot taken when the handle was made; a field documented as
    /// filling in later has to be read from storage to be seen.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage) if the namespace cannot be read, and
    /// [`Internal`](crate::ErrorCode::Internal) if the record has gone, which can only happen if
    /// something outside this crate wrote to its namespace.
    pub(crate) async fn reload(&self) -> Result<OperationRecord> {
        use fedimint_core::db::IDatabaseTransactionOpsCoreTyped;

        let db = self.federation.db();
        let mut dbtx = db.begin_transaction_nc().await;
        dbtx.get_value(&crate::db::OperationRecordKey(self.id))
            .await
            .ok_or_else(|| {
                Error::new(
                    ErrorCode::Internal,
                    format!("no record for operation {}", self.id.fmt_full()),
                )
            })
    }

    /// Records a final state on this operation's record, once.
    ///
    /// Called the first time a final state is observed, from either
    /// [`Operation::state`] or [`OperationUpdates::next`], so that an operation reads as
    /// finished without its state having to be decoded again. Writing it twice is not an error
    /// and not a second write: the first value stands, because a final state is final.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage).
    pub(crate) async fn record_final_state(&self, encoded: String) -> Result<()> {
        use fedimint_core::db::IDatabaseTransactionOpsCoreTyped;

        let db = self.federation.db();
        let id = self.id;
        db.autocommit(
            |dbtx, _| {
                let encoded = encoded.clone();
                Box::pin(async move {
                    let key = crate::db::OperationRecordKey(id);
                    let Some(mut record) = dbtx.get_value(&key).await else {
                        // The operation log is authoritative and this record is a decoration
                        // over it; nothing to decorate is not a failure to observe.
                        return Ok(());
                    };
                    if record.final_state.is_some() {
                        return Ok(());
                    }
                    record.final_state = Some(encoded);
                    dbtx.insert_entry(&key, &record).await;
                    Ok::<(), core::convert::Infallible>(())
                })
            },
            Some(100),
        )
        .await
        .map_err(crate::db::storage_error)
    }

    /// Records that a cancellation was asked for, once.
    ///
    /// The whole of what `Ok` means on
    /// [`request_cancel`](Operation::<crate::EcashSendState>::request_cancel): the intent is in
    /// local storage and will survive a restart or a period offline. Nothing here reaches the
    /// federation.
    ///
    /// A second request is not a second intent, and a request made after the outcome was already
    /// recorded does nothing, so both are `Ok` and neither writes.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage), and nothing else: an unreachable federation or a
    /// slow guardian is not a failure of recording an intent.
    pub(crate) async fn persist_cancel_request(&self) -> Result<()> {
        use fedimint_core::db::IDatabaseTransactionOpsCoreTyped;

        let db = self.federation.db();
        let id = self.id;
        // Hoisted out of the closure, which may run more than once
        // (`fedimint-core/src/db/mod.rs:534-536`).
        let requested_at = crate::db::now_millis();
        db.autocommit(
            |dbtx, _| {
                Box::pin(async move {
                    let key = crate::db::OperationRecordKey(id);
                    let Some(mut record) = dbtx.get_value(&key).await else {
                        return Ok(());
                    };
                    // Finality is read off the record rather than by asking the driver, so that
                    // this call cannot fail in any way its documented error set does not allow.
                    // An operation that has finished but whose outcome nobody has observed yet
                    // still records the intent, which is harmless: the reclaim it schedules is a
                    // no-op once the notes are gone.
                    if record.cancel_requested_at.is_some() || record.final_state.is_some() {
                        return Ok(());
                    }
                    record.cancel_requested_at = Some(requested_at);
                    dbtx.insert_entry(&key, &record).await;
                    Ok::<(), core::convert::Infallible>(())
                })
            },
            Some(100),
        )
        .await
        .map_err(crate::db::storage_error)
    }

    /// Records how far this operation has got, for the mappings that need to know after a
    /// restart.
    ///
    /// Two upstream events carry the same name for opposite outcomes depending on whether
    /// funding completed, and once the process has restarted the persisted phase is the only
    /// thing that tells them apart. It only ever moves forward, because a subscription replays
    /// the current state on every re-subscribe and must not walk it back.
    ///
    /// # Errors
    ///
    /// [`Storage`](crate::ErrorCode::Storage).
    pub(crate) async fn record_phase(&self, phase: u32) -> Result<()> {
        use fedimint_core::db::IDatabaseTransactionOpsCoreTyped;

        let db = self.federation.db();
        let id = self.id;
        db.autocommit(
            |dbtx, _| {
                Box::pin(async move {
                    let key = crate::db::OperationRecordKey(id);
                    let Some(mut record) = dbtx.get_value(&key).await else {
                        return Ok(());
                    };
                    if record.phase.is_some_and(|reached| reached >= phase) {
                        return Ok(());
                    }
                    record.phase = Some(phase);
                    dbtx.insert_entry(&key, &record).await;
                    Ok::<(), core::convert::Infallible>(())
                })
            },
            Some(100),
        )
        .await
        .map_err(crate::db::storage_error)
    }
}

/// The shared state behind a type-erased operation handle.
///
/// The record was read to produce the handle, so every question [`AnyOperation`] answers is
/// answered from here without reading storage or touching the network, which is what lets all
/// four of them be infallible and cheap.
#[derive(Debug)]
struct AnyOperationInner {
    /// Everything a typed handle would need, ready to be handed to one.
    operation: Arc<OperationInner>,
    /// This build's reading of the record's tag.
    kind: OperationKind,
    /// What the record literally says, for logs and bug reports.
    raw: RawOperationKind,
    /// How far this build can go with the record, decided once when the handle was made.
    support: OperationSupport,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Timestamp;

    /// A stand-in for one facade's state enum, and its details record, wired
    /// up exactly the way a facade module wires up a real pair. Nothing here
    /// is part of the public API; it exists so that the shape the facade
    /// modules are asked to follow is compiled and checked in one place,
    /// rather than being described in prose and discovered to be
    /// unimplementable three times over.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ProbeState {
        Running,
        Done,
    }

    /// The details record for [`ProbeState`]: a plain, concrete, non-generic
    /// struct of owned fields, like every real one.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ProbeDetails {
        /// Stands in for a one-shot artifact fixed at creation.
        artifact: String,
        /// Stands in for a fact established at a transition that later
        /// states do not carry: absent until it is known, then set once.
        settled_at: Option<Timestamp>,
    }

    impl sealed::Sealed for ProbeState {}

    impl OperationState for ProbeState {
        fn is_final(&self) -> bool {
            match self {
                ProbeState::Running => false,
                ProbeState::Done => true,
            }
        }
    }

    impl sealed::Sealed for ProbeDetails {}

    impl OperationDetails for ProbeDetails {}

    impl DetailedOperationState for ProbeState {
        type Details = ProbeDetails;
    }

    /// The JSON shape [`ProbeDetails`] persists as.
    ///
    /// Every real details record has one of these beside it, for the same reason this one does:
    /// the record itself holds the crate's own value types (notes, an invoice, an address),
    /// which are not serde types and should not become serde types just because one storage
    /// format wants them to be. The wire record is where the JSON shape is pinned, and it is
    /// what a facade hands to `create_operation`.
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    struct ProbeDetailsWire {
        artifact: String,
        settled_at: Option<u64>,
    }

    impl From<&ProbeDetails> for ProbeDetailsWire {
        fn from(details: &ProbeDetails) -> ProbeDetailsWire {
            ProbeDetailsWire {
                artifact: details.artifact.clone(),
                settled_at: details.settled_at.map(Timestamp::epoch_millis),
            }
        }
    }

    impl From<ProbeDetailsWire> for ProbeDetails {
        fn from(wire: ProbeDetailsWire) -> ProbeDetails {
            ProbeDetails {
                artifact: wire.artifact,
                settled_at: wire.settled_at.map(Timestamp::from_epoch_millis),
            }
        }
    }

    /// The details record every probe operation is created with.
    fn probe_details() -> ProbeDetails {
        ProbeDetails {
            artifact: "the notes, the invoice, the address".to_owned(),
            settled_at: None,
        }
    }

    /// A driver that replays scripted states instead of talking to a federation.
    ///
    /// One script per `subscribe` call, taken in order, so a test can say what a second
    /// subscription sees and what a re-subscription after a truncated stream sees. It exercises
    /// the engine, which is the part of the observation contract that is written once; the six
    /// real drivers are checked against the modules they map.
    struct ScriptedDriver {
        /// One script per subscription, in order. An exhausted queue yields an empty stream.
        scripts: std::sync::Mutex<std::collections::VecDeque<Vec<Result<ProbeState>>>>,
        /// What `current` reports.
        current: std::sync::Mutex<ProbeState>,
        /// How many subscriptions were opened.
        subscriptions: std::sync::Mutex<usize>,
    }

    impl ScriptedDriver {
        /// A driver whose subscriptions replay `scripts` in order.
        fn new(scripts: Vec<Vec<Result<ProbeState>>>) -> Arc<ScriptedDriver> {
            Arc::new(ScriptedDriver {
                scripts: std::sync::Mutex::new(scripts.into()),
                current: std::sync::Mutex::new(ProbeState::Running),
                subscriptions: std::sync::Mutex::new(0),
            })
        }

        /// How many subscriptions have been opened so far.
        fn subscriptions(&self) -> usize {
            *self
                .subscriptions
                .lock()
                .expect("test mutex is never poisoned")
        }

        /// Changes what `current` will report from now on.
        fn set_current(&self, state: ProbeState) {
            *self.current.lock().expect("test mutex is never poisoned") = state;
        }
    }

    impl Driver<ProbeState> for ScriptedDriver {
        fn current<'a>(
            &'a self,
            _federation: &'a FederationInner,
            _id: UpstreamOperationId,
            _record: &'a OperationRecord,
        ) -> BoxFuture<'a, Result<ProbeState>> {
            Box::pin(async move {
                Ok(self
                    .current
                    .lock()
                    .expect("test mutex is never poisoned")
                    .clone())
            })
        }

        fn subscribe<'a>(
            &'a self,
            _federation: &'a FederationInner,
            _id: UpstreamOperationId,
            _record: &'a OperationRecord,
        ) -> BoxFuture<'a, Result<BoxStream<'static, Result<ProbeState>>>> {
            Box::pin(async move {
                let script = {
                    let mut scripts = self.scripts.lock().expect("test mutex is never poisoned");
                    scripts.pop_front().unwrap_or_default()
                };
                *self
                    .subscriptions
                    .lock()
                    .expect("test mutex is never poisoned") += 1;
                Ok(Box::pin(futures::stream::iter(script)) as BoxStream<'static, _>)
            })
        }

        fn same_state(&self, previous: &ProbeState, next: &ProbeState) -> bool {
            previous == next
        }

        fn encode_state(&self, state: &ProbeState) -> Result<String> {
            Ok(match state {
                ProbeState::Running => "\"Running\"".to_owned(),
                ProbeState::Done => "\"Done\"".to_owned(),
            })
        }

        fn decode_details(&self, json: &str) -> Result<Box<dyn Any + Send + Sync>> {
            let wire: ProbeDetailsWire = serde_json::from_str(json).map_err(|err| {
                Error::new(
                    ErrorCode::Internal,
                    format!("could not read this operation's details record: {err}"),
                )
            })?;
            Ok(Box::new(ProbeDetails::from(wire)))
        }
    }

    /// Generic over the pattern rather than over one kind: this compiles only
    /// if a state type names its record and that record satisfies every
    /// bound [`OperationDetails`] imposes.
    fn round_trip_details<S: DetailedOperationState>(details: S::Details) -> S::Details {
        details
    }

    #[test]
    fn a_state_type_can_name_its_details_record() {
        let details = ProbeDetails {
            artifact: "the notes, the invoice, the address".to_owned(),
            settled_at: None,
        };
        let same = round_trip_details::<ProbeState>(details.clone());
        assert_eq!(same, details);
        // The record survives the transition it describes: the field fills
        // in once and the rest is untouched.
        let settled = ProbeDetails {
            settled_at: Some(Timestamp::from_epoch_millis(1)),
            ..details.clone()
        };
        assert_eq!(settled.artifact, details.artifact);
        assert_ne!(settled, details);
    }

    #[test]
    fn probe_state_finality_is_unaffected_by_having_details() {
        assert!(!ProbeState::Running.is_final());
        assert!(ProbeState::Done.is_final());
    }

    #[test]
    fn raw_operation_kind_keeps_the_persisted_discriminator() {
        let raw = RawOperationKind {
            kind: "mint_spend_oob".to_owned(),
            module: Some("mint".to_owned()),
            schema_version: Some(4),
        };
        assert_eq!(raw.kind, "mint_spend_oob");
        assert_eq!(raw.module.as_deref(), Some("mint"));
        assert_eq!(raw.schema_version, Some(4));
        // The whole reason this type exists is that a log line can name what
        // was not understood, so the tag has to survive `Debug`.
        assert!(format!("{raw:?}").contains("mint_spend_oob"));
    }

    #[test]
    fn raw_operation_kind_distinguishes_schema_versions() {
        let at_three = RawOperationKind {
            kind: "wallet_deposit".to_owned(),
            module: Some("wallet".to_owned()),
            schema_version: Some(3),
        };
        let at_four = RawOperationKind {
            schema_version: Some(4),
            ..at_three.clone()
        };
        assert_ne!(at_three, at_four);
        assert_eq!(at_three, at_three.clone());
    }

    #[test]
    fn raw_operation_kind_tolerates_a_record_that_names_no_module_or_version() {
        let bare = RawOperationKind {
            kind: "something_this_build_never_heard_of".to_owned(),
            module: None,
            schema_version: None,
        };
        assert_eq!(bare.module, None);
        assert_eq!(bare.schema_version, None);
    }

    /// Every kind this build knows, in the order [`AnyOperation`] declares
    /// its accessors.
    const KNOWN_KINDS: [OperationKind; 7] = [
        OperationKind::EcashSend,
        OperationKind::EcashReceive,
        OperationKind::LnSend,
        OperationKind::LnReceive,
        OperationKind::OnchainSend,
        OperationKind::OnchainReceive,
        OperationKind::Recovery,
    ];

    /// A raw record carrying the given tag and schema version.
    ///
    /// The tag never feeds the decision, [`OperationKind`] is this crate's
    /// reading of it, and is passed separately, but it does feed the error
    /// message, so the tests use realistic ones.
    fn recorded(tag: &str, schema_version: Option<u32>) -> RawOperationKind {
        RawOperationKind {
            kind: tag.to_owned(),
            module: Some("mint".to_owned()),
            schema_version,
        }
    }

    #[test]
    fn every_known_kind_has_an_accessor() {
        // Naming each accessor without calling it: this compiles only if the
        // accessor exists, so a kind added to `KNOWN_KINDS` without one is
        // caught here rather than reported as supported on a guess.
        let _: fn(&AnyOperation) -> Option<Operation<EcashSendState>> = AnyOperation::as_ecash_send;
        let _: fn(&AnyOperation) -> Option<Operation<EcashReceiveState>> =
            AnyOperation::as_ecash_receive;
        let _: fn(&AnyOperation) -> Option<Operation<LnSendState>> = AnyOperation::as_ln_send;
        let _: fn(&AnyOperation) -> Option<Operation<LnReceiveState>> = AnyOperation::as_ln_receive;
        let _: fn(&AnyOperation) -> Option<Operation<OnchainSendState>> =
            AnyOperation::as_onchain_send;
        let _: fn(&AnyOperation) -> Option<Operation<OnchainReceiveState>> =
            AnyOperation::as_onchain_receive;
        let _: fn(&AnyOperation) -> Option<Operation<RecoveryState>> = AnyOperation::as_recovery;
        // And every one of them is a real kind, never the reading of one.
        for kind in KNOWN_KINDS {
            assert_ne!(kind, OperationKind::Unknown);
        }
    }

    #[test]
    fn a_kind_this_build_knows_is_supported_at_or_below_its_schema() {
        for kind in KNOWN_KINDS {
            for schema_version in [None, Some(0), Some(READABLE_STATE_SCHEMA)] {
                let raw = recorded("mint_spend_oob", schema_version);
                assert_eq!(
                    support_of(kind, &raw),
                    OperationSupport::Observable,
                    "{kind:?} at {schema_version:?}",
                );
                assert_eq!(supported_kind_of(kind, &raw).map_err(|e| e.code), Ok(kind));
            }
        }
    }

    #[test]
    fn an_unknown_record_is_unsupported_and_never_answers_ok() {
        let raw = recorded("something_this_build_never_heard_of", Some(9));
        assert_eq!(
            support_of(OperationKind::Unknown, &raw),
            OperationSupport::UnknownKind
        );
        let err = supported_kind_of(OperationKind::Unknown, &raw)
            .expect_err("an uninterpretable record must not report a supported kind");
        assert_eq!(err.code, ErrorCode::UnsupportedOperation);
        // The message has to name the record, or a bug report cannot.
        assert!(err.message.contains("something_this_build_never_heard_of"));
        assert!(err.message.contains("does not recognise"));
    }

    #[test]
    fn a_state_schema_newer_than_this_build_reads_is_unsupported() {
        let newer = READABLE_STATE_SCHEMA + 1;
        for kind in KNOWN_KINDS {
            let raw = recorded("mint_spend_oob", Some(newer));
            // The kind guard alone would have passed this: the discriminator
            // is one this build has always known.
            assert_eq!(
                support_of(kind, &raw),
                OperationSupport::StateSchemaTooNew,
                "{kind:?} at schema {newer}",
            );
            let err = supported_kind_of(kind, &raw)
                .expect_err("a state this build cannot read must not report a supported kind");
            assert_eq!(err.code, ErrorCode::UnsupportedOperation);
            // Both halves of "what happened": what was written, and what this
            // build reads.
            assert!(err.message.contains(&format!("schema version {newer}")));
            assert!(
                err.message
                    .contains(&format!("up to {}", kind.readable_state_schema()))
            );
        }
    }

    #[test]
    fn the_error_names_the_module_when_the_record_does() {
        let with_module = recorded("wallet_deposit", Some(READABLE_STATE_SCHEMA + 1));
        let err = supported_kind_of(OperationKind::OnchainReceive, &with_module)
            .expect_err("newer schema is unsupported");
        assert!(err.message.contains("wallet_deposit"));
        assert!(err.message.contains("mint"));

        let bare = RawOperationKind {
            kind: "wallet_deposit".to_owned(),
            module: None,
            schema_version: None,
        };
        // No module and no version recorded is not a failure to read either,
        // so nothing rules the record out.
        assert_eq!(
            support_of(OperationKind::OnchainReceive, &bare),
            OperationSupport::Observable
        );
    }

    #[test]
    fn every_reason_is_a_distinct_answer() {
        let reasons = [
            OperationSupport::Observable,
            OperationSupport::UnknownKind,
            OperationSupport::StateSchemaTooNew,
        ];
        for (index, reason) in reasons.iter().enumerate() {
            for other in &reasons[index + 1..] {
                assert_ne!(reason, other);
            }
            // Each has to survive `Debug`: these end up in log lines and bug
            // reports, which is the whole reason they are separate variants.
            assert!(!format!("{reason:?}").is_empty());
        }
    }

    #[test]
    fn unknown_reads_no_state_schema_at_all() {
        assert_eq!(OperationKind::Unknown.readable_state_schema(), 0);
    }

    /// A type-erased handle over a record with the given tag, module and schema version.
    ///
    /// `AnyOperation::from_record` is a pure wrapper over a record that has already been read, so
    /// it never writes storage; but the typed accessors it hands back reload from storage on
    /// every call (`Operation::state`, for instance), so the record has to actually be persisted
    /// here first.
    async fn any_operation(kind: &str, module: &str, schema_version: u32) -> AnyOperation {
        use fedimint_core::db::IDatabaseTransactionOpsCoreTyped;

        let db = crate::db::federation_namespace(&crate::db::in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db, true);
        let id = UpstreamOperationId([5u8; 32]);
        let record = OperationRecord {
            schema_version,
            kind: kind.to_owned(),
            module: module.to_owned(),
            created_at: 1_700_000_000_000,
            details: "{}".to_owned(),
            phase: None,
            cancel_requested_at: None,
            final_state: None,
        };
        let db = federation.db();
        let mut dbtx = db.begin_transaction().await;
        dbtx.insert_entry(&crate::db::OperationRecordKey(id), &record)
            .await;
        dbtx.commit_tx().await;
        AnyOperation::from_record(Arc::new(OperationInner {
            federation,
            id,
            record,
        }))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_record_this_build_knows_becomes_a_typed_handle() {
        let any = any_operation(kinds::ECASH_SEND, "mint", READABLE_STATE_SCHEMA).await;
        assert_eq!(any.kind(), OperationKind::EcashSend);
        assert_eq!(any.support(), OperationSupport::Observable);
        assert_eq!(
            any.supported_kind().expect("supported"),
            OperationKind::EcashSend
        );
        assert_eq!(
            any.id(),
            crate::OperationId::from_upstream(UpstreamOperationId([5u8; 32]))
        );
        let typed = any
            .as_ecash_send()
            .expect("the kind matches and is supported");
        assert_eq!(typed.id(), any.id());
        assert_eq!(
            typed.state().await.expect("state"),
            EcashSendState::Redeemed
        );
        // And only that one: the accessors do not guess.
        assert!(any.as_ecash_receive().is_none());
        assert!(any.as_ln_send().is_none());
        assert!(any.as_ln_receive().is_none());
        assert!(any.as_onchain_send().is_none());
        assert!(any.as_onchain_receive().is_none());
        assert!(any.as_recovery().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_tag_this_build_does_not_know_is_reported_rather_than_refused() {
        let any = any_operation("something_else", "mint", 1).await;
        assert_eq!(any.kind(), OperationKind::Unknown);
        assert_eq!(any.support(), OperationSupport::UnknownKind);
        assert!(any.supported_kind().is_err());
        assert!(any.as_ecash_send().is_none());
        // The record still says what it says, which is the whole point of keeping it readable.
        assert_eq!(any.raw_kind().kind, "something_else");
        assert_eq!(any.raw_kind().module.as_deref(), Some("mint"));
        assert_eq!(any.raw_kind().schema_version, Some(1));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_state_schema_newer_than_this_build_reads_yields_no_handle() {
        let any = any_operation(kinds::ECASH_SEND, "mint", READABLE_STATE_SCHEMA + 1).await;
        // It still knows what it is; it just cannot act on it.
        assert_eq!(any.kind(), OperationKind::EcashSend);
        assert_eq!(any.support(), OperationSupport::StateSchemaTooNew);
        assert!(any.as_ecash_send().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_kind_this_build_has_no_driver_for_yields_no_handle() {
        let any = any_operation(kinds::LN_SEND, "lnv2", READABLE_STATE_SCHEMA).await;
        assert_eq!(any.kind(), OperationKind::LnSend);
        // `support` is about the record rather than about what this build can observe, so it
        // still says observable; the accessor is where a kind no facade has written a driver for
        // yet answers `None`, in exactly the way a kind mismatch does.
        assert_eq!(any.support(), OperationSupport::Observable);
        assert!(any.as_ln_send().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_record_with_no_module_marker_reports_none_rather_than_an_empty_name() {
        let any = any_operation(kinds::RECOVERY, "", READABLE_STATE_SCHEMA).await;
        assert_eq!(any.kind(), OperationKind::Recovery);
        assert_eq!(any.raw_kind().module, None);
    }

    #[test]
    fn every_tag_this_build_writes_reads_back_as_its_kind() {
        let pairs = [
            (kinds::ECASH_SEND, OperationKind::EcashSend),
            (kinds::ECASH_RECEIVE, OperationKind::EcashReceive),
            (kinds::LN_SEND, OperationKind::LnSend),
            (kinds::LN_RECEIVE, OperationKind::LnReceive),
            (kinds::ONCHAIN_SEND, OperationKind::OnchainSend),
            (kinds::ONCHAIN_RECEIVE, OperationKind::OnchainReceive),
            (kinds::RECOVERY, OperationKind::Recovery),
        ];
        for (tag, kind) in pairs {
            assert_eq!(kind_of_tag(tag), kind, "{tag}");
        }
        // A module kind is not a kind tag: a record backfilled from a log entry this build
        // cannot place reads back as unknown, which is exactly what makes it observable
        // without being actionable.
        assert_eq!(kind_of_tag("mint"), OperationKind::Unknown);
        assert_eq!(kind_of_tag(""), OperationKind::Unknown);
        // Every tag is distinct, or two kinds would collide in storage.
        let mut tags: Vec<_> = pairs.iter().map(|(tag, _)| *tag).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), pairs.len());
    }

    #[test]
    fn a_driver_is_shareable_on_the_targets_that_have_threads() {
        // The crate promises `Operation` is `Send + Sync` on native targets
        // (`lib.rs:281-293`), and the driver is inside it, so the bound has to hold here.
        #[cfg(not(target_family = "wasm"))]
        fn assert_send_sync<T: Send + Sync>() {}
        #[cfg(not(target_family = "wasm"))]
        assert_send_sync::<Arc<dyn Driver<ProbeState>>>();
    }

    /// A probe operation over a fresh in-memory namespace, observed through `driver`.
    async fn probe_operation_with(driver: Arc<dyn Driver<ProbeState>>) -> Operation<ProbeState> {
        let db = crate::db::federation_namespace(&crate::db::in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db, true);
        federation
            .create_operation(
                UpstreamOperationId([5u8; 32]),
                "probe",
                "probe_module",
                &ProbeDetailsWire::from(&probe_details()),
                driver,
            )
            .await
            .expect("creating an operation over an empty namespace cannot fail")
    }

    /// A probe operation whose subscriptions replay `scripts` in order.
    ///
    /// Returns the driver too, so a test can change what `current` reports and count
    /// subscriptions.
    async fn probe_operation(
        scripts: Vec<Vec<Result<ProbeState>>>,
    ) -> (Arc<ScriptedDriver>, Operation<ProbeState>) {
        let driver = ScriptedDriver::new(scripts);
        let operation = probe_operation_with(driver.clone() as Arc<dyn Driver<ProbeState>>).await;
        (driver, operation)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_handle_reports_the_id_it_was_created_with() {
        let (_driver, operation) = probe_operation(vec![]).await;
        assert_eq!(
            operation.id(),
            crate::OperationId::from_upstream(UpstreamOperationId([5u8; 32]))
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn state_reports_what_the_driver_reports_now() {
        let (driver, operation) = probe_operation(vec![]).await;
        assert_eq!(operation.state().await.expect("state"), ProbeState::Running);
        driver.set_current(ProbeState::Done);
        assert_eq!(operation.state().await.expect("state"), ProbeState::Done);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn state_records_a_final_state_the_first_time_it_sees_one() {
        let (driver, operation) = probe_operation(vec![]).await;
        assert_eq!(
            operation.inner.reload().await.expect("record").final_state,
            None
        );
        driver.set_current(ProbeState::Done);
        operation.state().await.expect("state");
        assert_eq!(
            operation.inner.reload().await.expect("record").final_state,
            Some("\"Done\"".to_owned())
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn details_return_the_record_written_when_the_operation_was_created() {
        let (_driver, operation) = probe_operation(vec![]).await;
        assert_eq!(operation.details().await.expect("details"), probe_details());
        // Twice, with the same answer: a details record is not consumed by reading it.
        assert_eq!(operation.details().await.expect("details"), probe_details());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn every_fallible_call_on_a_closed_federation_says_so() {
        let (_driver, live) = probe_operation(vec![]).await;
        // The same record, reached through a federation that has since been closed.
        let closed = FederationInner::detached(live.inner.federation.db(), false);
        let operation = Operation::attach(
            Arc::new(OperationInner {
                federation: closed,
                id: live.inner.id,
                record: live.inner.record.clone(),
            }),
            ScriptedDriver::new(vec![]) as Arc<dyn Driver<ProbeState>>,
        );
        assert_eq!(
            operation.state().await.expect_err("closed").code,
            ErrorCode::FederationClosed
        );
        assert_eq!(
            operation.details().await.expect_err("closed").code,
            ErrorCode::FederationClosed
        );
    }

    #[test]
    fn this_build_observes_the_kinds_it_has_a_driver_for_and_no_others() {
        // The one arm T6 fills in is the probe fixture standing in for the ecash-send driver
        // T7 writes. Every other kind is a record this build can find, list and label but not
        // observe, which the accessors report as `None` rather than as a failure.
        assert!(matches!(
            driver_for(kinds::ECASH_SEND),
            Some(ErasedDriver::EcashSend(_))
        ));
        assert!(driver_for(kinds::LN_SEND).is_none());
        assert!(driver_for(kinds::RECOVERY).is_none());
        // A tag this build does not know is not a lookup failure either.
        assert!(driver_for("something_else").is_none());
        // Backfillers are a list rather than a lookup: one is asked about an upstream module
        // kind, and one module kind can produce several of the SDK's kinds.
        let backfillers = backfillers();
        assert_eq!(backfillers.len(), 1);
        assert!(
            backfillers[0]
                .backfill("probe_module", &serde_json::Value::Null)
                .is_some()
        );
        assert!(
            backfillers[0]
                .backfill("mint", &serde_json::Value::Null)
                .is_none()
        );
    }

    /// A driver whose one subscription is fed by hand, for the timing the scripted driver
    /// cannot express: a state that arrives while nobody is awaiting.
    struct ChannelDriver {
        stream: std::sync::Mutex<Option<BoxStream<'static, Result<ProbeState>>>>,
    }

    impl ChannelDriver {
        /// A driver and the sender that feeds its one subscription.
        fn new() -> (
            Arc<ChannelDriver>,
            futures::channel::mpsc::UnboundedSender<Result<ProbeState>>,
        ) {
            let (sender, receiver) = futures::channel::mpsc::unbounded();
            let driver = ChannelDriver {
                stream: std::sync::Mutex::new(Some(Box::pin(receiver) as BoxStream<'static, _>)),
            };
            (Arc::new(driver), sender)
        }
    }

    impl Driver<ProbeState> for ChannelDriver {
        fn current<'a>(
            &'a self,
            _federation: &'a FederationInner,
            _id: UpstreamOperationId,
            _record: &'a OperationRecord,
        ) -> BoxFuture<'a, Result<ProbeState>> {
            Box::pin(async { Ok(ProbeState::Running) })
        }

        fn subscribe<'a>(
            &'a self,
            _federation: &'a FederationInner,
            _id: UpstreamOperationId,
            _record: &'a OperationRecord,
        ) -> BoxFuture<'a, Result<BoxStream<'static, Result<ProbeState>>>> {
            Box::pin(async move {
                let stream = self
                    .stream
                    .lock()
                    .expect("test mutex is never poisoned")
                    .take();
                Ok(stream.unwrap_or_else(|| Box::pin(futures::stream::empty())))
            })
        }

        fn same_state(&self, previous: &ProbeState, next: &ProbeState) -> bool {
            previous == next
        }

        fn encode_state(&self, state: &ProbeState) -> Result<String> {
            Ok(format!("{state:?}"))
        }

        fn decode_details(&self, _json: &str) -> Result<Box<dyn Any + Send + Sync>> {
            Err(Error::new(
                ErrorCode::Internal,
                "the channel probe has no details",
            ))
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_first_next_yields_the_current_state_without_waiting() {
        use futures::FutureExt;

        let (_driver, operation) = probe_operation(vec![vec![Ok(ProbeState::Running)]]).await;
        let mut updates = operation.updates();
        assert_eq!(
            updates
                .next()
                .now_or_never()
                .expect("the current state is available at once")
                .expect("next"),
            Some(ProbeState::Running)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_repeated_state_is_handed_out_only_once() {
        // Two upstream events that mean the same thing here: the mappings are many-to-one, so
        // this is the normal case rather than a defect in a driver.
        let (_driver, operation) = probe_operation(vec![vec![
            Ok(ProbeState::Running),
            Ok(ProbeState::Running),
            Ok(ProbeState::Done),
        ]])
        .await;
        let mut updates = operation.updates();
        assert_eq!(
            updates.next().await.expect("next"),
            Some(ProbeState::Running)
        );
        assert_eq!(updates.next().await.expect("next"), Some(ProbeState::Done));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_final_state_is_followed_by_none_for_ever() {
        let (_driver, operation) = probe_operation(vec![vec![
            Ok(ProbeState::Running),
            Ok(ProbeState::Done),
            Ok(ProbeState::Running),
        ]])
        .await;
        let mut updates = operation.updates();
        assert_eq!(
            updates.next().await.expect("next"),
            Some(ProbeState::Running)
        );
        assert_eq!(updates.next().await.expect("next"), Some(ProbeState::Done));
        // The subscription closed at the final state; the state after it in the script is not a
        // transition this subscriber can be told about, because there are none after a final
        // state.
        assert_eq!(updates.next().await.expect("next"), None);
        assert_eq!(updates.next().await.expect("next"), None);
        assert_eq!(updates.next().await.expect("next"), None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn two_subscriptions_see_the_same_sequence_and_neither_steals_from_the_other() {
        let (driver, operation) = probe_operation(vec![
            vec![Ok(ProbeState::Running), Ok(ProbeState::Done)],
            vec![Ok(ProbeState::Running), Ok(ProbeState::Done)],
        ])
        .await;
        let mut screen = operation.updates();
        let mut sync = operation.updates();
        assert_eq!(
            screen.next().await.expect("next"),
            Some(ProbeState::Running)
        );
        assert_eq!(sync.next().await.expect("next"), Some(ProbeState::Running));
        assert_eq!(screen.next().await.expect("next"), Some(ProbeState::Done));
        assert_eq!(sync.next().await.expect("next"), Some(ProbeState::Done));
        assert_eq!(screen.next().await.expect("next"), None);
        assert_eq!(sync.next().await.expect("next"), None);
        assert_eq!(driver.subscriptions(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_stream_that_ends_short_is_subscribed_to_again() {
        // The client's notifier ends a subscriber's stream when it falls behind, with nothing
        // but a log line to say so, so "the stream ended" never means "the operation finished".
        let (driver, operation) = probe_operation(vec![
            vec![Ok(ProbeState::Running)],
            vec![Ok(ProbeState::Running), Ok(ProbeState::Done)],
        ])
        .await;
        let mut updates = operation.updates();
        assert_eq!(
            updates.next().await.expect("next"),
            Some(ProbeState::Running)
        );
        // The truncation is invisible to the caller: the second subscription's repeat of the
        // current state is dropped and the transition after it is what arrives.
        assert_eq!(updates.next().await.expect("next"), Some(ProbeState::Done));
        assert_eq!(updates.next().await.expect("next"), None);
        assert_eq!(driver.subscriptions(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_stream_that_ends_short_twice_running_is_an_error() {
        let (driver, operation) =
            probe_operation(vec![vec![Ok(ProbeState::Running)], vec![], vec![]]).await;
        let mut updates = operation.updates();
        assert_eq!(
            updates.next().await.expect("next"),
            Some(ProbeState::Running)
        );
        let err = updates
            .next()
            .await
            .expect_err("a subscription that cannot be re-established must not look finished");
        assert_eq!(err.code, ErrorCode::Internal);
        assert_eq!(driver.subscriptions(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_empty_first_stream_falls_back_to_current_before_giving_up() {
        // A driver's stream is supposed to yield the current state first, but this subscriber has
        // no way to tell "the driver's stream is broken" apart from "the operation was already
        // final and the driver had nothing left to stream" without asking `current` directly.
        // Only the latter should happen in practice, and it must not surface as the `Internal`
        // error `a_stream_that_ends_short_twice_running_is_an_error` covers above.
        let (driver, operation) = probe_operation(vec![vec![]]).await;
        driver.set_current(ProbeState::Done);
        let mut updates = operation.updates();
        assert_eq!(updates.next().await.expect("next"), Some(ProbeState::Done));
        assert_eq!(updates.next().await.expect("next"), None);
        // Settled by the direct read, with no second subscription needed.
        assert_eq!(driver.subscriptions(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_state_reached_while_no_future_was_pending_is_still_delivered() {
        use futures::FutureExt;

        let (driver, sender) = ChannelDriver::new();
        let operation = probe_operation_with(driver as Arc<dyn Driver<ProbeState>>).await;
        let mut updates = operation.updates();
        // Nothing has happened yet, so this cannot resolve; dropping it is exactly what
        // `select!` and a timeout do to the losing branch.
        assert!(updates.next().now_or_never().is_none());
        // The transition happens while no future is pending.
        sender
            .unbounded_send(Ok(ProbeState::Running))
            .expect("the subscription is still open");
        // And it is still the next thing this subscriber is told, with the cursor where it was.
        assert_eq!(
            updates.next().await.expect("next"),
            Some(ProbeState::Running)
        );
        sender
            .unbounded_send(Ok(ProbeState::Done))
            .expect("the subscription is still open");
        assert_eq!(updates.next().await.expect("next"), Some(ProbeState::Done));
        assert_eq!(updates.next().await.expect("next"), None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_error_from_the_stream_reaches_the_caller_unchanged() {
        let (_driver, operation) = probe_operation(vec![vec![
            Ok(ProbeState::Running),
            Err(Error::new(ErrorCode::Storage, "the disk went away")),
        ]])
        .await;
        let mut updates = operation.updates();
        assert_eq!(
            updates.next().await.expect("next"),
            Some(ProbeState::Running)
        );
        let err = updates.next().await.expect_err("the storage failure");
        assert_eq!(err.code, ErrorCode::Storage);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn await_final_reads_until_the_operation_settles() {
        let (_driver, operation) = probe_operation(vec![vec![
            Ok(ProbeState::Running),
            Ok(ProbeState::Running),
            Ok(ProbeState::Done),
        ]])
        .await;
        assert_eq!(
            operation.await_final().await.expect("final"),
            ProbeState::Done
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn await_final_returns_at_once_for_an_operation_that_already_settled() {
        use futures::FutureExt;

        let (_driver, operation) = probe_operation(vec![vec![Ok(ProbeState::Done)]]).await;
        assert_eq!(
            operation
                .await_final()
                .now_or_never()
                .expect("the final state is available at once")
                .expect("final"),
            ProbeState::Done
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_final_state_a_subscriber_hands_out_is_recorded() {
        let (_driver, operation) =
            probe_operation(vec![vec![Ok(ProbeState::Running), Ok(ProbeState::Done)]]).await;
        let mut updates = operation.updates();
        while updates.next().await.expect("next").is_some() {}
        assert_eq!(
            operation.inner.reload().await.expect("record").final_state,
            Some("\"Done\"".to_owned())
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn next_on_a_closed_federation_says_so() {
        let (_driver, live) = probe_operation(vec![vec![Ok(ProbeState::Running)]]).await;
        let closed = FederationInner::detached(live.inner.federation.db(), false);
        let operation = Operation::attach(
            Arc::new(OperationInner {
                federation: closed,
                id: live.inner.id,
                record: live.inner.record.clone(),
            }),
            ScriptedDriver::new(vec![vec![Ok(ProbeState::Running)]]) as Arc<dyn Driver<ProbeState>>,
        );
        // `updates` itself cannot fail, so the closure is reported where a caller can act on it.
        let mut updates = operation.updates();
        assert_eq!(
            updates.next().await.expect_err("closed").code,
            ErrorCode::FederationClosed
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_subscriber_parked_on_a_stream_is_released_when_the_federation_stops() {
        let (driver, _sender) = ChannelDriver::new();
        let operation = probe_operation_with(driver as Arc<dyn Driver<ProbeState>>).await;
        let mut updates = operation.updates();
        let federation = operation.inner.federation.clone();
        // The `next` below parks on a stream nothing will ever feed. Closing the federation is
        // what has to release it: with only the check at the top of the loop this waits for
        // ever, and the shutdown that is waiting for the subscriber waits with it.
        let (released, ()) = tokio::join!(updates.next(), async {
            tokio::task::yield_now().await;
            federation.set_status(crate::FederationStatus::Closed);
        });
        let err = released.expect_err("the subscriber is released");
        assert_eq!(err.code, ErrorCode::FederationClosed);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_final_state_left_in_handoff_is_retried_and_persisted() {
        // Stands in for a `next()` future dropped after it committed to handing out a final
        // state but before the persist that follows completed: `self.handoff` is exactly what
        // that in-flight future would have left behind, planted directly here because neither
        // the scripted nor the channel driver can pause a real subscriber mid-persist. Nothing
        // reads the stream in this test at all, which is the point: the handoff alone must be
        // enough to finish the job.
        let (_driver, operation) = probe_operation(vec![vec![]]).await;
        let mut updates = operation.updates();
        updates.handoff = Some(ProbeState::Done);
        assert_eq!(
            operation.inner.reload().await.expect("record").final_state,
            None
        );
        assert_eq!(updates.next().await.expect("next"), Some(ProbeState::Done));
        // Persisted by the retry, not lost with the future that first reached it.
        assert_eq!(
            operation.inner.reload().await.expect("record").final_state,
            Some("\"Done\"".to_owned())
        );
        // And the subscription is closed for good, exactly as if the persist had gone through
        // on the first attempt.
        assert_eq!(updates.next().await.expect("next"), None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_cancellation_intent_is_written_once_and_survives_a_second_request() {
        let (_driver, operation) = probe_operation(vec![]).await;
        assert_eq!(
            operation
                .inner
                .reload()
                .await
                .expect("record")
                .cancel_requested_at,
            None
        );

        operation
            .inner
            .persist_cancel_request()
            .await
            .expect("request");
        let first = operation
            .inner
            .reload()
            .await
            .expect("record")
            .cancel_requested_at
            .expect("the intent was recorded");

        operation
            .inner
            .persist_cancel_request()
            .await
            .expect("second request");
        assert_eq!(
            operation
                .inner
                .reload()
                .await
                .expect("record")
                .cancel_requested_at,
            Some(first),
            "a second request is not a second intent"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_cancellation_asked_for_after_the_end_changes_nothing() {
        let (driver, operation) = probe_operation(vec![]).await;
        driver.set_current(ProbeState::Done);
        operation
            .state()
            .await
            .expect("state records the final state");

        operation
            .inner
            .persist_cancel_request()
            .await
            .expect("request");
        assert_eq!(
            operation
                .inner
                .reload()
                .await
                .expect("record")
                .cancel_requested_at,
            None,
            "there is nothing left to cancel once the outcome is recorded"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_phase_only_ever_moves_forward() {
        let (_driver, operation) = probe_operation(vec![]).await;
        assert_eq!(operation.inner.reload().await.expect("record").phase, None);

        operation.inner.record_phase(2).await.expect("phase");
        assert_eq!(
            operation.inner.reload().await.expect("record").phase,
            Some(2)
        );

        // A stream that replays an earlier transition must not walk the phase back: after a
        // restart the phase is the only thing that tells two identically named upstream events
        // apart.
        operation.inner.record_phase(1).await.expect("phase");
        assert_eq!(
            operation.inner.reload().await.expect("record").phase,
            Some(2)
        );

        operation.inner.record_phase(3).await.expect("phase");
        assert_eq!(
            operation.inner.reload().await.expect("record").phase,
            Some(3)
        );
    }
}
