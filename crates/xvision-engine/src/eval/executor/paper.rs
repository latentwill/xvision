//! `PaperExecutor` — drives a strategy against a `BrokerSurface` (e.g.
//! Alpaca paper). Records every decision and post-tick balance to the
//! `RunStore`. Computes naive metrics on completion (Sharpe + drawdown
//! refinement lands with the Phase 3.C metrics module).
//!
//! Use `PaperExecutor::new(Arc<dyn BrokerSurface>)`. In production the
//! broker is `AlpacaPaperSurface::from_env()` (PR #5). In tests the
//! broker is `MockBrokerSurface` (PR #5) so no network is required.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::Duration;
use serde::Deserialize;
use xvision_execution::broker_surface::{BrokerSurface, OrderRequest, Side};

use crate::agent::llm::LlmDispatch;
use crate::agent::pipeline::{run_pipeline, PipelineInputs};
use crate::bundle::StrategyBundle;
use crate::eval::executor::Executor;
use crate::eval::metrics::{
    annualization_periods_per_year, equity_to_returns, max_drawdown_pct, sharpe_from_returns,
    total_return_pct,
};
use crate::eval::run::{MetricsSummary, Run, RunStatus};
use crate::eval::scenario::Scenario;
use crate::eval::store::{DecisionRow, RunStore};
use crate::tools::ToolRegistry;

/// Reference base-asset price used to size orders in base units when the
/// broker doesn't expose a live quote method. Production AlpacaPaperSurface
/// recomputes notional from `get_position(symbol).current_price` internally
/// — this constant is only the basis for the *base-asset units* number we
/// hand the broker. v1 BTC-only.
///
/// Future: lift this into a `BrokerSurface::quote(asset)` method or a
/// dedicated price-discovery dependency. Tracked for v1.1.
const BTC_REFERENCE_PRICE_USD: f64 = 70_000.0;

pub struct PaperExecutor {
    broker: Arc<dyn BrokerSurface>,
}

impl PaperExecutor {
    pub fn new(broker: Arc<dyn BrokerSurface>) -> Self {
        Self { broker }
    }
}

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

fn is_actionable(action: &str) -> bool {
    matches!(action, "long_open" | "short_open")
}

#[async_trait]
impl Executor for PaperExecutor {
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
            .ok_or_else(|| anyhow::anyhow!("scenario {} has empty asset_universe", scenario.id))?
            .clone();

        let cadence = Duration::minutes(bundle.manifest.decision_cadence_minutes as i64);
        if cadence.num_seconds() <= 0 {
            anyhow::bail!(
                "bundle {} has non-positive decision_cadence_minutes",
                bundle.manifest.id
            );
        }

        let initial_balance = self.broker.balance().await?;
        let mut equity_samples: Vec<f64> = Vec::new();
        let mut decision_idx = 0u32;
        let mut n_trades = 0u32;
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;

        let mut ts = scenario.time_window.start;
        while ts < scenario.time_window.end {
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

            let mut order_size: Option<f64> = None;
            let mut fill_price: Option<f64> = None;
            let mut fill_size: Option<f64> = None;
            let mut fee: Option<f64> = None;

            if is_actionable(&parsed.action) {
                let usd_at_risk = balance * bundle.risk.risk_pct_per_trade;
                let size = (usd_at_risk / BTC_REFERENCE_PRICE_USD).max(0.0);
                let req = OrderRequest {
                    asset: asset.clone(),
                    side: if parsed.action == "long_open" {
                        Side::Buy
                    } else {
                        Side::Sell
                    },
                    size,
                    stop_loss_pct: Some(
                        (bundle.risk.stop_loss_atr_multiple as f32).max(0.5),
                    ),
                    take_profit_pct: Some(5.0),
                    idempotency_key: format!("{}-{}", run.id, decision_idx),
                };
                let conf = self.broker.submit_order(req).await?;
                fill_price = conf.fill_price;
                fill_size = Some(conf.fill_size);
                fee = conf.fee;
                order_size = Some(size);
                n_trades += 1;
            }

            store
                .record_decision(&DecisionRow {
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
                })
                .await?;

            let post_balance = self.broker.balance().await?;
            store.record_equity(&run.id, ts, post_balance).await?;
            equity_samples.push(post_balance);

            decision_idx += 1;
            ts += cadence;
        }

        let final_balance = self.broker.balance().await?;
        // Prepend the initial balance so equity_to_returns covers the first
        // tick's drift from the seed balance, not just inter-tick drift.
        let mut full_curve = Vec::with_capacity(equity_samples.len() + 1);
        full_curve.push(initial_balance);
        full_curve.extend_from_slice(&equity_samples);

        let returns = equity_to_returns(&full_curve);
        let periods_per_year =
            annualization_periods_per_year(bundle.manifest.decision_cadence_minutes);

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
