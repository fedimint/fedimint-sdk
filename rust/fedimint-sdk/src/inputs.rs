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
//! # What the evidence is
//!
//! The authoritative answer lives in the mint's input state machine, which ends in
//! `RefundSuccess` or in `Error`. That type is not nameable outside `fedimint-mint-client`
//! (its `input` module is private), so this cannot read it, and no public API reports it.
//! Two public readings stand in for it, and restoration needs both:
//!
//! 1. **The submission state of every transaction the operation made**, which upstream itself
//!    reads the same way to compute an operation's fees (`Client::get_operation_fees`). A
//!    refund is a transaction, so an accepted one after the rejected funding is where the
//!    recovery happened.
//! 2. **Whether that transaction's primary-module outputs issued their notes.** Acceptance
//!    alone does not establish this: an accepted transaction's mint outputs can still end in
//!    `Aborted` or `Failed` without producing anything spendable, and the operation goes quiet
//!    either way. `Client::await_primary_bitcoin_module_output` is the public answer, and it
//!    reads a notifier that replays state machines from the database, so it resolves for an
//!    output that reached its ending before this ever asked, including across a restart.
//!
//! The reading is sound but incomplete: the mint may split a refund that failed as a bundle
//! into per-note transactions, so a rejected refund followed by accepted ones is a restoration
//! this module still reports as unproven. It errs that way deliberately. The contract is that
//! `Refunded` is only ever emitted when the value is known to be spendable again, and a
//! `Failed` an application investigates is a smaller harm than a `Refunded` that is wrong.
//!
//! Tightening this needs upstream to report the input recovery's own outcome; the gaps are
//! tracked as fedimint#9099, fedimint#6546 and fedimint#8421.

use std::collections::BTreeMap;
use std::sync::Weak;

use fedimint_client::Client;
use fedimint_client_module::transaction::{TxSubmissionStates, TxSubmissionStatesSM};
use fedimint_core::config::FederationId;
use fedimint_core::core::OperationId;
use fedimint_core::{OutPoint, TransactionId};

use crate::Result;
use crate::sdk::SdkInner;

/// How often the gate re-reads whether the operation still has state machines running.
///
/// There is nothing to subscribe to here: the executor does not publish "this operation went
/// quiet", so the only way to learn it is to look. A second between looks is short against a
/// recovery measured in consensus rounds and long enough that a parked subscription is not
/// doing meaningful work.
///
/// The loop this paces is deliberately unbounded, and deliberately has no backoff. It asks the
/// local database whether the operation still has state machines running, so it costs nothing
/// remote however long it runs, and there is no failure to back off from: the recovery either
/// finishes or the client stops. What ends it is not a retry limit but the caller going away or
/// the client's own shutdown, both of which [`settle`] documents.
#[cfg(not(test))]
const SETTLE_POLL: core::time::Duration = core::time::Duration::from_secs(1);
/// Shortened under `cfg(test)` so a test that drives the gate does not sit out the production
/// interval to do it.
#[cfg(test)]
const SETTLE_POLL: core::time::Duration = core::time::Duration::from_millis(25);

/// How long one recovery output gets to report whether it issued its notes.
///
/// This is asked only after the operation has gone quiet, so every output state machine has
/// already reached its ending and the notifier's replay answers immediately. The bound is for
/// the case that assumption is wrong: an answer that never comes must not park the gate for
/// ever, and a wait that elapses counts against restoration like any other unproven answer.
#[cfg(not(test))]
const OUTPUT_ISSUANCE_CHECK: core::time::Duration = core::time::Duration::from_secs(10);
/// Shortened under `cfg(test)` for the same reason [`SETTLE_POLL`] is.
#[cfg(test)]
const OUTPUT_ISSUANCE_CHECK: core::time::Duration = core::time::Duration::from_millis(50);

/// What settling a rejected funding transaction established about the value it removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Restoration {
    /// The value is spendable again: a transaction accepted after the rejected funding issued
    /// its notes, and nothing else about the operation is unresolved.
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

/// How one of the operation's transactions ended, as its submission state machine records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// Submitted, with no acceptance or rejection recorded for it.
    Submitted,
    /// Accepted in consensus.
    Accepted,
    /// Refused by a quorum on submission.
    Rejected,
}

/// One transaction the operation submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Submission {
    txid: TransactionId,
    outcome: Outcome,
}

