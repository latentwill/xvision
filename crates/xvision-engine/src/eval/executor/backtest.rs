//! `BacktestExecutor` — replays an OHLCV fixture in chronological order,
//! invoking the bundle's pipeline at each decision boundary and simulating
//! fills against the next bar's open with linear slippage + taker fees. No
//! broker is involved; positions and equity are tracked in-memory.
//!
//! This is the v1 demo path that doesn't require external broker keys.
//! Pair with `xvn eval run --mode backtest --strategy <id> --scenario <id>`.
//!
//! Out of scope (deferred):
//! - Multi-asset universes (uses `scenario.asset_universe[0]` only — v1
//!   constraint, same as PaperExecutor).
//! - Indicator panel injection into the pipeline seed (matching what
//!   PaperExecutor passes today, which is just portfolio_state).
//! - Win-rate sourced from realized-PnL pairs across decisions (the
//!   `MetricsSummary.win_rate` is left at 0.0 the same way PaperExecutor
//!   leaves it — Phase 3.C work).

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::Deserialize;
use xvision_data::fixtures::load_ohlcv_fixture;

use crate::agent::llm::LlmDispatch;
use crate::agent::pipeline::{run_pipeline, PipelineInputs};
use crate::bundle::StrategyBundle;
use crate::eval::executor::Executor;
use crate::eval::metrics::{
    annualization_periods_per_year, equity_to_returns, max_drawdown_pct, sharpe_from_returns,
    total_return_pct,
};
use crate::eval::run::{MetricsSummary, Run, RunStatus};
use crate::eval::scenario::{Scenario, SlippageModel};
use crate::eval::store::{DecisionRow, RunStore};
use crate::tools::ToolRegistry;

/// Bars before this index are treated as warm-up history and skipped — gives
/// any future indicator-panel computation enough lookback.
const WARMUP_BARS: usize = 200;

pub struct BacktestExecutor;

#[derive(Debug, Deserialize)]
struct TraderOutput {
    action: String,
    #[serde(default)]
    conviction: f64,
    #[serde(default)]
    justification: String,
}

impl TraderOutput {
    fn flat() -> Self {
        Self {
            action: "flat".into(),
            conviction: 0.0,
            justification: "parse error or missing — fell back to flat".into(),
        }
    }
}

