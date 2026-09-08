//! The v1 lightning module (`ln`): mappings, subscriptions, and the facade operations.

use fedimint_client::Client;
use fedimint_client_module::ClientModuleInstance;
use fedimint_core::core::OperationId;
use fedimint_core::util::BoxStream;
use fedimint_ln_client::{InternalPayState, LightningClientModule, LnPayState};
use futures::StreamExt;

use super::driver::until_final;
use super::subscribe_error;
use crate::federation::FederationInner;
use crate::{
    Amount, Error, ErrorCode, LightningRoute, LnSendDetails, LnSendState, Preimage, Result,
};

// Upstream `LnPayState` onto `LnSendState`. The fee and the route come from the executed quote:
// the v1 progress stream carries neither.
//
// | upstream                          | here                                   |
// | --------------------------------- | -------------------------------------- |
// | `Created`                         | `Created`                              |
// | `Funded`, `AwaitingChange`        | `Funded`                               |
// | `WaitingForRefund`                | `Funded` (a refund is still in flight) |
// | `Success { preimage }`            | `Success`                              |
// | `Canceled` (never funded)         | `Refunded`                             |
// | `Refunded`                        | `Refunded`                             |
// | `UnexpectedError`                 | `Failed`                               |
pub(super) fn map_ln_pay(
    state: &LnPayState,
    fee: Amount,
    route: &LightningRoute,
) -> Result<LnSendState> {
    Ok(match state {
        LnPayState::Created => LnSendState::Created,
        LnPayState::Funded { .. }
        | LnPayState::AwaitingChange
        | LnPayState::WaitingForRefund { .. } => LnSendState::Funded,
        LnPayState::Success { preimage } => LnSendState::Success {
            // v1 reports the preimage as the hex text the gateway answered with, unvalidated.
            preimage: preimage.parse::<Preimage>().map_err(|err| {
                Error::new(
                    ErrorCode::Internal,
                    format!("the gateway reported an unusable preimage: {err}"),
                )
            })?,
            fee,
            route: route.clone(),
        },
        LnPayState::Canceled | LnPayState::Refunded { .. } => LnSendState::Refunded,
        LnPayState::UnexpectedError { error_message } => LnSendState::Failed {
            reason: error_message.clone(),
        },
    })
}

// Upstream `InternalPayState` onto `LnSendState`, for a payment settled inside the federation.
//
// | upstream                       | here        |
// | ------------------------------ | ----------- |
// | `Funding`                      | `Created`   |
// | `Preimage`                     | `Success`   |
// | `RefundSuccess`                | `Refunded`  |
// | `FundingFailed` (never debited)| `Refunded`  |
// | `RefundError`                  | `Failed`    |
// | `UnexpectedError`              | `Failed`    |
pub(super) fn map_internal_pay(
    state: &InternalPayState,
    fee: Amount,
    route: &LightningRoute,
) -> LnSendState {
    match state {
        InternalPayState::Funding => LnSendState::Created,
        InternalPayState::Preimage(preimage) => LnSendState::Success {
            preimage: Preimage::from_bytes(preimage.0),
            fee,
            route: route.clone(),
        },
        InternalPayState::RefundSuccess { .. } | InternalPayState::FundingFailed { .. } => {
            LnSendState::Refunded
        }
        InternalPayState::RefundError { error_message, .. } => LnSendState::Failed {
            reason: error_message.clone(),
        },
        InternalPayState::UnexpectedError(message) => LnSendState::Failed {
            reason: message.clone(),
        },
    }
}

/// The v1 module on a live client, or `NotSupported` when the federation dropped it.
pub(super) fn module_of(
    client: &Client,
) -> Result<ClientModuleInstance<'_, LightningClientModule>> {
    client
        .get_first_module::<LightningClientModule>()
        .map_err(|_| {
            Error::new(
                ErrorCode::NotSupported,
                "this federation no longer has a v1 lightning module",
            )
        })
}

/// A fresh stream over a v1 send, chosen by the route the record says the payment took.
///
/// The client guard is held only for the bounded upstream read that produces the stream; the
/// stream itself registers with the module's notifier when first polled, as the driver contract
/// requires.
pub(super) async fn subscribe_send(
    federation: &FederationInner,
    id: OperationId,
    details: &LnSendDetails,
) -> Result<BoxStream<'static, Result<LnSendState>>> {
    let client = federation.client(false).await?;
    let module = module_of(&client)?;
    let fee = details.fee;
    let route = details.route.clone();
    let stream: BoxStream<'static, Result<LnSendState>> = match &route {
        LightningRoute::Internal => {
            let upstream = module
                .subscribe_internal_pay(id)
                .await
                .map_err(subscribe_error)?
                .into_stream();
            Box::pin(upstream.map(move |state| Ok(map_internal_pay(&state, fee, &route))))
        }
        LightningRoute::Gateway { .. } => {
            let upstream = module
                .subscribe_ln_pay(id)
                .await
                .map_err(subscribe_error)?
                .into_stream();
            Box::pin(upstream.map(move |state| map_ln_pay(&state, fee, &route)))
        }
    };
    Ok(until_final(stream))
}