/// What the submission history establishes, before anything is asked about the outputs.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Evidence {
    /// The funding was the only rejection, and these transactions were accepted. Whether the
    /// value is actually back still depends on what their outputs did.
    Recovery(Vec<TransactionId>),
    /// The history alone rules restoration out.
    Unproven(String),
}

/// What the operation's own transactions and their outputs say once it has gone quiet.
///
/// Read through `Executor::get_operation_states`, the same accessor and the same downcast
/// upstream's fee accounting uses. That accessor is documented upstream as being for debug
/// tooling, which is a fair warning and the reason nothing here depends on the shape of what it
/// returns beyond the three submission outcomes: it is the only public view of the question.
async fn restoration_of(client: &Client, id: OperationId) -> Restoration {
    let (submissions, gave_up, bodies) = submissions_of(client, id).await;
    let recovery = match evidence(&submissions, gave_up) {
        Evidence::Unproven(why) => return Restoration::Unproven(why),
        Evidence::Recovery(txids) => txids,
    };

    // Only outputs the primary module owns can put notes back, so only those are asked about.
    // Anything else in the transaction would never be reported by this call and would turn the
    // bounded wait below into a timeout for no reason.
    let primary = client.primary_module_for_btc().0;
    let mut answers = Vec::new();
    for txid in &recovery {
        for out_idx in bodies
            .get(txid)
            .into_iter()
            .flatten()
            .filter(|(_, module)| *module == primary)
            .map(|(out_idx, _)| *out_idx)
        {
            let out_point = OutPoint {
                txid: *txid,
                out_idx,
            };
            let answer = match fedimint_core::runtime::timeout(
                OUTPUT_ISSUANCE_CHECK,
                client.await_primary_bitcoin_module_output(id, out_point),
            )
            .await
            {
                Ok(Ok(())) => Ok(()),
                Ok(Err(err)) => Err(err.to_string()),
                Err(_) => Err("it did not report an outcome".to_owned()),
            };
            answers.push((*txid, out_idx, answer));
        }
    }

    restoration(&recovery, &answers)
}

/// Every transaction the operation submitted, what became of it, and what it was to create.
///
/// Returns the per-transaction outcomes, whether any submission gave up without naming a
/// transaction, and the primary-module-owned output indices of each transaction body seen.
///
/// The history is cumulative, not a snapshot: the executor moves a state machine's *previous*
/// state into the inactive set on every transition, so a transaction that was created and then
/// accepted appears twice, once as `Created` and once as `Accepted`. Both are the same
/// transaction, which is why this keys on the id rather than counting entries.
async fn submissions_of(
    client: &Client,
    id: OperationId,
) -> (
    Vec<Submission>,
    bool,
    BTreeMap<TransactionId, Vec<(u64, u16)>>,
) {
    let (active, inactive) = client.executor().get_operation_states(id).await;
    let states = active
        .into_iter()
        .map(|(state, _)| state)
        .chain(inactive.into_iter().map(|(state, _)| state))
        .filter_map(|state| {
            state
                .as_any()
                .downcast_ref::<TxSubmissionStatesSM>()
                .map(|sm| sm.state.clone())
        });

    let mut submissions = Vec::new();
    let mut gave_up = false;
    let mut bodies = BTreeMap::new();
    for state in states {
        match state {
            TxSubmissionStates::Created(transaction) => {
                let txid = transaction.tx_hash();
                bodies.entry(txid).or_insert_with(|| {
                    transaction
                        .outputs
                        .iter()
                        .enumerate()
                        .map(|(out_idx, output)| (out_idx as u64, output.module_instance_id()))
                        .collect::<Vec<_>>()
                });
                submissions.push(Submission {
                    txid,
                    outcome: Outcome::Submitted,
                });
            }
            TxSubmissionStates::Accepted(txid) => submissions.push(Submission {
                txid,
                outcome: Outcome::Accepted,
            }),
            TxSubmissionStates::Rejected(txid, _) => submissions.push(Submission {
                txid,
                outcome: Outcome::Rejected,
            }),
            // This one names no transaction, so it cannot be reconciled with the rest and is
            // carried on its own.
            TxSubmissionStates::NonRetryableError(_) => gave_up = true,
        }
    }
    (submissions, gave_up, bodies)
}

