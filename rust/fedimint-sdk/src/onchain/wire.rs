//! The persisted shapes of the on-chain facade: details records and final states as JSON, and
//! the phase a phase-keyed mapping reads after a restart.
//!
//! These are storage format. A field added later must be `Option` with `#[serde(default)]`, and
//! a field is never renamed or removed: every record already written reads through this file.

use serde::{Deserialize, Serialize};

use crate::{
    Address, Amount, Error, ErrorCode, OnchainReceiveDetails, OnchainReceiveFeeBreakdown,
    OnchainReceiveState, OnchainSendDetails, OnchainSendState, Result, Sats, Timestamp, Txid,
};

/// The operation has seen a transaction past
/// [`WaitingForTransaction`](crate::OnchainReceiveState::WaitingForTransaction): a deposit
/// address has stopped being a pure watch. The only phase an on-chain record ever carries.
pub(super) const PHASE_SEEN: u32 = 1;

/// [`OnchainSendDetails`] as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct OnchainSendDetailsWire {
    pub(super) address: String,
    pub(super) amount_sats: u64,
    pub(super) fee_msats: u64,
    pub(super) total_msats: u64,
    pub(super) created_at: u64,
}

impl From<&OnchainSendDetails> for OnchainSendDetailsWire {
    fn from(details: &OnchainSendDetails) -> OnchainSendDetailsWire {
        OnchainSendDetailsWire {
            address: details.address.to_string(),
            amount_sats: details.amount.sats(),
            fee_msats: details.fee.msats(),
            total_msats: details.total.msats(),
            created_at: details.created_at.epoch_millis(),
        }
    }
}

impl TryFrom<OnchainSendDetailsWire> for OnchainSendDetails {
    type Error = Error;

    fn try_from(wire: OnchainSendDetailsWire) -> Result<OnchainSendDetails> {
        Ok(OnchainSendDetails {
            address: parse_address(&wire.address)?,
            amount: Sats::from_sats(wire.amount_sats),
            fee: Amount::from_msats(wire.fee_msats),
            total: Amount::from_msats(wire.total_msats),
            created_at: Timestamp::from_epoch_millis(wire.created_at),
        })
    }
}

/// [`OnchainReceiveFeeBreakdown`] as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ReceiveFeeBreakdownWire {
    pub(super) peg_in_msats: u64,
    pub(super) network_claim_msats: u64,
    pub(super) primary_module_msats: u64,
    pub(super) dust_msats: u64,
}

impl From<&OnchainReceiveFeeBreakdown> for ReceiveFeeBreakdownWire {
    fn from(breakdown: &OnchainReceiveFeeBreakdown) -> ReceiveFeeBreakdownWire {
        ReceiveFeeBreakdownWire {
            peg_in_msats: breakdown.peg_in.msats(),
            network_claim_msats: breakdown.network_claim.msats(),
            primary_module_msats: breakdown.primary_module.msats(),
            dust_msats: breakdown.dust.msats(),
        }
    }
}

impl From<ReceiveFeeBreakdownWire> for OnchainReceiveFeeBreakdown {
    fn from(wire: ReceiveFeeBreakdownWire) -> OnchainReceiveFeeBreakdown {
        OnchainReceiveFeeBreakdown {
            peg_in: Amount::from_msats(wire.peg_in_msats),
            network_claim: Amount::from_msats(wire.network_claim_msats),
            primary_module: Amount::from_msats(wire.primary_module_msats),
            dust: Amount::from_msats(wire.dust_msats),
        }
    }
}

/// [`OnchainReceiveDetails`] as stored, plus the two private fill-in-later fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct OnchainReceiveDetailsWire {
    pub(super) address: String,
    pub(super) txid: Option<String>,
    pub(super) gross_deposited_sats: Option<u64>,
    pub(super) fee_msats: Option<u64>,
    pub(super) fee_breakdown: Option<ReceiveFeeBreakdownWire>,
    pub(super) net_credit_msats: Option<u64>,
    pub(super) created_at: u64,
    /// walletv2 only: the upstream operation the claim runs under once one is linked. Not
    /// part of the public record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) upstream_operation_id: Option<String>,
    /// walletv2 only: the event-log position the next scan starts from. Not part of the
    /// public record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) event_cursor: Option<u64>,
}

