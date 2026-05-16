//! `BacktestExecutor` — replays an OHLCV fixture in chronological order,
//! invoking the strategy's pipeline at each decision boundary and simulating
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
use xvision_core::market::Ohlcv;
use xvision_data::fixtures::load_ohlcv_fixture;

use crate::agent::llm::LlmDispatch;
use crate::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use crate::api::chart::{
    ChartEquityPoint, HoldMarker, LiveDecisionRow, MarkerEvent, RunChartEvent, RunEventBus, TradeMarker,
    TradeSide,
};
use crate::eval::executor::Executor;
use crate::eval::metrics::{
    annualization_periods_per_year, equity_to_returns, max_drawdown_pct, sharpe_from_returns,
    total_return_pct,
};
use crate::eval::progress::{send_event, ProgressEvent, ProgressTx};
use crate::eval::run::{MetricsSummary, Run, RunStatus};
use crate::eval::scenario::{Scenario, SlippageModel};
use crate::eval::store::{DecisionRow, RunStore};
use crate::strategies::Strategy;
use crate::tools::ToolRegistry;

use super::trader_output::TraderOutput;

#[derive(Default)]
pub struct BacktestExecutor {
    /// Optional progress channel. When `None` the executor is silent
    /// (today's `api::eval::run_with_deps` callers); when `Some`, every
    /// significant action emits a `ProgressEvent`. Send-when-no-subscribers
    /// is a no-op via `send_event`. Mirrors PR #35's PaperExecutor wiring
    /// so SSE / CLI subscribers see both run modes through the same bus.
    progress: Option<ProgressTx>,
    /// Optional pre-loaded bars. When `Some`, the executor skips the
    /// `load_ohlcv_fixture` path and replays the provided bars directly.
    /// Populated by Task 8's DB-resolved path in `api::eval::run_inner`
    /// (bars come from the `eval::bars::load_bars` cache wrapper). When
    /// `None` (the legacy / canonical-scenario fallback), bars are loaded
    /// from `data/probes/<scenario.bar_cache_policy.cache_key>.parquet`
    /// via `load_ohlcv_fixture`.
    injected_bars: Option<Vec<Ohlcv>>,
    /// Optional live-stream event bus. When `Some`, the executor emits
    /// `RunChartEvent::Equity` and `RunChartEvent::Marker` events after
    /// each decision cycle so SSE subscribers at `/live/<run_id>` see
    /// real-time chart updates. When `None` (most unit tests), emission
    /// is a no-op.
    event_bus: Option<Arc<RunEventBus>>,
}

impl BacktestExecutor {
    /// Constructor without progress wiring. Existing callers
    /// (`api::eval::run_with_deps` today, plus tests against legacy
    /// `canonical_scenarios()` ids) keep working unchanged — bars get
    /// loaded from `data/probes/<cache_key>.parquet`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Constructor that wires this executor to a `ProgressTx`. New
    /// callers (CLI progress bar, dashboard SSE endpoint) hand in a
    /// sender from a shared `ProgressBus`.
    pub fn with_progress(progress: ProgressTx) -> Self {
        Self {
            progress: Some(progress),
            injected_bars: None,
            event_bus: None,
        }
    }

    /// Constructor that injects bars directly, bypassing the fixture
    /// loader. Used by `api::eval::run_inner` when the scenario comes
    /// from the new DB-backed registry: bars are fetched / cached via
    /// `eval::bars::load_bars` and handed to the executor pre-loaded.
    ///
    /// Bars must be in chronological order and contain at least two entries:
    /// one decision bar and one next bar to fill against.
    pub fn with_bars(bars: Vec<Ohlcv>) -> Self {
        Self {
            progress: None,
            injected_bars: Some(bars),
            event_bus: None,
        }
    }

    /// Both bars + progress.
    pub fn with_bars_and_progress(bars: Vec<Ohlcv>, progress: ProgressTx) -> Self {
        Self {
            progress: Some(progress),
            injected_bars: Some(bars),
            event_bus: None,
        }
    }

