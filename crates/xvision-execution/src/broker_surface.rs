//! Unified broker surface — extracted from Plan 2c §Task 7 for the v1
//! eval engine plan to depend on without pulling in the deferred scheduler
//! / live daemon. Wraps the existing `alpaca` and `orderly` modules behind a
//! single trait + enum so callers (eval paper executor, future live daemon)
//! pick a broker at runtime.
//!
//! The trait is intentionally narrower than the existing `Executor` trait
//! (which takes a fully-formed `RiskDecision`). Callers that already have a
//! pipeline-driven decision should keep using `Executor::submit`. Callers
//! that just need to "place this order at the broker" — e.g. eval paper
//! mode, or a future deterministic test harness — use `BrokerSurface`.
//!
//! See `docs/superpowers/plans/2026-05-08-strategy-engine-2c-scheduler-live-exec.md#task-7-brokersurface-trait--dispatch`
//! for the original spec, and `team/briefings/broker-surface.md` for the v1
//! scope cut (Alpaca paper surface fully implemented; live surfaces stubbed).

use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use xvision_core::AssetSymbol;

use crate::alpaca::{AlpacaApi, ApacClientApi, OrderRequest as ApacOrderRequest, OrderSide as ApacSide};

/// Returns `true` when `asset` parses as an Alpaca crypto whitelist symbol
/// (`"BTC"`, `"BTC/USD"`, `"BTCUSD"`, etc.). Used by `AlpacaPaperSurface` and
/// downstream callers (the eval paper executor) to gate behaviour that
/// differs between Alpaca's crypto and (future) equities API surfaces:
///
/// - Crypto orders never use bracket / OCO / OTOCO classes — only simple
///   market or limit orders. Bracket take-profit / stop-loss legs are
///   silently dropped before submission.
/// - Crypto is long-only on Alpaca; opening a short position from flat is
///   not supported. Callers should refuse `short_open` for crypto assets
///   rather than round-tripping through the API.
///
/// Non-crypto symbols (e.g. a future `"AAPL"` equity) return `false` and
/// take the legacy bracket + bidirectional code path.
pub fn is_alpaca_crypto(asset: &str) -> bool {
    AssetSymbol::from_str(asset).is_ok()
}

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrokerKind {
    AlpacaPaper,
    AlpacaLive,
    OrderlyLive,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrderRequest {
    pub asset: String,
    pub side: Side,
    /// Base-asset units (e.g., 0.05 BTC). The broker impl converts to
    /// whatever unit its API requires (notional for Alpaca, base qty for
    /// Orderly).
    pub size: f64,
    /// Reference price chosen by the caller for base-size to notional
    /// conversion and bracket-leg derivation. Paper evals pass the current
    /// historical replay bar close here so execution is tied to the scenario,
    /// not to live quotes.
    pub reference_price_usd: f64,
    pub stop_loss_pct: Option<f32>,
    pub take_profit_pct: Option<f32>,
    /// Echoed to the broker as `client_order_id`. Brokers dedupe on this so
    /// duplicate retries collapse to a single fill.
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderConfirmation {
    pub broker_order_id: String,
    pub fill_price: Option<f64>,
    pub fill_size: f64,
    pub fee: Option<f64>,
}

// ── Trait ────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait BrokerSurface: Send + Sync {
    /// Submit an order. Polls the broker until the order reaches a terminal
    /// state (filled / canceled / rejected) before returning.
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation>;

    /// Current position size in base-asset units (signed: positive = long,
    /// negative = short). Returns 0.0 when no position is open.
    async fn position(&self, asset: &str) -> anyhow::Result<f64>;

    /// Account equity in USD.
    async fn balance(&self) -> anyhow::Result<f64>;
}

// ── AlpacaPaperSurface ───────────────────────────────────────────────────────

/// Wraps the existing `alpaca::AlpacaApi` (paper environment) behind the
/// unified `BrokerSurface`. Used by eval paper mode and any caller that
/// wants Alpaca paper without going through the existing `RiskDecision`-
/// shaped `Executor` trait.
pub struct AlpacaPaperSurface {
    api: Arc<dyn AlpacaApi>,
}

impl AlpacaPaperSurface {
    /// Build from any `AlpacaApi` impl. Used by tests with mocks.
    pub fn with_api(api: Arc<dyn AlpacaApi>) -> Self {
        Self { api }
    }

    /// Build from environment variables (`APCA_API_KEY_ID`,
    /// `APCA_API_SECRET_KEY`, `APCA_API_BASE_URL`). Falls back to Alpaca
    /// paper-trading URL if `APCA_API_BASE_URL` is absent. Production entry
    /// point for eval paper mode.
    pub fn from_env() -> anyhow::Result<Self> {
        let api_info =
            apca::ApiInfo::from_env().map_err(|e| anyhow::anyhow!("alpaca ApiInfo::from_env: {e}"))?;
        let client = apca::Client::new(api_info);
        Ok(Self {
            api: Arc::new(ApacClientApi::new(client)),
        })
    }

    /// Build from explicit credentials. Useful for tests that hit the real
    /// paper API without relying on the process environment.
    pub fn from_credentials(key_id: &str, secret: &str, base_url: &str) -> anyhow::Result<Self> {
        let api_info = apca::ApiInfo::from_parts(base_url, key_id, secret)
            .map_err(|e| anyhow::anyhow!("alpaca ApiInfo::from_parts: {e}"))?;
        let client = apca::Client::new(api_info);
        Ok(Self {
            api: Arc::new(ApacClientApi::new(client)),
        })
    }
}

const ALPACA_FILL_POLL_MAX: u32 = 5;
const ALPACA_FILL_POLL_DELAY_MS: u64 = 200;

#[async_trait]
impl BrokerSurface for AlpacaPaperSurface {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        // 1. Use the caller-selected reference price for size→notional
        //    conversion + bracket leg derivation. Paper evals source this
        //    from the historical replay bar so a flat Alpaca account does
        //    not force a live quote or a hard-coded BTC price.
        let reference_price = if req.reference_price_usd > 0.0 && req.reference_price_usd.is_finite() {
            req.reference_price_usd
        } else {
            anyhow::bail!(
                "alpaca paper order missing positive reference_price_usd for {}",
                req.asset
            );
        };

        // 2. Crypto pre-flight. Alpaca's crypto API only accepts simple
        //    market/limit orders and is long-only; selling from flat is
        //    rejected by the server and selling more than the open long
        //    would net into a short on fill. We refuse both cases here
        //    with a classifier-friendly message ("broker_unsupported")
        //    so the eval executor surfaces a clean class tag if it ever
        //    reaches the surface. The eval paper executor sizes
        //    crypto sells against the open long before this point in
        //    normal operation; this guard is the hard backstop.
        let asset_is_crypto = is_alpaca_crypto(&req.asset);
        if asset_is_crypto && matches!(req.side, Side::Sell) {
            let current_position = self
                .api
                .get_position(&req.asset)
                .await
                .map_err(|e| anyhow::anyhow!("alpaca get_position: {e}"))?
                .map(|p| if p.side == "long" { p.qty } else { -p.qty })
                .unwrap_or(0.0);
            if current_position <= 0.0 {
                anyhow::bail!(
                    "alpaca crypto broker_unsupported: short_open is not supported for {} (asset is not shortable on Alpaca crypto)",
                    req.asset
                );
            }
            if req.size > current_position {
                anyhow::bail!(
                    "alpaca crypto broker_unsupported: sell size {} exceeds open long position {} for {} (would net into a short, which Alpaca crypto does not support)",
                    req.size,
                    current_position,
                    req.asset
                );
            }
        }

        let notional = req.size * reference_price;
        tracing::debug!(
            target: "xvision::alpaca",
            asset = %req.asset,
            side = ?req.side,
            size = req.size,
            reference_price_usd = reference_price,
            reference_price_source = "eval_bar",
            notional,
            "alpaca paper order notional resolved"
        );

        // 3. Bracket legs — only for non-crypto. Alpaca's crypto API rejects
        //    `Class::Bracket` outright; submit a simple market order instead.
        //    Strategies that wire stop/tp percentages still see them at
        //    decision time, but they are not enforced server-side for
        //    crypto (a follow-up track can model client-side TP/SL
        //    tracking if needed).
        let (take_profit_price, stop_loss_price) = if asset_is_crypto {
            if req.take_profit_pct.is_some() || req.stop_loss_pct.is_some() {
                tracing::debug!(
                    target: "xvision::alpaca",
                    asset = %req.asset,
                    "alpaca crypto does not support bracket orders; dropping take_profit / stop_loss legs"
                );
            }
            (None, None)
        } else {
            match (req.take_profit_pct, req.stop_loss_pct) {
                (Some(tp_pct), Some(sl_pct)) => match req.side {
                    Side::Buy => (
                        Some(reference_price * (1.0 + tp_pct as f64 / 100.0)),
                        Some(reference_price * (1.0 - sl_pct as f64 / 100.0)),
                    ),
                    Side::Sell => (
                        Some(reference_price * (1.0 - tp_pct as f64 / 100.0)),
                        Some(reference_price * (1.0 + sl_pct as f64 / 100.0)),
                    ),
                },
                _ => (None, None),
            }
        };

        // 3. Submit via existing AlpacaApi.
        let apac_req = ApacOrderRequest {
            symbol: req.asset.clone(),
            notional,
            side: match req.side {
                Side::Buy => ApacSide::Buy,
                Side::Sell => ApacSide::Sell,
            },
            take_profit_price,
            stop_loss_price,
            client_order_id: req.idempotency_key.clone(),
        };

        let order = self
            .api
            .create_order(apac_req)
            .await
            .map_err(|e| anyhow::anyhow!("alpaca create_order: {e}"))?;
        let order_id = order.id.clone();

        // 4. Poll for terminal state.
        let filled = self.await_fill(&order_id).await?;

        Ok(OrderConfirmation {
            broker_order_id: filled.id,
            fill_price: filled.avg_fill_price,
            fill_size: filled.filled_qty,
            fee: None, // Alpaca paper has no fee model in v1
        })
    }

    async fn position(&self, asset: &str) -> anyhow::Result<f64> {
        let pos = self
            .api
            .get_position(asset)
            .await
            .map_err(|e| anyhow::anyhow!("alpaca get_position: {e}"))?;
        Ok(pos
            .map(|p| if p.side == "long" { p.qty } else { -p.qty })
            .unwrap_or(0.0))
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        let acct = self
            .api
            .get_account()
            .await
            .map_err(|e| anyhow::anyhow!("alpaca get_account: {e}"))?;
        Ok(acct.equity)
    }
}

