//! The persisted shapes of the lightning facade: details records and final states as JSON, and
//! the phase a phase-keyed mapping reads after a restart.
//!
//! These are storage format. A field added later must be `Option` with `#[serde(default)]`, and
//! a field is never renamed or removed: every record already written reads through this file.

use serde::{Deserialize, Serialize};

use crate::{
    Amount, Error, ErrorCode, LightningRoute, LnReceiveDetails, LnReceiveState, LnSendDetails,
    LnSendState, Preimage, Result, Timestamp,
};

/// The operation reached the funded stage: upstream accepted the payment's funding, or the
/// incoming contract was confirmed paid. The only phase a lightning record ever carries.
pub(super) const PHASE_FUNDED: u32 = 1;

/// The key under which the SDK's own details record rides inside the module's metadata, so a
/// record rebuilt from the module's log entry after a crash carries the exact quoted terms.
pub(super) const CUSTOM_META_KEY: &str = "fedimint_sdk";

/// The details record wrapped for the module's custom metadata.
pub(super) fn custom_meta<W>(wire: &W) -> Result<serde_json::Value>
where
    W: Serialize,
{
    let wire = serde_json::to_value(wire).map_err(encode_error)?;
    Ok(serde_json::json!({ CUSTOM_META_KEY: wire }))
}

/// The details record carried inside the module's custom metadata, if this SDK put one there.
pub(super) fn from_custom_meta<W>(meta: &serde_json::Value) -> Option<W>
where
    W: serde::de::DeserializeOwned,
{
    let wire = meta.as_object()?.get(CUSTOM_META_KEY)?;
    serde_json::from_value(wire.clone()).ok()
}

/// [`LightningRoute`] as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum RouteWire {
    Internal,
    Gateway { gateway_id: String },
}

impl From<&LightningRoute> for RouteWire {
    fn from(route: &LightningRoute) -> RouteWire {
        match route {
            LightningRoute::Internal => RouteWire::Internal,
            LightningRoute::Gateway { gateway_id } => RouteWire::Gateway {
                gateway_id: gateway_id.to_string(),
            },
        }
    }
}

impl TryFrom<RouteWire> for LightningRoute {
    type Error = Error;

    fn try_from(wire: RouteWire) -> Result<LightningRoute> {
        Ok(match wire {
            RouteWire::Internal => LightningRoute::Internal,
            RouteWire::Gateway { gateway_id } => LightningRoute::Gateway {
                gateway_id: gateway_id.parse().map_err(|err: Error| {
                    Error::new(
                        ErrorCode::Internal,
                        format!("a stored gateway id does not parse: {err}"),
                    )
                })?,
            },
        })
    }
}

/// [`LnSendDetails`] as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct LnSendDetailsWire {
    pub(super) invoice: String,
    pub(super) invoice_amount_msats: u64,
    pub(super) fee_msats: u64,
    pub(super) total_msats: u64,
    pub(super) route: RouteWire,
    pub(super) created_at: u64,
    /// v1 only: the gateway's share of `fee_msats` at quote time, so a record rebuilt from the
    /// module's log can back it out when the module settled the payment internally after all.
    /// Not part of the public record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) gateway_fee_msats: Option<u64>,
}

impl From<&LnSendDetails> for LnSendDetailsWire {
    fn from(details: &LnSendDetails) -> LnSendDetailsWire {
        LnSendDetailsWire {
            invoice: details.invoice.to_string(),
            invoice_amount_msats: details.invoice_amount.msats(),
            fee_msats: details.fee.msats(),
            total_msats: details.total.msats(),
            route: RouteWire::from(&details.route),
            created_at: details.created_at.epoch_millis(),
            gateway_fee_msats: None,
        }
    }
}

impl TryFrom<LnSendDetailsWire> for LnSendDetails {
    type Error = Error;

    fn try_from(wire: LnSendDetailsWire) -> Result<LnSendDetails> {
        Ok(LnSendDetails {
            invoice: parse_invoice(&wire.invoice)?,
            invoice_amount: Amount::from_msats(wire.invoice_amount_msats),
            fee: Amount::from_msats(wire.fee_msats),
            total: Amount::from_msats(wire.total_msats),
            route: LightningRoute::try_from(wire.route)?,
            created_at: Timestamp::from_epoch_millis(wire.created_at),
        })
    }
}

/// [`LnReceiveDetails`] as stored, plus the one private fill-in-later field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct LnReceiveDetailsWire {
    pub(super) invoice: String,
    pub(super) description: String,
    pub(super) requested_amount_msats: u64,
    pub(super) invoice_amount_msats: u64,
    pub(super) fee_msats: u64,
    pub(super) net_credit_msats: u64,
    pub(super) gateway_id: Option<String>,
    pub(super) expires_at: u64,
    pub(super) created_at: u64,
    /// v1 only: the upstream operation a retried claim runs under, set once when a rejected
    /// claim is retried. Not part of the public record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) reclaim_operation_id: Option<String>,
}

