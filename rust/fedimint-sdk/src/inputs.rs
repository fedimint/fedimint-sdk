//! Whether the value a rejected funding transaction removed is spendable again.
//!
//! # Why a rejection is not an ending
//!
//! Funding a send submits a client transaction whose inputs are notes the primary module
//! selected, and selecting them takes them out of the spendable set before consensus has said
//! anything. If the federation then rejects that transaction the notes were never actually
//! consumed, but the client has already stopped treating them as spendable, and putting them
//! back is a second transaction the mint's own input state machine submits afterwards. That
//! recovery runs asynchronously, it can take several consensus rounds, and it can itself fail.
//!
//! So the upstream signal "the funding transaction was rejected" is the *start* of the last
//! part of the operation, not the end of it. Treating it as
//! [`Refunded`](crate::LnSendState::Refunded) on the spot is what this module exists to stop:
//! that state promises an application the money is spendable again, and at the moment of the
//! rejection nobody knows yet whether it is.
//!
//! [`settle`] is therefore what a send driver calls instead of ending: it waits for the
//! operation to have nothing left to run, then reports which ending the evidence supports.
//!
//! # What can be proven here, and what cannot
//!
//! The authoritative answer lives in the mint's input state machine, which ends in
//! `RefundSuccess` or in `Error`. That type is not nameable outside `fedimint-mint-client`
//! (its `input` module is private), so this cannot read it, and no public API reports it.
//! What *is* public is the submission state of every transaction the operation made, which
//! upstream itself reads the same way to compute an operation's fees
//! (`Client::get_operation_fees`). A refund is a transaction, so an accepted one is real
//! evidence that the notes were reissued.
//!
//! That evidence is sound but incomplete: the mint may split a refund that failed as a bundle
//! into per-note transactions, so a rejected refund followed by accepted ones is a restoration
//! this module still reports as unproven. It errs that way deliberately. The contract is that
//! `Refunded` is only ever emitted when the value is known to be spendable again, and a
//! `Failed` an application investigates is a smaller harm than a `Refunded` that is wrong.
//!
//! Tightening this needs upstream to report the input recovery's own outcome; the gaps are
//! tracked as fedimint#9099, fedimint#6546 and fedimint#8421.

use std::sync::Weak;

use fedimint_client::Client;
use fedimint_client_module::transaction::{TxSubmissionStates, TxSubmissionStatesSM};
use fedimint_core::config::FederationId;
use fedimint_core::core::OperationId;

use crate::Result;
use crate::sdk::SdkInner;

/// How often the gate re-reads whether the operation still has state machines running.
///
/// There is nothing to subscribe to here: the executor does not publish "this operation went
/// quiet", so the only way to learn it is to look. A second between looks is short against a
/// recovery measured in consensus rounds and long enough that a parked subscription is not
/// doing meaningful work.
#[cfg(not(test))]
const SETTLE_POLL: core::time::Duration = core::time::Duration::from_secs(1);
/// Shortened under `cfg(test)` so a test that drives the gate does not sit out the production
/// interval to do it.
#[cfg(test)]
const SETTLE_POLL: core::time::Duration = core::time::Duration::from_millis(25);

/// What settling a rejected funding transaction established about the value it removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Restoration {
    /// The value is spendable again: the operation's recovery transaction was accepted, and
    /// nothing else about the operation is unresolved.
    Restored,
    /// The recovery settled without establishing a clean return. The reason is diagnostic.
    Unproven(String),
}

/// Waits for an operation's state machines to finish, then reports what that established.
///
/// Call this instead of ending a send at the upstream signal that its funding was rejected.
/// It returns once the operation has nothing left running, which is the earliest moment the
/// question "is the value back?" has an answer at all.
///
/// # Holding the client
///
/// The wait is unbounded: a federation that is slow or unreachable never ends it. It runs
/// through [`crate::federation::wait_holding_client`] for the reason that function documents
/// at length: a driver's stream outlives the call that built it, an idle subscriber stops
/// polling it, and a stream parked on an `await` that holds a `ClientHandleArc` never gives
/// the handle back, so every later close fails. Running the wait on its own task keeps the
/// handle reclaimable and lets the client's own shutdown end it.
///
/// The federation is looked up again rather than captured, so nothing here keeps it alive.
pub(crate) async fn settle(
    sdk: Weak<SdkInner>,
    federation_id: FederationId,
    id: OperationId,
) -> Result<Restoration> {
    let federation = crate::federation::federation_again(&sdk, federation_id)?;
    let client = federation.client(false).await?;
    let handle = client.handle();
    // The read guard goes before the wait starts: everything below is unbounded, and holding
    // it across that would block `quiesce` from ever taking its write lock.
    drop(client);

    let stop = handle.task_group().make_handle().make_shutdown_rx();
    crate::federation::wait_holding_client(handle, stop, move |client| async move {
        while client.has_active_states(id).await {
            fedimint_core::runtime::sleep(SETTLE_POLL).await;
        }
        Ok(restoration_of(&client, id).await)
    })
    .await
}

