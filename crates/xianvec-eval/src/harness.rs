//! Phase 8.2 — `BacktestRunner` drives multiple `Strategy` arms through a
//! synchronised OHLCV window via independent `BacktestExecutor` instances.
//!
//! ## Design notes
//! - Each arm gets its own `BacktestExecutor` with independent portfolio state
//!   (Tier 1 fix #1: pairing is performed upstream via the cached briefing /
//!   snapshot; the executor portfolios stay independent).
//! - `step_hours >= horizon_hours` is enforced at construction time
//!   (Tier 1 fix #4).
//! - Returns are `pnl_i / nav_initial` (Tier 1 fix #8 — constant denominator).

use std::collections::BTreeMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use xianvec_core::market::MarketSnapshot;
use xianvec_core::trading::{AssetSymbol, RiskDecision, TraderDecision, Regime};
use xianvec_execution::{ExecutionReceipt, Executor};
use xianvec_risk::RiskLayer;

use crate::backtest::{BacktestConfig, BacktestExecutor, MarketBar};
use crate::result::{ArmResult, BacktestResult, EquityPoint};
use crate::strategy::Strategy;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum HarnessError {
    #[error(
        "step_hours ({step}) must be >= horizon_hours ({horizon}) — Tier 1 fix #4"
    )]
    StepLessThanHorizon { step: u32, horizon: u32 },
    #[error("bars must cover snapshot timestamps")]
    BarCoverage,
    #[error("executor: {0}")]
    Executor(String),
}

