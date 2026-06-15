//! Plain `hyperliquid` live venue — native Hyperliquid perps, EIP-712 signed in
//! Rust, **no npm, no fund-capable key in env**.
//!
//! This is DISTINCT from the [`crate::virtuals::DegenArenaSurface`]
//! (`broker_creds_ref = "degen_arena"`) which carries the AI-Pot / Virtuals
//! Degen Arena product framing. `HyperliquidSurface` (`broker_creds_ref =
//! "hyperliquid"`) is the plain "I want to trade on Hyperliquid perps natively
//! without the Arena product layer" venue.
//!
//! ## Delegation
//!
//! All order mechanics — EIP-712 signing, TP/SL bracket legs, asset-index
//! resolution, slippage, wire protocol — are identical to `DegenArenaSurface`
//! and are **delegated** to it rather than reimplemented. The only observable
//! difference is:
//!
//! - `venue()` returns `"hyperliquid"` (not the default `"live"`).
//! - `signing_scheme()` returns `"eip712"` (native EIP-712, no subprocess).
//! - `is_perp_venue()` returns `true` (Hyperliquid is a directional-perps venue).
//!
//! ## Forward-compat caveat
//!
//! When `DegenArenaSurface` gains Arena-specific logic (ACP eligibility checks,
//! AI-Pot attribution), `HyperliquidSurface` MUST stop delegating to it and
//! instead hold a bare `ReqwestHyperliquidApi` directly, calling the same
//! low-level signing path but skipping the Arena layer. Track this divergence
//! as a TODO when Arena-specific logic lands.
//!
//! ## Credentials
//!
//! Uses separate env vars (`HL_API_KEY` / `HL_ACCOUNT_ADDRESS` / `HL_NETWORK`)
//! and a separate `[hyperliquid]` section in `brokers.toml` so the two venues
//! can carry independent keys (an operator may run both simultaneously with
//! different wallets).

use async_trait::async_trait;

use crate::broker_surface::{BrokerSurface, OrderConfirmation, OrderRequest};
use crate::executor::ExecutorError;
use crate::virtuals::{DegenArenaSurface, HyperliquidApi, ReqwestHyperliquidApi};

/// `BrokerSurface` for the plain native Hyperliquid perps venue.
///
/// Delegates all order mechanics to an inner [`DegenArenaSurface`] (same
/// native EIP-712 signing) but reports `venue() == "hyperliquid"` and
/// `is_perp_venue() == true`. See the module-level doc for the forward-compat
/// caveat about when delegation must stop.
pub struct HyperliquidSurface<A = ReqwestHyperliquidApi> {
    inner: DegenArenaSurface<A>,
}

impl HyperliquidSurface<ReqwestHyperliquidApi> {
    /// Build from environment variables:
    /// - `HL_API_KEY` — trade-only HL agent-wallet private key (`0x…`).
    /// - `HL_ACCOUNT_ADDRESS` — master account address (for reads).
    /// - `HL_NETWORK` — `mainnet` (default) or `testnet`.
    pub fn from_env() -> Result<Self, ExecutorError> {
        let key =
            std::env::var("HL_API_KEY").map_err(|_| ExecutorError::Auth("HL_API_KEY not set".into()))?;
        let addr = std::env::var("HL_ACCOUNT_ADDRESS")
            .map_err(|_| ExecutorError::Auth("HL_ACCOUNT_ADDRESS not set".into()))?;
        let network = std::env::var("HL_NETWORK").unwrap_or_else(|_| "mainnet".into());
        Self::from_credentials(&key, &addr, &network)
    }

    /// Build from explicit credentials. `network` containing "testnet" selects
    /// the testnet host; anything else is mainnet. Mirrors
    /// `DegenArenaSurface::from_credentials` exactly — the signing path is
    /// identical.
    pub fn from_credentials(
        api_key: &str,
        account_address: &str,
        network: &str,
    ) -> Result<Self, ExecutorError> {
        Ok(Self {
            inner: DegenArenaSurface::from_credentials(api_key, account_address, network)?,
        })
    }
}

impl<A: HyperliquidApi> HyperliquidSurface<A> {
    /// Build from any [`HyperliquidApi`] impl. Used by tests with mocks.
    pub fn with_api(api: A) -> Self {
        Self {
            inner: DegenArenaSurface::with_api(api),
        }
    }
}

