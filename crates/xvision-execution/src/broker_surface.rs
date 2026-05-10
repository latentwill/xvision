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

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::alpaca::{AlpacaApi, OrderRequest as ApacOrderRequest, OrderSide as ApacSide};

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
    /// Build from any `AlpacaApi` impl. In production: `ApacClientApi`
    /// constructed from env via `AlpacaExecutor::from_env()` (then peel the
    /// `api` field off — or add a constructor on `AlpacaExecutor` to expose
    /// a `BrokerSurface`). In tests: any custom mock.
    pub fn with_api(api: Arc<dyn AlpacaApi>) -> Self {
        Self { api }
    }
}

const ALPACA_FILL_POLL_MAX: u32 = 5;
const ALPACA_FILL_POLL_DELAY_MS: u64 = 200;

#[async_trait]
impl BrokerSurface for AlpacaPaperSurface {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        // 1. Resolve a reference price for size→notional conversion + bracket
        //    leg derivation. Prefer the open position's current_price; fall
        //    back to avg_entry_price; if neither, error out — without a price
        //    we can't size the notional safely.
        let pos = self.api.get_position(&req.asset).await.map_err(|e| {
            anyhow::anyhow!("alpaca get_position({}) failed: {e}", req.asset)
        })?;
        let reference_price = pos
            .as_ref()
            .and_then(|p| p.current_price)
            .or_else(|| pos.as_ref().map(|p| p.avg_entry_price))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "alpaca: cannot derive reference price for {} (no open position and no quote)",
                    req.asset
                )
            })?;

        let notional = req.size * reference_price;

        // 2. Bracket legs.
        let (take_profit_price, stop_loss_price) = match (req.take_profit_pct, req.stop_loss_pct) {
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
        Ok(pos.map(|p| {
            if p.side == "long" {
                p.qty
            } else {
                -p.qty
            }
        })
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
    async fn await_fill(
        &self,
        order_id: &str,
    ) -> anyhow::Result<crate::alpaca::AlpacaOrder> {
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
            fill_price: Some(self.fill_price),
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