impl From<&OnchainReceiveDetails> for OnchainReceiveDetailsWire {
    fn from(details: &OnchainReceiveDetails) -> OnchainReceiveDetailsWire {
        OnchainReceiveDetailsWire {
            address: details.address.to_string(),
            txid: details.txid.as_ref().map(ToString::to_string),
            gross_deposited_sats: details.gross_deposited.map(Sats::sats),
            fee_msats: details.fee.map(Amount::msats),
            fee_breakdown: details
                .fee_breakdown
                .as_ref()
                .map(ReceiveFeeBreakdownWire::from),
            net_credit_msats: details.net_credit.map(Amount::msats),
            created_at: details.created_at.epoch_millis(),
            upstream_operation_id: None,
            event_cursor: None,
        }
    }
}

impl TryFrom<OnchainReceiveDetailsWire> for OnchainReceiveDetails {
    type Error = Error;

    fn try_from(wire: OnchainReceiveDetailsWire) -> Result<OnchainReceiveDetails> {
        let txid = match wire.txid {
            Some(txid) => Some(parse_txid(&txid)?),
            None => None,
        };
        Ok(OnchainReceiveDetails {
            address: parse_address(&wire.address)?,
            txid,
            gross_deposited: wire.gross_deposited_sats.map(Sats::from_sats),
            fee: wire.fee_msats.map(Amount::from_msats),
            fee_breakdown: wire.fee_breakdown.map(OnchainReceiveFeeBreakdown::from),
            net_credit: wire.net_credit_msats.map(Amount::from_msats),
            created_at: Timestamp::from_epoch_millis(wire.created_at),
        })
    }
}

/// [`OnchainSendState`] as stored on `OperationRecord::final_state`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum OnchainSendStateWire {
    Created,
    Succeeded { txid: String },
    Refunded { reason: String },
    Failed { reason: String },
}

/// [`OnchainReceiveState`] as stored on `OperationRecord::final_state`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum OnchainReceiveStateWire {
    WaitingForTransaction,
    WaitingForConfirmation {
        txid: String,
        gross_deposited_sats: u64,
    },
    Confirmed {
        txid: String,
        gross_deposited_sats: u64,
    },
    Claimed {
        txid: String,
        gross_deposited_sats: u64,
        net_credit_msats: u64,
    },
    Failed {
        reason: String,
    },
}

pub(super) fn encode_send_state(state: &OnchainSendState) -> Result<String> {
    let wire = match state {
        OnchainSendState::Created => OnchainSendStateWire::Created,
        OnchainSendState::Succeeded { txid } => OnchainSendStateWire::Succeeded {
            txid: txid.to_string(),
        },
        OnchainSendState::Refunded { reason } => OnchainSendStateWire::Refunded {
            reason: reason.clone(),
        },
        OnchainSendState::Failed { reason } => OnchainSendStateWire::Failed {
            reason: reason.clone(),
        },
    };
    serde_json::to_string(&wire).map_err(encode_error)
}

pub(super) fn decode_send_state(encoded: &str) -> Result<OnchainSendState> {
    let wire: OnchainSendStateWire = serde_json::from_str(encoded).map_err(decode_error)?;
    Ok(match wire {
        OnchainSendStateWire::Created => OnchainSendState::Created,
        OnchainSendStateWire::Succeeded { txid } => OnchainSendState::Succeeded {
            txid: parse_txid(&txid)?,
        },
        OnchainSendStateWire::Refunded { reason } => OnchainSendState::Refunded { reason },
        OnchainSendStateWire::Failed { reason } => OnchainSendState::Failed { reason },
    })
}

pub(super) fn encode_receive_state(state: &OnchainReceiveState) -> Result<String> {
    let wire = match state {
        OnchainReceiveState::WaitingForTransaction => {
            OnchainReceiveStateWire::WaitingForTransaction
        }
        OnchainReceiveState::WaitingForConfirmation {
            txid,
            gross_deposited,
        } => OnchainReceiveStateWire::WaitingForConfirmation {
            txid: txid.to_string(),
            gross_deposited_sats: gross_deposited.sats(),
        },
        OnchainReceiveState::Confirmed {
            txid,
            gross_deposited,
        } => OnchainReceiveStateWire::Confirmed {
            txid: txid.to_string(),
            gross_deposited_sats: gross_deposited.sats(),
        },
        OnchainReceiveState::Claimed {
            txid,
            gross_deposited,
            net_credit,
        } => OnchainReceiveStateWire::Claimed {
            txid: txid.to_string(),
            gross_deposited_sats: gross_deposited.sats(),
            net_credit_msats: net_credit.msats(),
        },
        OnchainReceiveState::Failed { reason } => OnchainReceiveStateWire::Failed {
            reason: reason.clone(),
        },
    };
    serde_json::to_string(&wire).map_err(encode_error)
}

