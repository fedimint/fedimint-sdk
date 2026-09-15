//! The two on-chain drivers and the backfiller, chosen by the record's module.

use std::any::Any;

use fedimint_core::core::OperationId;
use fedimint_core::util::{BoxFuture, BoxStream};

use super::{v1, v2, wire};
use crate::db::OperationRecord;
use crate::federation::FederationInner;
use crate::operation::{Backfilled, Backfiller, Driver, first_state, settled};
use crate::{Error, ErrorCode, OnchainReceiveState, OnchainSendState, Result};

/// Observes an on-chain withdrawal of either wallet module generation, chosen by the record's
/// module.
pub(crate) struct OnchainSendDriver;

impl Driver<OnchainSendState> for OnchainSendDriver {
    fn current<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<OnchainSendState>> {
        Box::pin(async move {
            if let Some(encoded) = &record.final_state {
                return wire::decode_send_state(encoded);
            }
            match record.module.as_str() {
                "wallet" => first_state(self.subscribe(federation, id, record).await?).await,
                // walletv2 has no incremental subscription to settle: `current_send` is the
                // module's own bounded final-state await, already capped to the 500 ms rule.
                "walletv2" => v2::current_send(federation, id).await,
                other => Err(unknown_module(other)),
            }
        })
    }

    fn subscribe<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<OnchainSendState>>>> {
        Box::pin(async move {
            let stream = match record.module.as_str() {
                "wallet" => v1::subscribe_withdraw(federation, id).await,
                "walletv2" => v2::subscribe_send(federation, id).await,
                other => Err(unknown_module(other)),
            }?;
            Ok(settled(stream))
        })
    }

    fn same_state(&self, previous: &OnchainSendState, next: &OnchainSendState) -> bool {
        previous == next
    }

    fn encode_state(&self, state: &OnchainSendState) -> Result<String> {
        wire::encode_send_state(state)
    }

    fn decode_state(&self, encoded: &str) -> Result<OnchainSendState> {
        wire::decode_send_state(encoded)
    }

    fn decode_details(&self, json: &str) -> Result<Box<dyn Any + Send + Sync>> {
        Ok(Box::new(wire::decode_send_details(json)?))
    }
}

/// Observes an on-chain deposit of either wallet module generation.
pub(crate) struct OnchainReceiveDriver;

impl Driver<OnchainReceiveState> for OnchainReceiveDriver {
    fn current<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<OnchainReceiveState>> {
        Box::pin(async move {
            if let Some(encoded) = &record.final_state {
                return wire::decode_receive_state(encoded);
            }
            match record.module.as_str() {
                "wallet" => first_state(self.subscribe(federation, id, record).await?).await,
                "walletv2" => v2::current_receive(federation, id).await,
                other => Err(unknown_module(other)),
            }
        })
    }

    fn subscribe<'a>(
        &'a self,
        federation: &'a FederationInner,
        id: OperationId,
        record: &'a OperationRecord,
    ) -> BoxFuture<'a, Result<BoxStream<'static, Result<OnchainReceiveState>>>> {
        Box::pin(async move {
            let stream = match record.module.as_str() {
                "wallet" => v1::subscribe_deposit(federation, id).await,
                "walletv2" => v2::subscribe_receive(federation, id).await,
                other => Err(unknown_module(other)),
            }?;
            Ok(settled(stream))
        })
    }

    fn same_state(&self, previous: &OnchainReceiveState, next: &OnchainReceiveState) -> bool {
        previous == next
    }

    fn encode_state(&self, state: &OnchainReceiveState) -> Result<String> {
        wire::encode_receive_state(state)
    }

    fn decode_state(&self, encoded: &str) -> Result<OnchainReceiveState> {
        wire::decode_receive_state(encoded)
    }

    fn decode_details(&self, json: &str) -> Result<Box<dyn Any + Send + Sync>> {
        Ok(Box::new(wire::decode_receive_details(json)?))
    }
}

/// Rebuilds an on-chain record from the wallet module's own log entry, for either generation.
pub(crate) struct OnchainBackfiller;

impl Backfiller for OnchainBackfiller {
    fn backfill(
        &self,
        id: OperationId,
        module_kind: &str,
        meta: &serde_json::Value,
        created_at: u64,
    ) -> Option<Backfilled> {
        match module_kind {
            "wallet" => v1::backfill(id, meta, created_at),
            "walletv2" => v2::backfill(id, meta, created_at),
            _ => None,
        }
    }
}

fn unknown_module(module: &str) -> Error {
    Error::new(
        ErrorCode::Internal,
        format!("a record names a wallet module this build does not know: {module:?}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::federation::FederationInner;
    use crate::operation::kinds;
    use crate::{Amount, Sats, Txid};

    fn a_txid() -> Txid {
        "0000000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .expect("a well-formed transaction id")
    }

    #[test]
    fn send_driver_decodes_what_it_encodes() {
        let driver = OnchainSendDriver;
        for state in [
            OnchainSendState::Created,
            OnchainSendState::Succeeded { txid: a_txid() },
            OnchainSendState::Refunded {
                reason: "the federation rejected the funding transaction".to_owned(),
            },
            OnchainSendState::Failed {
                reason: "the funding was accepted but no transaction came of it".to_owned(),
            },
        ] {
            let encoded = driver.encode_state(&state).expect("encode");
            assert_eq!(driver.decode_state(&encoded).expect("decode"), state);
        }
    }

    #[test]
    fn receive_driver_decodes_what_it_encodes() {
        let driver = OnchainReceiveDriver;
        for state in [
            OnchainReceiveState::WaitingForTransaction,
            OnchainReceiveState::WaitingForConfirmation {
                txid: a_txid(),
                gross_deposited: Sats::from_sats(100_000),
            },
            OnchainReceiveState::Confirmed {
                txid: a_txid(),
                gross_deposited: Sats::from_sats(100_000),
            },
            OnchainReceiveState::Claimed {
                txid: a_txid(),
                gross_deposited: Sats::from_sats(100_000),
                net_credit: Amount::from_msats(99_998_500),
            },
            OnchainReceiveState::Failed {
                reason: "the deposit does not exceed the federation's deposit fee and was not \
                          claimed"
                    .to_owned(),
            },
        ] {
            let encoded = driver.encode_state(&state).expect("encode");
            assert_eq!(driver.decode_state(&encoded).expect("decode"), state);
        }
    }

    /// A record naming a wallet module this build does not know is `Internal` from `current`.
    /// The federation here is detached with no client at all, which is what proves the module
    /// check runs before the client is ever taken: were it the other way round, this would
    /// report `FederationClosed` instead.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_driver_on_an_unknown_module_reports_internal_from_current() {
        let db = crate::db::federation_namespace(&crate::db::in_memory_root(), [1u8; 32]);
        let federation = FederationInner::detached(db, false);
        let record = OperationRecord {
            schema_version: crate::operation::READABLE_STATE_SCHEMA,
            kind: kinds::ONCHAIN_RECEIVE.to_owned(),
            module: "made_up_wallet_module".to_owned(),
            created_at: 0,
            details: "{}".to_owned(),
            phase: None,
            cancel_requested_at: None,
            final_state: None,
        };
        let err = OnchainReceiveDriver
            .current(&federation, OperationId::new_random(), &record)
            .await
            .expect_err("an unknown module cannot be observed");
        assert_eq!(err.code, ErrorCode::Internal);
    }
}