impl AlpacaPaperSurface {
    async fn await_fill(&self, order_id: &str) -> anyhow::Result<crate::alpaca::AlpacaOrder> {
        for _ in 0..ALPACA_FILL_POLL_MAX {
            let order = self
                .api
                .get_order(order_id)
                .await
                .map_err(|e| anyhow::anyhow!("alpaca get_order: {e}"))?;
            if order.is_terminal() {
                if order.is_rejected() {
                    return Err(anyhow::anyhow!("alpaca order {order_id} rejected"));
                }
                return Ok(order);
            }
            tokio::time::sleep(Duration::from_millis(ALPACA_FILL_POLL_DELAY_MS)).await;
        }
        Err(anyhow::anyhow!(
            "alpaca order {order_id} did not fill within {} polls",
            ALPACA_FILL_POLL_MAX
        ))
    }
}

// ── AlpacaLiveSurface — stubbed for v1 ───────────────────────────────────────

/// Placeholder for Alpaca live trading. v1 test ships paper-only; this
/// type exists so `BrokerKind::AlpacaLive` can be matched without compile
/// errors. Activation requires a non-paper `APCA_API_BASE_URL` and explicit
/// operator opt-in — not in scope until post-v1.
pub struct AlpacaLiveSurface;

#[async_trait]
impl BrokerSurface for AlpacaLiveSurface {
    async fn submit_order(&self, _req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        Err(anyhow::anyhow!(
            "AlpacaLiveSurface is stubbed for v1. Use AlpacaPaperSurface for v1 test scope."
        ))
    }

