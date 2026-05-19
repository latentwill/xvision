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
use xvision_execution::broker_surface::{
    classify_broker_error_message, extract_requested_available, is_alpaca_crypto, BrokerErrorClass,
    BrokerSurface, OrderRequest, Side,
};
use xvision_observability::{BrokerCallOutcome, BrokerSide};

use crate::agent::llm::LlmDispatch;
use crate::agent::observability::{fresh_span_id, ObsEmitter};
use crate::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use crate::api::chart::{ChartEquityPoint, LiveDecisionRow, RunChartEvent, RunEventBus};
use crate::eval::executor::Executor;
use crate::eval::guardrails::{
    self as guardrails, position_state_from_size, supervisor_note_content, Action as GuardAction,
    GuardrailDecision,
};
use crate::eval::metrics::{
    annualization_periods_per_year, equity_to_returns, max_drawdown_pct, sharpe_from_returns,
    total_return_pct,
};
use crate::eval::progress::{send_event, ProgressEvent, ProgressTx};
use crate::eval::run::{MetricsSummary, Run, RunStatus};
use crate::eval::scenario::Scenario;
use crate::eval::store::{DecisionRow, RunStore};
use crate::strategies::agent_ref::canonical_role;
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
    /// Optional observability emitter (`qa-eval-observability-wiring`).
    /// See `BacktestExecutor::obs_emitter`.
    obs_emitter: Option<ObsEmitter>,
    /// Pre-submit minimum-notional gate (`risk-gate-min-notional`).
    /// When `Some(min)` and `min > 0.0`, orders with notional (size ×
    /// reference price) strictly less than `min` are vetoed before
    /// `submit_order` fires. The broker never sees them, the trace
    /// records a `BelowVenueMinNotional` veto, and the next cycle
    /// proceeds. `None` / `Some(0.0)` disables the gate (matches the
    /// pre-rule behavior on venues we haven't catalogued yet).
    min_notional_usd: Option<f64>,
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
            obs_emitter: None,
            min_notional_usd: None,
        }
    }

    pub fn with_bars(broker: Arc<dyn BrokerSurface>, bars: Vec<Ohlcv>) -> Self {
        Self {
            broker,
            injected_bars: Some(bars),
            warmup_bars: Vec::new(),
            progress: None,
            event_bus: None,
            obs_emitter: None,
            min_notional_usd: None,
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
            obs_emitter: None,
            min_notional_usd: None,
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
            obs_emitter: None,
            min_notional_usd: None,
        }
    }

    pub fn with_event_bus(mut self, bus: Arc<RunEventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Attach an observability emitter (`qa-eval-observability-wiring`).
    pub fn with_observability(mut self, emitter: ObsEmitter) -> Self {
        self.obs_emitter = Some(emitter);
        self
    }

    /// Pre-window warmup bars for the seed's rolling `bar_history`. Never
    /// iterated for decisions. Chains with `with_bars` / `with_progress` /
    /// `with_event_bus`.
    pub fn with_warmup(mut self, warmup_bars: Vec<Ohlcv>) -> Self {
        self.warmup_bars = warmup_bars;
        self
    }

    /// Wire the pre-submit minimum-notional gate
    /// (`risk-gate-min-notional`). Pass `0.0` to disable. Production
    /// call sites should derive this from `xvision_risk::RiskConfig`'s
    /// `[venues.<id>]` block (e.g. `risk_cfg.venue_limits("paper")
    /// .min_notional_usd`).
    pub fn with_min_notional_usd(mut self, min_notional_usd: f64) -> Self {
        self.min_notional_usd = Some(min_notional_usd);
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

/// Map the trader's action + the wire-level Buy/Sell `Side` onto the
/// trace-dock-visible side enum. `short_open` lands as `Short` even
/// though the underlying order is a `Sell`, so the operator sees the
/// strategy intent rather than the broker leg. `*_close` collapses
/// onto `Close` regardless of long-vs-short.
fn broker_side_for_action(action: &str, side: Side) -> BrokerSide {
    if action.ends_with("_close") || action == "close" {
        BrokerSide::Close
    } else if action == "short_open" {
        BrokerSide::Short
    } else {
        match side {
            Side::Buy => BrokerSide::Buy,
            Side::Sell => BrokerSide::Sell,
        }
    }
}

/// Compact classification for broker-call failures surfaced on the
/// trace. Mirrors the engine-side `classify_run_failure` patterns but
/// returns a string the dashboard can render verbatim without joining
/// against the eval-runs failure column.
/// Structured diagnostic the executor stashes after a recoverable
/// broker error and ships into the next bar's seed under
/// `agent_error_feedback`. The agent reads this on its NEXT decision
/// cycle and self-heals (re-decide with smaller size, flat,
/// close-first, …). Cleared on read so the agent doesn't see a
/// stale error forever.
///
/// `agent-error-feedback-self-healing` round-trip carrier.
#[derive(Debug, Clone, serde::Serialize)]
struct BrokerErrorFeedback {
    class: BrokerErrorClass,
    message: String,
    requested: Option<f64>,
    available: Option<f64>,
    asset: String,
    decision_index: u32,
}

/// Build a `DecisionRow` for a bar whose broker submit raised a
/// RECOVERABLE error. The row shows the agent's original intent
/// (action / asset / size) plus a `[<error_class>]` prefix on the
/// justification so the operator sees the failed submit alongside
/// the agent's reasoning on the decisions table.
#[allow(clippy::too_many_arguments)]
fn recoverable_broker_decision_row(
    run_id: &str,
    decision_idx: u32,
    bar: &Ohlcv,
    asset: &str,
    parsed: &crate::eval::executor::trader_output::TraderOutput,
    class: BrokerErrorClass,
    message: &str,
    requested: Option<f64>,
    available: Option<f64>,
) -> DecisionRow {
    let mut justification = format!("[{}] {}", class.as_tag(), parsed.justification.trim());
    if let (Some(req), Some(avail)) = (requested, available) {
        justification.push_str(&format!(" (requested={req:.2}, available={avail:.2})"));
    } else if !message.is_empty() {
        let snip = message.chars().take(200).collect::<String>();
        justification.push_str(&format!(" — {snip}"));
    }
    DecisionRow {
        run_id: run_id.to_string(),
        decision_index: decision_idx,
        timestamp: bar.timestamp,
        asset: asset.to_string(),
        action: parsed.action.clone(),
        conviction: Some(parsed.conviction),
        justification: Some(justification),
        reasoning: None,
        order_size: None,
        fill_price: None,
        fill_size: None,
        fee: None,
        pnl_realized: None,
    }
}

/// Find the trader slot's model id, used to decorate trader-output
/// failures with the reasoning-class hint (q15 §1). Prefers an attached
/// agent with role `trader`, then falls back to the legacy
/// `strategy.trader_slot`. Returns `None` when neither is present or
/// neither has a model pinned.
fn trader_model_id(agent_slots: &[ResolvedAgentSlot], strategy: &Strategy) -> Option<String> {
    if let Some(resolved) = agent_slots.iter().find(|r| canonical_role(&r.role) == "trader") {
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
        let combined_bars: Vec<&Ohlcv> = self.warmup_bars.iter().chain(decision_bars.iter()).collect();
        let history_window = scenario.warmup_bars as usize;

        let initial_balance = self.broker.balance().await?;
        let mut equity_samples: Vec<f64> = Vec::new();
        let mut decision_idx = 0u32;
        let mut n_trades = 0u32;
        // agent-error-feedback-self-healing: counter is for tests +
        // future metrics surface; the value lives only in this scope
        // because the run still terminates fatally on un-recoverable
        // broker errors.
        let mut n_recoverable_broker_errors: u32 = 0;
        // Most-recent recoverable broker error, fed into the NEXT
        // bar's seed under `agent_error_feedback` so the trader agent
        // can self-heal.
        let mut last_broker_error: Option<BrokerErrorFeedback> = None;

        // engine-trade-guardrails-pyramid-flip-block (F-7):
        // tracks the trader's most recent emitted open direction on the
        // asset so the guardrail can detect a one-step flip even when
        // the live broker position is momentarily flat between a close
        // and an opposite open. Cleared on emitted/applied `flat`.
        let mut last_open_direction: Option<GuardAction> = None;

        // eval-broker-error-circuit-breaker: tracks the most recent
        // recoverable broker rejection's class and the number of
        // consecutive identical rejections. The gate is the triple
        // `(error_class, severity, outcome)`:
        //
        //   - error_class — must match the previous rejection's class;
        //     switching classes resets the counter (a transient
        //     network blip should NOT push a deterministic
        //     min-order-size loop over the threshold).
        //   - severity   — only `warn`-or-higher rejections count;
        //     informational broker chatter never gates the run.
        //   - outcome    — only `rejected` (recoverable BrokerSurface
        //     errors that the agent-error-feedback path is round-
        //     tripping to the trader). Fatal errors already terminate
        //     the run on the existing error path, so they don't reach
        //     this counter.
        //
        // Successful broker outcomes (filled / accepted) reset the
        // counter — the moment the loop makes forward progress, the
        // strikes start over. On reaching `CIRCUIT_BREAKER_THRESHOLD`
        // the run aborts with a classified `repeated_broker_error`
        // anyhow chain — no further trader invocation, no further
        // broker submits.
        //
        // The threshold is hard-coded for v1 per the contract's status
        // note. Promotion to run-config or strategy-config when there's
        // operator demand: replace this constant with a read from
        // `strategy.risk` (preferred long-term home) or
        // `run.params_override`, and thread the value through.
        const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;
        let mut consecutive_broker_error_class: Option<BrokerErrorClass> = None;
        let mut consecutive_broker_error_count: u32 = 0;
        let mut consecutive_broker_error_last_msg: String = String::new();
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        // Running peak for drawdown_pct in MetricsUpdated. Start at the
        // initial balance so the first tick's drawdown is well-defined.
        let mut peak_equity = initial_balance.max(0.0);
        // Tracks the average entry price of the current open position so
        // that realized PnL can be computed on closes — mirrors the same
        // local variable in `backtest.rs` (see :353 and :475).
        // Single f64 is correct here: the bar loop processes one asset
        // (hoisted above as `asset`) so there is never more than one
        // position in scope per run.
        let mut entry_price: f64 = 0.0;

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
            let buying_power = self.broker.buying_power(&asset).await?;
            let market_data = bar_seed(&asset, bar, bar_history);
            let reference_price_usd = bar.close;
            let mut seed = serde_json::json!({
                "decision_index": decision_idx,
                "asset": asset,
                "timestamp": bar.timestamp,
                "market_data": market_data,
                "portfolio_state": {
                    "position_size": position,
                    "equity": balance,
                    // Settled cash (for crypto) or buying_power (for equities).
                    // This is the hard cap on the next buy — equity is not.
                    "buying_power": buying_power,
                    "mark_price": reference_price_usd,
                },
            });
            // agent-error-feedback-self-healing: inject the most-
            // recent recoverable broker error so the trader agent
            // can self-heal on the next cycle (re-decide with a
            // smaller size, flat, close-first, etc.). The field is
            // CONSUMED on read — clearing it here means each error
            // is delivered exactly once and the agent doesn't see
            // stale feedback forever.
            if let Some(fb) = last_broker_error.take() {
                if let Some(obj) = seed.as_object_mut() {
                    obj.insert(
                        "agent_error_feedback".into(),
                        serde_json::to_value(&fb).unwrap_or(serde_json::Value::Null),
                    );
                }
            }

            let outs = run_pipeline(PipelineInputs {
                strategy,
                agent_slots,
                seed_inputs: seed,
                dispatch: dispatch.clone(),
                tools: tools.clone(),
                obs: self.obs_emitter.clone(),
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

            let pre_fill_position = position;
            let mut order_size: Option<f64> = None;
            let mut fill_price: Option<f64> = None;
            let mut fill_size: Option<f64> = None;
            let mut fee: Option<f64> = None;
            let mut realized_pnl: Option<f64> = None;

            // engine-trade-guardrails-pyramid-flip-block (F-7):
            // Server-side gate at the apply seam. The trader's emitted
            // action stays in `parsed.action` (preserved verbatim in
            // `eval_decisions.action` below); `applied_action` is what
            // the broker-submit planner sees. A non-`Allow` outcome
            // writes a `supervisor_notes` row so the operator sees the
            // block in the trace dock.
            //
            // This supersedes the legacy inline "already long/short"
            // no-op below — the typed guardrail handles pyramid AND
            // one-step flip uniformly. The legacy inline check is left
            // as a defence-in-depth catch (it short-circuits the broker
            // for the same situations) but the supervisor_notes row
            // here is the canonical audit trail.
            let original_action_g = GuardAction::parse(&parsed.action);
            let position_state_g = position_state_from_size(position);
            let guard_decision =
                guardrails::classify(original_action_g, position_state_g, last_open_direction);
            let applied_action: String = match &guard_decision {
                GuardrailDecision::Allow => parsed.action.clone(),
                GuardrailDecision::RewriteTo { action, reason } => {
                    let note = supervisor_note_content(
                        *reason,
                        original_action_g,
                        *action,
                        &asset,
                        decision_idx,
                    );
                    store
                        .record_supervisor_note(&run.id, "guard", "warn", &note)
                        .await?;
                    tracing::warn!(
                        run_id = %run.id,
                        decision_index = decision_idx,
                        asset = %asset,
                        reason = reason.as_str(),
                        original = original_action_g.as_str(),
                        applied = action.as_str(),
                        "eval guardrail rewrote trader action",
                    );
                    action.as_str().to_string()
                }
            };

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
            // Plan dispatch reads `applied_action` (the post-guardrail
            // action). The existing inline already-long / already-short
            // no-ops stay in place as defence-in-depth (the guardrail
            // would also catch them).
            //
            // Guardrail-rewritten `flat` (one-step flip block) is a
            // CLOSE: it must hit the broker to actually flatten the
            // position. Trader-emitted `flat` keeps the legacy v1
            // semantics of "no broker submit" — preserving the
            // `paper_executor_skips_broker_for_flat_decisions`
            // invariant. The two are distinguished by inspecting
            // `guard_decision`.
            let guard_rewrote_to_flat = matches!(
                &guard_decision,
                GuardrailDecision::RewriteTo { action: GuardAction::Flat, .. }
            );
            let plan: Option<(Side, f64)> = if guard_rewrote_to_flat {
                // Close any open position. Crypto venues are long-only
                // on Alpaca; closing a long means a sell sized to the
                // long. With no open position the flip block is
                // effectively a no-op.
                if position > 0.0 {
                    Some((Side::Sell, position))
                } else if position < 0.0 {
                    Some((Side::Buy, position.abs()))
                } else {
                    None
                }
            } else if !is_actionable(&applied_action) {
                None
            } else if applied_action == "short_open" && is_alpaca_crypto(&asset) {
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
            } else if applied_action == "long_open" && position > 0.0 {
                // Already long this asset: don't pile on. Re-running long_open
                // every cycle is the failure mode that produced run
                // 01KRWZHHSXAWHRZSG1X65CZMCD — 29 consecutive long_open
                // requests after the first fill, all rejected for insufficient
                // cash. The decision is still recorded so the trace shows the
                // agent's intent; we just don't submit the order. The
                // typed guardrail above already wrote a supervisor_notes
                // row for this case.
                None
            } else if applied_action == "short_open" && position < 0.0 {
                // Symmetric: already short.
                None
            } else {
                // Size against *buying power* (settled cash for crypto), not
                // equity. `balance` above is equity (cash + open-position
                // mark-to-market); after the first fill it stays roughly
                // constant while cash drops, so equity-based sizing chronically
                // overshoots available cash and Alpaca returns 403
                // "insufficient balance for USD".
                let buying_power = self.broker.buying_power(&asset).await?;
                let usd_at_risk = buying_power * strategy.risk.risk_pct_per_trade;
                let size = (usd_at_risk / reference_price_usd).max(0.0);
                let side = if applied_action == "long_open" {
                    Side::Buy
                } else {
                    Side::Sell
                };
                Some((side, size))
            };

            // risk-gate-min-notional: pre-submit veto when the planned
            // order's notional (size × reference price) is below the
            // venue's configured minimum. The broker never sees a
            // known-bad order; the operator sees a clean
            // `BelowVenueMinNotional` veto in the trace instead of an
            // opaque broker rejection on the post-submit path.
            //
            // Parallel-safe with `eval-broker-error-circuit-breaker`:
            // that track owns the post-submit rejection path; this gate
            // is strictly pre-submit and short-circuits before the
            // broker.call span fires.
            let min_notional_veto = self.min_notional_usd.filter(|m| *m > 0.0).and_then(|min| {
                plan.and_then(|(_side, size)| {
                    let notional = size * reference_price_usd;
                    if size > 0.0 && notional > 0.0 && notional < min {
                        Some((notional, min))
                    } else {
                        None
                    }
                })
            });
            if let Some((notional, min)) = min_notional_veto {
                tracing::warn!(
                    run_id = %run.id,
                    decision_index = decision_idx,
                    asset = %asset,
                    notional,
                    min_notional_usd = min,
                    "MinNotional veto (pre-submit): order below venue minimum"
                );
                let justification = format!(
                    "[below_venue_min_notional] {} (notional=${:.4}, min=${:.2})",
                    parsed.justification.trim(),
                    notional,
                    min,
                );
                let row = DecisionRow {
                    run_id: run.id.clone(),
                    decision_index: decision_idx,
                    timestamp: bar.timestamp,
                    asset: asset.clone(),
                    action: parsed.action.clone(),
                    conviction: Some(parsed.conviction),
                    justification: Some(justification),
                    reasoning: None,
                    order_size: None,
                    fill_price: None,
                    fill_size: None,
                    fee: None,
                    pnl_realized: None,
                };
                store.record_decision(&row).await?;
                self.emit_chart(&run.id, RunChartEvent::Decision(LiveDecisionRow::from(&row)))
                    .await;
                self.emit(ProgressEvent::DecisionEmitted {
                    run_id: run.id.clone(),
                    action: parsed.action.clone(),
                    asset: asset.clone(),
                    size: 0.0,
                    conviction: parsed.conviction,
                });
                // Equity unchanged (no fill); record so the chart series
                // stays dense per bar — same pattern as the recoverable
                // broker-error path above.
                let balance_now = self.broker.balance().await?;
                store.record_equity(&run.id, bar.timestamp, balance_now).await?;
                equity_samples.push(balance_now);
                decision_idx += 1;
                continue;
            }

            if let Some((side, size)) = plan {
                // Hold this; the strike state may be reset below on
                // successful submit. The no-plan branch resets too
                // (see else block) so a non-submit tick — hold, flat,
                // already-long, etc. — breaks any in-flight rejection
                // streak. Without that, a `hold` between two rejections
                // would still count as 2 strikes against the threshold;
                // see PR #320 review (P2).
                let idempotency_key = format!("{}-{}", run.id, decision_idx);
                let req = OrderRequest {
                    asset: asset.clone(),
                    side,
                    size,
                    reference_price_usd,
                    stop_loss_pct: Some((strategy.risk.stop_loss_atr_multiple as f32).max(0.5)),
                    take_profit_pct: Some(5.0),
                    idempotency_key: idempotency_key.clone(),
                };

                // qa-trace-broker-spans: wrap every broker submit in a
                // `broker.call` span so the operator can audit Buy /
                // Sell / Close / Short submissions in the trace dock.
                // The `BrokerSide::{Close, Short}` intents are derived
                // from the trader's action, not the wire-level
                // Buy/Sell — short-sale fills (#14 round-2 intake)
                // surface as side=Short even though the underlying
                // order is a Sell.
                // Trace side derived from the APPLIED action so a
                // guardrail-rewritten `flat` reads as a close, not the
                // trader's original short_open intent.
                let trace_side = broker_side_for_action(&applied_action, side);
                let span_id_opt = self.obs_emitter.as_ref().map(|_| fresh_span_id());
                if let (Some(em), Some(sid)) = (self.obs_emitter.as_ref(), span_id_opt.as_deref()) {
                    em.emit_broker_call_started(
                        sid,
                        None,
                        trace_side,
                        asset.clone(),
                        size as f64,
                        Some(reference_price_usd as f64),
                        "market".to_string(),
                        "paper".to_string(),
                        Some(idempotency_key.clone()),
                    )
                    .await;
                }

                let submit_res = self.broker.submit_order(req).await.with_context(|| {
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
                });

                let conf = match submit_res {
                    Ok(conf) => {
                        if let (Some(em), Some(sid)) = (self.obs_emitter.as_ref(), span_id_opt.as_deref()) {
                            em.emit_broker_call_finished(
                                sid,
                                BrokerCallOutcome::Filled,
                                conf.fill_price.map(|p| p as f64),
                                Some(conf.fill_size as f64),
                                conf.fee.map(|f| f as f64),
                                Some(conf.broker_order_id.clone()),
                                None,
                                None,
                                None,
                            )
                            .await;
                        }
                        // eval-broker-error-circuit-breaker: successful
                        // outcome resets the consecutive-rejection
                        // counter. Any forward progress wipes the
                        // strike state — the next sequence has to
                        // build from scratch.
                        consecutive_broker_error_class = None;
                        consecutive_broker_error_count = 0;
                        consecutive_broker_error_last_msg.clear();
                        conf
                    }
                    Err(e) => {
                        // agent-error-feedback-self-healing: classify
                        // the broker error and split recoverable vs
                        // fatal. Recoverable errors mark the
                        // broker.call span as severity=warn and stash
                        // a `BrokerErrorFeedback` on the executor so
                        // the next bar's seed injects the diagnostic
                        // into the trader agent. Fatal errors keep
                        // the existing terminate path.
                        let msg = format!("{e:#}");
                        let class = classify_broker_error_message(&msg);
                        let (requested, available) = extract_requested_available(&msg);
                        let severity = if class.is_recoverable() { "warn" } else { "error" };
                        let outcome = if class.is_recoverable() {
                            BrokerCallOutcome::Rejected
                        } else {
                            BrokerCallOutcome::Failed
                        };
                        if let (Some(em), Some(sid)) = (self.obs_emitter.as_ref(), span_id_opt.as_deref()) {
                            em.emit_broker_call_finished(
                                sid,
                                outcome,
                                None,
                                None,
                                None,
                                None,
                                Some(class.as_tag().to_string()),
                                Some(msg.clone()),
                                Some(severity),
                            )
                            .await;
                        }
                        if class.is_recoverable() {
                            last_broker_error = Some(BrokerErrorFeedback {
                                class,
                                message: msg.clone(),
                                requested,
                                available,
                                asset: asset.clone(),
                                decision_index: decision_idx,
                            });
                            // The executor doesn't have a
                            // confirmation, so skip the trade counter
                            // / equity update but DO record the
                            // decision row so the operator sees the
                            // failed submit alongside the agent's
                            // intent. The next iteration's seed
                            // surfaces `last_broker_error` to the
                            // trader so it can self-heal.
                            self.emit(ProgressEvent::DecisionEmitted {
                                run_id: run.id.clone(),
                                action: parsed.action.clone(),
                                asset: asset.clone(),
                                size: 0.0,
                                conviction: parsed.conviction,
                            });
                            let row = recoverable_broker_decision_row(
                                &run.id,
                                decision_idx,
                                bar,
                                &asset,
                                &parsed,
                                class,
                                &msg,
                                requested,
                                available,
                            );
                            store.record_decision(&row).await?;
                            self.emit_chart(&run.id, RunChartEvent::Decision(LiveDecisionRow::from(&row)))
                                .await;
                            // Equity is unchanged (no fill); still
                            // record it so the chart series stays
                            // dense per bar.
                            let balance_now = self.broker.balance().await?;
                            store.record_equity(&run.id, bar.timestamp, balance_now).await?;
                            equity_samples.push(balance_now);
                            n_recoverable_broker_errors += 1;
                            tracing::warn!(
                                run_id = %run.id,
                                decision_index = decision_idx,
                                error_class = class.as_tag(),
                                n_recoverable = n_recoverable_broker_errors,
                                "recoverable broker error fed back to agent for next cycle",
                            );

                            // eval-broker-error-circuit-breaker: gate
                            // on the `(error_class, severity, outcome)`
                            // triple. We are inside the
                            // `class.is_recoverable()` branch, so:
                            //   - severity is `warn` (set above for
                            //     recoverable rejections);
                            //   - outcome is `BrokerCallOutcome::Rejected`
                            //     (also set above for the recoverable
                            //     branch).
                            // The only varying axis at this point is
                            // `error_class`. Increment when it matches
                            // the previous strike's class; reset to
                            // 1-with-new-class when it doesn't.
                            if consecutive_broker_error_class == Some(class) {
                                consecutive_broker_error_count += 1;
                            } else {
                                consecutive_broker_error_class = Some(class);
                                consecutive_broker_error_count = 1;
                            }
                            consecutive_broker_error_last_msg = msg.clone();

                            if consecutive_broker_error_count >= CIRCUIT_BREAKER_THRESHOLD {
                                // Structured failure reason consumed by
                                // `classify_run_failure` (extended with
                                // `repeated_broker_error` class tag) and
                                // surfaced in `EvalResult` / `RunSummary`
                                // via the persisted error string. Format
                                // is parsed by no one — the tag prefix
                                // is the wire contract, the body is
                                // human-readable diagnostic text
                                // including the offending class, the
                                // count, and the last broker message.
                                let summary = format!(
                                    "repeated_broker_error: aborted after {count} consecutive {class_tag} rejections; \
                                     run_id={run_id} decision_index={decision_idx} asset={asset} \
                                     last_error={last_msg}",
                                    count = consecutive_broker_error_count,
                                    class_tag = class.as_tag(),
                                    run_id = run.id,
                                    decision_idx = decision_idx,
                                    asset = asset,
                                    last_msg = consecutive_broker_error_last_msg,
                                );
                                tracing::error!(
                                    run_id = %run.id,
                                    decision_index = decision_idx,
                                    error_class = class.as_tag(),
                                    count = consecutive_broker_error_count,
                                    "eval circuit breaker tripped — aborting run",
                                );
                                // Exit on the SAME iteration that hit
                                // the threshold — no further trader
                                // invocation, no further broker
                                // submit. The outer `Executor::run`
                                // wrapper persists the failure with
                                // the classified `[repeated_broker_error]`
                                // prefix via `format_failure_reason`.
                                return Err(anyhow!(summary));
                            }

                            decision_idx += 1;
                            continue;
                        } else {
                            return Err(e);
                        }
                    }
                };
                fill_price = conf.fill_price;
                fill_size = Some(conf.fill_size);
                fee = conf.fee;
                order_size = Some(size);
                n_trades += 1;

                // Compute realized PnL for this fill using the same
                // formula as `backtest::simulate_fill`:
                //   realized = pre_fill_position × (fill_price − entry_price) − fee
                // When the pre-fill position is zero the fill is a pure open;
                // only the fee is realized (negative).
                let fp = fill_price.unwrap_or(0.0);
                let fee_paid = conf.fee.unwrap_or(0.0);
                let raw_pnl = if pre_fill_position != 0.0 {
                    pre_fill_position * (fp - entry_price)
                } else {
                    0.0
                };
                realized_pnl = Some(raw_pnl - fee_paid);

                // Update entry_price for the next cycle. After a close-to-
                // flat the new position is 0 → reset to 0.0. After an open
                // or partial reduce, the fill price becomes the new average
                // entry. This mirrors `fill.new_entry` in backtest.rs (:762).
                let new_pos = self.broker.position(&asset).await.unwrap_or(0.0);
                entry_price = if new_pos == 0.0 { 0.0 } else { fp };

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
            } else {
                // No broker submit attempted this iteration (hold /
                // flat / already-long / already-short / short_open on
                // long-only venue with no long open). Reset the
                // consecutive-rejection strike state so a non-rejection
                // tick can't be counted against the circuit-breaker
                // threshold. Without this, the sequence
                //   reject → reject → hold → reject
                // would trip on the third reject as "3 consecutive"
                // even though a non-rejecting hold sat in between.
                // See PR #320 review (P2).
                consecutive_broker_error_class = None;
                consecutive_broker_error_count = 0;
                consecutive_broker_error_last_msg.clear();
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
                pnl_realized: realized_pnl,
            };
            store.record_decision(&decision_row).await?;
            self.emit_chart(
                &run.id,
                RunChartEvent::Decision(LiveDecisionRow::from(&decision_row)),
            )
            .await;

            // engine-trade-guardrails-pyramid-flip-block (F-7):
            // Update per-asset open-direction memory for the next
            // cycle's flip detection. Driven by the APPLIED action so
            // a guardrail-rewritten `hold` keeps the existing direction
            // and a rewritten `flat` (one-step flip block) clears it.
            match GuardAction::parse(&applied_action) {
                GuardAction::LongOpen => last_open_direction = Some(GuardAction::LongOpen),
                GuardAction::ShortOpen => last_open_direction = Some(GuardAction::ShortOpen),
                GuardAction::Flat => last_open_direction = None,
                GuardAction::Hold | GuardAction::Other => {}
            }

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

#[cfg(test)]
mod role_tests {
    use super::*;
    use crate::strategies::manifest::{PublicManifest, RegimeFit};
    use crate::strategies::risk::RiskPreset;
    use crate::strategies::slot::LLMSlot;
    use crate::strategies::{PipelineDef, Strategy};

    fn empty_strategy() -> Strategy {
        Strategy {
            manifest: PublicManifest {
                id: "01H8N7Z000".into(),
                display_name: "T".into(),
                plain_summary: "x".into(),
                creator: "@t".into(),
                template: "mean_reversion".into(),
                regime_fit: vec![RegimeFit::RangeBound],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 15,
                required_models: vec!["m".into()],
                required_tools: vec!["ohlcv".into()],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
            },
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({}),
        }
    }

    fn resolved(role: &str, model: &str) -> ResolvedAgentSlot {
        ResolvedAgentSlot {
            role: role.into(),
            slot: LLMSlot {
                role: role.into(),
                prompt: "p".into(),
                model_requirement: model.into(),
                allowed_tools: Vec::new(),
                provider: None,
                model: Some(model.into()),
            },
            max_tokens: None,
        }
    }

    #[test]
    fn trader_model_id_returns_canonical_trader_model() {
        // QA #7 — see equivalent test in backtest.rs.
        let strategy = empty_strategy();
        for variant in [" trader ", "Trader", "TRADER", "trader"] {
            let slots = vec![resolved(variant, "claude-opus-4-7")];
            assert_eq!(
                trader_model_id(&slots, &strategy).as_deref(),
                Some("claude-opus-4-7"),
                "role variant `{variant}` should resolve to the trader model",
            );
        }
    }
}