#[async_trait]
impl Executor for BacktestExecutor {
    async fn run(
        &self,
        run: &mut Run,
        bundle: &StrategyBundle,
        scenario: &Scenario,
        dispatch: Arc<dyn LlmDispatch>,
        tools: Arc<ToolRegistry>,
        store: &RunStore,
    ) -> Result<MetricsSummary> {
        store
            .update_status(&run.id, RunStatus::Running, None)
            .await?;
        run.status = RunStatus::Running;

        let asset = scenario
            .asset_universe
            .first()
            .ok_or_else(|| anyhow!("scenario {} has empty asset_universe", scenario.id))?
            .clone();

        let cadence_min = bundle.manifest.decision_cadence_minutes as i64;
        if cadence_min <= 0 {
            anyhow::bail!(
                "bundle {} has non-positive decision_cadence_minutes",
                bundle.manifest.id
            );
        }

        let bars = load_ohlcv_fixture(&scenario.data_seed, &asset, usize::MAX)
            .map_err(|e| anyhow!("load fixture {}: {e}", scenario.data_seed))?;
        if bars.len() <= WARMUP_BARS + 1 {
            anyhow::bail!(
                "fixture {} has only {} bars; need > {}",
                scenario.data_seed,
                bars.len(),
                WARMUP_BARS + 1
            );
        }

        let initial = scenario.capital.initial;
        let slip_bps = match &scenario.slippage {
            SlippageModel::Linear { bps } => *bps as f64,
            SlippageModel::None => 0.0,
        };
        let taker_bps = scenario.fees.taker_bps as f64;

        let mut equity = initial;
        let mut equity_curve: Vec<f64> = vec![initial];
        let mut position: f64 = 0.0; // base-asset units; +long, -short
        let mut entry_price: f64 = 0.0;
        let mut realized_total: f64 = 0.0;
        let mut decision_idx = 0u32;
        let mut n_trades = 0u32;
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;

        for (i, bar) in bars.iter().enumerate() {
            if i < WARMUP_BARS {
                continue;
            }
            // Cadence gate: only fire on bars whose minute-aligned timestamp
            // is divisible by the bundle's cadence. With hourly bars and
            // 60-min cadence this always matches.
            if (bar.timestamp.timestamp() / 60) % cadence_min != 0 {
                continue;
            }
            // Need a next bar to fill against.
            let Some(next_bar) = bars.get(i + 1) else {
                break;
            };

            let seed = serde_json::json!({
                "decision_index": decision_idx,
                "asset": asset,
                "timestamp": bar.timestamp,
                "portfolio_state": {
                    "position_size": position,
                    "equity": equity,
                },
            });

            let outs = run_pipeline(PipelineInputs {
                bundle,
                seed_inputs: seed,
                dispatch: dispatch.clone(),
                tools: tools.clone(),
            })
            .await?;
            total_input_tokens += outs.total_input_tokens as u64;
            total_output_tokens += outs.total_output_tokens as u64;

            let parsed = outs
                .trader
                .as_ref()
                .and_then(|t| serde_json::from_str::<TraderOutput>(&t.text()).ok())
                .unwrap_or_else(TraderOutput::flat);

            let fill = simulate_fill(SimulateFillArgs {
                pos: position,
                entry: entry_price,
                action: &parsed.action,
                next_open: next_bar.open,
                slip_bps,
                taker_bps,
                equity,
                risk_pct: bundle.risk.risk_pct_per_trade,
            });
            position = fill.new_pos;
            entry_price = fill.new_entry;
            realized_total += fill.realized_pnl;
            if fill.fill_price.is_some() {
                n_trades += 1;
            }

            // Mark equity to the next bar's open.
            equity = initial + realized_total
                + position * (next_bar.open - entry_price);

            store
                .record_decision(&DecisionRow {
                    run_id: run.id.clone(),
                    decision_index: decision_idx,
                    timestamp: bar.timestamp,
                    asset: asset.clone(),
                    action: parsed.action.clone(),
                    conviction: Some(parsed.conviction),
                    justification: Some(parsed.justification.clone()),
                    order_size: fill.fill_size,
                    fill_price: fill.fill_price,
                    fill_size: fill.fill_size,
                    fee: fill.fee,
                    pnl_realized: if fill.realized_pnl != 0.0 {
                        Some(fill.realized_pnl)
                    } else {
                        None
                    },
                })
                .await?;
            store.record_equity(&run.id, bar.timestamp, equity).await?;
            equity_curve.push(equity);

            decision_idx += 1;
        }

        let returns = equity_to_returns(&equity_curve);
        let periods_per_year =
            annualization_periods_per_year(bundle.manifest.decision_cadence_minutes);

        let metrics = MetricsSummary {
            total_return_pct: total_return_pct(initial, equity),
            sharpe: sharpe_from_returns(&returns, periods_per_year),
            max_drawdown_pct: max_drawdown_pct(&equity_curve),
            win_rate: 0.0,
            n_trades,
            n_decisions: decision_idx,
        };

        run.actual_input_tokens = Some(total_input_tokens);
        run.actual_output_tokens = Some(total_output_tokens);
        run.metrics = Some(metrics.clone());
        run.status = RunStatus::Completed;
        store.finalize(&run.id, &metrics).await?;
        Ok(metrics)
    }
}

struct SimulateFillArgs<'a> {
    pos: f64,
    entry: f64,
    action: &'a str,
    next_open: f64,
    slip_bps: f64,
    taker_bps: f64,
    equity: f64,
    risk_pct: f64,
}

struct FillOutcome {
    new_pos: f64,
    new_entry: f64,
    fill_price: Option<f64>,
    fill_size: Option<f64>,
    fee: Option<f64>,
    realized_pnl: f64,
}