impl From<xianvec_execution::ExecutorError> for HarnessError {
    fn from(e: xianvec_execution::ExecutorError) -> Self {
        HarnessError::Executor(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

/// Per-arm wrapper: name + strategy.
pub struct ArmConfig {
    pub name: String,
    pub strategy: Box<dyn Strategy>,
}

/// Static configuration for a multi-arm backtest run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestRunConfig {
    pub initial_nav_usd: f64,
    pub fee_bps: u32,
    pub slippage_atr_frac: f64,
    pub instrument: AssetSymbol,
    /// Minimum gap between consecutive decision snapshots. Must be >=
    /// `horizon_hours` (Tier 1 fix #4).
    pub step_hours: u32,
    /// Forward-looking evaluation horizon; mirrors `MarketSnapshot::horizon_hours`.
    pub horizon_hours: u32,
    /// Held for `compute_pre_committed`; not consumed inside `run()`.
    pub n_bootstrap_resamples: usize,
    pub block_size: Option<usize>,
}

// ---------------------------------------------------------------------------
// Per-arm mutable state during the run
// ---------------------------------------------------------------------------

struct ArmState {
    exec: BacktestExecutor,
    decisions: Vec<TraderDecision>,
    risk_outcomes: Vec<RiskDecision>,
    fills: Vec<ExecutionReceipt>,
    equity_curve: Vec<EquityPoint>,
    /// Per-setup PnL: NAV_after_horizon - NAV_before_decision (approx).
    pnl_per_setup: Vec<f32>,
    regimes: Vec<Regime>,
    /// Bar cursor: index of the first bar NOT yet ticked into this executor.
    bar_cursor: usize,
}

// ---------------------------------------------------------------------------
// BacktestRunner
// ---------------------------------------------------------------------------

/// Multi-arm backtest runner.
pub struct BacktestRunner {
    pub config: BacktestRunConfig,
    arms: Vec<ArmConfig>,
}

impl BacktestRunner {
    /// Construct a new runner. Returns `HarnessError::StepLessThanHorizon`
    /// when `config.step_hours < config.horizon_hours`.
    pub fn new(config: BacktestRunConfig, arms: Vec<ArmConfig>) -> Result<Self, HarnessError> {
        if config.step_hours < config.horizon_hours {
            return Err(HarnessError::StepLessThanHorizon {
                step: config.step_hours,
                horizon: config.horizon_hours,
            });
        }
        Ok(Self { config, arms })
    }

    /// Run all arms over the provided `snapshots` (decision points) and `bars`
    /// (the OHLCV time grid that the `BacktestExecutor` is ticked through).
    ///
    /// The bars must fully cover all snapshot timestamps; otherwise
    /// `HarnessError::BarCoverage` is returned.
    ///
    /// ## Run-loop
    /// For each snapshot:
    /// 1. Every arm decides (`strategy.decide`).
    /// 2. The decision (if `Some`) is passed through the `RiskLayer`.
    /// 3. If approved or modified, it is submitted to that arm's executor.
    /// 4. After all arms decide on this snapshot, every executor is ticked
    ///    through all bars between this snapshot and the next (or through the
    ///    remaining bars for the last snapshot).
    /// 5. After the final tick, the executor's NAV is sampled for the equity
    ///    curve.
    pub async fn run(
        &mut self,
        snapshots: &[MarketSnapshot],
        bars: &[MarketBar],
        risk: &RiskLayer,
    ) -> Result<BacktestResult, HarnessError> {
        let started_at = Utc::now();
        let nav_initial = self.config.initial_nav_usd;

        if bars.is_empty() || snapshots.is_empty() {
            let arm_results: BTreeMap<String, ArmResult> = self
                .arms
                .iter()
                .map(|a| {
                    (
                        a.name.clone(),
                        ArmResult {
                            name: a.name.clone(),
                            equity_curve: vec![],
                            fills: vec![],
                            decisions: vec![],
                            risk_outcomes: vec![],
                            returns: vec![],
                            realized_pnl_total_usd: 0.0,
                            regimes: vec![],
                        },
                    )
                })
                .collect();
            return Ok(BacktestResult {
                arms: arm_results,
                setups_evaluated: 0,
                initial_nav_usd: nav_initial,
                started_at,
                finished_at: Utc::now(),
            });
        }

        // Map each snapshot to the bar index with timestamp >= snapshot.timestamp
        let snap_bar_indices: Vec<usize> = snapshots
            .iter()
            .map(|snap| {
                bars.iter()
                    .position(|b| b.timestamp >= snap.timestamp)
                    .unwrap_or(bars.len())
            })
            .collect();

        for &idx in &snap_bar_indices {
            if idx >= bars.len() {
                return Err(HarnessError::BarCoverage);
            }
        }

        let exec_cfg = BacktestConfig {
            initial_equity_usd: nav_initial,
            instrument: self.config.instrument,
            fee_bps: self.config.fee_bps,
            slippage_atr_frac: self.config.slippage_atr_frac,
            max_history_days: 30,
        };

        let opening_bar = bars[0].clone();

        // Initialise per-arm state
        let mut arm_states: Vec<ArmState> = self
            .arms
            .iter()
            .map(|_| {
                let exec = BacktestExecutor::new(exec_cfg.clone(), opening_bar.clone());
                let initial_nav = exec.portfolio_snapshot().equity_usd;
                let equity_curve = vec![EquityPoint {
                    timestamp: opening_bar.timestamp,
                    nav_usd: initial_nav,
                }];
                ArmState {
                    exec,
                    decisions: Vec::new(),
                    risk_outcomes: Vec::new(),
                    fills: Vec::new(),
                    equity_curve,
                    pnl_per_setup: Vec::new(),
                    regimes: Vec::new(),
                    bar_cursor: 1, // bar[0] is the opening bar passed to ::new
                }
            })
            .collect();

        for (snap_i, snapshot) in snapshots.iter().enumerate() {
            let _bar_idx = snap_bar_indices[snap_i];
            let next_bar_idx = if snap_i + 1 < snapshots.len() {
                snap_bar_indices[snap_i + 1]
            } else {
                bars.len()
            };

            // --- Decision phase: all arms decide on this snapshot ---
            for (arm_idx, arm) in self.arms.iter().enumerate() {
                let st = &mut arm_states[arm_idx];
                let nav_before = st.exec.portfolio_snapshot().equity_usd;

                let maybe_decision = arm.strategy.decide(snapshot).await;
                if let Some(td) = maybe_decision {
                    st.decisions.push(td.clone());
                    st.regimes.push(snapshot.regime);

                    let portfolio = st.exec.portfolio_snapshot();
                    let risk_outcome =
                        risk.evaluate(td, &portfolio, self.config.instrument);
                    st.risk_outcomes.push(risk_outcome.clone());

                    if risk_outcome.effective().is_some() {
                        match st.exec.submit(&risk_outcome).await {
                            Ok(receipt) => st.fills.push(receipt),
                            Err(xianvec_execution::ExecutorError::NotActionable(_)) => {}
                            Err(e) => return Err(HarnessError::Executor(e.to_string())),
                        }
                    }

                    // Record NAV before this setup for PnL delta calculation
                    // We will compute the delta after ticking through the horizon bars.
                    // Store nav_before as a sentinel — we'll append PnL after ticking.
                    // Use a side-channel vec to track pending PnL computations.
                    let _ = nav_before; // will be read via portfolio_snapshot after ticks
                }
            }

            // --- Tick phase: advance all executors through bars to next snapshot ---
            let tick_end = next_bar_idx.min(bars.len());
            for arm_state in arm_states.iter_mut() {
                let nav_before_tick = arm_state.exec.portfolio_snapshot().equity_usd;
                let cursor_start = arm_state.bar_cursor;

                for bar in bars.iter().take(tick_end).skip(cursor_start) {
                    let tick_result = arm_state
                        .exec
                        .tick(bar.clone())
                        .map_err(|e| HarnessError::Executor(e.to_string()))?;
                    for fill in tick_result.auto_filled_receipts {
                        arm_state.fills.push(fill);
                    }
                    let nav = arm_state.exec.portfolio_snapshot().equity_usd;
                    arm_state.equity_curve.push(EquityPoint {
                        timestamp: bar.timestamp,
                        nav_usd: nav,
                    });
                }
                arm_state.bar_cursor = tick_end;

                // Compute per-setup PnL as NAV change over this decision's horizon.
                // We only push a PnL entry if this arm made a decision at this snapshot.
                // Check: decisions grew since last snapshot → yes, use NAV delta.
                let nav_after_tick = arm_state.exec.portfolio_snapshot().equity_usd;
                let pnl = (nav_after_tick - nav_before_tick) as f32;
                // We pushed pnl_per_setup for every snapshot where a decision occurred.
                // The regimes vec tracks this: same length as decisions.
                // We need to match pnl_per_setup length to decisions length.
                // Since we tick after decisions, we push here if a decision was made.
                // Use the decisions length delta as a signal.
                let _ = pnl; // Handled in the snapshot-level logic below
            }

            // Push PnL entries for arms that made a decision at this snapshot
            // (decisions.len() increased by 1 for those arms)
            for (arm_idx, arm) in self.arms.iter().enumerate() {
                let st = &mut arm_states[arm_idx];
                let snap_decision_count = st.regimes.len(); // regimes grows in lock-step with decisions
                // If pnl_per_setup is shorter than decisions, we owe a PnL entry
                if st.pnl_per_setup.len() < snap_decision_count {
                    // Find the nav at the start of this snap's tick window
                    // We already ticked — read current NAV
                    let nav_now = st.exec.portfolio_snapshot().equity_usd;
                    // Approximate: NAV change since previous decision count
                    // For simplicity, track cumulative PnL and take the delta
                    let cumulative_pnl: f32 = st.pnl_per_setup.iter().sum::<f32>();
                    let total_pnl = (nav_now - nav_initial) as f32;
                    let this_setup_pnl = total_pnl - cumulative_pnl;
                    st.pnl_per_setup.push(this_setup_pnl);
                }
                let _ = arm; // suppress lint
            }
        }

        // Assemble final results
        let mut arm_results: BTreeMap<String, ArmResult> = BTreeMap::new();
        for (arm_idx, arm) in self.arms.iter().enumerate() {
            let st = &arm_states[arm_idx];
            let pnl_slice = &st.pnl_per_setup;
            let returns = crate::metrics::returns_from_pnl(pnl_slice, nav_initial as f32);
            let realized_pnl_total = pnl_slice.iter().sum::<f32>() as f64;

            arm_results.insert(
                arm.name.clone(),
                ArmResult {
                    name: arm.name.clone(),
                    equity_curve: st.equity_curve.clone(),
                    fills: st.fills.clone(),
                    decisions: st.decisions.clone(),
                    risk_outcomes: st.risk_outcomes.clone(),
                    returns,
                    realized_pnl_total_usd: realized_pnl_total,
                    regimes: st.regimes.clone(),
                },
            );
        }

        Ok(BacktestResult {
            arms: arm_results,
            setups_evaluated: snapshots.len(),
            initial_nav_usd: nav_initial,
            started_at,
            finished_at: Utc::now(),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;
    use xianvec_core::market::{IndicatorPanel, OnchainPanel, Ohlcv};
    use xianvec_core::trading::{Action, AssetSymbol, Direction, Regime, TraderDecision};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn bar_at(secs: i64, price: f64) -> MarketBar {
        MarketBar {
            timestamp: Utc.timestamp_opt(secs, 0).single().unwrap(),
            open: price,
            high: price * 1.01,
            low: price * 0.99,
            close: price,
            volume: 10_000.0,
        }
    }

    fn snapshot_at(secs: i64, price: f64) -> MarketSnapshot {
        MarketSnapshot {
            setup_id: Uuid::new_v4(),
            asset: AssetSymbol::Btc,
            timestamp: Utc.timestamp_opt(secs, 0).single().unwrap(),
            price,
            volume_24h: None,
            recent_bars: vec![Ohlcv {
                timestamp: Utc.timestamp_opt(secs, 0).single().unwrap(),
                open: price,
                high: price * 1.01,
                low: price * 0.99,
                close: price,
                volume: 10_000.0,
            }],
            indicators: IndicatorPanel::default(),
            onchain: OnchainPanel::default(),
            regime: Regime::Bull,
            horizon_hours: 1,
        }
    }

    struct AlwaysBuy;
    #[async_trait::async_trait]
    impl Strategy for AlwaysBuy {
        fn name(&self) -> &'static str {
            "always_buy"
        }
        async fn decide(&self, snapshot: &MarketSnapshot) -> Option<TraderDecision> {
            Some(TraderDecision {
                setup_id: snapshot.setup_id,
                action: Action::Buy,
                size_bps: 100,
                direction: Direction::Long,
                stop_loss_pct: 5.0,
                take_profit_pct: 10.0,
                trader_summary: "AlwaysBuy test strategy for harness smoke test.".into(),
            })
        }
    }

    struct AlwaysFlat;
    #[async_trait::async_trait]
    impl Strategy for AlwaysFlat {
        fn name(&self) -> &'static str {
            "always_flat"
        }
        async fn decide(&self, _snapshot: &MarketSnapshot) -> Option<TraderDecision> {
            None
        }
    }

    fn default_risk() -> RiskLayer {
        use std::path::Path;
        RiskLayer::from_config(
            Path::new("../../config/risk.toml"),
            Path::new("../../config/whitelist.toml"),
        )
        .expect("should load from workspace config files")
    }

    // -----------------------------------------------------------------------
    // step < horizon rejected at construction
    // -----------------------------------------------------------------------

    #[test]
    fn step_less_than_horizon_rejected() {
        let cfg = BacktestRunConfig {
            initial_nav_usd: 10_000.0,
            fee_bps: 10,
            slippage_atr_frac: 0.0,
            instrument: AssetSymbol::Btc,
            step_hours: 1,
            horizon_hours: 4,
            n_bootstrap_resamples: 100,
            block_size: None,
        };
        let arms = vec![ArmConfig {
            name: "a".into(),
            strategy: Box::new(AlwaysBuy),
        }];
        assert!(matches!(
            BacktestRunner::new(cfg, arms),
            Err(HarnessError::StepLessThanHorizon { .. })
        ));
    }

    // -----------------------------------------------------------------------
    // Smoke test: two arms, 5 bars, 2 snapshots
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn smoke_two_arms_five_bars() {
        // 5 bars, 1 hour apart
        let bars: Vec<MarketBar> = (0..5)
            .map(|i| bar_at(i * 3600, 50_000.0 + i as f64 * 100.0))
            .collect();

        // 2 snapshots at bars 0 and 2
        let snapshots = vec![
            snapshot_at(0, 50_000.0),
            snapshot_at(2 * 3600, 50_200.0),
        ];

        let cfg = BacktestRunConfig {
            initial_nav_usd: 100_000.0,
            fee_bps: 10,
            slippage_atr_frac: 0.0,
            instrument: AssetSymbol::Btc,
            step_hours: 2,
            horizon_hours: 1,
            n_bootstrap_resamples: 10,
            block_size: None,
        };

        let arms = vec![
            ArmConfig {
                name: "buy_arm".into(),
                strategy: Box::new(AlwaysBuy),
            },
            ArmConfig {
                name: "flat_arm".into(),
                strategy: Box::new(AlwaysFlat),
            },
        ];

        let risk = default_risk();
        let mut runner = BacktestRunner::new(cfg, arms).expect("valid config");
        let result = runner.run(&snapshots, &bars, &risk).await.expect("run must succeed");

        assert_eq!(result.setups_evaluated, 2);
        assert!(result.arms.contains_key("buy_arm"));
        assert!(result.arms.contains_key("flat_arm"));

        let buy_arm = &result.arms["buy_arm"];
        let flat_arm = &result.arms["flat_arm"];

        // buy_arm made 2 decisions; flat_arm made 0
        assert_eq!(buy_arm.decisions.len(), 2, "buy_arm should have 2 decisions");
        assert_eq!(flat_arm.decisions.len(), 0, "flat_arm should have 0 decisions");

        // buy_arm's equity curve should have more than 1 point (initial + ticks)
        assert!(
            buy_arm.equity_curve.len() > 1,
            "buy_arm equity curve must have multiple points"
        );

        // flat_arm: no decisions, no fills, empty returns
        assert!(flat_arm.fills.is_empty());
        assert!(flat_arm.returns.is_empty());

        assert_eq!(result.initial_nav_usd, 100_000.0);
    }

    // -----------------------------------------------------------------------
    // Equity curves are recorded
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn equity_curves_are_recorded() {
        let bars: Vec<MarketBar> = (0..6)
            .map(|i| bar_at(i * 3600, 50_000.0))
            .collect();
        let snapshots = vec![snapshot_at(0, 50_000.0)];

        let cfg = BacktestRunConfig {
            initial_nav_usd: 100_000.0,
            fee_bps: 0,
            slippage_atr_frac: 0.0,
            instrument: AssetSymbol::Btc,
            step_hours: 1,
            horizon_hours: 1,
            n_bootstrap_resamples: 10,
            block_size: None,
        };

        let arms = vec![
            ArmConfig {
                name: "buy_arm".into(),
                strategy: Box::new(AlwaysBuy),
            },
            ArmConfig {
                name: "flat_arm".into(),
                strategy: Box::new(AlwaysFlat),
            },
        ];

        let risk = default_risk();
        let mut runner = BacktestRunner::new(cfg, arms).expect("valid config");
        let result = runner.run(&snapshots, &bars, &risk).await.expect("run must succeed");

        let buy_ec = &result.arms["buy_arm"].equity_curve;
        let flat_ec = &result.arms["flat_arm"].equity_curve;

        // Both should have equity curves recorded
        assert!(!buy_ec.is_empty(), "buy arm equity curve should be non-empty");
        assert!(!flat_ec.is_empty(), "flat arm equity curve should be non-empty");
    }

    // -----------------------------------------------------------------------
    // Regimes are tagged in lock-step with decisions
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn regimes_tagged_with_decisions() {
        let bars: Vec<MarketBar> = (0..4)
            .map(|i| bar_at(i * 3600, 50_000.0))
            .collect();
        let snapshots = vec![
            snapshot_at(0, 50_000.0),
            snapshot_at(3600, 50_000.0),
        ];

        let cfg = BacktestRunConfig {
            initial_nav_usd: 100_000.0,
            fee_bps: 0,
            slippage_atr_frac: 0.0,
            instrument: AssetSymbol::Btc,
            step_hours: 1,
            horizon_hours: 1,
            n_bootstrap_resamples: 10,
            block_size: None,
        };

        let arms = vec![ArmConfig {
            name: "buy_arm".into(),
            strategy: Box::new(AlwaysBuy),
        }];

        let risk = default_risk();
        let mut runner = BacktestRunner::new(cfg, arms).expect("valid config");
        let result = runner.run(&snapshots, &bars, &risk).await.expect("run must succeed");

        let arm = &result.arms["buy_arm"];
        assert_eq!(
            arm.regimes.len(),
            arm.decisions.len(),
            "regimes and decisions must have the same length"
        );
    }
}
