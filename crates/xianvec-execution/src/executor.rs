//! Phase 6.1 — `Executor` trait and shared types.
//!
//! The trait is implemented by:
//! - `xianvec_execution::alpaca::AlpacaExecutor` — live paper-trading on Alpaca
//! - `xianvec_execution::orderly::OrderlyExecutor` — live perps on Orderly/Mantle (Phase 6.3)
//! - `xianvec_eval::backtest::BacktestExecutor` — stateful in-process simulator
//!
//! Idempotency: every `submit` call carries the `RiskDecision`'s underlying
//! `setup_id` (via `RiskDecision::effective().setup_id`); executors must use
//! that as their venue-side client order id so duplicate retries collapse to a
//! single fill.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use xianvec_core::{AssetSymbol, PortfolioState, RiskDecision};

/// One executor-side outcome record. Persisted to the `execution_receipts`
/// table for reconciliation between trader intent and venue fill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    /// The setup_id that drove this submit. Mirrored from the RiskDecision's
    /// effective TraderDecision; used as venue-side client order id.
    pub setup_id: Uuid,
    /// `"alpaca"`, `"orderly"`, or `"backtest"`.
    pub venue: String,
    /// Venue-assigned id (server order id, sim sequence number, etc.).
    pub venue_order_id: String,
    pub asset: AssetSymbol,
    /// Filled size in basis points of NAV at submit time. Zero means rejected
    /// or fully unfilled (caller decides whether that is an error).
    pub filled_size_bps: u32,
    /// Average fill price in quote currency (USD-equivalent for v1).
    pub avg_fill_price: f64,
    /// Realised fee in basis points of notional.
    pub fee_bps: u32,
    pub submitted_at: DateTime<Utc>,
    pub filled_at: Option<DateTime<Utc>>,
    /// Free-form note from the executor (e.g. partial-fill explanation,
    /// slippage estimate, vetoed-at-venue reason).
    #[serde(default)]
    pub note: Option<String>,
}

/// Errors any executor may surface. Network / venue-specific errors are
/// stringified; downstream code should not pattern-match on the inner string.
#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("network: {0}")]
    Network(String),
    /// Venue rejected the order (insufficient balance, market closed, …).
    #[error("rejected by venue: {0}")]
    Rejected(String),
    /// The decision passed risk but the executor refuses to execute it
    /// (e.g. a Vetoed decision was forwarded by mistake).
    #[error("decision not actionable: {0}")]
    NotActionable(String),
    /// Venue timeout / unavailable; caller may retry.
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("auth: {0}")]
    Auth(String),
    #[error("io: {0}")]
    Io(String),
    #[error("internal: {0}")]
    Internal(String),
}

#[async_trait]
pub trait Executor: Send + Sync {
    /// Submit a `RiskDecision` to the venue. Vetoed decisions return
    /// `Err(NotActionable)`; Approved or Modified decisions submit the
    /// `effective()` TraderDecision.
    async fn submit(&self, decision: &RiskDecision) -> Result<ExecutionReceipt, ExecutorError>;

    /// Close any open position in `asset`. No-op (returns a zero-fill receipt)
    /// if there is no open position. Implementations that lack a native
    /// "close" primitive (Orderly) submit an opposing market order.
    async fn close_position(&self, asset: AssetSymbol) -> Result<ExecutionReceipt, ExecutorError>;

    /// Read the live portfolio state from the venue. Implementations should
    /// return a fresh value (no caching) — caching is the caller's choice.
    async fn portfolio(&self) -> Result<PortfolioState, ExecutorError>;
}
