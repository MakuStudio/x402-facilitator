//! x402 Facilitator HTTP entrypoint.
//!
//! This binary launches an Axum-based HTTP server that exposes the x402 protocol interface
//! for payment verification and settlement via Ethereum-compatible networks.
//!
//! Endpoints:
//! - `GET /verify` – Supported verification schema
//! - `POST /verify` – Verify a payment payload against requirements
//! - `GET /settle` – Supported settlement schema
//! - `POST /settle` – Settle an accepted payment payload on-chain
//! - `GET /supported` – List supported payment kinds (version/scheme/network)
//!
//! This server includes:
//! - OpenTelemetry tracing via `TraceLayer`
//! - CORS support for cross-origin clients
//! - Ethereum provider cache for per-network RPC routing
//!
//! Environment:
//! - `.env` values loaded at startup
//! - `HOST`, `PORT` control binding address
//! - `OTEL_*` variables enable tracing to systems like Honeycomb

use axum::Router;
use axum::http::Method;
use dotenvy::dotenv;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors;

use crate::facilitator_local::FacilitatorLocal;
use crate::provider_cache::ProviderCache;
use crate::rate_limit::RateLimitConfig;
use crate::sig_down::SigDown;
use crate::telemetry::Telemetry;

mod chain;
mod facilitator;
mod facilitator_local;
mod from_env;
mod handlers;
mod network;
mod provider_cache;
mod rate_limit;
mod sig_down;
mod telemetry;
mod timestamp;
mod types;

/// Initializes the x402 facilitator server.
///
/// - Loads `.env` variables.
/// - Initializes OpenTelemetry tracing.
/// - Connects to Ethereum providers for supported networks.
/// - Starts an Axum HTTP server with the x402 protocol handlers.
///
/// Binds to the address specified by the `HOST` and `PORT` env vars.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env variables
    dotenv().ok();

    let telemetry = Telemetry::new()
        .with_name(env!("CARGO_PKG_NAME"))
        .with_version(env!("CARGO_PKG_VERSION"))
        .register();

    let provider_cache = ProviderCache::from_env().await;
    // Abort if we can't initialise Ethereum providers early
    let provider_cache = match provider_cache {
        Ok(provider_cache) => provider_cache,
        Err(e) => {
            tracing::error!("Failed to create Ethereum providers: {}", e);
            std::process::exit(1);
        }
    };
    let facilitator = FacilitatorLocal::new(provider_cache);
    let axum_state = Arc::new(facilitator);

    // Load rate limiting configuration
    let rate_limit_config = RateLimitConfig::from_env();
    if let Some(ref config) = rate_limit_config {
        tracing::info!(
            verify_per_minute = config.verify_per_minute,
            settle_per_minute = config.settle_per_minute,
            transaction_status_per_minute = config.transaction_status_per_minute,
            general_per_minute = config.general_per_minute,
            "Rate limiting enabled"
        );
    } else {
        tracing::info!("Rate limiting disabled");
    }

    // Build routes with rate limiting
    let mut http_endpoints = Router::new()
        .merge(handlers::routes_with_transaction_status().with_state(axum_state));

    // TODO: Rate limiting is currently disabled due to Clone trait bound issues with RateLimit.
    // The tower::limit::RateLimitLayer's service type (RateLimit) doesn't implement Clone,
    // which is required by axum's Router::layer(). This needs to be addressed with an
    // axum-compatible rate limiting solution or by wrapping the rate limiter appropriately.
    // if let Some(config) = rate_limit_config {
    //     http_endpoints = http_endpoints.layer(config.general_layer());
    // }

    http_endpoints = http_endpoints
        .layer(telemetry.http_tracing())
        .layer(
            cors::CorsLayer::new()
                .allow_origin(cors::Any)
                .allow_methods([Method::GET, Method::POST])
                .allow_headers(cors::Any),
        );

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8080);

    let addr = SocketAddr::new(host.parse().expect("HOST must be a valid IP address"), port);
    tracing::info!("Starting server at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        });

    let sig_down = SigDown::try_new()?;
    let axum_cancellation_token = sig_down.cancellation_token();
    let axum_graceful_shutdown = async move { axum_cancellation_token.cancelled().await };
    axum::serve(listener, http_endpoints)
        .with_graceful_shutdown(axum_graceful_shutdown)
        .await?;

    Ok(())
}
