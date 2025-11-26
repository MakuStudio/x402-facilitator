//! HTTP endpoints implemented by the x402 **facilitator**.
//!
//! These are the server-side handlers for processing client-submitted x402 payments.
//! They include both protocol-critical endpoints (`/verify`, `/settle`) and discovery endpoints (`/supported`, etc).
//!
//! All payloads follow the types defined in the `x402-facilitator` crate, and are compatible
//! with the TypeScript and Go client SDKs.
//!
//! Each endpoint consumes or produces structured JSON payloads defined in `x402-facilitator`,
//! and is compatible with official x402 client SDKs.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router, response::IntoResponse};
use serde_json::json;
use tracing::instrument;

use crate::chain::FacilitatorLocalError;
use crate::facilitator::Facilitator;
use crate::facilitator_local::FacilitatorLocal;
use crate::provider_cache::ProviderCache;
use crate::types::{
    ErrorResponse, FacilitatorErrorReason, MixedAddress, SettleRequest, TransactionHash,
    VerifyRequest, VerifyResponse,
};

/// `GET /verify`: Returns a machine-readable description of the `/verify` endpoint.
///
/// This is served by the facilitator to help clients understand how to construct
/// a valid [`VerifyRequest`] for payment verification.
///
/// This is optional metadata and primarily useful for discoverability and debugging tools.
#[instrument(skip_all)]
pub async fn get_verify_info() -> impl IntoResponse {
    Json(json!({
        "endpoint": "/verify",
        "description": "POST to verify x402 payments",
        "body": {
            "paymentPayload": "PaymentPayload",
            "paymentRequirements": "PaymentRequirements",
        }
    }))
}

/// `GET /settle`: Returns a machine-readable description of the `/settle` endpoint.
///
/// This is served by the facilitator to describe the structure of a valid
/// [`SettleRequest`] used to initiate on-chain payment settlement.
#[instrument(skip_all)]
pub async fn get_settle_info() -> impl IntoResponse {
    Json(json!({
        "endpoint": "/settle",
        "description": "POST to settle x402 payments",
        "body": {
            "paymentPayload": "PaymentPayload",
            "paymentRequirements": "PaymentRequirements",
        }
    }))
}

pub fn routes<A>() -> Router<A>
where
    A: Facilitator + Clone + Send + Sync + 'static,
    A::Error: IntoResponse,
{
    Router::new()
        .route("/", get(get_root))
        .route("/verify", get(get_verify_info))
        .route("/verify", post(post_verify::<A>))
        .route("/settle", get(get_settle_info))
        .route("/settle", post(post_settle::<A>))
        .route("/health", get(get_health::<A>))
        .route("/supported", get(get_supported::<A>))
}

/// Routes specifically for FacilitatorLocal with transaction status support.
pub fn routes_with_transaction_status() -> Router<std::sync::Arc<FacilitatorLocal<ProviderCache>>> {
    Router::new()
        .route("/", get(get_root))
        .route("/verify", get(get_verify_info))
        .route("/verify", post(post_verify_facilitator_local))
        .route("/settle", get(get_settle_info))
        .route("/settle", post(post_settle_facilitator_local))
        .route("/health", get(get_health_facilitator_local))
        .route("/supported", get(get_supported_facilitator_local))
        .route("/transaction/:tx_hash", get(get_transaction_status))
}

/// Wrapper handlers for FacilitatorLocal<ProviderCache>
async fn post_verify_facilitator_local(
    State(facilitator): State<std::sync::Arc<FacilitatorLocal<ProviderCache>>>,
    Json(body): Json<VerifyRequest>,
) -> impl IntoResponse {
    post_verify(State(facilitator), Json(body)).await
}

async fn post_settle_facilitator_local(
    State(facilitator): State<std::sync::Arc<FacilitatorLocal<ProviderCache>>>,
    Json(body): Json<SettleRequest>,
) -> impl IntoResponse {
    post_settle(State(facilitator), Json(body)).await
}

async fn get_health_facilitator_local(
    State(facilitator): State<std::sync::Arc<FacilitatorLocal<ProviderCache>>>,
) -> impl IntoResponse {
    get_health(State(facilitator)).await
}

async fn get_supported_facilitator_local(
    State(facilitator): State<std::sync::Arc<FacilitatorLocal<ProviderCache>>>,
) -> impl IntoResponse {
    get_supported(State(facilitator)).await
}

/// `GET /`: Returns a simple greeting message from the facilitator.
#[instrument(skip_all)]
pub async fn get_root() -> impl IntoResponse {
    let pkg_name = env!("CARGO_PKG_NAME");
    (StatusCode::OK, format!("Hello from {pkg_name}!"))
}

/// `GET /supported`: Lists the x402 payment schemes and networks supported by this facilitator.
///
/// Facilitators may expose this to help clients dynamically configure their payment requests
/// based on available network and scheme support.
#[instrument(skip_all)]
pub async fn get_supported<A>(State(facilitator): State<A>) -> impl IntoResponse
where
    A: Facilitator,
    A::Error: IntoResponse,
{
    match facilitator.supported().await {
        Ok(supported) => (StatusCode::OK, Json(json!(supported))).into_response(),
        Err(error) => error.into_response(),
    }
}

#[instrument(skip_all)]
pub async fn get_health<A>(State(facilitator): State<A>) -> impl IntoResponse
where
    A: Facilitator,
    A::Error: IntoResponse,
{
    get_supported(State(facilitator)).await
}