#[cfg(test)]
mod tests {
    use fedimint_ln_client::pay::GatewayPayError;

    use super::*;
    use crate::Amount;

    fn fee() -> Amount {
        Amount::from_msats(1_050)
    }

    fn gateway_route() -> LightningRoute {
        LightningRoute::Gateway {
            gateway_id: "0218845781f631c48f1c9709e23092067d06837f30aa0cd0544ac887fe91ddd166"
                .parse()
                .expect("a gateway id"),
        }
    }

    #[test]
    fn ln_pay_states_fold_onto_the_send_lifecycle() {
        let cases = [
            (LnPayState::Created, LnSendState::Created),
            (LnPayState::Funded { block_height: 7 }, LnSendState::Funded),
            (LnPayState::AwaitingChange, LnSendState::Funded),
            (
                LnPayState::WaitingForRefund {
                    error_reason: "no route".to_owned(),
                },
                LnSendState::Funded,
            ),
            (LnPayState::Canceled, LnSendState::Refunded),
            (
                LnPayState::Refunded {
                    gateway_error: GatewayPayError::OutgoingContractError,
                },
                LnSendState::Refunded,
            ),
            (
                LnPayState::UnexpectedError {
                    error_message: "boom".to_owned(),
                },
                LnSendState::Failed {
                    reason: "boom".to_owned(),
                },
            ),
        ];
        for (upstream, expected) in cases {
            assert_eq!(
                map_ln_pay(&upstream, fee(), &gateway_route()).expect("maps"),
                expected,
                "{upstream:?}"
            );
        }
    }

    #[test]
    fn a_v1_success_parses_the_hex_preimage_and_carries_the_quoted_terms() {
        let mapped = map_ln_pay(
            &LnPayState::Success {
                preimage: "11".repeat(32),
            },
            fee(),
            &gateway_route(),
        )
        .expect("maps");
        assert_eq!(
            mapped,
            LnSendState::Success {
                preimage: Preimage::from_bytes([0x11; 32]),
                fee: fee(),
                route: gateway_route(),
            }
        );
    }

    #[test]
    fn a_v1_success_with_an_unparsable_preimage_is_internal() {
        let err = map_ln_pay(
            &LnPayState::Success {
                preimage: "not hex".to_owned(),
            },
            fee(),
            &gateway_route(),
        )
        .expect_err("refused");
        assert_eq!(err.code, ErrorCode::Internal);
    }

    #[test]
    fn internal_pay_states_fold_onto_the_send_lifecycle() {
        use fedimint_ln_client::incoming::IncomingSmError;

        let error = IncomingSmError::TimeoutFetchingOffer {
            payment_hash: fedimint_core::bitcoin::hashes::Hash::hash(b"x"),
        };
        let cases = [
            (InternalPayState::Funding, LnSendState::Created),
            (
                InternalPayState::Preimage(fedimint_ln_common::contracts::Preimage([0x22; 32])),
                LnSendState::Success {
                    preimage: Preimage::from_bytes([0x22; 32]),
                    fee: fee(),
                    route: LightningRoute::Internal,
                },
            ),
            (
                InternalPayState::RefundSuccess {
                    out_points: Vec::new(),
                    error: error.clone(),
                },
                LnSendState::Refunded,
            ),
            (
                InternalPayState::FundingFailed {
                    error: error.clone(),
                },
                LnSendState::Refunded,
            ),
            (
                InternalPayState::RefundError {
                    error_message: "stuck".to_owned(),
                    error,
                },
                LnSendState::Failed {
                    reason: "stuck".to_owned(),
                },
            ),
            (
                InternalPayState::UnexpectedError("odd".to_owned()),
                LnSendState::Failed {
                    reason: "odd".to_owned(),
                },
            ),
        ];
        for (upstream, expected) in cases {
            assert_eq!(
                map_internal_pay(&upstream, fee(), &LightningRoute::Internal),
                expected,
                "{upstream:?}"
            );
        }
    }
}