impl From<&LnReceiveDetails> for LnReceiveDetailsWire {
    fn from(details: &LnReceiveDetails) -> LnReceiveDetailsWire {
        LnReceiveDetailsWire {
            invoice: details.invoice.to_string(),
            description: details.description.clone(),
            requested_amount_msats: details.requested_amount.msats(),
            invoice_amount_msats: details.invoice_amount.msats(),
            fee_msats: details.fee.msats(),
            net_credit_msats: details.net_credit.msats(),
            gateway_id: details.gateway_id.as_ref().map(ToString::to_string),
            expires_at: details.expires_at.epoch_millis(),
            created_at: details.created_at.epoch_millis(),
            reclaim_operation_id: None,
        }
    }
}

impl TryFrom<LnReceiveDetailsWire> for LnReceiveDetails {
    type Error = Error;

    fn try_from(wire: LnReceiveDetailsWire) -> Result<LnReceiveDetails> {
        let gateway_id = match wire.gateway_id {
            Some(id) => Some(id.parse().map_err(|err: Error| {
                Error::new(
                    ErrorCode::Internal,
                    format!("a stored gateway id does not parse: {err}"),
                )
            })?),
            None => None,
        };
        Ok(LnReceiveDetails {
            invoice: parse_invoice(&wire.invoice)?,
            description: wire.description,
            requested_amount: Amount::from_msats(wire.requested_amount_msats),
            invoice_amount: Amount::from_msats(wire.invoice_amount_msats),
            fee: Amount::from_msats(wire.fee_msats),
            net_credit: Amount::from_msats(wire.net_credit_msats),
            gateway_id,
            expires_at: Timestamp::from_epoch_millis(wire.expires_at),
            created_at: Timestamp::from_epoch_millis(wire.created_at),
        })
    }
}

/// [`LnSendState`] as stored on `OperationRecord::final_state`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum LnSendStateWire {
    Created,
    Funded,
    Success {
        preimage: String,
        fee_msats: u64,
        route: RouteWire,
    },
    Refunded,
    Failed {
        reason: String,
    },
}

/// [`LnReceiveState`] as stored on `OperationRecord::final_state`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum LnReceiveStateWire {
    Created,
    WaitingForPayment,
    Funded,
    Claimed,
    Canceled { reason: String },
    Expired,
    Failed,
}

pub(super) fn encode_send_state(state: &LnSendState) -> Result<String> {
    let wire = match state {
        LnSendState::Created => LnSendStateWire::Created,
        LnSendState::Funded => LnSendStateWire::Funded,
        LnSendState::Success {
            preimage,
            fee,
            route,
        } => LnSendStateWire::Success {
            preimage: preimage.to_string(),
            fee_msats: fee.msats(),
            route: RouteWire::from(route),
        },
        LnSendState::Refunded => LnSendStateWire::Refunded,
        LnSendState::Failed { reason } => LnSendStateWire::Failed {
            reason: reason.clone(),
        },
    };
    serde_json::to_string(&wire).map_err(encode_error)
}

pub(super) fn decode_send_state(encoded: &str) -> Result<LnSendState> {
    let wire: LnSendStateWire = serde_json::from_str(encoded).map_err(decode_error)?;
    Ok(match wire {
        LnSendStateWire::Created => LnSendState::Created,
        LnSendStateWire::Funded => LnSendState::Funded,
        LnSendStateWire::Success {
            preimage,
            fee_msats,
            route,
        } => LnSendState::Success {
            preimage: preimage.parse::<Preimage>().map_err(|err| {
                Error::new(
                    ErrorCode::Internal,
                    format!("a stored preimage does not parse: {err}"),
                )
            })?,
            fee: Amount::from_msats(fee_msats),
            route: LightningRoute::try_from(route)?,
        },
        LnSendStateWire::Refunded => LnSendState::Refunded,
        LnSendStateWire::Failed { reason } => LnSendState::Failed { reason },
    })
}

pub(super) fn encode_receive_state(state: &LnReceiveState) -> Result<String> {
    let wire = match state {
        LnReceiveState::Created => LnReceiveStateWire::Created,
        LnReceiveState::WaitingForPayment => LnReceiveStateWire::WaitingForPayment,
        LnReceiveState::Funded => LnReceiveStateWire::Funded,
        LnReceiveState::Claimed => LnReceiveStateWire::Claimed,
        LnReceiveState::Canceled { reason } => LnReceiveStateWire::Canceled {
            reason: reason.clone(),
        },
        LnReceiveState::Expired => LnReceiveStateWire::Expired,
        LnReceiveState::Failed => LnReceiveStateWire::Failed,
    };
    serde_json::to_string(&wire).map_err(encode_error)
}

