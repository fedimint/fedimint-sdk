//! The operation model: typed state machines observed from the outside.
//!
//! Everything the SDK does that takes longer than a single call — paying an
//! invoice, waiting for a deposit to confirm, redeeming ecash — is an
//! *operation*. An operation is created by a facade call, runs in the
//! background from that moment, is persisted as it goes, and reports its
//! progress as a sequence of states. This module defines the vocabulary
//! shared by all of them: the [`OperationState`] trait each state enum
//! implements, the [`Operation`] handle used to observe one, the
//! [`OperationUpdates`] subscriber that streams its transitions, the
//! type-erased [`AnyOperation`] returned when an operation is looked up by
//! id after a restart, and the [`OperationSupport`] answer that says how far
//! *this* build can go with such a record — which is not a property of the
//! record alone, because a build compiled without a kind's accessor, or older
//! than the schema a state was written at, can read the row and still not
//! observe the operation.
//!
//! It also defines the half of an operation that is *not* a state: the
//! persisted [`OperationDetails`] record each kind keeps — the notes handed
//! out, the invoice issued, the address allocated, the fee and route of the
//! quote that was executed — read back through [`Operation::details`]. States
//! say where an operation has got to; details say what it is. Both are needed
//! to make good on the crate's promise that an operation id is all it takes
//! to pick an operation back up, because a subscription yields the current
//! state and never replays the ones before it. [`OperationDetails`] states
//! the rule for which of the two any given value belongs in, and that rule is
//! binding on every facade module in this crate.

use std::marker::PhantomData;
use std::sync::Arc;

use crate::{
    EcashReceiveState, EcashSendState, Error, ErrorCode, LnReceiveState, LnSendState,
    OnchainReceiveState, OnchainSendState, OperationId, RecoveryState, Result,
};

