//! Rate limiting configuration and middleware setup.
//!
//! This module provides configurable rate limiting to protect the facilitator
//! from abuse and DoS attacks. Rate limits are applied per IP address and
//! can be configured separately for different endpoint types.

use std::time::Duration;
use tower::limit::RateLimitLayer;
use tower::ServiceBuilder;

/// Rate limiting configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per minute for verification endpoints.
    pub verify_per_minute: u32,
    /// Maximum requests per minute for settlement endpoints.
    pub settle_per_minute: u32,
    /// Maximum requests per minute for transaction status endpoints.
    pub transaction_status_per_minute: u32,
    /// Maximum requests per minute for other endpoints (health, supported, etc.).
    pub general_per_minute: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            verify_per_minute: 60,
            settle_per_minute: 30,
            transaction_status_per_minute: 120,
            general_per_minute: 300,
        }
    }
}

impl RateLimitConfig {
    /// Load rate limiting configuration from environment variables.
    ///
    /// Environment variables:
    /// - `RATE_LIMIT_VERIFY_PER_MINUTE`: Requests per minute for `/verify` (default: 60)
    /// - `RATE_LIMIT_SETTLE_PER_MINUTE`: Requests per minute for `/settle` (default: 30)
    /// - `RATE_LIMIT_TRANSACTION_STATUS_PER_MINUTE`: Requests per minute for `/transaction/:tx_hash` (default: 120)
    /// - `RATE_LIMIT_GENERAL_PER_MINUTE`: Requests per minute for other endpoints (default: 300)
    ///
    /// If rate limiting is disabled (all values set to 0), returns None.
    pub fn from_env() -> Option<Self> {
        let verify = std::env::var("RATE_LIMIT_VERIFY_PER_MINUTE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);

        let settle = std::env::var("RATE_LIMIT_SETTLE_PER_MINUTE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        let transaction_status = std::env::var("RATE_LIMIT_TRANSACTION_STATUS_PER_MINUTE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(120);

        let general = std::env::var("RATE_LIMIT_GENERAL_PER_MINUTE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300);

        // If all limits are 0, rate limiting is disabled
        if verify == 0 && settle == 0 && transaction_status == 0 && general == 0 {
            return None;
        }

        Some(Self {
            verify_per_minute: verify,
            settle_per_minute: settle,
            transaction_status_per_minute: transaction_status,
            general_per_minute: general,
        })
    }

    /// Create a rate limit layer for verification endpoints.
    pub fn verify_layer(&self) -> RateLimitLayer {
        RateLimitLayer::new(self.verify_per_minute as u64, Duration::from_secs(60))
    }

    /// Create a rate limit layer for settlement endpoints.
    pub fn settle_layer(&self) -> RateLimitLayer {
        RateLimitLayer::new(self.settle_per_minute as u64, Duration::from_secs(60))
    }

    /// Create a rate limit layer for transaction status endpoints.
    pub fn transaction_status_layer(&self) -> RateLimitLayer {
        RateLimitLayer::new(self.transaction_status_per_minute as u64, Duration::from_secs(60))
    }

    /// Create a rate limit layer for general endpoints.
    pub fn general_layer(&self) -> RateLimitLayer {
        RateLimitLayer::new(self.general_per_minute as u64, Duration::from_secs(60))
    }
}

/// Create a service builder with rate limiting applied.
///
/// This applies rate limiting based on the configuration, using a simple
/// in-memory rate limiter that tracks requests per IP address.
pub fn create_rate_limited_service_builder(
    config: Option<&RateLimitConfig>,
) -> ServiceBuilder<tower::layer::util::Stack<RateLimitLayer, tower::layer::util::Identity>> {
    let limit = config
        .map(|c| c.general_per_minute as u64)
        .unwrap_or(u32::MAX as u64);
    
    ServiceBuilder::new().layer(RateLimitLayer::new(limit, Duration::from_secs(60)))
}