pub(super) fn decode_receive_state(encoded: &str) -> Result<LnReceiveState> {
    let wire: LnReceiveStateWire = serde_json::from_str(encoded).map_err(decode_error)?;
    Ok(match wire {
        LnReceiveStateWire::Created => LnReceiveState::Created,
        LnReceiveStateWire::WaitingForPayment => LnReceiveState::WaitingForPayment,
        LnReceiveStateWire::Funded => LnReceiveState::Funded,
        LnReceiveStateWire::Claimed => LnReceiveState::Claimed,
        LnReceiveStateWire::Canceled { reason } => LnReceiveState::Canceled { reason },
        LnReceiveStateWire::Expired => LnReceiveState::Expired,
        LnReceiveStateWire::Failed => LnReceiveState::Failed,
    })
}

pub(super) fn decode_send_details(json: &str) -> Result<LnSendDetails> {
    let wire: LnSendDetailsWire = serde_json::from_str(json).map_err(decode_error)?;
    LnSendDetails::try_from(wire)
}

pub(super) fn decode_receive_details(json: &str) -> Result<LnReceiveDetails> {
    LnReceiveDetails::try_from(decode_receive_wire(json)?)
}

pub(super) fn decode_receive_wire(json: &str) -> Result<LnReceiveDetailsWire> {
    serde_json::from_str(json).map_err(decode_error)
}

pub(super) fn encode_receive_wire(wire: &LnReceiveDetailsWire) -> Result<String> {
    serde_json::to_string(wire).map_err(encode_error)
}

fn parse_invoice(text: &str) -> Result<crate::Bolt11Invoice> {
    text.parse().map_err(|err: Error| {
        Error::new(
            ErrorCode::Internal,
            format!("a stored invoice does not parse: {err}"),
        )
    })
}

fn encode_error(err: serde_json::Error) -> Error {
    Error::new(
        ErrorCode::Internal,
        format!("could not encode a lightning record: {err}"),
    )
}