/// `POST /verify`: Facilitator-side verification of a proposed x402 payment.
///
/// This endpoint checks whether a given payment payload satisfies the declared
/// [`PaymentRequirements`], including signature validity, scheme match, and fund sufficiency.
///
/// Responds with a [`VerifyResponse`] indicating whether the payment can be accepted.
#[instrument(skip_all)]
pub async fn post_verify<A>(
    State(facilitator): State<A>,
    Json(body): Json<VerifyRequest>,
) -> impl IntoResponse
where
    A: Facilitator,
    A::Error: IntoResponse,
{
    match facilitator.verify(&body).await {
        Ok(valid_response) => (StatusCode::OK, Json(valid_response)).into_response(),
        Err(error) => {
            tracing::warn!(
                error = ?error,
                body = %serde_json::to_string(&body).unwrap_or_else(|_| "<can-not-serialize>".to_string()),
                "Verification failed"
            );
            error.into_response()
        }
    }
}

/// `POST /settle`: Facilitator-side execution of a valid x402 payment on-chain.
///
/// Given a valid [`SettleRequest`], this endpoint attempts to execute the payment
/// via ERC-3009 `transferWithAuthorization`, and returns a [`SettleResponse`] with transaction details.
///
/// This endpoint is typically called after a successful `/verify` step.
#[instrument(skip_all)]
pub async fn post_settle<A>(
    State(facilitator): State<A>,
    Json(body): Json<SettleRequest>,
) -> impl IntoResponse
where
    A: Facilitator,
    A::Error: IntoResponse,
{
    match facilitator.settle(&body).await {
        Ok(valid_response) => (StatusCode::OK, Json(valid_response)).into_response(),
        Err(error) => {
            tracing::warn!(
                error = ?error,
                body = %serde_json::to_string(&body).unwrap_or_else(|_| "<can-not-serialize>".to_string()),
                "Settlement failed"
            );
            error.into_response()
        }
    }
}

/// `GET /transaction/:tx_hash`: Query the status of a transaction by its hash.
///
/// This endpoint allows clients to check the status of a previously settled payment transaction.
/// It returns the current status (pending, confirmed, failed, or not found), along with
/// block number, confirmations, and any error information.
///
/// The transaction hash can be in EVM format (0x-prefixed hex) or Solana format (base58).
#[instrument(skip_all, err, fields(tx_hash = %tx_hash_str))]
pub async fn get_transaction_status(
    State(facilitator): State<std::sync::Arc<FacilitatorLocal<ProviderCache>>>,
    Path(tx_hash_str): Path<String>,
) -> impl IntoResponse {
    // Parse transaction hash from string using serde_json (which uses the Deserialize impl)
    let tx_hash = match serde_json::from_str::<TransactionHash>(&format!("\"{}\"", tx_hash_str)) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::debug!(
                error = %e,
                tx_hash = %tx_hash_str,
                "Failed to parse transaction hash"
            );
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid transaction hash format: {}. Expected EVM (0x-prefixed hex) or Solana (base58) format.", tx_hash_str),
                }),
            )
                .into_response();
        }
    };

    match facilitator.get_transaction_status(&tx_hash).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(error) => {
            tracing::warn!(
                error = ?error,
                tx_hash = %tx_hash_str,
                "Failed to query transaction status"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to query transaction status: {}", error),
                }),
            )
                .into_response()
        }
    }
}

fn invalid_schema(payer: Option<MixedAddress>) -> VerifyResponse {
    VerifyResponse::invalid(payer, FacilitatorErrorReason::InvalidScheme)
}

impl IntoResponse for FacilitatorLocalError {
    fn into_response(self) -> Response {
        let error = self;

        let bad_request = (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid request".to_string(),
            }),
        )
            .into_response();

        match error {
            FacilitatorLocalError::SchemeMismatch(payer, ..) => {
                (StatusCode::OK, Json(invalid_schema(payer))).into_response()
            }
            FacilitatorLocalError::ReceiverMismatch(payer, ..)
            | FacilitatorLocalError::InvalidSignature(payer, ..)
            | FacilitatorLocalError::InvalidTiming(payer, ..)
            | FacilitatorLocalError::InsufficientValue(payer) => {
                (StatusCode::OK, Json(invalid_schema(Some(payer)))).into_response()
            }
            FacilitatorLocalError::NetworkMismatch(payer, ..)
            | FacilitatorLocalError::UnsupportedNetwork(payer) => (
                StatusCode::OK,
                Json(VerifyResponse::invalid(
                    payer,
                    FacilitatorErrorReason::InvalidNetwork,
                )),
            )
                .into_response(),
            FacilitatorLocalError::ContractCall(..)
            | FacilitatorLocalError::InvalidAddress(..)
            | FacilitatorLocalError::ClockError(_) => bad_request,
            FacilitatorLocalError::DecodingError(reason) => (
                StatusCode::OK,
                Json(VerifyResponse::invalid(
                    None,
                    FacilitatorErrorReason::FreeForm(reason),
                )),
            )
                .into_response(),
            FacilitatorLocalError::InsufficientFunds(payer) => (
                StatusCode::OK,
                Json(VerifyResponse::invalid(
                    Some(payer),
                    FacilitatorErrorReason::InsufficientFunds,
                )),
            )
                .into_response(),
        }
    }
}