/// The sealing module for [`OperationState`] and [`OperationDetails`].
///
/// It is `pub(crate)` rather than private so that the facade modules can
/// name [`sealed::Sealed`] to implement it next to each state enum and each
/// details record they define. Because the module is not reachable from
/// outside this crate, downstream code still cannot name the trait and
/// therefore cannot implement either of the traits it seals.
pub(crate) mod sealed {
    /// Marker that only this crate can implement, sealing
    /// [`OperationState`](super::OperationState) and
    /// [`OperationDetails`](super::OperationDetails).
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

/// The persisted, per-kind record of what an operation *is*, as opposed to
/// where it has got to.
///
/// Each kind of operation has its own record — `EcashSendDetails`,
/// `LnReceiveDetails`, `OnchainReceiveDetails`, and so on, each defined
/// beside the facade that creates that operation — and this trait is what
/// they have in common. It has no methods: a details record is plain data,
/// read field by field, and this trait exists to name the contract all of
/// them obey and to be the bound on [`DetailedOperationState::Details`].
/// Read one with [`Operation::details`].
///
/// # Why operations need this
///
/// This crate promises that an [`OperationId`] is all it takes to pick an
/// operation back up. Without a persisted record that promise is false, and
/// the gap is not a small one: the notes a sender must hand to a receiver,
/// the invoice a payee must show as a QR code, and the deposit address a
/// depositor must display exist only in the value the original facade call
/// returned. A [`Operation::updates`] subscription cannot recover them,
/// because it is not a replay — it yields the state the operation is in
/// *now*, and the artifact was never in a state to begin with. After a
/// restart the data needed to render or complete the operation would simply
/// be gone.
///
/// The other half of the same gap is the terms an operation was executed
/// on. A lightning fee and route are quoted once, do not appear in
/// upstream's progress stream at all, and must remain readable however the
/// payment ends — a refunded or failed send has a fee and a route just as a
/// successful one does, and a receipt that can only be produced for
/// successes is not a receipt.
///
/// # The placement rule
///
/// Every value an operation exposes belongs in exactly one of three places,
/// decided by asking when it comes into existence and how long it stays
/// readable. This rule is binding on every facade module in this crate.
///
/// 1. **Fixed when the operation is created, or when its quote was
///    executed → the details record, and only there.** The notes, the
///    invoice, the address, the destination, the resolved amounts, the fee
///    and route the quote committed to, the moment it started. None of it
///    ever changes, so none of it belongs in a stream of transitions:
///    putting it there would re-deliver the same bytes on every update, and
///    for a bearer instrument like [`Notes`](crate::Notes) it would copy
///    spendable value into every subscriber, every log line and every
///    `Debug` rendering of a state.
/// 2. **Comes into existence at a transition *and* is carried by every
///    state from then on, final states included → the state, and only
///    there.** A final state is sticky: it never transitions again, so
///    [`Operation::state`] returns it for the rest of time and anything on
///    it is already durable. The preimage of a successful lightning payment
///    is the example — it exists exactly when
///    [`LnSendState::Success`](crate::LnSendState::Success) does, and
///    copying it into the record would duplicate a value that can never be
///    missed.
/// 3. **Comes into existence at a transition but is *not* on every later
///    state → both.** The state announces it; the record keeps it. The
///    funding transaction a caller learns from
///    [`OnchainReceiveState::WaitingForConfirmation`](crate::OnchainReceiveState::WaitingForConfirmation)
///    is gone by
///    [`Claimed`](crate::OnchainReceiveState::Claimed), and a lightning
///    send's fee and route appear only on success. These are exactly the
///    values that "subscriptions do not replay" turns into lost data, and
///    they are what an `Option` field on a record is for: absent until the
///    fact is established, then set once and never changed again.
///
/// The test to apply is a single sentence: **a caller must never need to
/// have seen an earlier state.** Whatever it takes to render or complete a
/// reattached operation, [`Operation::details`] and the current state must
/// supply between them. Case 3 is the only licence to duplicate a state's
/// payload, and it must be justified in the field's own documentation.
///
/// # What implementing this obliges an implementation to do
///
/// - **The record is committed before the creating call returns.** The
///   write that creates the operation writes its details, in the same
///   storage transaction. A process that dies immediately after
///   [`Ecash::send`](crate::Ecash::send) returns must still find the notes
///   on the next start; that is the whole point.
/// - **Fields fill in at most once and never move.** A field is either set
///   at creation or set in the same write that records the transition
///   establishing it. `None` becomes `Some` at most once — an optional fact
///   need never come to exist, and a record whose operation ended without
///   it keeps the `None` for good — and a value never changes to a
///   different value, and never reverts.
/// - **Nothing is derived at read time from a state that may have been
///   missed.** If a value can only be observed as a transition happens, it
///   is persisted as it happens.
/// - **No secrets beyond what the caller already holds.** A record may
///   carry the caller's own bearer artifacts (that is what makes it useful),
///   and never seed material.
///
/// # Supertraits, and the shape a binding sees
///
/// `Clone` because a record is a value handed out per call; `Send + Sync +
/// 'static` because it crosses the same task and thread boundaries as the
/// handle that produced it. Unlike [`OperationState`], `Debug` *is*
/// required: a details record is plain data whose purpose is to be rendered
/// and logged, every one of them derives it anyway under this crate's
/// `missing_debug_implementations` lint, and the types that must not appear
/// in a log redact their own `Debug` rather than relying on containers to
/// omit them (see [`Notes`](crate::Notes)).
///
/// Each record is a concrete, non-generic struct of plain owned fields — no
/// generics, no tuples, no borrowed data, no `impl Trait` — so it generates
/// into a Swift or Kotlin data class and a TypeScript interface
/// mechanically. [`Operation`] stays generic in Rust and the FFI layer
/// monomorphises it, which turns [`Operation::details`] into one method per
/// kind returning one concrete record.
///
/// # Sealed
///
/// Sealed for the same reason [`OperationState`] is: the set of operation
/// kinds, and therefore of details records, is this crate's to define, and a
/// record type the SDK does not know about could not cross a
/// foreign-function boundary at all.
pub trait OperationDetails:
    sealed::Sealed + Clone + core::fmt::Debug + Send + Sync + 'static
{
}

/// An [`OperationState`] whose kind persists an [`OperationDetails`] record.
///
/// This is the link between the two halves of the pattern. It names, for one
/// operation kind, the record that kind persists, which is what lets
/// [`Operation::details`] be written once — one signature, one error
/// contract, one piece of documentation — instead of six times across the
/// facade modules. A facade adds the second half of the pattern in one line
/// beside its state enum:
///
/// ```ignore
/// impl DetailedOperationState for EcashSendState {
///     type Details = EcashSendDetails;
/// }
/// ```
///
/// # Why this is not an associated type on `OperationState`
///
/// A required associated item on [`OperationState`] would have to be
/// supplied by every implementor at once, and a kind that genuinely has no
/// fixed facts worth persisting — a recovery is one — should be able to say
/// so by not implementing this trait, rather than by declaring an empty
/// record that a binding then has to generate and a caller has to wonder
/// about. Both traits are sealed, so which kinds have details remains this
/// crate's decision either way.
pub trait DetailedOperationState: OperationState {
    /// The record [`Operation::details`] returns for this kind.
    ///
    /// One concrete type per kind, never a generic one: the FFI layer
    /// monomorphises [`Operation`] and needs a nameable return type per
    /// generated class.
    type Details: OperationDetails;
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
/// # What survives a restart, and where to read it
///
/// Two things, and it takes both to render a reattached operation:
///
/// - **The current state**, from [`state`](Operation::state) or
///   [`updates`](Operation::updates). This is where the operation has got
///   to. It is *not* a history: a subscription opened after the fact yields
///   the state now and never the states before it.
/// - **The persisted details**, from [`details`](Operation::details) — the
///   notes handed out, the invoice issued, the address allocated, the fee
///   and route of the quote that was executed. These are the facts that were
///   fixed when the operation was created and that no state carries.
///
/// So the full reattachment path is
/// [`Federation::operation`](crate::Federation::operation) by id, the
/// matching accessor on [`AnyOperation`] for a typed handle, then
/// `details()` and `state()` — and nothing else. An application never has to
/// have kept the value the original facade call returned, and never has to
/// have been watching. [`OperationDetails`] states which values live in
/// which of the two, and why.
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
    ///
    /// Never
    /// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation): a
    /// typed handle exists only where [`AnyOperation::support`] was
    /// [`Observable`](OperationSupport::Observable), so a record this build
    /// cannot read is refused before a handle for it exists. The residue that
    /// check cannot see — a state that named a schema version this build reads
    /// and then did not honour it — is
    /// [`Internal`](crate::ErrorCode::Internal), because it is a bug on one
    /// side or the other rather than a version gap to report.
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

impl<S: DetailedOperationState> Operation<S> {
    /// Reads this operation's persisted details: the one-shot artifacts it
    /// produced and the terms it was executed on.
    ///
    /// This is the other half of observing an operation, beside
    /// [`state`](Operation::state). The state says where the operation has
    /// got to; this says what it is — the notes to hand over, the invoice to
    /// show, the address to display, the amounts, the fee and route that were
    /// committed to. It is the call that makes an operation id sufficient to
    /// pick an operation back up, because none of that is in the state
    /// stream and the stream does not replay.
    ///
    /// Which values appear here rather than on a state, and why, is
    /// [`OperationDetails`]'s placement rule. The short version: fixed facts
    /// and artifacts here, progress there, and anything a transition
    /// announces but later states drop appears in both.
    ///
    /// Every kind gets its own concrete record, so this is effectively an
    /// inherent method per monomorphisation — `Operation<EcashSendState>`
    /// returns an `EcashSendDetails` and nothing else, exactly as
    /// [`request_cancel`](crate::Operation::request_cancel) is inherent to
    /// that same concrete type. The generic impl block is how the contract gets
    /// written once, not a way of returning one type for several kinds.
    ///
    /// # It is stable, and it is not a race
    ///
    /// Calling this twice returns the same values, with one exception: a
    /// field documented as filling in later (a deposit's transaction id, for
    /// instance) goes from `None` to `Some` at most once and then never
    /// changes — and stays `None` if the fact it records never comes to
    /// exist. Nothing here is ever rewritten or withdrawn, so details read
    /// before a state and details read after it never contradict each other,
    /// and there is no ordering a caller has to get right between this call
    /// and [`state`](Operation::state).
    ///
    /// # Async and fallible for the same reason as `Federation::operation`
    ///
    /// The record lives in storage, not in this handle. Reading it can fail
    /// the way any read can, and it must be awaited.
    ///
    /// # Errors
    ///
    /// Only infrastructure failures —
    /// [`Storage`](crate::ErrorCode::Storage) if the record cannot be read,
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed) if the
    /// federation was closed or the SDK shut down, and
    /// [`Internal`](crate::ErrorCode::Internal). Never
    /// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation): a
    /// typed handle exists only for an operation this build can observe — a
    /// kind it knows, an accessor it was compiled with, a state schema it
    /// reads — and that determination is made once, earlier, by
    /// [`AnyOperation::supported_kind`]. The record is never absent for an
    /// operation that exists — it is written in the same storage transaction
    /// as the operation itself — so there is no `Option` here to unwrap.
    pub async fn details(&self) -> Result<S::Details> {
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
/// # Two different drops, two different meanings
///
/// These are separate events and are easy to conflate, so they are stated
/// apart. Neither ever touches the operation itself, which runs detached.
///
/// - **Dropping a pending [`next`](OperationUpdates::next) future** cancels
///   only *that wait*. The subscriber survives and stays usable: a later
///   `next()` resumes from the same cursor position, and no transition that
///   happened in between is lost or skipped. This is what makes the
///   subscriber safe to use inside `select!`, a timeout, or any other
///   combinator that drops the loser — the ordinary way an async caller
///   waits on two things at once.
/// - **Dropping the subscriber** ends *that subscription* and nothing else.
///   Other subscribers keep their own cursors, and
///   [`Operation::updates`] hands out a fresh one at any time.
///
/// See [`next`](OperationUpdates::next) for the full contract, including
/// what this requires of an implementation.
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
    /// # Dropping this future is safe, and is not dropping the subscription
    ///
    /// This call is **cancellation-safe**. Dropping the future it returns
    /// before it resolves cancels only that one wait. Specifically:
    ///
    /// - The subscriber remains valid and usable. Calling `next()` again is
    ///   correct and expected.
    /// - The cursor does not move. The next call resumes from exactly the
    ///   position the abandoned one was waiting at.
    /// - **No transition is lost.** A state the operation reached while no
    ///   future was pending is still delivered by the following `next()`; it
    ///   is not dropped on the floor because nobody happened to be awaiting
    ///   at that instant.
    ///
    /// That is what lets a caller write the ordinary things — race `next()`
    /// against a timeout, put it in a `select!` beside a shutdown signal,
    /// abandon it when a screen closes — without having to reason about
    /// whether doing so silently skipped a state.
    ///
    /// Dropping the **subscriber** is the different event: it ends that
    /// subscription. Dropping either one leaves the operation itself running,
    /// as always.
    ///
    /// This is a constraint on the implementation, not merely a description
    /// of one. The subscription must be built so that its position advances
    /// when a state is *handed to the caller*, never when a future is merely
    /// polled — a naive implementation that consumes from a shared queue
    /// inside the future, or that only buffers while someone is awaiting,
    /// violates it and drops states under exactly the `select!` usage above.
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
///
/// # What "supported" means here
///
/// One sentence, and the rest of this type follows from it: **an operation is
/// supported when this build can actually observe its typed state.** That is
/// a claim about the reader as much as about the record, and it takes two
/// things to hold at once.
///
/// 1. **The discriminator is one this build knows.** It maps onto an
///    [`OperationKind`] other than [`Unknown`](OperationKind::Unknown).
/// 2. **The state schema is one this build reads.** A record written by a
///    newer SDK can name a kind this build knows while its persisted *state*
///    uses a schema this build has never seen. The schema version travels on
///    [`RawOperationKind::schema_version`], so this is decidable up front
///    rather than at the point of reading the state.
///
/// Anything less and there is no typed handle to be had, so reporting it as
/// supported would only move the disappointment later — which is exactly what
/// [`supported_kind`](AnyOperation::supported_kind) exists to prevent.
///
/// # Four questions, four calls
///
/// They are easy to conflate, and each has exactly one answer:
///
/// - **"What is it?"** — [`kind`](AnyOperation::kind). Never fails, always
///   answers, and answers [`OperationKind::Unknown`] only when condition 1
///   fails. This is what a history screen labels rows with, so a record whose
///   kind is known but whose *state* this build cannot read still reports its
///   real kind: the discriminator was read and understood, and the row should
///   say what the record says rather than be degraded on the reader's
///   account.
/// - **"How far can this build go with it, and why no further?"** —
///   [`support`](AnyOperation::support). One of
///   [`OperationSupport`]'s variants: `Observable`, or which of the two
///   conditions failed. Infallible, so it is the form to log, show in a bug
///   report, or use to decide between "written by a newer version" and
///   "not recognised at all" in a message to a user.
/// - **"May I act on it, and what is it?"** —
///   [`supported_kind`](AnyOperation::supported_kind). `Ok(kind)` exactly when
///   [`support`](AnyOperation::support) is
///   [`Observable`](OperationSupport::Observable), and
///   [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation)
///   otherwise. This is the gate to pass before doing anything with the
///   operation, and the one call in this crate that produces that code.
/// - **"What exactly was recorded?"** —
///   [`raw_kind`](AnyOperation::raw_kind). The persisted discriminator
///   verbatim, for logs and bug reports, so an application can say *which*
///   thing it did not understand instead of only that something was
///   unrecognised.
///
/// ## What `None` from an accessor means
///
/// The seven `as_*` accessors below answer one narrow question — "is it
/// *this* kind, and if so give me the handle" — and they return `Some` on
/// exactly the condition above: the kind matches *and*
/// [`support`](AnyOperation::support) is
/// [`Observable`](OperationSupport::Observable). So `None` covers three
/// situations, and deliberately does not distinguish them:
///
/// - it is a different kind — the ordinary mismatch;
/// - the record is one this build cannot interpret at all
///   ([`UnknownKind`](OperationSupport::UnknownKind));
/// - it *is* this kind, and this build still cannot observe it
///   ([`StateSchemaTooNew`](OperationSupport::StateSchemaTooNew)).
///
/// That third case is why the accessors are not merely a kind test. Handing
/// back a typed [`Operation`] whose state cannot be decoded would move the
/// failure to [`Operation::state`], one call after the caller stopped
/// checking; `None` keeps the whole question in one place. Which of the three
/// it was is available, once, from [`support`](AnyOperation::support) and
/// [`supported_kind`](AnyOperation::supported_kind), rather than making each
/// of seven accessors return a two-level answer that most callers would
/// unwrap twice for nothing. A caller that will act on the operation asks
/// first; a caller that is only sorting rows by kind never has to.
///
/// ## What no pre-read check can know
///
/// [`Observable`](OperationSupport::Observable) is everything that can be
/// established *before* the operation's state is read: a known kind, and a
/// recorded schema version this build reads. It
/// is not a promise that the read will succeed, and this type does not
/// pretend otherwise — the check sees a version number, not the bytes behind
/// it, and a record that carries no version at all
/// ([`schema_version`](RawOperationKind::schema_version) is `None`) offers
/// nothing to compare, so it is treated as not ruled out rather than as
/// ruled in.
///
/// What that buys is that every reason knowable in advance is reported in
/// advance, by one call, instead of surfacing as a surprise from a typed
/// handle the caller already holds. What is left over is the residue: a state
/// that passed both conditions and still cannot be decoded means a
/// producer wrote a schema version it did not honour, which is a bug on one
/// side or the other and not a forward-compatibility case. That surfaces as
/// [`Internal`](crate::ErrorCode::Internal) from
/// [`Operation::state`], never as
/// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation) — the
/// code keeps meaning "this build cannot interpret this operation", decided
/// once, at the gate.
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
    /// Never fails and never refuses: an operation recorded by a build that
    /// understood something this one does not still has an id, still has a
    /// row, and reports [`OperationKind::Unknown`] here — see that variant.
    /// Use [`raw_kind`](AnyOperation::raw_kind) to find out what it was
    /// recorded as, and
    /// [`supported_kind`](AnyOperation::supported_kind) instead of this when
    /// the next thing you do is act on the operation rather than label it.
    ///
    /// This is the reading of the *discriminator* and nothing more, so it
    /// answers with a real kind even when nothing can be done with the
    /// operation: a record whose state was written at a schema this build
    /// cannot read still says it is a lightning send, and that is an honest
    /// label, reported as unsupported by [`support`](AnyOperation::support).
    /// Nothing here degrades to [`Unknown`](OperationKind::Unknown) on the
    /// reader's account.
    pub fn kind(&self) -> OperationKind {
        unimplemented!()
    }

    /// How far this build can go with this operation, and why no further.
    ///
    /// The reason behind [`supported_kind`](AnyOperation::supported_kind), as
    /// an ordinary value instead of an error. Use it where the answer is
    /// going into a log line, a bug report, or a message to a user — telling
    /// someone "this operation was written by a newer version than this one"
    /// and "this version does not recognise that operation at all" are
    /// different things to say, and
    /// [`supported_kind`](AnyOperation::supported_kind) collapses them into
    /// one code on purpose.
    ///
    /// [`Observable`](OperationSupport::Observable) is the answer that means
    /// supported: the matching `as_*` accessor will hand back a typed handle.
    /// Every other variant names one of the two conditions under
    /// *What "supported" means here*, and means no typed handle exists in this
    /// build — pair it with [`raw_kind`](AnyOperation::raw_kind) to say which
    /// record it was about.
    ///
    /// Infallible and cheap, like [`kind`](AnyOperation::kind) and
    /// [`raw_kind`](AnyOperation::raw_kind): the decision is made from those
    /// two answers alone, so it reads no storage, touches no network, and
    /// does not read the operation's state. What that last part costs is
    /// stated under *What no pre-read check can know*.
    pub fn support(&self) -> OperationSupport {
        support_of(self.kind(), &self.raw_kind())
    }

    /// This operation's kind if this build can observe its typed state, and
    /// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation) if it
    /// cannot.
    ///
    /// The fallible twin of [`kind`](AnyOperation::kind), the gate to pass
    /// before acting on an operation, and the one call in this crate that
    /// produces that code. It exists because the two failures a caller can
    /// meet here are not the same failure: asking an ecash accessor about a
    /// lightning payment is an ordinary mismatch to branch on, while meeting
    /// an operation this build cannot observe is a condition to report, log,
    /// and stop on. Both look like `None` from an `as_*` accessor, which also
    /// left [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation)
    /// documented but unreachable.
    ///
    /// `Ok(kind)` means both conditions under *What "supported" means here*
    /// hold, so the accessor for that kind will hand back a typed handle. Two
    /// consequences worth stating outright:
    ///
    /// - It is **never** `Ok(OperationKind::Unknown)`: that is precisely the
    ///   case that becomes the error.
    /// - It is **never** `Ok` for a record whose state was written at a schema
    ///   version newer than this build reads, even when the kind itself is one
    ///   this build knows. The kind discriminator is only half the question,
    ///   and a caller that got past this gate and then failed to read the
    ///   state would be meeting the very confusion this call removes.
    ///
    /// [`support`](AnyOperation::support) answers the same question and says
    /// *which* condition failed, without an error.
    ///
    /// # Errors
    ///
    /// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation), and
    /// nothing else, for each of the two: an unrecognised discriminator, and
    /// a state schema newer than this build reads. The message names the record and
    /// the reason, and [`support`](AnyOperation::support) reports the same
    /// reason machine-readably. This reads no storage and touches no network:
    /// the record was already read to produce this handle.
    pub fn supported_kind(&self) -> Result<OperationKind> {
        supported_kind_of(self.kind(), &self.raw_kind())
    }

    /// The discriminator this operation was persisted under, verbatim.
    ///
    /// [`kind`](AnyOperation::kind) is this SDK's reading of the record;
    /// this is what the record actually says. The difference matters when the
    /// reading is [`OperationKind::Unknown`]: an application that can only
    /// report "an operation from another version" cannot help anyone, whereas
    /// one that reports the module and tag it did not recognise gives a user
    /// something to show and a maintainer something to fix.
    ///
    /// Available for every kind, not only the unknown one. A bug report about
    /// a mint spend recorded at an older schema version is as useful as one
    /// about a record from the future, and restricting the raw discriminator
    /// to unrecognised records would mean it is missing from exactly the
    /// logs written before anyone knew there was a problem.
    ///
    /// **For humans, never for control flow.** Branching on these strings
    /// would recreate the free-form text matching this crate refuses
    /// everywhere else — it is why failure states carry no machine-readable
    /// `reason` and why [`Error::code`](crate::Error::code) exists apart from
    /// its message. [`kind`](AnyOperation::kind) and
    /// [`supported_kind`](AnyOperation::supported_kind) are the values to
    /// branch on.
    ///
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
    /// Without this a recovery would be the one operation an application
    /// could not reattach to after a restart. A process that dies mid-rescan
    /// leaves a persisted recovery running; on the next build,
    /// [`Federation::operation`](crate::Federation::operation) finds it and
    /// [`kind`](AnyOperation::kind) reports [`OperationKind::Recovery`], and
    /// this is how its progress is then observed — rather than by attempting
    /// a spend and catching [`Recovering`](crate::ErrorCode::Recovering),
    /// which is precisely the error-driven discovery this crate rejects.
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
/// operation can still say *what* it could not interpret — and so that a
/// build which can is not left unable to report the schema it read.
///
/// # Why this is a record beside the enum, not a payload inside it
///
/// The obvious alternative is
/// `OperationKind::Unknown(String)`. It is the wrong shape, for the same
/// reasons [`ErrorCode`](crate::ErrorCode) is a fieldless `Copy` enum
/// carrying no payloads:
///
/// - [`OperationKind`] is `Copy`, fieldless, and matched with unit patterns.
///   A payload breaks `Copy`, breaks every one of those matches, and turns a
///   plain Swift, Kotlin or TypeScript enum into an
///   enum-with-associated-values in three languages at once.
/// - It is also a *field* on [`ActivityItem`](crate::ActivityItem), a
///   list-rendering type deliberately kept small. A payload would put two
///   strings and a version number on every row in history for the benefit of
///   the rare unrecognised one.
/// - A payload on `Unknown` could only ever describe unknown operations,
///   whereas the raw discriminator is worth having for every kind. A side
///   accessor gives it for all of them.
///
/// # Do not branch on these strings
///
/// They are diagnostics: log them, show them in a bug report, put them
/// behind a "details" disclosure. Matching on them would be the same mistake
/// as parsing [`Error::message`](crate::Error::message) — this crate keeps a
/// stable machine-readable answer ([`OperationKind`], [`OperationSupport`],
/// [`AnyOperation::supported_kind`]) precisely so that nothing has to. That
/// applies to [`schema_version`](RawOperationKind::schema_version) too: it is
/// a number rather than a string, and this crate does compare it — but it
/// compares it against a ceiling only this build knows, and reports the
/// outcome as [`OperationSupport`]. Re-deriving that comparison outside the
/// crate would pin an application to a ceiling that moves every release.
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
    /// one separately from [`kind`](RawOperationKind::kind) — `"mint"`,
    /// `"ln"`, `"wallet"`, or a module this build has never heard of.
    ///
    /// `None` when the persisted form carries no separate module marker,
    /// which is not a failure to read one: some records simply do not have
    /// it.
    pub module: Option<String>,
    /// The schema version the record was written with, when one was
    /// recorded.
    ///
    /// This is what makes an unrecognised record actionable rather than
    /// merely puzzling: "a `mint` operation at schema 4 and this build reads
    /// up to 3" tells a maintainer what happened, where "an operation from
    /// another version" does not. `None` when the record predates versioning
    /// or does not carry a version.
    ///
    /// It is also the input to the second of the two conditions behind
    /// [`AnyOperation::support`]: a version newer than this build reads for
    /// that kind means the operation's typed state cannot be observed here,
    /// however familiar its kind. Read the verdict from
    /// [`AnyOperation::support`] rather than comparing this against a
    /// hard-coded ceiling — and note what `None` does *not* mean. A record
    /// carrying no version offers nothing to compare, so it is treated as
    /// "not ruled out", never as "known good": that is the honest reading of
    /// a check made before the state itself is read.
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
/// must include a wildcard arm. Note that this is a Rust-only guarantee — a
/// generated binding pinned to an older SDK fails to decode a kind added
/// since rather than mapping it to its own unknown case, which is why
/// [`Unknown`](OperationKind::Unknown) exists as a real variant that every
/// binding already has: the mapping happens inside this crate, so no decoder
/// is handed a tag it does not know. See the crate-level
/// forward-compatibility section.
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
    /// id, so reporting it as `Unknown` is strictly better than failing the
    /// lookup — an application can list it as "an operation from a newer
    /// version" instead of pretending the record does not exist.
    ///
    /// This variant is the *reading*, not the record. What was actually
    /// persisted stays readable through [`AnyOperation::raw_kind`]: the tag,
    /// the module it belonged to, and the schema version it was written at,
    /// so an application can report which thing it did not understand rather
    /// than only that something was unrecognised.
    ///
    /// None of the typed accessors on [`AnyOperation`] match it — they return
    /// `None`, exactly as they do for a mismatched kind — and
    /// [`AnyOperation::support`] is where the difference between those
    /// `None`s becomes visible: it reports
    /// [`OperationSupport::UnknownKind`] for this case and one of two other
    /// reasons for a record whose kind *is* known but whose state this build
    /// cannot observe. All of them reach
    /// [`ErrorCode::UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation)
    /// through [`AnyOperation::supported_kind`], which is what makes that code
    /// reachable; this variant is the reason it exists, not the only route to
    /// it.
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
/// Returned by [`AnyOperation::support`], which is where the two conditions
/// under *What "supported" means here* are checked.
/// [`Observable`](OperationSupport::Observable) is the one answer that means
/// supported; every other variant names the condition that failed and means
/// there is no typed handle to be had in this build.
///
/// # Why the reasons are worth telling apart
///
/// Because they are different things to say and different things to do.
/// "Written by a newer version than this one" is an application that needs
/// updating, and the data is ahead of the code. "This build does not
/// recognise it at all" is the one to put in a bug report verbatim.
/// [`AnyOperation::supported_kind`] flattens both into
/// [`UnsupportedOperation`](crate::ErrorCode::UnsupportedOperation) because
/// control flow does not care which; a log line, a support ticket, and a
/// message shown to a user all do.
///
/// # The shape a binding sees
///
/// Fieldless and `Copy`, for the same reasons [`OperationKind`] and
/// [`ErrorCode`](crate::ErrorCode) are: it is matched with unit patterns and
/// generates into a plain Swift, Kotlin or TypeScript enum. What was recorded
/// is not duplicated onto it — that stays beside it, in
/// [`AnyOperation::raw_kind`], which is the value to log next to this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OperationSupport {
    /// This build can observe the operation's typed state: the kind is one it
    /// knows, and the recorded state schema is one it reads.
    ///
    /// The matching `as_*` accessor on [`AnyOperation`] returns `Some`, and
    /// [`AnyOperation::supported_kind`] returns `Ok`.
    ///
    /// This is everything establishable before the state is read, which is
    /// not the same as a promise that reading it will succeed; see *What no
    /// pre-read check can know* on [`AnyOperation`] for the residue and where
    /// it surfaces.
    Observable,
    /// The persisted discriminator is not one this build maps onto a kind, so
    /// there is nothing to interpret it as.
    ///
    /// [`AnyOperation::kind`] reports [`OperationKind::Unknown`] for the same
    /// record, and [`AnyOperation::raw_kind`] says what was actually written —
    /// which is what to log, because "an operation from another version" on
    /// its own helps nobody.
    UnknownKind,
    /// The kind is known, but the record's state was written at a schema
    /// version newer than this build reads.
    ///
    /// The kind discriminator is only half of what it takes to read an
    /// operation. A newer SDK can record a kind this build has always known
    /// while persisting its state in a form this build has never seen, and
    /// [`RawOperationKind::schema_version`] is what makes that detectable
    /// before anything tries to decode it. Reporting it here rather than at
    /// the point of reading the state is the whole point of the gate: a caller
    /// meets it once, from [`AnyOperation::supported_kind`], instead of
    /// meeting it from a typed handle it was already holding.
    ///
    /// This describes the gap between the record and this build, not a
    /// defect in the record: the operation is real, the state is intact, and
    /// a build of the version that wrote it reads it fine.
    StateSchemaTooNew,
}

/// The newest operation-state schema version this build can read, for every
/// kind it knows.
///
/// One number today, because every kind's state schema is at its first
/// version and they were introduced together. It is reached through
/// `OperationKind::readable_state_schema` rather than compared directly, so
/// that a kind whose state schema is revised on its own — which is how module
/// state machines actually change — becomes a one-line divergence there
/// instead of a redesign here.
const READABLE_STATE_SCHEMA: u32 = 1;

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
        // Either a version this build reads, or a record that carries none —
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
        /// states do not carry — absent until it is known, then set once.
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
    /// The tag never feeds the decision — [`OperationKind`] is this crate's
    /// reading of it, and is passed separately — but it does feed the error
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
}