/// Simulate a market-order fill at the next bar's open, applying linear
/// slippage and a taker fee. Realized PnL is booked when an existing
/// position is reduced or reversed; new entries open at the slippage-adjusted
/// fill price.
///
/// Action semantics (matches the v1 trader-output schema):
/// - `long_open`: hold long, reverse short → long, or open long from flat.
/// - `short_open`: hold short, reverse long → short, or open short from flat.
/// - `flat` (or any unknown action): close any open position; otherwise no-op.
fn simulate_fill(a: SimulateFillArgs) -> FillOutcome {
    let want_long = a.action == "long_open";
    let want_short = a.action == "short_open";
    let want_flat = !want_long && !want_short;

    // No-op when target direction matches current position.
    if (want_long && a.pos > 0.0)
        || (want_short && a.pos < 0.0)
        || (want_flat && a.pos == 0.0)
    {
        return FillOutcome {
            new_pos: a.pos,
            new_entry: a.entry,
            fill_price: None,
            fill_size: None,
            fee: None,
            realized_pnl: 0.0,
        };
    }

    // Direction of the trade we're about to execute.
    // If reversing, this matches the new direction (which also closes out
    // the old leg). If just closing to flat, direction is opposite of
    // current pos.
    let trade_long = if want_long {
        true
    } else if want_short {
        false
    } else {
        a.pos < 0.0 // closing a short means buying
    };

    let slip = a.slip_bps / 10_000.0;
    let fill_price = if trade_long {
        a.next_open * (1.0 + slip)
    } else {
        a.next_open * (1.0 - slip)
    };

    // Realized PnL from closing the existing leg, if any.
    let realized = if a.pos != 0.0 {
        // pos > 0 (long): pnl = pos * (close - entry)
        // pos < 0 (short): pnl = -pos * (entry - close) = pos * (close - entry)
        a.pos * (fill_price - a.entry)
    } else {
        0.0
    };

    // New position size for the open leg, if any.
    let new_pos_units = if want_flat {
        0.0
    } else {
        let usd_at_risk = a.equity * a.risk_pct;
        let units = (usd_at_risk / fill_price).max(0.0);
        if want_long {
            units
        } else {
            -units
        }
    };

    // Units we cross the book on: pure-open is |new|, pure-close is |old|,
    // reversing pays both legs.
    let traded_units = if a.pos == 0.0 {
        new_pos_units.abs()
    } else if new_pos_units == 0.0 {
        a.pos.abs()
    } else {
        a.pos.abs() + new_pos_units.abs()
    };
    let notional = traded_units * fill_price;
    let fee = notional * (a.taker_bps / 10_000.0);

    let new_entry = if new_pos_units == 0.0 {
        0.0
    } else {
        fill_price
    };

    FillOutcome {
        new_pos: new_pos_units,
        new_entry,
        fill_price: Some(fill_price),
        fill_size: Some(traded_units),
        fee: Some(fee),
        realized_pnl: realized - fee,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(pos: f64, action: &'static str) -> SimulateFillArgs<'static> {
        SimulateFillArgs {
            pos,
            entry: 50_000.0,
            action,
            next_open: 60_000.0,
            slip_bps: 10.0,    // 0.1%
            taker_bps: 25.0,   // 0.25%
            equity: 10_000.0,
            risk_pct: 0.02,    // 2%
        }
    }

    #[test]
    fn flat_when_already_flat_is_noop() {
        let out = simulate_fill(args(0.0, "flat"));
        assert_eq!(out.new_pos, 0.0);
        assert!(out.fill_price.is_none());
        assert_eq!(out.realized_pnl, 0.0);
    }

    #[test]
    fn long_open_from_flat_opens_long_at_slipped_up_price() {
        let out = simulate_fill(args(0.0, "long_open"));
        assert!(out.new_pos > 0.0);
        let fp = out.fill_price.unwrap();
        assert!(fp > 60_000.0); // slip adds for buys
        assert!((fp - 60_060.0).abs() < 1e-6); // 60_000 * 1.001
    }

    #[test]
    fn flat_closes_long_and_books_realized() {
        // pos=0.001 BTC bought at 50_000, close at 60_000-slip
        let out = simulate_fill(args(0.001, "flat"));
        assert_eq!(out.new_pos, 0.0);
        assert!(out.fill_price.is_some());
        // 60_000 * (1 - 0.001) = 59_940
        // realized = 0.001 * (59_940 - 50_000) = 9.94
        // fee = 0.001 * 59_940 * 0.0025 = 0.14985
        // realized_pnl = 9.94 - 0.14985 ≈ 9.79
        assert!(out.realized_pnl > 9.0 && out.realized_pnl < 10.0);
    }

    #[test]
    fn long_open_when_already_long_is_noop() {
        let out = simulate_fill(args(0.001, "long_open"));
        assert_eq!(out.new_pos, 0.001);
        assert!(out.fill_price.is_none());
    }

    #[test]
    fn short_open_from_long_reverses_and_books_realized() {
        let out = simulate_fill(args(0.001, "short_open"));
        assert!(out.new_pos < 0.0);
        assert!(out.fill_price.is_some());
        // Closes long (booking gain) AND opens short at the same fill_price.
        // realized leg from long close should be positive (60k > 50k entry).
        // After fee, still > 0.
        assert!(out.realized_pnl > 0.0);
    }
}
