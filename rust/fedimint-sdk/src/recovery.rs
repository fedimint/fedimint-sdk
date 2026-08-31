//! Seed-based wallet recovery. **Experimental.**
//!
//! Everything in this module is behind the crate's off-by-default
//! `experimental` cargo feature and is excluded from the 0.1 stability
//! contract: names, shapes, and semantics here may change in a patch
//! release. Build with `--features experimental` to get it, and treat
//! anything built on it as provisional.
//!
//! # Why it is gated
//!
//! Recovery is the one operation whose failure mode is silent fund loss: a
//! rescan that stops early, or that double-applies a checkpoint after a
//! crash, produces a wallet that looks fine and is not. Two pieces of
//! upstream work this SDK depends on for a correct implementation are still
//! open — [fedimint/fedimint#8908](https://github.com/fedimint/fedimint/issues/8908)
//! and [fedimint/fedimint#8934](https://github.com/fedimint/fedimint/issues/8934) —
//! and beyond them the SDK owes its own crash-at-every-checkpoint
//! idempotency tests: kill the process at each persisted checkpoint of a
//! recovery, restart, and assert the recovered wallet is identical to one
//! recovered without interruption. The feature stays off until both the
//! upstream fixes have landed and those tests pass, so that the default
//! build never offers an API it cannot yet stand behind.

use crate::{Federation, InviteCode, Operation, OperationState, Result, Sdk};

impl Sdk {
    /// Joins a federation and restores this seed's wallet in it from the
    /// federation's backup plus a rescan. **Experimental**, see the module
    /// documentation.
    ///
    /// Use this instead of [`Sdk::join`] when the instance was built from a
    /// mnemonic the user restored and the federation may already hold funds
    /// belonging to that seed. A plain join starts a fresh client and would
    /// not look for them.
    ///
    /// The call returns as soon as the recovery has started, with a
    /// [`Recovery`] carrying both the joined [`Federation`] and the
    /// [`Operation`] tracking the rescan — recovery can take a long time,
    /// so it is observed like any other background operation rather than
    /// awaited inline.
    ///
    /// # Errors
    ///
    /// The same errors as [`Sdk::join`]:
    /// [`AlreadyJoined`](crate::ErrorCode::AlreadyJoined),
    /// [`FederationUnreachable`](crate::ErrorCode::FederationUnreachable),
    /// [`Timeout`](crate::ErrorCode::Timeout),
    /// [`UnsupportedFederation`](crate::ErrorCode::UnsupportedFederation),
    /// [`Storage`](crate::ErrorCode::Storage), and
    /// [`FederationClosed`](crate::ErrorCode::FederationClosed).
    pub async fn recover(&self, invite: &InviteCode) -> Result<Recovery> {
        unimplemented!()
    }
}

/// A federation that is being recovered, plus the operation doing it.
/// **Experimental**, see the module documentation.
///
/// # What is usable while recovery runs
///
/// The [`Federation`] handle is live immediately — its identity, network,
/// metadata, and capabilities are all readable, and an application can show
/// the federation in its list right away. What is *not* trustworthy yet is
/// anything derived from the wallet's contents:
///
/// - **Spending and receiving are refused.** Every ecash, lightning, and
///   on-chain send or receive against this federation fails with
///   [`ErrorCode::Recovering`](crate::ErrorCode::Recovering) until the
///   rescan finishes. This is not a race the SDK tries to win: a payment
///   funded from a note set that is still being discovered could
///   double-spend a note the rescan has not reached yet.
/// - **Balance and activity are incomplete and moving.**
///   [`Federation::balance`](crate::Federation::balance) reports what has
///   been recovered *so far* and will generally rise as the rescan
///   proceeds; [`Federation::activity`](crate::Federation::activity) shows
///   only what has been reconstructed so far. Both are safe to display —
///   and worth displaying, so the user sees progress — but an application
///   should label them as provisional rather than presenting a partial
///   balance as the final one.
///
/// Observe [`Recovery::progress`] to know when that changes. The operation
/// is an ordinary background operation: it survives restarts, resumes on
/// the next build, and dropping this struct does not stop it.
#[derive(Debug)]
#[non_exhaustive]
pub struct Recovery {
    /// The joined federation. Usable for identity and metadata
    /// immediately; spends and receives fail with
    /// [`Recovering`](crate::ErrorCode::Recovering) until the rescan
    /// completes.
    pub federation: Federation,
    /// The rescan, observable like any other operation.
    pub progress: Operation<RecoveryState>,
}

/// How a recovery is going. **Experimental**, see the module
/// documentation.
///
/// Deliberately coarse. Upstream recovery does not currently expose a
/// meaningful completion fraction, and a made-up percentage would be worse
/// than none; this reports only what can be said truthfully. Finer-grained
/// progress is an additive change if upstream grows it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecoveryState {
    /// The rescan is running. Spends and receives are refused with
    /// [`Recovering`](crate::ErrorCode::Recovering); balance and activity
    /// are incomplete.
    Running,
    /// Final: the wallet is fully recovered and the federation behaves
    /// like any other joined federation.
    Done,
    /// Final: the recovery could not be completed.
    ///
    /// The federation stays joined and the recovery can be retried; the
    /// wallet's contents should be treated as incomplete until one
    /// succeeds.
    Failed {
        /// Human-readable explanation. Diagnostic only — not a stable
        /// contract, and not something to match on.
        reason: String,
    },
}

impl crate::operation::sealed::Sealed for RecoveryState {}

impl OperationState for RecoveryState {
    fn is_final(&self) -> bool {
        match self {
            RecoveryState::Running => false,
            RecoveryState::Done | RecoveryState::Failed { .. } => true,
        }
    }
}

#[cfg(all(test, feature = "experimental"))]
mod tests {
    use super::*;

    #[test]
    fn recovery_state_running_is_not_final() {
        assert!(!RecoveryState::Running.is_final());
    }

    #[test]
    fn recovery_state_done_is_final() {
        assert!(RecoveryState::Done.is_final());
    }

    #[test]
    fn recovery_state_failed_is_final() {
        assert!(
            RecoveryState::Failed {
                reason: String::new(),
            }
            .is_final()
        );
    }
}
