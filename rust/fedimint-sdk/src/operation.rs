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
use std::marker::PhantomData;
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
// Implementation notes (delete once implemented):
// - The record must be committed in the same storage transaction that creates the operation,
//   before the creating call returns.
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
        unimplemented!()
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
        unimplemented!()
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
        // Implementation notes (delete once implemented):
        // The cursor must advance when a state is handed to the caller, never when the future is
        // merely polled. Consuming from a shared queue inside the future, or buffering only while
        // someone is awaiting, drops states under the `select!` usage the doc above promises is
        // safe.
        unimplemented!()
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
        unimplemented!()
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
        unimplemented!()
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
    pub fn support(&self) -> OperationSupport {
        support_of(self.kind(), &self.raw_kind())
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
        unimplemented!()
    }

    /// Recovers a typed handle if this is an out-of-band ecash send.
    ///
    /// `None` for any other kind, for a record this build cannot interpret,
    /// and for a record of *this* kind whose typed state this build cannot
    /// observe; see the type documentation for how to tell those apart.
    pub fn as_ecash_send(&self) -> Option<Operation<EcashSendState>> {
        unimplemented!()
    }

    /// Recovers a typed handle if this is an ecash redemption.
    ///
    /// `None` for any other kind, for a record this build cannot interpret,
    /// and for a record of *this* kind whose typed state this build cannot
    /// observe; see the type documentation for how to tell those apart.
    pub fn as_ecash_receive(&self) -> Option<Operation<EcashReceiveState>> {
        unimplemented!()
    }

    /// Recovers a typed handle if this is an outgoing lightning payment.
    ///
    /// `None` for any other kind, for a record this build cannot interpret,
    /// and for a record of *this* kind whose typed state this build cannot
    /// observe; see the type documentation for how to tell those apart.
    pub fn as_ln_send(&self) -> Option<Operation<LnSendState>> {
        unimplemented!()
    }

    /// Recovers a typed handle if this is an incoming lightning payment.
    ///
    /// `None` for any other kind, for a record this build cannot interpret,
    /// and for a record of *this* kind whose typed state this build cannot
    /// observe; see the type documentation for how to tell those apart.
    pub fn as_ln_receive(&self) -> Option<Operation<LnReceiveState>> {
        unimplemented!()
    }

    /// Recovers a typed handle if this is an on-chain withdrawal.
    ///
    /// `None` for any other kind, for a record this build cannot interpret,
    /// and for a record of *this* kind whose typed state this build cannot
    /// observe; see the type documentation for how to tell those apart.
    pub fn as_onchain_send(&self) -> Option<Operation<OnchainSendState>> {
        unimplemented!()
    }

    /// Recovers a typed handle if this is an on-chain deposit.
    ///
    /// `None` for any other kind, for a record this build cannot interpret,
    /// and for a record of *this* kind whose typed state this build cannot
    /// observe; see the type documentation for how to tell those apart.
    pub fn as_onchain_receive(&self) -> Option<Operation<OnchainReceiveState>> {
        unimplemented!()
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
        unimplemented!()
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

/// Placeholder for the shared per-operation state a typed handle and its
/// subscribers observe.
#[derive(Debug)]
struct OperationInner;

/// Placeholder for the shared state behind a type-erased operation handle.
#[derive(Debug)]
struct AnyOperationInner;

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
}
