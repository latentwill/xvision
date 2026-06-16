//! Per-IP token-bucket rate limiter for the public x402/facilitator routes.
//!
//! Uses `tower_governor` 0.4 (compatible with axum 0.7 + governor 0.6).
//! Default: ~5 req/s (1 token / 200ms), burst 20. Tunable via env:
//!   XVN_X402_RATELIMIT_REPLENISH_MS — milliseconds per token (default 200)
//!   XVN_X402_RATELIMIT_BURST        — burst capacity (default 20)

use std::sync::Arc;

use tower_governor::{
    governor::GovernorConfigBuilder,
    key_extractor::PeerIpKeyExtractor,
    GovernorLayer,
};

// Re-exported by tower_governor; no need for a direct `governor` dep.
use ::governor::middleware::NoOpMiddleware;

/// Build a per-IP rate-limit layer for the x402 public routes.
///
/// Returns a `GovernorLayer` keyed on the client's peer IP (requires the server
/// to be started with `into_make_service_with_connect_info::<SocketAddr>()`,
/// which `serve()` in `server.rs` already does).
pub fn x402_rate_limit_layer() -> GovernorLayer<PeerIpKeyExtractor, NoOpMiddleware> {
    let per_ms: u64 = std::env::var("XVN_X402_RATELIMIT_REPLENISH_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200); // 1 token per 200ms = ~5 req/s

    let burst: u32 = std::env::var("XVN_X402_RATELIMIT_BURST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

    let cfg = GovernorConfigBuilder::default()
        .per_millisecond(per_ms)
        .burst_size(burst)
        .finish()
        .expect("valid governor config");

    GovernorLayer {
        config: Arc::new(cfg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_builds_with_defaults() {
        // Verifies that the GovernorConfigBuilder accepts the default env-tunable
        // parameters and that the layer can be constructed without panicking.
        let _layer = x402_rate_limit_layer();
    }
}