fn decode_error(err: serde_json::Error) -> Error {
    Error::new(
        ErrorCode::Internal,
        format!("could not decode a lightning record: {err}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Amount;

    const GATEWAY_ID: &str = "0218845781f631c48f1c9709e23092067d06837f30aa0cd0544ac887fe91ddd166";
    const INVOICE: &str = "lnbcrt1u1pj48ugqdq2vdhkven9v5pp5g3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zqsp5242424242424242424242424242424242424242424242424242s9qrsgqcqzys2reg4wsryjt5w8z33ugydecgfmgyvtttwa7e0yzlm803z203j9hqspa4lr6m09cd808xkw9uh4sxc8wf3w6k0gaf5zrqm7zhcxug0vqqpdkpja";

    fn send_details() -> LnSendDetails {
        LnSendDetails {
            invoice: INVOICE.parse().expect("a valid regtest invoice"),
            invoice_amount: Amount::from_msats(100_000),
            fee: Amount::from_msats(1_050),
            total: Amount::from_msats(101_050),
            route: LightningRoute::Gateway {
                gateway_id: GATEWAY_ID.parse().expect("a valid gateway id"),
            },
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    fn receive_details() -> LnReceiveDetails {
        LnReceiveDetails {
            invoice: INVOICE.parse().expect("a valid regtest invoice"),
            description: "coffee".to_owned(),
            requested_amount: Amount::from_msats(100_000),
            invoice_amount: Amount::from_msats(100_000),
            fee: Amount::from_msats(500),
            net_credit: Amount::from_msats(99_500),
            gateway_id: Some(GATEWAY_ID.parse().expect("a valid gateway id")),
            expires_at: Timestamp::from_epoch_millis(1_700_003_600_000),
            created_at: Timestamp::from_epoch_millis(1_700_000_000_000),
        }
    }

    #[test]
    fn send_details_round_trip_through_json() {
        let details = send_details();
        let json = serde_json::to_string(&LnSendDetailsWire::from(&details)).expect("encode");
        assert_eq!(decode_send_details(&json).expect("decode"), details);
    }

    #[test]
    fn the_send_wire_round_trips_with_and_without_the_gateway_fee() {
        let mut wire = LnSendDetailsWire::from(&send_details());
        wire.gateway_fee_msats = Some(50);
        let json = serde_json::to_string(&wire).expect("encode");
        assert!(json.contains("gateway_fee_msats"), "{json}");
        let decoded: LnSendDetailsWire = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, wire);

        wire.gateway_fee_msats = None;
        let json = serde_json::to_string(&wire).expect("encode");
        assert!(!json.contains("gateway_fee_msats"), "{json}");
        let decoded: LnSendDetailsWire = serde_json::from_str(&json).expect("decode");
        assert_eq!(decoded, wire);
    }

    #[test]
    fn a_send_record_without_the_gateway_fee_field_decodes() {
        // An older build's record, written before this field existed: the key is absent
        // entirely, not just `null`.
        let mut value =
            serde_json::to_value(LnSendDetailsWire::from(&send_details())).expect("encode");
        value
            .as_object_mut()
            .expect("an object")
            .remove("gateway_fee_msats");
        let wire: LnSendDetailsWire = serde_json::from_value(value).expect("decode");
        assert_eq!(wire.gateway_fee_msats, None);
        assert_eq!(
            LnSendDetails::try_from(wire).expect("decode"),
            send_details()
        );
    }

    #[test]
    fn an_internal_route_round_trips() {
        let details = LnSendDetails {
            route: LightningRoute::Internal,
            ..send_details()
        };
        let json = serde_json::to_string(&LnSendDetailsWire::from(&details)).expect("encode");
        assert!(json.contains(r#""kind":"internal""#), "{json}");
        assert_eq!(decode_send_details(&json).expect("decode"), details);
    }

    #[test]
    fn receive_details_round_trip_through_json() {
        let details = receive_details();
        let json = serde_json::to_string(&LnReceiveDetailsWire::from(&details)).expect("encode");
        assert!(!json.contains("reclaim_operation_id"), "{json}");
        assert_eq!(decode_receive_details(&json).expect("decode"), details);
    }

    #[test]
    fn a_receive_without_a_gateway_round_trips() {
        let details = LnReceiveDetails {
            gateway_id: None,
            ..receive_details()
        };
        let json = serde_json::to_string(&LnReceiveDetailsWire::from(&details)).expect("encode");
        assert_eq!(decode_receive_details(&json).expect("decode"), details);
    }

    #[test]
    fn the_reclaim_id_is_kept_on_the_wire_record_only() {
        let mut wire = LnReceiveDetailsWire::from(&receive_details());
        wire.reclaim_operation_id = Some("ab".repeat(32));
        let json = encode_receive_wire(&wire).expect("encode");
        assert_eq!(
            decode_receive_wire(&json)
                .expect("decode")
                .reclaim_operation_id
                .as_deref(),
            Some("ab".repeat(32).as_str())
        );
        // The public record does not expose it.
        assert_eq!(
            decode_receive_details(&json).expect("decode"),
            receive_details()
        );
    }

    #[test]
    fn custom_meta_round_trips_through_from_custom_meta() {
        let wire = LnSendDetailsWire::from(&send_details());
        let meta = custom_meta(&wire).expect("encode");
        assert_eq!(from_custom_meta::<LnSendDetailsWire>(&meta), Some(wire));
    }

    #[test]
    fn from_custom_meta_is_none_when_absent_or_malformed() {
        assert_eq!(
            from_custom_meta::<LnSendDetailsWire>(&serde_json::Value::Null),
            None
        );
        assert_eq!(
            from_custom_meta::<LnSendDetailsWire>(&serde_json::json!({"other": 1})),
            None
        );
        assert_eq!(
            from_custom_meta::<LnSendDetailsWire>(
                &serde_json::json!({ CUSTOM_META_KEY: "not the right shape" })
            ),
            None
        );
    }

    #[test]
    fn every_final_send_state_round_trips() {
        let preimage: Preimage = "11".repeat(32).parse().expect("a preimage");
        for state in [
            LnSendState::Success {
                preimage,
                fee: Amount::from_msats(1_050),
                route: LightningRoute::Internal,
            },
            LnSendState::Refunded,
            LnSendState::Failed {
                reason: "gone".to_owned(),
            },
        ] {
            let encoded = encode_send_state(&state).expect("encode");
            assert_eq!(decode_send_state(&encoded).expect("decode"), state);
        }
    }

    #[test]
    fn every_final_receive_state_round_trips() {
        for state in [
            LnReceiveState::Claimed,
            LnReceiveState::Canceled {
                reason: "withdrawn".to_owned(),
            },
            LnReceiveState::Expired,
            LnReceiveState::Failed,
        ] {
            let encoded = encode_receive_state(&state).expect("encode");
            assert_eq!(decode_receive_state(&encoded).expect("decode"), state);
        }
    }

    #[test]
    fn a_malformed_record_is_an_internal_error() {
        for json in ["", "{}", r#"{"invoice":"nope"}"#] {
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
    }
}