pub(super) fn decode_receive_state(encoded: &str) -> Result<OnchainReceiveState> {
    let wire: OnchainReceiveStateWire = serde_json::from_str(encoded).map_err(decode_error)?;
    Ok(match wire {
        OnchainReceiveStateWire::WaitingForTransaction => {
            OnchainReceiveState::WaitingForTransaction
        }
        OnchainReceiveStateWire::WaitingForConfirmation {
            txid,
            gross_deposited_sats,
        } => OnchainReceiveState::WaitingForConfirmation {
            txid: parse_txid(&txid)?,
            gross_deposited: Sats::from_sats(gross_deposited_sats),
        },
        OnchainReceiveStateWire::Confirmed {
            txid,
            gross_deposited_sats,
        } => OnchainReceiveState::Confirmed {
            txid: parse_txid(&txid)?,
            gross_deposited: Sats::from_sats(gross_deposited_sats),
        },
        OnchainReceiveStateWire::Claimed {
            txid,
            gross_deposited_sats,
            net_credit_msats,
        } => OnchainReceiveState::Claimed {
            txid: parse_txid(&txid)?,
            gross_deposited: Sats::from_sats(gross_deposited_sats),
            net_credit: Amount::from_msats(net_credit_msats),
        },
        OnchainReceiveStateWire::Failed { reason } => OnchainReceiveState::Failed { reason },
    })
}

pub(super) fn decode_send_details(json: &str) -> Result<OnchainSendDetails> {
    let wire: OnchainSendDetailsWire = serde_json::from_str(json).map_err(decode_error)?;
    OnchainSendDetails::try_from(wire)
}

pub(super) fn decode_receive_details(json: &str) -> Result<OnchainReceiveDetails> {
    OnchainReceiveDetails::try_from(decode_receive_wire(json)?)
}

pub(super) fn decode_receive_wire(json: &str) -> Result<OnchainReceiveDetailsWire> {
    serde_json::from_str(json).map_err(decode_error)
}

pub(super) fn encode_receive_wire(wire: &OnchainReceiveDetailsWire) -> Result<String> {
    serde_json::to_string(wire).map_err(encode_error)
}

pub(super) fn encode_send_wire(wire: &OnchainSendDetailsWire) -> Result<String> {
    serde_json::to_string(wire).map_err(encode_error)
}

fn parse_address(text: &str) -> Result<Address> {
    text.parse().map_err(|err: Error| {
        Error::new(
            ErrorCode::Internal,
            format!("a stored address does not parse: {err}"),
        )
    })
}

fn parse_txid(text: &str) -> Result<Txid> {
    text.parse().map_err(|err: Error| {
        Error::new(
            ErrorCode::Internal,
            format!("a stored transaction id does not parse: {err}"),
        )
    })
}

fn encode_error(err: serde_json::Error) -> Error {
    Error::new(
        ErrorCode::Internal,
        format!("could not encode an on-chain record: {err}"),
    )
}

