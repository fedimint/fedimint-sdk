//! How the three multisig operation kinds are observed, and how their details are persisted.
//!
//! One driver for all three: the backend owns each kind's stream of states, and everything else,
//! from the persisted final state to the details record, has the same shape for every kind.

use std::any::Any;
use std::marker::PhantomData;

use fedimint_client::Client;
use fedimint_core::core::OperationId as UpstreamOperationId;
use fedimint_core::util::{BoxFuture, BoxStream};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::{
    MultisigBackend, MultisigDepositDetails, MultisigDepositState, MultisigSweepDetails,
    MultisigSweepState, MultisigTransferDetails, MultisigTransferState,
};
use crate::db::OperationRecord;
use crate::federation::FederationInner;
use crate::multisig::account::AmountWire;
use crate::operation::{Driver, OperationState, first_state, settled, until_final};
use crate::{Amount, Error, ErrorCode, MultisigAmount, Result, Timestamp};

/// Observes [`MultisigDepositState`].
pub(crate) type MultisigDepositDriver = MultisigDriver<MultisigDepositState>;
/// Observes [`MultisigTransferState`].
pub(crate) type MultisigTransferDriver = MultisigDriver<MultisigTransferState>;
/// Observes [`MultisigSweepState`].
pub(crate) type MultisigSweepDriver = MultisigDriver<MultisigSweepState>;

/// The driver for one multisig kind, `S` being its state.
pub(crate) struct MultisigDriver<S> {
    // `fn() -> S` rather than `S`: the driver holds no state, and its `Send`/`Sync` must not
    // depend on the state type's.
    kind: PhantomData<fn() -> S>,
}

impl<S> Default for MultisigDriver<S> {
    fn default() -> Self {
        MultisigDriver { kind: PhantomData }
    }
}

/// What differs between the three kinds.
pub(crate) trait MultisigKind: OperationState + PartialEq {
    /// The details record, as the public type.
    type Details: Send + Sync + 'static;
    /// The details record, as it is persisted.
    type Wire: Serialize + DeserializeOwned + TryInto<Self::Details, Error = Error>;

    /// The backend's stream of this kind's states.
    fn updates<'a>(
        backend: &'a dyn MultisigBackend,
        client: &'a Client,
        id: UpstreamOperationId,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<Self>>>>;

    /// The state as it is persisted once final.
    fn encode(&self) -> String;

    /// The state back from [`encode`](MultisigKind::encode), if it is one.
    fn parse(encoded: &str) -> Option<Self>;
}