/// The rule over the submission history, separated from the reading so it can be tested
/// without a federation.
///
/// Entries are collapsed onto their transaction first: a terminal entry supersedes the
/// `Submitted` one for the same id, so only a transaction with no terminal entry at all counts
/// as unresolved. Restoration is then claimed for the one shape that means it and nothing
/// else: the funding attempt is the single rejection, at least one transaction was accepted,
/// and nothing is left hanging.
fn evidence(submissions: &[Submission], gave_up: bool) -> Evidence {
    let mut by_txid: BTreeMap<TransactionId, Outcome> = BTreeMap::new();
    for submission in submissions {
        let entry = by_txid.entry(submission.txid).or_insert(Outcome::Submitted);
        if submission.outcome != Outcome::Submitted {
            *entry = submission.outcome;
        }
    }

    let unresolved = by_txid
        .values()
        .filter(|outcome| **outcome == Outcome::Submitted)
        .count();
    if gave_up || unresolved > 0 {
        return Evidence::Unproven(format!(
            "this payment's funding was rejected and {} of its transactions never resolved, so \
             the SDK cannot tell whether the value came back",
            unresolved + usize::from(gave_up)
        ));
    }

    let rejected = by_txid
        .values()
        .filter(|outcome| **outcome == Outcome::Rejected)
        .count();
    if rejected > 1 {
        return Evidence::Unproven(
            "this payment's funding was rejected and so was a transaction that followed it, so \
             the SDK cannot tell whether the value came back"
                .to_owned(),
        );
    }

    let accepted: Vec<TransactionId> = by_txid
        .iter()
        .filter(|(_, outcome)| **outcome == Outcome::Accepted)
        .map(|(txid, _)| *txid)
        .collect();
    if accepted.is_empty() {
        return Evidence::Unproven(
            "this payment's funding was rejected and nothing the SDK can see put the value back"
                .to_owned(),
        );
    }

    Evidence::Recovery(accepted)
}

