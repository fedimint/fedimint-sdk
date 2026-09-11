//! The persisted form of a recovery attempt's state.

use serde::{Deserialize, Serialize};

use crate::{Error, ErrorCode, RecoveryState, Result};

/// [`RecoveryState`] as stored on `OperationRecord::final_state`.
#[derive(Debug, Serialize, Deserialize)]
enum RecoveryStateWire {
    Running,
    Done,
    Failed { reason: String },
}

pub(super) fn encode_state(state: &RecoveryState) -> Result<String> {
    let wire = match state {
        RecoveryState::Running => RecoveryStateWire::Running,
        RecoveryState::Done => RecoveryStateWire::Done,
        RecoveryState::Failed { reason } => RecoveryStateWire::Failed {
            reason: reason.clone(),
        },
    };
    serde_json::to_string(&wire).map_err(encode_error)
}

pub(super) fn decode_state(encoded: &str) -> Result<RecoveryState> {
    let wire: RecoveryStateWire = serde_json::from_str(encoded).map_err(decode_error)?;
    Ok(match wire {
        RecoveryStateWire::Running => RecoveryState::Running,
        RecoveryStateWire::Done => RecoveryState::Done,
        RecoveryStateWire::Failed { reason } => RecoveryState::Failed { reason },
    })
}

fn encode_error(err: serde_json::Error) -> Error {
    Error::new(
        ErrorCode::Internal,
        format!("could not encode a recovery record: {err}"),
    )
}

fn decode_error(err: serde_json::Error) -> Error {
    Error::new(
        ErrorCode::Internal,
        format!("could not decode a recovery record: {err}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_round_trips() {
        for state in [
            RecoveryState::Running,
            RecoveryState::Done,
            RecoveryState::Failed {
                reason: "gone".to_owned(),
            },
        ] {
            let encoded = encode_state(&state).expect("encode");
            assert_eq!(decode_state(&encoded).expect("decode"), state);
        }
    }

    #[test]
    fn a_state_this_build_does_not_know_is_refused() {
        assert_eq!(
            decode_state(r#"{"Paused":{}}"#).expect_err("refused").code,
            ErrorCode::Internal
        );
    }

    #[test]
    fn the_wire_is_the_documented_shape() {
        assert_eq!(
            encode_state(&RecoveryState::Running).expect("encode"),
            "\"Running\""
        );
        assert_eq!(
            encode_state(&RecoveryState::Failed {
                reason: "x".to_owned(),
            })
            .expect("encode"),
            r#"{"Failed":{"reason":"x"}}"#
        );
    }
}