#[async_trait]
impl<A: HyperliquidApi + 'static> BrokerSurface for HyperliquidSurface<A> {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        self.inner.submit_order(req).await
    }

    async fn position(&self, asset: &str) -> anyhow::Result<f64> {
        self.inner.position(asset).await
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        self.inner.balance().await
    }

    /// `"hyperliquid"` — distinguishes this venue from the Degen Arena product
    /// layer and from generic `"live"` mocks. Stamped onto every live trace
    /// event so operators can tell the two native-HL venues apart at a glance.
    fn venue(&self) -> &str {
        "hyperliquid"
    }

    /// `"eip712"` — native EIP-712 L1-action signing (no subprocess).
    fn signing_scheme(&self) -> &str {
        "eip712"
    }

    /// `true` — Hyperliquid trades directional perpetual futures.
    fn is_perp_venue(&self) -> bool {
        true
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::broker_surface::Side;
    use crate::virtuals::{HlOrderAck, HlPosition, MockHyperliquidApi};

    fn buy_req(asset: &str, size: f64) -> OrderRequest {
        OrderRequest {
            asset: asset.into(),
            side: Side::Buy,
            size,
            reference_price_usd: 70_000.0,
            stop_loss_pct: None,
            take_profit_pct: None,
            idempotency_key: "cycle-hl-test".into(),
        }
    }

    // DoD 4: venue() == "hyperliquid"
    #[test]
    fn venue_is_hyperliquid() {
        let surface = HyperliquidSurface::with_api(MockHyperliquidApi::default());
        assert_eq!(surface.venue(), "hyperliquid");
    }

    // DoD 4: signing_scheme is "eip712"
    #[test]
    fn signing_scheme_is_eip712() {
        let surface = HyperliquidSurface::with_api(MockHyperliquidApi::default());
        assert_eq!(surface.signing_scheme(), "eip712");
    }

    // DoD 4: is_perp_venue is true
    #[test]
    fn is_perp_venue_is_true() {
        let surface = HyperliquidSurface::with_api(MockHyperliquidApi::default());
        assert!(surface.is_perp_venue(), "Hyperliquid is a perp venue");
    }

    // DoD 4: submit_order delegates to DegenArenaSurface (uses MockHyperliquidApi seam)
    #[tokio::test]
    async fn submit_order_delegates_to_inner() {
        let surface = HyperliquidSurface::with_api(MockHyperliquidApi {
            ack: Some(HlOrderAck {
                oid: 42,
                status: "filled".into(),
                avg_px: Some(70_000.0),
                filled_sz: 0.05,
            }),
            ..Default::default()
        });
        let conf = surface.submit_order(buy_req("BTC/USD", 0.05)).await.unwrap();
        assert_eq!(conf.fill_size, 0.05);
        assert_eq!(conf.fill_price, Some(70_000.0));
        // Still reports hyperliquid as the venue
        assert_eq!(surface.venue(), "hyperliquid");
    }

    // DoD 4: position delegates (returns signed position from mock)
    #[tokio::test]
    async fn position_delegates() {
        let surface = HyperliquidSurface::with_api(MockHyperliquidApi {
            positions: vec![HlPosition {
                coin: "BTC".into(),
                szi: 0.5,
            }],
            ..Default::default()
        });
        let pos = surface.position("BTC/USD").await.unwrap();
        assert!((pos - 0.5).abs() < 1e-9);
    }

    // DoD 4: balance delegates
    #[tokio::test]
    async fn balance_delegates() {
        let surface = HyperliquidSurface::with_api(MockHyperliquidApi {
            equity: 5000.0,
            ..Default::default()
        });
        let bal = surface.balance().await.unwrap();
        assert!((bal - 5000.0).abs() < 1e-9);
    }

    // Shared pointer works (Arc<dyn BrokerSurface>)
    #[test]
    fn can_wrap_in_arc_dyn_broker_surface() {
        let surface: Arc<dyn BrokerSurface> =
            Arc::new(HyperliquidSurface::with_api(MockHyperliquidApi::default()));
        assert_eq!(surface.venue(), "hyperliquid");
        assert!(surface.is_perp_venue());
    }
}
