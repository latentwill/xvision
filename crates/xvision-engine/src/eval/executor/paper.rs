//! `PaperExecutor` — drives a strategy against a `BrokerSurface` (e.g.
//! Alpaca paper). Records every decision and post-tick balance to the
//! `RunStore`. Computes naive metrics on completion (Sharpe + drawdown
//! refinement lands with the Phase 3.C metrics module).
//!
//! Use `PaperExecutor::new(Arc<dyn BrokerSurface>)`. In production the
//! broker is `AlpacaPaperSurface::from_env()` (PR #5). In tests the
//! broker is `MockBrokerSurface` (PR #5) so no network is required.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Duration;
use xvision_execution::broker_surface::{BrokerSurface, OrderRequest, Side};

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

const DEFAULT_REFERENCE_PRICE_USD: f64 = 70_000.0;

pub struct PaperExecutor {
    broker: Arc<dyn BrokerSurface>,
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
            progress: Some(progress),
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, bus: Arc<RunEventBus>) -> Self {
        self.event_bus = Some(bus);
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

fn configured_reference_price_usd() -> f64 {
    std::env::var("XVN_PAPER_REFERENCE_PRICE_USD")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(DEFAULT_REFERENCE_PRICE_USD)
}

fn is_actionable(action: &str) -> bool {
    matches!(action, "long_open" | "short_open")
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
                    return result;
                }
                self.emit(ProgressEvent::RunFailed {
                    run_id: run.id.clone(),
                    error: e.to_string(),
                });
                self.emit_chart(
                    &run.id,
                    RunChartEvent::Status {
                        phase: "failed".into(),
                        message: Some(e.to_string()),
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
        store
            .update_status(&run.id, RunStatus::Running, None)
            .await?;
        run.status = RunStatus::Running;

        // TODO(Task 5): pull from Strategy. For now we read the first
        // venue_symbol off the scenario's asset list — preserves v1 BTC-only
        // semantics (canonical scenarios all have asset[0].venue_symbol = "BTC/USD").
        let asset = scenario
            .asset
            .first()
            .map(|a| a.venue_symbol.clone())
            .ok_or_else(|| anyhow::anyhow!("scenario {} has empty asset list", scenario.id))?;

        let cadence = Duration::minutes(strategy.manifest.decision_cadence_minutes as i64);
        if cadence.num_seconds() <= 0 {
            anyhow::bail!(
                "strategy {} has non-positive decision_cadence_minutes",
                strategy.manifest.id
            );
        }

        let total_window = (scenario.time_window.end - scenario.time_window.start)
            .num_seconds()
            .max(1) as f64;

        let initial_balance = self.broker.balance().await?;
        let mut equity_samples: Vec<f64> = Vec::new();
        let mut decision_idx = 0u32;
        let mut n_trades = 0u32;
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        let mut reference_price_usd = configured_reference_price_usd();
        // Running peak for drawdown_pct in MetricsUpdated. Start at the
        // initial balance so the first tick's drawdown is well-defined.
        let mut peak_equity = initial_balance.max(0.0);

        let mut ts = scenario.time_window.start;
        while ts < scenario.time_window.end {
            if store.is_cancelled(&run.id).await? {
                anyhow::bail!("eval run cancelled");
            }
            // Emit RunTick before pipeline work so dashboard progress
            // bars can advance even if the LLM call is slow.
            let elapsed = (ts - scenario.time_window.start).num_seconds() as f64;
            let scenario_progress_pct = ((elapsed / total_window) * 100.0).clamp(0.0, 100.0);
            self.emit(ProgressEvent::RunTick {
                run_id: run.id.clone(),
                scenario_progress_pct,
                current_ts: ts,
            });

            let position = self.broker.position(&asset).await?;
            let balance = self.broker.balance().await?;
            let seed = serde_json::json!({
                "decision_index": decision_idx,
                "asset": asset,
                "timestamp": ts,
                "portfolio_state": {
                    "position_size": position,
                    "equity": balance,
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

            if store.is_cancelled(&run.id).await? {
                anyhow::bail!("eval run cancelled");
            }

            let trader = outs
                .trader
                .as_ref()
                .ok_or_else(|| anyhow!("run {} decision {}: trader output missing", run.id, decision_idx))?;
            let parsed = TraderOutput::parse_strict(&trader.text(), &run.id, decision_idx)?;

            let mut order_size: Option<f64> = None;
            let mut fill_price: Option<f64> = None;
            let mut fill_size: Option<f64> = None;
            let mut fee: Option<f64> = None;

            if is_actionable(&parsed.action) {
                let usd_at_risk = balance * strategy.risk.risk_pct_per_trade;
                let size = (usd_at_risk / reference_price_usd).max(0.0);
                let side = if parsed.action == "long_open" {
                    Side::Buy
                } else {
                    Side::Sell
                };
                let req = OrderRequest {
                    asset: asset.clone(),
                    side,
                    size,
                    stop_loss_pct: Some(
                        (strategy.risk.stop_loss_atr_multiple as f32).max(0.5),
                    ),
                    take_profit_pct: Some(5.0),
                    idempotency_key: format!("{}-{}", run.id, decision_idx),
                };
                let conf = self.broker.submit_order(req).await?;
                fill_price = conf.fill_price;
                if let Some(px) = conf.fill_price.filter(|px| *px > 0.0) {
                    // Keep sizing in sync with the latest executable price.
                    reference_price_usd = px;
                }
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
                timestamp: ts,
                asset: asset.clone(),
                action: parsed.action.clone(),
                conviction: Some(parsed.conviction),
                justification: Some(parsed.justification.clone()),
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
            store.record_equity(&run.id, ts, post_balance).await?;
            self.emit_chart(
                &run.id,
                RunChartEvent::Equity(ChartEquityPoint {
                    time: ts.timestamp(),
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
            ts += cadence;
        }

        if store.is_cancelled(&run.id).await? {
            anyhow::bail!("eval run cancelled");
        }

        let final_balance = self.broker.balance().await?;
        // Prepend the initial balance so equity_to_returns covers the first
        // tick's drift from the seed balance, not just inter-tick drift.
        let mut full_curve = Vec::with_capacity(equity_samples.len() + 1);
        full_curve.push(initial_balance);
        full_curve.extend_from_slice(&equity_samples);

        let returns = equity_to_returns(&full_curve);
        let periods_per_year =
            annualization_periods_per_year(strategy.manifest.decision_cadence_minutes);

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
