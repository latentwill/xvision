//! `PaperExecutor` — drives a strategy against a `BrokerSurface` (e.g.
//! Alpaca paper). Records every decision and post-tick balance to the
//! `RunStore`. Computes naive metrics on completion (Sharpe + drawdown
//! refinement lands with the Phase 3.C metrics module).
//!
//! Use `PaperExecutor::new(Arc<dyn BrokerSurface>)`. In production the
//! broker is `AlpacaPaperSurface::from_env()` (PR #5). In tests the
//! broker is `MockBrokerSurface` (PR #5) so no network is required.

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use xvision_core::market::Ohlcv;
use xvision_execution::broker_surface::{is_alpaca_crypto, BrokerSurface, OrderRequest, Side};

use crate::agent::llm::LlmDispatch;
use crate::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use crate::api::chart::{ChartEquityPoint, LiveDecisionRow, RunChartEvent, RunEventBus};
use crate::eval::executor::Executor;
use crate::eval::metrics::{
    annualization_periods_per_year, equity_to_returns, max_drawdown_pct, sharpe_from_returns,
    total_return_pct,
};
use crate::eval::progress::{send_event, ProgressEvent, ProgressTx};
use crate::eval::run::{MetricsSummary, Run, RunStatus};
use crate::eval::scenario::Scenario;
use crate::eval::store::{DecisionRow, RunStore};
use crate::strategies::Strategy;
use crate::tools::ToolRegistry;

use super::trader_output::TraderOutput;

pub struct PaperExecutor {
    broker: Arc<dyn BrokerSurface>,
    /// Historical scenario bars used to drive paper eval decisions and
    /// broker reference prices. Paper mode sends orders to Alpaca paper, but
    /// the agent and sizing still run against the scenario replay timeline.
    injected_bars: Option<Vec<Ohlcv>>,
    /// Pre-window warmup bars prepended to the decision seed's rolling
    /// `bar_history` window. Same role as `BacktestExecutor::warmup_bars`
    /// — they never drive decisions; they only feed context so the trader
    /// LLM can compute crossovers / momentum from real prior bars at
    /// bar 1 of the paper window. See `eval::bars::load_warmup_bars`.
    warmup_bars: Vec<Ohlcv>,
    /// Optional progress channel. When `None` the executor is silent
    /// (today's `eval::run` callers); when `Some`, every significant
    /// action emits a `ProgressEvent`. Send-when-no-subscribers is a
    /// no-op via `send_event`.
    progress: Option<ProgressTx>,
    /// Optional live-stream event bus for dashboard SSE subscribers.
    event_bus: Option<Arc<RunEventBus>>,
}

impl PaperExecutor {
    /// Constructor without progress wiring. Existing callers (and tests
    /// that don't care about events) keep working unchanged.
    pub fn new(broker: Arc<dyn BrokerSurface>) -> Self {
        Self {
            broker,
            injected_bars: None,
            warmup_bars: Vec::new(),
            progress: None,
            event_bus: None,
        }
    }

    pub fn with_bars(broker: Arc<dyn BrokerSurface>, bars: Vec<Ohlcv>) -> Self {
        Self {
            broker,
            injected_bars: Some(bars),
            warmup_bars: Vec::new(),
            progress: None,
            event_bus: None,
        }
    }

    /// Constructor that wires this executor to a `ProgressTx`. New
    /// callers (CLI progress bar, dashboard SSE endpoint) hand in a
    /// sender from a shared `ProgressBus`.
    pub fn with_progress(broker: Arc<dyn BrokerSurface>, progress: ProgressTx) -> Self {
        Self {
            broker,
            injected_bars: None,
            warmup_bars: Vec::new(),
            progress: Some(progress),
            event_bus: None,
        }
    }