    async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
        Err(anyhow::anyhow!("AlpacaLiveSurface stubbed"))
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        Err(anyhow::anyhow!("AlpacaLiveSurface stubbed"))
    }
}

// ── OrderlyLiveSurface — stubbed for v1 ──────────────────────────────────────

/// Placeholder for Orderly Network live trading. v1 test ships Alpaca paper
/// only; the existing `OrderlyExecutor` covers the live path through the
/// `Executor` trait. A future plan can wire a thin `BrokerSurface` impl over
/// the existing `OrderlyApi` trait the same way `AlpacaPaperSurface` does
/// over `AlpacaApi`.
pub struct OrderlyLiveSurface;

#[async_trait]
impl BrokerSurface for OrderlyLiveSurface {
    async fn submit_order(&self, _req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        Err(anyhow::anyhow!(
            "OrderlyLiveSurface is stubbed for v1 BrokerSurface. \
             Use xvision_execution::OrderlyExecutor for live Orderly trading via the Executor trait."
        ))
    }

    async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
        Err(anyhow::anyhow!("OrderlyLiveSurface stubbed"))
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        Err(anyhow::anyhow!("OrderlyLiveSurface stubbed"))
    }
}

// ── MockBrokerSurface ────────────────────────────────────────────────────────

/// Deterministic in-memory `BrokerSurface` for downstream tests
/// (eval engine, wizard, etc.). Records every submission, fills at a fake
/// constant price (`70_000.0` if no override), tracks per-asset position,
/// and never hits the network.
///
/// Public so downstream crates can use it without re-implementing it.
pub struct MockBrokerSurface {
    state: Mutex<MockState>,
    fill_price: f64,
}