impl<S> Driver<S> for MultisigDriver<S>
where
    S: MultisigKind,
{
    fn current<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: UpstreamOperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<S>> {
        Box::pin(async move {
            if let Some(state) = record.final_state.as_deref().and_then(S::parse) {
                return Ok(state);
            }
            first_state(self.subscribe(federation, id, record).await?).await
        })
    }

    fn subscribe<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: UpstreamOperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<S>>>> {
        Box::pin(async move {
            if let Some(state) = record.final_state.as_deref().and_then(S::parse) {
                return Ok(Box::pin(futures::stream::iter(vec![Ok(state)]))
                    as BoxStream<'static, Result<S>>);
            }
            let backend = federation.multisig_backend().ok_or_else(|| {
                Error::new(
                    ErrorCode::NotSupported,
                    "this build cannot follow multisig operations on this federation",
                )
            })?;
            let client = federation.client(false).await?;
            // The backend's stream holds no client (see `MultisigBackend`), so the guard goes
            // with this call.
            let stream = S::updates(backend.as_ref(), &client, id).await?;
            Ok(settled(until_final(stream)))
        })
    }

    fn same_state(&self, previous: &S, next: &S) -> bool {
        previous == next
    }

    fn encode_state(&self, state: &S) -> Result<String> {
        Ok(state.encode())
    }

    fn decode_state(&self, encoded: &str) -> Result<S> {
        S::parse(encoded).ok_or_else(|| {
            Error::new(
                ErrorCode::Internal,
                format!("unrecognised multisig operation state: {encoded}"),
            )
        })
    }

    fn decode_details(&self, json: &str) -> Result<Box<dyn Any + Send + Sync>> {
        let wire: S::Wire = serde_json::from_str(json).map_err(|err| {
            Error::new(
                ErrorCode::Internal,
                format!("could not decode multisig operation details: {err}"),
            )
        })?;
        let details: S::Details = wire.try_into()?;
        Ok(Box::new(details))
    }
}

// Every state is spelled by its variant name, and `Failed` is followed by its reason, verbatim:
// a prefix no other variant produces, so the reason, whatever it contains, never has to be told
// apart from the encoding around it.
const CREATED: &str = "Created";
const ACCEPTED: &str = "Accepted";
const DONE: &str = "Done";
const FAILED: &str = "Failed:";

impl MultisigKind for MultisigDepositState {
    type Details = MultisigDepositDetails;
    type Wire = DepositDetailsWire;

    fn updates<'a>(
        backend: &'a dyn MultisigBackend,
        client: &'a Client,
        id: UpstreamOperationId,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<Self>>>> {
        backend.deposit_updates(client, id)
    }

    fn encode(&self) -> String {
        match self {
            MultisigDepositState::Created => CREATED.to_owned(),
            MultisigDepositState::Accepted => ACCEPTED.to_owned(),
            MultisigDepositState::Done => DONE.to_owned(),
            MultisigDepositState::Failed { reason } => format!("{FAILED}{reason}"),
        }
    }

    fn parse(encoded: &str) -> Option<Self> {
        if let Some(reason) = encoded.strip_prefix(FAILED) {
            return Some(MultisigDepositState::Failed {
                reason: reason.to_owned(),
            });
        }
        match encoded {
            CREATED => Some(MultisigDepositState::Created),
            ACCEPTED => Some(MultisigDepositState::Accepted),
            DONE => Some(MultisigDepositState::Done),
            _ => None,
        }
    }
}

impl MultisigKind for MultisigTransferState {
    type Details = MultisigTransferDetails;
    type Wire = TransferDetailsWire;

    fn updates<'a>(
        backend: &'a dyn MultisigBackend,
        client: &'a Client,
        id: UpstreamOperationId,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<Self>>>> {
        backend.transfer_updates(client, id)
    }

    fn encode(&self) -> String {
        match self {
            MultisigTransferState::Created => CREATED.to_owned(),
            MultisigTransferState::Done => DONE.to_owned(),
            MultisigTransferState::Failed { reason } => format!("{FAILED}{reason}"),
        }
    }

    fn parse(encoded: &str) -> Option<Self> {
        if let Some(reason) = encoded.strip_prefix(FAILED) {
            return Some(MultisigTransferState::Failed {
                reason: reason.to_owned(),
            });
        }
        match encoded {
            CREATED => Some(MultisigTransferState::Created),
            DONE => Some(MultisigTransferState::Done),
            _ => None,
        }
    }
}

impl MultisigKind for MultisigSweepState {
    type Details = MultisigSweepDetails;
    type Wire = SweepDetailsWire;

    fn updates<'a>(
        backend: &'a dyn MultisigBackend,
        client: &'a Client,
        id: UpstreamOperationId,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<Self>>>> {
        backend.sweep_updates(client, id)
    }

    fn encode(&self) -> String {
        match self {
            MultisigSweepState::Created => CREATED.to_owned(),
            MultisigSweepState::Accepted => ACCEPTED.to_owned(),
            MultisigSweepState::Done => DONE.to_owned(),
            MultisigSweepState::Failed { reason } => format!("{FAILED}{reason}"),
        }
    }

    fn parse(encoded: &str) -> Option<Self> {
        if let Some(reason) = encoded.strip_prefix(FAILED) {
            return Some(MultisigSweepState::Failed {
                reason: reason.to_owned(),
            });
        }
        match encoded {
            CREATED => Some(MultisigSweepState::Created),
            ACCEPTED => Some(MultisigSweepState::Accepted),
            DONE => Some(MultisigSweepState::Done),
            _ => None,
        }
    }
}

/// [`MultisigDepositDetails`] as it is persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DepositDetailsWire {
    account_id: String,
    amount_msats: u64,
    created_at_epoch_ms: u64,
}

impl From<&MultisigDepositDetails> for DepositDetailsWire {
    fn from(details: &MultisigDepositDetails) -> Self {
        DepositDetailsWire {
            account_id: details.account_id.to_string(),
            amount_msats: details.amount.msats(),
            created_at_epoch_ms: details.created_at.epoch_millis(),
        }
    }
}

impl TryFrom<DepositDetailsWire> for MultisigDepositDetails {
    type Error = Error;

    fn try_from(wire: DepositDetailsWire) -> Result<Self> {
        Ok(MultisigDepositDetails {
            account_id: wire.account_id.parse()?,
            amount: Amount::from_msats(wire.amount_msats),
            created_at: Timestamp::from_epoch_millis(wire.created_at_epoch_ms),
        })
    }
}

/// [`MultisigTransferDetails`] as it is persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TransferDetailsWire {
    account_id: String,
    denomination: u8,
    /// Millisatoshi for bitcoin, the currency's smallest unit for fiat.
    value: u64,
    /// Empty for bitcoin.
    currency: String,
    created_at_epoch_ms: u64,
}

