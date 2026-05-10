//! Phase 8 backtest output shapes. Pinned upfront so Phase 7 (baselines) and
//! Phase 8 (harness + metrics) can compile against the same surface in
//! parallel.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use xvision_core::trading::{Regime, RiskDecision, TraderDecision};
use xvision_execution::ExecutionReceipt;

/// One sample point on an arm's equity curve. NAV is in USD.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EquityPoint {
    pub timestamp: DateTime<Utc>,
    pub nav_usd: f64,
}

/// One arm's full result over the backtest window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmResult {
    pub name: String,
    pub equity_curve: Vec<EquityPoint>,
    pub fills: Vec<ExecutionReceipt>,
    pub decisions: Vec<TraderDecision>,
    pub risk_outcomes: Vec<RiskDecision>,
    /// Per-setup return r_i = pnl_i / nav_initial (Tier 1 fix #8 — constant
    /// denominator, order-invariant).
    pub returns: Vec<f32>,
    pub realized_pnl_total_usd: f64,
    /// Per-setup regime label, parallel to `decisions` / `returns`. Lets the
    /// anti-overfit gate stratify by regime without re-walking the snapshots.
    pub regimes: Vec<Regime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub arms: BTreeMap<String, ArmResult>,
    pub cycles_evaluated: usize,
    pub initial_nav_usd: f64,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
}