    pub fn with_bars_and_progress(
        broker: Arc<dyn BrokerSurface>,
        bars: Vec<Ohlcv>,
        progress: ProgressTx,
    ) -> Self {
        Self {
            broker,
            injected_bars: Some(bars),
            warmup_bars: Vec::new(),
            progress: Some(progress),
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, bus: Arc<RunEventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Pre-window warmup bars for the seed's rolling `bar_history`. Never
    /// iterated for decisions. Chains with `with_bars` / `with_progress` /
    /// `with_event_bus`.
    pub fn with_warmup(mut self, warmup_bars: Vec<Ohlcv>) -> Self {
        self.warmup_bars = warmup_bars;
        self
    }

    fn emit(&self, event: ProgressEvent) {
        if let Some(tx) = self.progress.as_ref() {
            send_event(tx, event);
        }
    }

    async fn emit_chart(&self, run_id: &str, event: RunChartEvent) {
        if let Some(bus) = self.event_bus.as_ref() {
            bus.emit(run_id, event).await;
        }
    }
}

fn is_actionable(action: &str) -> bool {
    matches!(action, "long_open" | "short_open")
}

/// Find the trader slot's model id, used to decorate trader-output
/// failures with the reasoning-class hint (q15 §1). Prefers an attached
/// agent with role `trader`, then falls back to the legacy
/// `strategy.trader_slot`. Returns `None` when neither is present or
/// neither has a model pinned.
fn trader_model_id(
    agent_slots: &[ResolvedAgentSlot],
    strategy: &Strategy,
) -> Option<String> {
    if let Some(resolved) = agent_slots
        .iter()
        .find(|r| r.role.eq_ignore_ascii_case("trader"))
    {
        let model = resolved.slot.effective_model();
        if !model.trim().is_empty() {
            return Some(model);
        }
    }
    if let Some(slot) = strategy.trader_slot.as_ref() {
        let model = slot.effective_model();
        if !model.trim().is_empty() {
            return Some(model);
        }
    }
    None
}

fn bar_seed(asset: &str, bar: &Ohlcv, bar_history: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({
        "asset": asset,
        "current_bar": ohlcv_to_json(bar),
        "next_bar_open": serde_json::Value::Null,
        "reference_price_usd": bar.close,
        "reference_price_source": "eval_bar.close",
        "bar_history": bar_history,
    })
}

/// Serialize an Ohlcv bar as the same JSON shape used for
/// `market_data.current_bar` so `bar_history` entries are homogeneous
/// with the trader prompt's existing current-bar shape.
fn ohlcv_to_json(bar: &Ohlcv) -> serde_json::Value {
    serde_json::json!({
        "timestamp": bar.timestamp,
        "open": bar.open,
        "high": bar.high,
        "low": bar.low,
        "close": bar.close,
        "volume": bar.volume,
    })
}

#[async_trait]
impl Executor for PaperExecutor {
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
        if !store.begin_running(&run.id).await? {
            anyhow::bail!("eval run stopped");
        }
        run.status = RunStatus::Running;

        // RunStarted fires before any work so subscribers can show the
        // run as "in flight" even if the first tick is slow.
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
                let reason = super::format_failure_reason(e);
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

impl PaperExecutor {
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
        // TODO(Task 5): pull from Strategy. For now we read the first
        // venue_symbol off the scenario's asset list — preserves v1 BTC-only
        // semantics (canonical scenarios all have asset[0].venue_symbol = "BTC/USD").
        let asset = scenario
            .asset
            .first()
            .map(|a| a.venue_symbol.clone())
            .ok_or_else(|| anyhow::anyhow!("scenario {} has empty asset list", scenario.id))?;

        let cadence_min = strategy.manifest.decision_cadence_minutes as i64;
        if cadence_min <= 0 {
            anyhow::bail!(
                "strategy {} has non-positive decision_cadence_minutes",
                strategy.manifest.id
            );
        }

        let bars = self.injected_bars.clone().ok_or_else(|| {
            anyhow!(
                "paper eval requires historical scenario bars so the agent and broker reference price come from the eval timeline"
            )
        })?;
        let decision_bars: Vec<Ohlcv> = bars
            .into_iter()
            .filter(|bar| {
                bar.timestamp >= scenario.time_window.start && bar.timestamp < scenario.time_window.end
            })
            .filter(|bar| (bar.timestamp.timestamp() / 60) % cadence_min == 0)
            .filter(|bar| bar.close > 0.0 && bar.close.is_finite())
            .collect();
        if decision_bars.is_empty() {
            anyhow::bail!(
                "scenario {} has no usable paper eval bars for asset {} in {}..{} at {}m cadence",
                scenario.id,
                asset,
                scenario.time_window.start,
                scenario.time_window.end,
                cadence_min
            );
        }

        let total_decision_bars = decision_bars.len().max(1) as f64;

        // Per-decision rolling-history window. Warmup bars (from
        // `eval::bars::load_warmup_bars`) sit in front of the scenario
        // bars so we can slice the last `scenario.warmup_bars` items at
        // each decision and surface them in the seed as
        // `market_data.bar_history`. Same mechanism as BacktestExecutor.
        let warmup_count = self.warmup_bars.len();
        let combined_bars: Vec<&Ohlcv> =
            self.warmup_bars.iter().chain(decision_bars.iter()).collect();
        let history_window = scenario.warmup_bars as usize;

        let initial_balance = self.broker.balance().await?;
        let mut equity_samples: Vec<f64> = Vec::new();
        let mut decision_idx = 0u32;
        let mut n_trades = 0u32;
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        // Running peak for drawdown_pct in MetricsUpdated. Start at the
        // initial balance so the first tick's drawdown is well-defined.
        let mut peak_equity = initial_balance.max(0.0);

        for (i, bar) in decision_bars.iter().enumerate() {
            if store.is_terminal(&run.id).await? {
                anyhow::bail!("eval run stopped");
            }
            // Emit RunTick before pipeline work so dashboard progress
            // bars can advance even if the LLM call is slow.
            let scenario_progress_pct =
                ((decision_idx as f64 / total_decision_bars) * 100.0).clamp(0.0, 100.0);
            self.emit(ProgressEvent::RunTick {
                run_id: run.id.clone(),
                scenario_progress_pct,
                current_ts: bar.timestamp,
            });

            // Slice the last `history_window` bars strictly before the
            // current bar from the combined `[warmup..., decision...]`
            // series.
            let combined_idx = warmup_count + i;
            let history_start = combined_idx.saturating_sub(history_window);
            let bar_history: Vec<serde_json::Value> = combined_bars[history_start..combined_idx]
                .iter()
                .map(|b| ohlcv_to_json(b))
                .collect();

            let position = self.broker.position(&asset).await?;
            let balance = self.broker.balance().await?;
            let market_data = bar_seed(&asset, bar, bar_history);
            let reference_price_usd = bar.close;
            let seed = serde_json::json!({
                "decision_index": decision_idx,
                "asset": asset,
                "timestamp": bar.timestamp,
                "market_data": market_data,
                "portfolio_state": {
                    "position_size": position,
                    "equity": balance,
                    "mark_price": reference_price_usd,
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

            let trader = match outs.trader.as_ref() {
                Some(t) => t,
                None => {
                    return Err(TraderOutput::missing_response_error(&run.id, decision_idx).into());
                }
            };
            let trader_model_id = trader_model_id(agent_slots, strategy);
            let parsed = TraderOutput::parse_response(trader, &run.id, decision_idx)
                .map_err(|e| e.with_model_hint(trader_model_id.as_deref()))?;

            if store.is_terminal(&run.id).await? {
                anyhow::bail!("eval run stopped");
            }

            let mut order_size: Option<f64> = None;
            let mut fill_price: Option<f64> = None;
            let mut fill_size: Option<f64> = None;
            let mut fee: Option<f64> = None;

            // Plan the broker submission for this decision. Three cases:
            //
            //   1. Non-actionable action (`hold`, `flat`, etc.) → no
            //      submission.
            //   2. `short_open` on an Alpaca crypto asset → the broker is
            //      long-only, so we reinterpret the signal as "close any
            //      open long" (matches the reverse-from-long semantics in
            //      `backtest::simulate_fill`, collapsed to flat because
            //      the venue can't hold a short). Query the broker; if
            //      a long is open, submit a sell sized to the long
            //      (full close). If flat or short, skip — the LLM's
            //      intent still shows up in the decisions table and the
            //      run doesn't fail on broker rejection.
            //   3. Anything else actionable → submit a market order
            //      sized by `risk_pct_per_trade`.
            let plan: Option<(Side, f64)> = if !is_actionable(&parsed.action) {
                None
            } else if parsed.action == "short_open" && is_alpaca_crypto(&asset) {
                let pos = self.broker.position(&asset).await.with_context(|| {
                    format!(
                        "paper eval broker position query failed: run_id={} decision_index={} asset={}",
                        run.id, decision_idx, asset
                    )
                })?;
                if pos > 0.0 {
                    Some((Side::Sell, pos))
                } else {
                    None
                }
            } else {
                let usd_at_risk = balance * strategy.risk.risk_pct_per_trade;
                let size = (usd_at_risk / reference_price_usd).max(0.0);
                let side = if parsed.action == "long_open" {
                    Side::Buy
                } else {
                    Side::Sell
                };
                Some((side, size))
            };

            if let Some((side, size)) = plan {
                let req = OrderRequest {
                    asset: asset.clone(),
                    side,
                    size,
                    reference_price_usd,
                    stop_loss_pct: Some((strategy.risk.stop_loss_atr_multiple as f32).max(0.5)),
                    take_profit_pct: Some(5.0),
                    idempotency_key: format!("{}-{}", run.id, decision_idx),
                };
                let conf = self
                    .broker
                    .submit_order(req)
                    .await
                    .with_context(|| {
                        format!(
                            "paper eval submit_order failed: run_id={} decision_index={} asset={} action={} side={:?} size={} reference_price_usd={}",
                            run.id,
                            decision_idx,
                            asset,
                            parsed.action,
                            side,
                            size,
                            reference_price_usd
                        )
                    })?;
                fill_price = conf.fill_price;
                fill_size = Some(conf.fill_size);
                fee = conf.fee;
                order_size = Some(size);
                n_trades += 1;

                // FillRecorded fires only when an order actually went
                // through. Subscribers that draw trade markers on a
                // chart consume this.
                self.emit(ProgressEvent::FillRecorded {
                    run_id: run.id.clone(),
                    side: match side {
                        Side::Buy => "buy".into(),
                        Side::Sell => "sell".into(),
                    },
                    price: fill_price.unwrap_or(0.0),
                    qty: conf.fill_size,
                    fee: fee.unwrap_or(0.0),
                });
            }

            // DecisionEmitted fires for every cycle (actionable or not)
            // so subscribers see flat/hold decisions too.
            self.emit(ProgressEvent::DecisionEmitted {
                run_id: run.id.clone(),
                action: parsed.action.clone(),
                asset: asset.clone(),
                size: order_size.unwrap_or(0.0),
                conviction: parsed.conviction,
            });

            let decision_row = DecisionRow {
                run_id: run.id.clone(),
                decision_index: decision_idx,
                timestamp: bar.timestamp,
                asset: asset.clone(),
                action: parsed.action.clone(),
                conviction: Some(parsed.conviction),
                justification: Some(parsed.justification.clone()),
                reasoning: Some(parsed.justification.clone()),
                order_size,
                fill_price,
                fill_size,
                fee,
                pnl_realized: None,
            };
            store.record_decision(&decision_row).await?;
            self.emit_chart(
                &run.id,
                RunChartEvent::Decision(LiveDecisionRow::from(&decision_row)),
            )
            .await;

            let post_balance = self.broker.balance().await?;
            store.record_equity(&run.id, bar.timestamp, post_balance).await?;
            self.emit_chart(
                &run.id,
                RunChartEvent::Equity(ChartEquityPoint {
                    time: bar.timestamp.timestamp(),
                    equity_usd: post_balance,
                }),
            )
            .await;
            equity_samples.push(post_balance);

            // Running drawdown — the running peak is updated after each
            // tick so MetricsUpdated reflects the worst-observed-so-far
            // drawdown for live UI.
            if post_balance > peak_equity {
                peak_equity = post_balance;
            }
            let drawdown_pct = if peak_equity > 0.0 {
                ((peak_equity - post_balance) / peak_equity * 100.0).max(0.0)
            } else {
                0.0
            };
            self.emit(ProgressEvent::MetricsUpdated {
                run_id: run.id.clone(),
                equity: post_balance,
                drawdown_pct,
                n_trades,
            });

            decision_idx += 1;
        }

        if store.is_terminal(&run.id).await? {
            anyhow::bail!("eval run stopped");
        }

        let final_balance = self.broker.balance().await?;
        // Prepend the initial balance so equity_to_returns covers the first
        // tick's drift from the seed balance, not just inter-tick drift.
        let mut full_curve = Vec::with_capacity(equity_samples.len() + 1);
        full_curve.push(initial_balance);
        full_curve.extend_from_slice(&equity_samples);

        let returns = equity_to_returns(&full_curve);
        let periods_per_year = annualization_periods_per_year(strategy.manifest.decision_cadence_minutes);

        // Win rate from realized PnL is computed downstream once
        // PaperExecutor tracks entry/exit pairs. Until then it stays 0.0
        // — Phase 3.C findings are coming.
        let metrics = MetricsSummary {
            total_return_pct: total_return_pct(initial_balance, final_balance),
            sharpe: sharpe_from_returns(&returns, periods_per_year),
            max_drawdown_pct: max_drawdown_pct(&full_curve),
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