fn decode_error(err: serde_json::Error) -> Error {
    Error::new(
        ErrorCode::Internal,
        format!("could not decode an on-chain record: {err}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real regtest address, taken from `bitcoin`'s own test suite, matching the one
    /// `src/onchain.rs`'s own tests use.
    fn an_address() -> Address {
        "bcrt1q2nfxmhd4n3c8834pj72xagvyr9gl57n5r94fsl"
            .parse()
            .expect("a valid regtest address")
    }

    /// The all-zero txid, matching the one `src/onchain.rs`'s own tests use.
    fn a_txid() -> Txid {
        "0000000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .expect("a well-formed transaction id")
    }

    fn send_details() -> OnchainSendDetails {
        OnchainSendDetails {
            address: an_address(),
            amount: Sats::from_sats(25_000),
            fee: Amount::from_msats(1_234),
            total: Amount::from_msats(25_001_234),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    fn waiting_receive_details() -> OnchainReceiveDetails {
        OnchainReceiveDetails {
            address: an_address(),
            txid: None,
            gross_deposited: None,
            fee: None,
            fee_breakdown: None,
            net_credit: None,
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    fn claimed_receive_details() -> OnchainReceiveDetails {
        OnchainReceiveDetails {
            txid: Some(a_txid()),
            gross_deposited: Some(Sats::from_sats(100_000)),
            fee: Some(Amount::from_msats(1_500)),
            fee_breakdown: Some(OnchainReceiveFeeBreakdown {
                peg_in: Amount::from_msats(1_000),
                network_claim: Amount::from_msats(200),
                primary_module: Amount::from_msats(200),
                dust: Amount::from_msats(100),
            }),
            net_credit: Some(Amount::from_msats(99_998_500)),
            ..waiting_receive_details()
        }
    }

    #[test]
    fn send_details_round_trip_through_json() {
        let details = send_details();
        let json = serde_json::to_string(&OnchainSendDetailsWire::from(&details)).expect("encode");
        assert_eq!(decode_send_details(&json).expect("decode"), details);
    }

    #[test]
    fn receive_details_round_trip_through_json_with_every_option_none() {
        let details = waiting_receive_details();
        let json =
            serde_json::to_string(&OnchainReceiveDetailsWire::from(&details)).expect("encode");
        assert_eq!(decode_receive_details(&json).expect("decode"), details);
    }

    #[test]
    fn receive_details_round_trip_through_json_with_every_option_some() {
        let details = claimed_receive_details();
        let breakdown = details.fee_breakdown.clone().expect("set above");
        let summed = breakdown
            .peg_in
            .checked_add(breakdown.network_claim)
            .and_then(|partial| partial.checked_add(breakdown.primary_module))
            .and_then(|partial| partial.checked_add(breakdown.dust))
            .expect("no overflow at this magnitude");
        assert_eq!(summed, details.fee.expect("set above"));

        let json =
            serde_json::to_string(&OnchainReceiveDetailsWire::from(&details)).expect("encode");
        assert_eq!(decode_receive_details(&json).expect("decode"), details);
    }

    #[test]
    fn the_private_walletv2_fields_stay_off_the_public_record() {
        let details = waiting_receive_details();
        let mut wire = OnchainReceiveDetailsWire::from(&details);
        let json = encode_receive_wire(&wire).expect("encode");
        assert!(!json.contains("upstream_operation_id"), "{json}");
        assert!(!json.contains("event_cursor"), "{json}");
        assert_eq!(decode_receive_details(&json).expect("decode"), details);

        wire.upstream_operation_id = Some("ab".repeat(32));
        wire.event_cursor = Some(42);
        let json = encode_receive_wire(&wire).expect("encode");
        assert!(json.contains("upstream_operation_id"), "{json}");
        assert!(json.contains("event_cursor"), "{json}");
        let decoded = decode_receive_wire(&json).expect("decode");
        assert_eq!(decoded.upstream_operation_id, wire.upstream_operation_id);
        assert_eq!(decoded.event_cursor, wire.event_cursor);
        // The public record still does not expose them.
        assert_eq!(decode_receive_details(&json).expect("decode"), details);
    }

    #[test]
    fn every_final_send_state_round_trips() {
        for state in [
            OnchainSendState::Succeeded { txid: a_txid() },
            OnchainSendState::Refunded {
                reason: "the federation rejected the funding transaction".to_owned(),
            },
            OnchainSendState::Failed {
                reason: "the funding was accepted but no transaction came of it".to_owned(),
            },
        ] {
            let encoded = encode_send_state(&state).expect("encode");
            assert_eq!(decode_send_state(&encoded).expect("decode"), state);
        }
    }

    #[test]
    fn every_final_receive_state_round_trips() {
        for state in [
            // A sub-satoshi net credit: `to_sats_exact` on it is `None`.
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
            let encoded = encode_receive_state(&state).expect("encode");
            assert_eq!(decode_receive_state(&encoded).expect("decode"), state);
        }
    }

    #[test]
    fn a_malformed_record_is_an_internal_error() {
        for json in ["", "{}", r#"{"address":"nope"}"#] {
            assert_eq!(
                decode_send_details(json).expect_err("refused").code,
                ErrorCode::Internal
            );
            assert_eq!(
                decode_receive_details(json).expect_err("refused").code,
                ErrorCode::Internal
            );
        }
        assert_eq!(
            decode_send_state("nonsense").expect_err("refused").code,
            ErrorCode::Internal
        );
        assert_eq!(
            decode_receive_state("nonsense").expect_err("refused").code,
            ErrorCode::Internal
        );
    }

    #[test]
    fn an_invalid_stored_address_is_an_internal_error() {
        let mut value =
            serde_json::to_value(OnchainSendDetailsWire::from(&send_details())).expect("encode");
        value.as_object_mut().expect("an object").insert(
            "address".to_owned(),
            serde_json::json!("not a bitcoin address"),
        );
        let json = serde_json::to_string(&value).expect("encode");
        assert_eq!(
            decode_send_details(&json).expect_err("refused").code,
            ErrorCode::Internal
        );

        let mut value =
            serde_json::to_value(OnchainReceiveDetailsWire::from(&waiting_receive_details()))
                .expect("encode");
        value.as_object_mut().expect("an object").insert(
            "address".to_owned(),
            serde_json::json!("not a bitcoin address"),
        );
        let json = serde_json::to_string(&value).expect("encode");
        assert_eq!(
            decode_receive_details(&json).expect_err("refused").code,
            ErrorCode::Internal
        );
    }
}