/// The verdict over what the recovery transactions' outputs reported, also kept pure.
///
/// An accepted transaction is where the notes were to be reissued, not proof that they were:
/// its outputs can still end without issuing anything. Restoration therefore needs an answer
/// for every recovery transaction and needs all of them to be good ones.
fn restoration(
    recovery: &[TransactionId],
    answers: &[(TransactionId, u64, core::result::Result<(), String>)],
) -> Restoration {
    for (txid, out_idx, answer) in answers {
        if let Err(why) = answer {
            return Restoration::Unproven(format!(
                "this payment's funding was rejected and the recovery's own output {out_idx} of \
                 {txid} did not issue its notes: {why}"
            ));
        }
    }

    if let Some(txid) = recovery
        .iter()
        .find(|txid| !answers.iter().any(|(answered, _, _)| answered == *txid))
    {
        return Restoration::Unproven(format!(
            "this payment's funding was rejected and the transaction {txid} that followed it \
             issued nothing the SDK can account for"
        ));
    }

    Restoration::Restored
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txid(byte: u8) -> TransactionId {
        use fedimint_core::bitcoin::hashes::Hash as _;
        TransactionId::from_byte_array([byte; 32])
    }

    fn submission(txid: TransactionId, outcome: Outcome) -> Submission {
        Submission { txid, outcome }
    }

    /// The history a settled rejection-and-recovery actually leaves behind. The executor moves
    /// each superseded state into the inactive set, so both transactions appear twice, and
    /// counting entries rather than transactions reported every clean recovery as unproven.
    #[test]
    fn a_superseded_created_is_not_an_unresolved_transaction() {
        let (funding, recovery) = (txid(0xf1), txid(0x2e));
        let history = [
            submission(funding, Outcome::Submitted),
            submission(funding, Outcome::Rejected),
            submission(recovery, Outcome::Submitted),
            submission(recovery, Outcome::Accepted),
        ];
        assert_eq!(
            evidence(&history, false),
            Evidence::Recovery(vec![recovery]),
        );
    }

    /// Order is not relied on: the inactive set is not returned in transition order.
    #[test]
    fn the_history_may_arrive_in_any_order() {
        let (funding, recovery) = (txid(0xf1), txid(0x2e));
        let history = [
            submission(recovery, Outcome::Accepted),
            submission(funding, Outcome::Submitted),
            submission(recovery, Outcome::Submitted),
            submission(funding, Outcome::Rejected),
        ];
        assert_eq!(
            evidence(&history, false),
            Evidence::Recovery(vec![recovery]),
        );
    }

    #[test]
    fn a_transaction_with_no_terminal_entry_is_unresolved() {
        let (funding, hanging) = (txid(0xf1), txid(0x2e));
        let history = [
            submission(funding, Outcome::Submitted),
            submission(funding, Outcome::Rejected),
            submission(hanging, Outcome::Submitted),
        ];
        assert!(matches!(evidence(&history, false), Evidence::Unproven(_)));
    }

    #[test]
    fn a_submission_that_gave_up_is_unresolved() {
        let (funding, recovery) = (txid(0xf1), txid(0x2e));
        let history = [
            submission(funding, Outcome::Rejected),
            submission(recovery, Outcome::Accepted),
        ];
        assert!(matches!(evidence(&history, true), Evidence::Unproven(_)));
    }

    #[test]
    fn several_acceptances_after_the_one_rejection_are_all_recovery() {
        // A refund split across per-note transactions, none of which was rejected.
        let funding = txid(0xf1);
        let history = [
            submission(funding, Outcome::Rejected),
            submission(txid(0x01), Outcome::Accepted),
            submission(txid(0x02), Outcome::Accepted),
        ];
        assert_eq!(
            evidence(&history, false),
            Evidence::Recovery(vec![txid(0x01), txid(0x02)]),
        );
    }

    #[test]
    fn a_rejection_with_nothing_accepted_is_unproven() {
        let history = [submission(txid(0xf1), Outcome::Rejected)];
        assert!(matches!(evidence(&history, false), Evidence::Unproven(_)));
    }

    #[test]
    fn a_second_rejection_is_unproven_even_with_an_acceptance() {
        // The bundle-refund-then-per-note path lands here: the value may well be back, and
        // this deliberately refuses to say so rather than guessing.
        let history = [
            submission(txid(0xf1), Outcome::Rejected),
            submission(txid(0x02), Outcome::Rejected),
            submission(txid(0x03), Outcome::Accepted),
        ];
        assert!(matches!(evidence(&history, false), Evidence::Unproven(_)));
    }

    #[test]
    fn the_unproven_reason_is_specific_to_what_was_seen() {
        let funding = txid(0xf1);
        let Evidence::Unproven(never_resolved) = evidence(
            &[
                submission(funding, Outcome::Rejected),
                submission(txid(0x02), Outcome::Submitted),
            ],
            false,
        ) else {
            panic!("an unresolved transaction is unproven");
        };
        let Evidence::Unproven(nothing_back) =
            evidence(&[submission(funding, Outcome::Rejected)], false)
        else {
            panic!("a rejection with no acceptance is unproven");
        };
        assert_ne!(
            never_resolved, nothing_back,
            "the two cases an application would investigate differently read the same"
        );
    }

    #[test]
    fn a_recovery_whose_outputs_all_issued_is_a_restoration() {
        let recovery = txid(0x2e);
        let answers = [(recovery, 0, Ok(())), (recovery, 1, Ok(()))];
        assert_eq!(restoration(&[recovery], &answers), Restoration::Restored);
    }

    /// The case acceptance alone cannot see: consensus took the transaction, and its mint
    /// outputs still ended without producing anything spendable.
    #[test]
    fn a_recovery_output_that_failed_to_issue_is_unproven() {
        let recovery = txid(0x2e);
        let answers = [
            (recovery, 0, Ok(())),
            (
                recovery,
                1,
                Err("Failed to finalize transaction".to_owned()),
            ),
        ];
        let Restoration::Unproven(why) = restoration(&[recovery], &answers) else {
            panic!("an output that did not issue its notes cannot be a restoration");
        };
        assert!(why.contains("finalize"), "{why}");
    }

    /// A bounded wait that elapses is an unproven answer like any other, never a restoration.
    #[test]
    fn an_output_that_never_reported_is_unproven() {
        let recovery = txid(0x2e);
        let answers = [(recovery, 0, Err("it did not report an outcome".to_owned()))];
        assert!(matches!(
            restoration(&[recovery], &answers),
            Restoration::Unproven(_)
        ));
    }

    #[test]
    fn a_recovery_transaction_with_no_outputs_to_ask_about_is_unproven() {
        let recovery = txid(0x2e);
        assert!(matches!(
            restoration(&[recovery], &[]),
            Restoration::Unproven(_)
        ));
    }

    #[test]
    fn every_recovery_transaction_has_to_answer() {
        let (first, second) = (txid(0x01), txid(0x02));
        assert!(
            matches!(
                restoration(&[first, second], &[(first, 0, Ok(()))]),
                Restoration::Unproven(_)
            ),
            "a second recovery transaction that issued nothing was not noticed"
        );
    }
}