/// What the operation's own transactions say once it has gone quiet.
///
/// Read from the submission state machines the client keeps for every transaction the
/// operation made, which is the same evidence `Client::get_operation_fees` reads, through the
/// same accessor and the same downcast. That accessor is documented upstream as being for
/// debug tooling, which is a fair warning and the reason nothing here depends on the shape of
/// what it returns beyond three counts: it is the only public view of the question, and
/// upstream's own fee accounting relies on it too.
///
/// The counts are what matter, not the identities: a send funds itself with exactly one
/// transaction, so on this path one rejection is the funding attempt and any acceptance is the
/// recovery that followed it.
async fn restoration_of(client: &Client, id: OperationId) -> Restoration {
    let (active, inactive) = client.executor().get_operation_states(id).await;
    let submissions = active
        .into_iter()
        .map(|(state, _)| state)
        .chain(inactive.into_iter().map(|(state, _)| state))
        .filter_map(|state| {
            state
                .as_any()
                .downcast_ref::<TxSubmissionStatesSM>()
                .map(|sm| sm.state.clone())
        });

    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut unresolved = 0usize;
    for state in submissions {
        match state {
            TxSubmissionStates::Accepted(_) => accepted += 1,
            TxSubmissionStates::Rejected(..) => rejected += 1,
            // `Created` after the operation has gone quiet is a transaction whose submission
            // never resolved either way, and `NonRetryableError` is one that gave up. Neither
            // says where the value is.
            TxSubmissionStates::Created(_) | TxSubmissionStates::NonRetryableError(_) => {
                unresolved += 1;
            }
        }
    }

    verdict(accepted, rejected, unresolved)
}

/// The rule, separated from the reading so it can be tested without a federation.
///
/// Restoration is claimed only for the one shape that means it and nothing else: the funding
/// attempt is the single rejection, at least one later transaction was accepted, and no
/// transaction is left unresolved. Every other shape is unproven, including the honest
/// possibility that the recovery is fine and merely took a route this cannot follow.
fn verdict(accepted: usize, rejected: usize, unresolved: usize) -> Restoration {
    if unresolved > 0 {
        return Restoration::Unproven(format!(
            "this payment's funding was rejected and {unresolved} of its transactions never \
             resolved, so the SDK cannot tell whether the value came back"
        ));
    }
    if rejected > 1 {
        return Restoration::Unproven(
            "this payment's funding was rejected and so was a transaction that followed it, so \
             the SDK cannot tell whether the value came back"
                .to_owned(),
        );
    }
    if accepted == 0 {
        return Restoration::Unproven(
            "this payment's funding was rejected and nothing the SDK can see put the value back"
                .to_owned(),
        );
    }
    Restoration::Restored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_rejection_and_an_acceptance_is_a_restoration() {
        assert_eq!(verdict(1, 1, 0), Restoration::Restored);
    }

    #[test]
    fn several_acceptances_after_the_one_rejection_still_restore() {
        // A refund split across per-note transactions, none of which was rejected.
        assert_eq!(verdict(4, 1, 0), Restoration::Restored);
    }

    #[test]
    fn a_rejection_with_nothing_accepted_is_unproven() {
        assert!(matches!(verdict(0, 1, 0), Restoration::Unproven(_)));
    }

    #[test]
    fn a_second_rejection_is_unproven_even_with_an_acceptance() {
        // The bundle-refund-then-per-note path lands here: the value may well be back, and
        // this deliberately refuses to say so rather than guessing.
        assert!(matches!(verdict(2, 2, 0), Restoration::Unproven(_)));
    }

    #[test]
    fn an_unresolved_submission_is_unproven() {
        assert!(matches!(verdict(1, 1, 1), Restoration::Unproven(_)));
    }

    #[test]
    fn the_unproven_reason_is_specific_to_what_was_seen() {
        let Restoration::Unproven(never_resolved) = verdict(1, 1, 1) else {
            panic!("an unresolved submission is unproven");
        };
        let Restoration::Unproven(nothing_back) = verdict(0, 1, 0) else {
            panic!("a rejection with no acceptance is unproven");
        };
        assert_ne!(
            never_resolved, nothing_back,
            "the two cases an application would investigate differently read the same"
        );
    }
}