#[derive(Default)]
struct MockState {
    balance: f64,
    submitted: Vec<OrderRequest>,
    positions: std::collections::HashMap<String, f64>,
}

impl MockBrokerSurface {
    /// New mock seeded with `balance` USD and zero positions. Default fill
    /// price is 70_000.0; override with [`with_fill_price`].
    pub fn new(balance: f64) -> Self {
        Self {
            state: Mutex::new(MockState {
                balance,
                ..Default::default()
            }),
            fill_price: 70_000.0,
        }
    }

    pub fn with_fill_price(mut self, fill_price: f64) -> Self {
        self.fill_price = fill_price;
        self
    }

    /// Returns a clone of every order ever submitted to this mock.
    pub fn submitted(&self) -> Vec<OrderRequest> {
        self.state.lock().unwrap().submitted.clone()
    }
}

#[async_trait]
impl BrokerSurface for MockBrokerSurface {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let mut s = self.state.lock().unwrap();
        s.submitted.push(req.clone());

        let signed = match req.side {
            Side::Buy => req.size,
            Side::Sell => -req.size,
        };
        *s.positions.entry(req.asset.clone()).or_insert(0.0) += signed;

        Ok(OrderConfirmation {
            broker_order_id: format!("mock-{}", req.idempotency_key),
            fill_price: Some(req.reference_price_usd),
            fill_size: req.size,
            fee: None,
        })
    }

    async fn position(&self, asset: &str) -> anyhow::Result<f64> {
        let s = self.state.lock().unwrap();
        Ok(s.positions.get(asset).copied().unwrap_or(0.0))
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        Ok(self.state.lock().unwrap().balance)
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn is_alpaca_crypto_accepts_whitelist_forms() {
        for ok in [
            "BTC", "BTC/USD", "BTCUSD", "btc", "btc/usd", "ETH/USD", "SOL/USD", "DOGE", "USDC/USD",
        ] {
            assert!(is_alpaca_crypto(ok), "{ok} must be classified as crypto");
        }
    }

    #[test]
    fn is_alpaca_crypto_rejects_equities_and_unknown_symbols() {
        for not_ok in ["", "AAPL", "TSLA", "SPY", "XRP", "XRP/USD"] {
            assert!(
                !is_alpaca_crypto(not_ok),
                "{not_ok} must NOT be classified as crypto"
            );
        }
    }
}