impl From<&MultisigTransferDetails> for TransferDetailsWire {
    fn from(details: &MultisigTransferDetails) -> Self {
        let amount = AmountWire::from(&details.amount);
        TransferDetailsWire {
            account_id: details.account_id.to_string(),
            denomination: amount.denomination,
            value: amount.value,
            currency: amount.currency,
            created_at_epoch_ms: details.created_at.epoch_millis(),
        }
    }
}

impl TryFrom<TransferDetailsWire> for MultisigTransferDetails {
    type Error = Error;

    fn try_from(wire: TransferDetailsWire) -> Result<Self> {
        Ok(MultisigTransferDetails {
            account_id: wire.account_id.parse()?,
            amount: MultisigAmount::try_from(AmountWire {
                denomination: wire.denomination,
                value: wire.value,
                currency: wire.currency,
            })?,
            created_at: Timestamp::from_epoch_millis(wire.created_at_epoch_ms),
        })
    }
}

/// [`MultisigSweepDetails`] as it is persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SweepDetailsWire {
    amount_msats: u64,
    created_at_epoch_ms: u64,
}

impl From<&MultisigSweepDetails> for SweepDetailsWire {
    fn from(details: &MultisigSweepDetails) -> Self {
        SweepDetailsWire {
            amount_msats: details.amount.msats(),
            created_at_epoch_ms: details.created_at.epoch_millis(),
        }
    }
}

impl TryFrom<SweepDetailsWire> for MultisigSweepDetails {
    type Error = Error;

    fn try_from(wire: SweepDetailsWire) -> Result<Self> {
        Ok(MultisigSweepDetails {
            amount: Amount::from_msats(wire.amount_msats),
            created_at: Timestamp::from_epoch_millis(wire.created_at_epoch_ms),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_round_trips_through_its_persisted_form() {
        let reason = || "Done: or not".to_owned();
        for state in [
            MultisigDepositState::Created,
            MultisigDepositState::Accepted,
            MultisigDepositState::Done,
            MultisigDepositState::Failed { reason: reason() },
        ] {
            assert_eq!(MultisigDepositState::parse(&state.encode()), Some(state));
        }
        for state in [
            MultisigTransferState::Created,
            MultisigTransferState::Done,
            MultisigTransferState::Failed { reason: reason() },
        ] {
            assert_eq!(MultisigTransferState::parse(&state.encode()), Some(state));
        }
        for state in [
            MultisigSweepState::Created,
            MultisigSweepState::Accepted,
            MultisigSweepState::Done,
            MultisigSweepState::Failed { reason: reason() },
        ] {
            assert_eq!(MultisigSweepState::parse(&state.encode()), Some(state));
        }
        assert_eq!(MultisigTransferState::parse(ACCEPTED), None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_persisted_final_state_needs_no_backend() {
        use crate::db::{federation_namespace, in_memory_root};

        // A build without the backend, or a federation that is closed, still reports how an
        // operation ended once that is on its record.
        let federation =
            FederationInner::detached(federation_namespace(&in_memory_root(), [1u8; 32]), false);
        let record = OperationRecord {
            schema_version: crate::operation::READABLE_STATE_SCHEMA,
            kind: crate::operation::kinds::MULTISIG_SWEEP.to_owned(),
            module: "multi_sig_stability_pool".to_owned(),
            created_at: 0,
            details: "{}".to_owned(),
            phase: None,
            cancel_requested_at: None,
            final_state: Some(MultisigSweepState::Done.encode()),
        };
        let driver = MultisigSweepDriver::default();
        let state = driver
            .current(&federation, UpstreamOperationId([1u8; 32]), &record)
            .await
            .expect("read from the record");
        assert_eq!(state, MultisigSweepState::Done);

        let pending = OperationRecord {
            final_state: None,
            ..record
        };
        let err = driver
            .current(&federation, UpstreamOperationId([1u8; 32]), &pending)
            .await
            .expect_err("nothing can follow it");
        assert_eq!(err.code, ErrorCode::NotSupported);
    }

    #[test]
    fn details_round_trip_through_their_persisted_form() {
        let account_id = "aa".repeat(32).parse().expect("a valid id");
        let transfer = MultisigTransferDetails {
            account_id,
            amount: MultisigAmount::Fiat {
                minor_units: 1_250,
                currency: "EUR".to_owned(),
            },
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        };
        let json = serde_json::to_string(&TransferDetailsWire::from(&transfer)).expect("json");
        let back: TransferDetailsWire = serde_json::from_str(&json).expect("json");
        assert_eq!(
            MultisigTransferDetails::try_from(back).expect("decodes"),
            transfer
        );

        let deposit = MultisigDepositDetails {
            account_id,
            amount: Amount::from_msats(10_000),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        };
        let wire = DepositDetailsWire::from(&deposit);
        assert_eq!(
            MultisigDepositDetails::try_from(wire).expect("decodes"),
            deposit
        );
    }
}