    /// Attach a live-stream event bus to an existing executor. Builder-style
    /// so callers can chain after `with_bars` / `with_progress`:
    ///   `BacktestExecutor::with_bars(bars).with_event_bus(bus)`.
    pub fn with_event_bus(mut self, bus: Arc<RunEventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    fn emit(&self, event: ProgressEvent) {
        if let Some(tx) = self.progress.as_ref() {
            send_event(tx, event);
        }
    }

    /// Emit a `RunChartEvent` onto the event bus if one is configured.
    /// Inline `.await` is fine here since `run_inner` is already `async`.
    async fn emit_chart(&self, run_id: &str, event: RunChartEvent) {
        if let Some(bus) = self.event_bus.as_ref() {
            bus.emit(run_id, event).await;
        }
    }
}

#[async_trait]
impl Executor for BacktestExecutor {
    async fn run(
        &self,
        run: &mut Run,
        strategy: &Strategy,
        scenario: &Scenario,
        agent_slots: &[ResolvedAgentSlot],
        dispatch: Arc<dyn LlmDispatch>,
        tools: Arc<ToolRegistry>,
        store: &RunStore,
    ) -> Result<MetricsSummary> {
        // RunStarted fires before fixture-loading work so subscribers
        // can show "in flight" even on a slow parquet read.
        self.emit(ProgressEvent::RunStarted {
            run_id: run.id.clone(),
            estimated_tokens: 0,
        });
        self.emit_chart(
            &run.id,
            RunChartEvent::Status {
                phase: "running".into(),
                message: None,
            },
        )
        .await;

        let result = self
            .run_inner(run, strategy, scenario, agent_slots, dispatch, tools, store)
            .await;

        match &result {
            Ok(metrics) => {
                let tokens_used = run
                    .actual_input_tokens
                    .unwrap_or(0)
                    .saturating_add(run.actual_output_tokens.unwrap_or(0));
                self.emit(ProgressEvent::RunCompleted {
                    run_id: run.id.clone(),
                    metrics: metrics.clone(),
                    tokens_used,
                });
                self.emit_chart(
                    &run.id,
                    RunChartEvent::Status {
                        phase: "completed".into(),
                        message: None,
                    },
                )
                .await;
                if let Some(bus) = self.event_bus.as_ref() {
                    bus.drop_channel(&run.id).await;
                }
            }
            Err(e) => {
                if matches!(store.is_cancelled(&run.id).await, Ok(true)) {
                    self.emit_chart(
                        &run.id,
                        RunChartEvent::Status {
                            phase: "cancelled".into(),
                            message: Some("cancelled by user".into()),
                        },
                    )
                    .await;
                    if let Some(bus) = self.event_bus.as_ref() {
                        bus.drop_channel(&run.id).await;
                    }
                    return result;
                }
                let reason = e.to_string();
                let _ = store.fail_active(&run.id, &reason).await;
                run.status = RunStatus::Failed;
                run.error = Some(reason.clone());
                self.emit(ProgressEvent::RunFailed {
                    run_id: run.id.clone(),
                    error: reason.clone(),
                });
                self.emit_chart(
                    &run.id,
                    RunChartEvent::Status {
                        phase: "failed".into(),
                        message: Some(reason),
                    },
                )
                .await;
                if let Some(bus) = self.event_bus.as_ref() {
                    bus.drop_channel(&run.id).await;
                }
            }
        }
        result
    }
}

impl BacktestExecutor {
    async fn run_inner(
        &self,
        run: &mut Run,
        strategy: &Strategy,
        scenario: &Scenario,
        agent_slots: &[ResolvedAgentSlot],
        dispatch: Arc<dyn LlmDispatch>,
        tools: Arc<ToolRegistry>,
        store: &RunStore,
    ) -> Result<MetricsSummary> {
        // TODO(Task 5): pull from Strategy. For v1 we read the first
        // venue_symbol off the scenario's asset list (BTC/USD for canonicals).
        let asset = scenario
            .asset
            .first()
            .map(|a| a.venue_symbol.clone())
            .ok_or_else(|| anyhow!("scenario {} has empty asset list", scenario.id))?;

        let cadence_min = strategy.manifest.decision_cadence_minutes as i64;
        if cadence_min <= 0 {
            anyhow::bail!(
                "strategy {} has non-positive decision_cadence_minutes",
                strategy.manifest.id
            );
        }

        // Bars come from one of two sources:
        // 1. Injected via `with_bars` — Task 8's DB-resolved path goes
        //    through `eval::bars::load_bars` and hands a pre-loaded
        //    `Vec<Ohlcv>` to the executor. This is the path the new
        //    `api::scenario::get`-based eval::run uses.
        // 2. Legacy fixture loader — the canonical-scenarios fallback
        //    still reads from `data/probes/<cache_key>.parquet`. Keeps
        //    pre-Task-8 tests working without a DB / Alpaca creds.
        let bars: Vec<Ohlcv> = if let Some(injected) = self.injected_bars.clone() {
            injected
        } else {
            let data_seed = &scenario.bar_cache_policy.cache_key;
            load_ohlcv_fixture(data_seed, &asset, usize::MAX)
                .map_err(|e| anyhow!("load fixture {}: {e}", data_seed))?
        };
        if bars.len() < 2 {
            anyhow::bail!(
                "scenario {} has only {} bars; need at least 2",
                scenario.id,
                bars.len(),
            );
        }

        // Used by RunTick to report bar-clock progress. Cadence can make
        // actual decisions sparser, but every decision still needs a following
        // bar to fill against, so the final bar is reserved as the fill source.
        let total_decision_bars = bars.len().saturating_sub(1).max(1) as f64;

        let initial = scenario.capital.initial;
        let slip_bps = match &scenario.venue.slippage {
            SlippageModel::Linear { bps } => *bps as f64,
            SlippageModel::None => 0.0,
        };
        let taker_bps = scenario.venue.fees.taker_bps as f64;

        let mut equity = initial;
        let mut equity_curve: Vec<f64> = vec![initial];
        let mut position: f64 = 0.0; // base-asset units; +long, -short
        let mut entry_price: f64 = 0.0;
        let mut realized_total: f64 = 0.0;
        let mut decision_idx = 0u32;
        let mut n_trades = 0u32;
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        // Running peak for drawdown_pct in MetricsUpdated. Start at the
        // initial capital so the first tick's drawdown is well-defined.
        let mut peak_equity = initial.max(0.0);

        for (i, bar) in bars.iter().enumerate() {
            if store.is_terminal(&run.id).await? {
                anyhow::bail!("eval run stopped");
            }
            // Cadence gate: only fire on bars whose minute-aligned timestamp
            // is divisible by the strategy's cadence. With hourly bars and
            // 60-min cadence this always matches.
            if (bar.timestamp.timestamp() / 60) % cadence_min != 0 {
                continue;
            }
            // Need a next bar to fill against.
            let Some(next_bar) = bars.get(i + 1) else {
                break;
            };

            // RunTick fires before the per-bar pipeline call so dashboards
            // can advance progress bars even when an LLM round-trip is slow.
            let scenario_progress_pct = ((i as f64 / total_decision_bars) * 100.0).clamp(0.0, 100.0);
            self.emit(ProgressEvent::RunTick {
                run_id: run.id.clone(),
                scenario_progress_pct,
                current_ts: bar.timestamp,
            });

            let seed = serde_json::json!({
                "decision_index": decision_idx,
                "asset": asset,
                "timestamp": bar.timestamp,
                "market_data": {
                    "asset": asset,
                    "current_bar": {
                        "timestamp": bar.timestamp,
                        "open": bar.open,
                        "high": bar.high,
                        "low": bar.low,
                        "close": bar.close,
                        "volume": bar.volume,
                    },
                    "next_bar_open": next_bar.open,
                    "reference_price_usd": bar.close,
                    "reference_price_source": "eval_bar.close",
                },
                "portfolio_state": {
                    "position_size": position,
                    "equity": equity,
                    "mark_price": bar.close,
                },
            });

            let outs = run_pipeline(PipelineInputs {
                strategy,
                agent_slots,
                seed_inputs: seed,
                dispatch: dispatch.clone(),
                tools: tools.clone(),
            })
            .await?;
            total_input_tokens += outs.total_input_tokens as u64;
            total_output_tokens += outs.total_output_tokens as u64;
            run.actual_input_tokens = Some(total_input_tokens);
            run.actual_output_tokens = Some(total_output_tokens);
            store
                .update_token_usage(&run.id, total_input_tokens, total_output_tokens)
                .await?;

            if store.is_terminal(&run.id).await? {
                anyhow::bail!("eval run stopped");
            }

            let trader = outs
                .trader
                .as_ref()
                .ok_or_else(|| anyhow!("run {} decision {}: trader output missing", run.id, decision_idx))?;
            let parsed = TraderOutput::parse_response(trader, &run.id, decision_idx)?;

            if store.is_terminal(&run.id).await? {
                anyhow::bail!("eval run stopped");
            }

            let pre_fill_position = position;
            let fill = simulate_fill(SimulateFillArgs {
                pos: pre_fill_position,
                entry: entry_price,
                action: &parsed.action,
                next_open: next_bar.open,
                slip_bps,
                taker_bps,
                equity,
                risk_pct: strategy.risk.risk_pct_per_trade,
            });
            position = fill.new_pos;
            entry_price = fill.new_entry;
            realized_total += fill.realized_pnl;
            let fill_happened = fill.fill_price.is_some();
            if fill_happened {
                n_trades += 1;

                // FillRecorded — only when an actionable decision actually
                // crossed the book. For close-to-flat decisions, side is
                // derived from the pre-fill position direction.
                let side = fill_side_for_action(&parsed.action, pre_fill_position);
                self.emit(ProgressEvent::FillRecorded {
                    run_id: run.id.clone(),
                    side: side.into(),
                    price: fill.fill_price.unwrap_or(0.0),
                    qty: fill.fill_size.unwrap_or(0.0),
                    fee: fill.fee.unwrap_or(0.0),
                });
            }

            // DecisionEmitted fires for every cycle so subscribers see
            // flat/hold decisions too.
            self.emit(ProgressEvent::DecisionEmitted {
                run_id: run.id.clone(),
                action: parsed.action.clone(),
                asset: asset.clone(),
                size: fill.fill_size.unwrap_or(0.0),
                conviction: parsed.conviction,
            });

            // Mark equity to the next bar's open.
            equity = initial + realized_total + position * (next_bar.open - entry_price);

            let decision_row = DecisionRow {
                run_id: run.id.clone(),
                decision_index: decision_idx,
                timestamp: bar.timestamp,
                asset: asset.clone(),
                action: parsed.action.clone(),
                conviction: Some(parsed.conviction),
                justification: Some(parsed.justification.clone()),
                reasoning: Some(parsed.justification.clone()),
                order_size: fill.fill_size,
                fill_price: fill.fill_price,
                fill_size: fill.fill_size,
                fee: fill.fee,
                pnl_realized: if fill.realized_pnl != 0.0 {
                    Some(fill.realized_pnl)
                } else {
                    None
                },
            };
            store.record_decision(&decision_row).await?;
            self.emit_chart(
                &run.id,
                RunChartEvent::Decision(LiveDecisionRow::from(&decision_row)),
            )
            .await;

            // Emit a marker event derived from this decision. Mirrors the
            // action → marker-variant mapping in `chart::split_markers`.
            // Only emit for actions where fill data is present (same guard
            // as split_markers uses for trade-like actions).
            let t = bar.timestamp.timestamp();
            let marker_event = match parsed.action.as_str() {
                "long_open" => {
                    if let (Some(price), Some(size)) = (fill.fill_price, fill.fill_size) {
                        Some(MarkerEvent::Trade(make_trade_marker(
                            TradeSide::Buy,
                            t,
                            price,
                            size,
                            fill.fee,
                            fill.realized_pnl,
                            decision_idx,
                            &parsed.justification,
                        )))
                    } else {
                        None
                    }
                }
                "short_open" | "flat" => {
                    if let (Some(price), Some(size)) = (fill.fill_price, fill.fill_size) {
                        Some(MarkerEvent::Trade(make_trade_marker(
                            TradeSide::Sell,
                            t,
                            price,
                            size,
                            fill.fee,
                            fill.realized_pnl,
                            decision_idx,
                            &parsed.justification,
                        )))
                    } else {
                        None
                    }
                }
                "hold" => Some(MarkerEvent::Hold(HoldMarker {
                    time: t,
                    price: next_bar.open,
                    conviction: Some(parsed.conviction),
                    decision_index: decision_idx,
                })),
                _ => None,
            };
            if let Some(marker) = marker_event {
                self.emit_chart(&run.id, RunChartEvent::Marker(marker)).await;
            }

            store.record_equity(&run.id, bar.timestamp, equity).await?;

            // Emit equity event for live-stream subscribers.
            self.emit_chart(
                &run.id,
                RunChartEvent::Equity(ChartEquityPoint {
                    time: bar.timestamp.timestamp(),
                    equity_usd: equity,
                }),
            )
            .await;

            equity_curve.push(equity);

            // Running drawdown — peak updates after each tick so
            // MetricsUpdated reflects worst-observed-so-far for live UI.
            if equity > peak_equity {
                peak_equity = equity;
            }
            let drawdown_pct = if peak_equity > 0.0 {
                ((peak_equity - equity) / peak_equity * 100.0).max(0.0)
            } else {
                0.0
            };
            self.emit(ProgressEvent::MetricsUpdated {
                run_id: run.id.clone(),
                equity,
                drawdown_pct,
                n_trades,
            });

            decision_idx += 1;
        }

        if store.is_terminal(&run.id).await? {
            anyhow::bail!("eval run stopped");
        }

        let returns = equity_to_returns(&equity_curve);
        let periods_per_year = annualization_periods_per_year(strategy.manifest.decision_cadence_minutes);

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
    if (want_long && a.pos > 0.0) || (want_short && a.pos < 0.0) || (want_flat && a.pos == 0.0) {
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

    let new_entry = if new_pos_units == 0.0 { 0.0 } else { fill_price };

    FillOutcome {
        new_pos: new_pos_units,
        new_entry,
        fill_price: Some(fill_price),
        fill_size: Some(traded_units),
        fee: Some(fee),
        realized_pnl: realized - fee,
    }
}

/// Build a `TradeMarker` from fill-level data. Extracted to avoid duplicating
/// the identical field construction across the `long_open` and
/// `short_open`/`flat` arms of the marker-event match.
fn make_trade_marker(
    side: TradeSide,
    time: i64,
    price: f64,
    size: f64,
    fee: Option<f64>,
    realized_pnl: f64,
    decision_index: u32,
    justification: &str,
) -> TradeMarker {
    TradeMarker {
        time,
        side,
        price,
        size,
        fee: fee.unwrap_or(0.0),
        pnl_realized: if realized_pnl != 0.0 {
            Some(realized_pnl)
        } else {
            None
        },
        decision_index,
        justification: Some(justification.to_owned()),
    }
}

fn fill_side_for_action(action: &str, pre_fill_position: f64) -> &'static str {
    if action == "long_open" {
        "buy"
    } else if action == "short_open" {
        "sell"
    } else if pre_fill_position > 0.0 {
        "sell"
    } else {
        "buy"
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
            slip_bps: 10.0,  // 0.1%
            taker_bps: 25.0, // 0.25%
            equity: 10_000.0,
            risk_pct: 0.02, // 2%
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

    #[test]
    fn fill_side_for_flat_close_of_long_is_sell() {
        assert_eq!(fill_side_for_action("flat", 0.5), "sell");
    }

    #[test]
    fn fill_side_for_flat_close_of_short_is_buy() {
        assert_eq!(fill_side_for_action("flat", -0.5), "buy");
    }
}
