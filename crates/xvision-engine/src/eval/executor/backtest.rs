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
use std::time::Instant;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use xvision_core::market::Ohlcv;
use xvision_data::fixtures::load_ohlcv_fixture;

use xvision_eval::baselines::bar_baselines;

use crate::agent::llm::LlmDispatch;
use crate::agent::observability::ObsEmitter;
use crate::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use crate::agents::InputsPolicy;
use crate::api::chart::{
    ChartEquityPoint, HoldMarker, LiveDecisionRow, MarkerEvent, RunChartEvent, RunEventBus, TradeMarker,
    TradeSide,
};
use crate::eval::early_stop::{self, EarlyStopConfig};
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
use crate::eval::run::{BaselineMetrics, BaselineRelative, BaselinesReport, MetricsSummary, Run, RunStatus};
use crate::eval::scenario::{Scenario, SlippageModel};
use crate::eval::store::{DecisionRow, RunStore};
use crate::strategies::agent_ref::canonical_role;
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
    /// Optional warmup bars to prepend before the scenario window. These
    /// are not iterated for decisions — they only feed the rolling
    /// `bar_history` window in each per-decision seed so the trader LLM
    /// (and any indicator tools it invokes) has real context at bar 1.
    /// See `crates/xvision-engine/src/eval/bars.rs::load_warmup_bars`.
    warmup_bars: Vec<Ohlcv>,
    /// Optional live-stream event bus. When `Some`, the executor emits
    /// `RunChartEvent::Equity` and `RunChartEvent::Marker` events after
    /// each decision cycle so SSE subscribers at `/live/<run_id>` see
    /// real-time chart updates. When `None` (most unit tests), emission
    /// is a no-op.
    event_bus: Option<Arc<RunEventBus>>,
    /// Optional observability emitter (`qa-eval-observability-wiring`,
    /// 2026-05-17). When `Some`, every LLM dispatch inside this run
    /// emits SpanStarted / SpanFinished + ModelCallFinished on the
    /// agent-run observability bus, so failures surface in
    /// `/api/agent-runs/<run_id>` and the trace dock. `None` keeps
    /// existing unit-test paths silent.
    obs_emitter: Option<ObsEmitter>,
    /// Optional per-run hard caps. When set, the per-bar loop checks
    /// `EvalLimits::check_for_cancel` after each decision's tokens are
    /// counted; on breach the run is marked Cancelled with a stable
    /// reason string in `Run.error`. `None` (the default) preserves
    /// pre-limits behavior. See `crate::eval::limits`.
    limits: Option<super::super::limits::EvalLimits>,
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
            warmup_bars: Vec::new(),
            event_bus: None,
            obs_emitter: None,
            limits: None,
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
            warmup_bars: Vec::new(),
            event_bus: None,
            obs_emitter: None,
            limits: None,
        }
    }

    /// Both bars + progress.
    pub fn with_bars_and_progress(bars: Vec<Ohlcv>, progress: ProgressTx) -> Self {
        Self {
            progress: Some(progress),
            injected_bars: Some(bars),
            warmup_bars: Vec::new(),
            event_bus: None,
            obs_emitter: None,
            limits: None,
        }
    }

    /// Attach a live-stream event bus to an existing executor. Builder-style
    /// so callers can chain after `with_bars` / `with_progress`:
    ///   `BacktestExecutor::with_bars(bars).with_event_bus(bus)`.
    pub fn with_event_bus(mut self, bus: Arc<RunEventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Attach an observability emitter (`qa-eval-observability-wiring`).
    /// The emitter is bound to a run id and threaded down into every
    /// `execute_slot` invocation via `PipelineInputs.obs`.
    pub fn with_observability(mut self, emitter: ObsEmitter) -> Self {
        self.obs_emitter = Some(emitter);
        self
    }

    /// Pre-window warmup bars. The decision loop never iterates these;
    /// they only feed the per-decision rolling `bar_history` window in
    /// the seed. Chains with `with_bars` / `with_progress` / `with_event_bus`:
    ///   `BacktestExecutor::with_bars(bars).with_warmup(warmup)`.
    pub fn with_warmup(mut self, warmup_bars: Vec<Ohlcv>) -> Self {
        self.warmup_bars = warmup_bars;
        self
    }

    /// Attach per-run hard caps. Builder-style so callers can chain after
    /// `with_bars` / `with_warmup` / `with_event_bus`. When the limits
    /// argument's `is_empty()` returns true, the executor stores it but
    /// the per-bar check is a constant-time no-op.
    pub fn with_limits(mut self, limits: super::super::limits::EvalLimits) -> Self {
        self.limits = Some(limits);
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
        if !store.begin_running(&run.id).await? {
            anyhow::bail!("eval run stopped");
        }
        run.status = RunStatus::Running;

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
        // An N-bar window is expected to produce N decisions
        // (qa-decisions-30day-count). The final bar fills against its own
        // close via the `next_bar_open` fallback below, so the only
        // genuinely-uninterpretable case is an empty bar list. Anything
        // narrower than that is a contract bug at the loader layer, not
        // a runtime input the executor should silently tolerate.
        if bars.is_empty() {
            anyhow::bail!("scenario {} has no bars; nothing to backtest", scenario.id,);
        }

        // Used by RunTick to report bar-clock progress. Cadence can make
        // actual decisions sparser; the final bar produces a decision too
        // (it fills against its own close instead of the absent T+1 open —
        // see the `next_bar_open` fallback in the loop below).
        let total_decision_bars = bars.len().max(1) as f64;

        // Per-decision rolling-history window. Warmup bars (from
        // `eval::bars::load_warmup_bars`) are concatenated in front of the
        // scenario bars so we can slice the last `scenario.warmup_bars`
        // bars at each decision and surface them in the seed as
        // `market_data.bar_history`. The slice excludes `current_bar`
        // (already in the seed). This is the mechanism the QA15 fix
        // relies on: bar 1 of a 30-bar EMA5/EMA13 scenario sees N≥13
        // prior bars when the scenario has `warmup_bars >= 13`.
        let warmup_count = self.warmup_bars.len();
        let combined_bars: Vec<&Ohlcv> = self.warmup_bars.iter().chain(bars.iter()).collect();
        let history_window = scenario.warmup_bars as usize;

        // F-6: per-run seed-sanitization policy. Mirror of the paper
        // executor path; `Raw` (default) reproduces the pre-F-6 JSON
        // byte-for-byte so this branch is a no-op for every existing
        // scenario+strategy combination that didn't opt into `Causal`.
        let inputs_policy = resolve_inputs_policy(agent_slots);
        // F-8: optional rolling-window cap (paper executor mirror).
        // `None` keeps today's behavior; `Some(n)` trims the slice to
        // the most-recent `n` entries.
        let bar_history_limit = resolve_bar_history_limit(agent_slots);
        // F-8 stats: snapshot the global counter so we can log the
        // per-run cache-hint delta at finalize.
        let cache_hint_start =
            crate::agent::llm::CACHE_HINT_EMITTED_CALLS.load(std::sync::atomic::Ordering::Relaxed);

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
        // Wall-clock anchor for `EvalLimits::max_wall_clock_secs`. Captured
        // at the start of the decision loop, NOT at `run.started_at` — the
        // CLI cap is measured against the executor's own running window so
        // a slow scenario-load doesn't burn the operator's budget.
        let run_started: Instant = Instant::now();
        // engine-trade-guardrails-pyramid-flip-block (F-7):
        // tracks the trader's most recent emitted open direction on the
        // asset so the guardrail can detect a same-bar flip even when
        // the executor's live position is momentarily flat between a
        // close and an opposite open. Cleared on emitted `flat`. Only
        // updated from the ORIGINAL trader action — a guardrail-rewritten
        // `hold` does not bump the direction state.
        let mut last_open_direction: Option<GuardAction> = None;
        // Running peak for drawdown_pct in MetricsUpdated. Start at the
        // initial capital so the first tick's drawdown is well-defined.
        let mut peak_equity = initial.max(0.0);
        // Cadence-gated decision bars: the subset of `bars` where the strategy
        // actually fired a decision. Used post-loop to compute baselines over
        // the same bar slice the strategy saw.
        //
        // NOTE: `bars` here are the scenario bars AFTER warmup bars have been
        // separated into `self.warmup_bars` (see `with_warmup()`). Warmup bars
        // are not passed to `compute_baselines` — they are only used to seed
        // the LLM rolling-history window and are not part of the tradable window.
        // This comment references the bar-slice assignment on line ~308 above:
        //   `let bars: Vec<Ohlcv> = if let Some(injected) = ... { ... } else { ... }`
        let mut decision_bars: Vec<Ohlcv> = Vec::new();

        // eval-flat-degeneracy-early-stop (F-9): rolling history of the
        // last `cfg.window` actions + convictions, plus a counter for
        // inherited decisions still owed when the policy is in skip
        // mode. The buffer is flushed when the policy fires (so we don't
        // re-trigger immediately after the skip window ends) and on any
        // reset trigger — non-`flat`/`hold` action or a portfolio change.
        let early_stop_cfg = EarlyStopConfig::from_env_or_default();
        let mut recent_actions: Vec<early_stop::Action> = Vec::with_capacity(early_stop_cfg.window);
        let mut recent_convictions: Vec<f64> = Vec::with_capacity(early_stop_cfg.window);
        let mut inherit_remaining: u32 = 0;
        let mut prev_position: f64 = position;

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
            // Track every cadence-gated bar so baselines can replay the same
            // bar slice post-loop (see `compute_baselines` call below).
            decision_bars.push(bar.clone());

            // A decision at bar T normally fills at T+1's open. For the
            // final bar of the window there is no T+1, so the fill source
            // falls back to the same bar's close. Without this fallback
            // an N-bar scenario would silently drop the last decision and
            // produce N-1 rows in `decisions` (operator-reported off-by-
            // one — `qa-decisions-30day-count`).
            let next_bar_open = bars.get(i + 1).map(|b| b.open).unwrap_or(bar.close);

            // RunTick fires before the per-bar pipeline call so dashboards
            // can advance progress bars even when an LLM round-trip is slow.
            let scenario_progress_pct = ((i as f64 / total_decision_bars) * 100.0).clamp(0.0, 100.0);
            self.emit(ProgressEvent::RunTick {
                run_id: run.id.clone(),
                scenario_progress_pct,
                current_ts: bar.timestamp,
            });

            // History slice: last `history_window` bars strictly before
            // the current bar. `combined_idx` points at `bar` inside the
            // combined `[warmup..., bars...]` series. When the run starts
            // and `warmup_count` covers it, the slice contains
            // `history_window` real prior bars (the QA15 fix).
            let combined_idx = warmup_count + i;
            let history_start = combined_idx.saturating_sub(history_window);
            let history_slice: &[&Ohlcv] = &combined_bars[history_start..combined_idx];
            // F-8: optional rolling-window cap. `None` preserves the
            // pre-022 wire shape; `Some(n)` trims to the most-recent
            // `n` entries — when the slice is already smaller we
            // send everything that's there.
            let history_slice: &[&Ohlcv] = match bar_history_limit {
                Some(n) if (n as usize) < history_slice.len() => {
                    let take = n as usize;
                    &history_slice[history_slice.len() - take..]
                }
                _ => history_slice,
            };
            let bar_history = build_bar_history(history_slice, inputs_policy);

            // F-6: `Causal` drops `decision_index` + `timestamp` from
            // both the top-level seed and the current-bar inline.
            // `Raw` / `Oracle` keep the original shape byte-for-byte
            // — the regression-guard test pins this.
            let current_bar_json = ohlcv_to_json(bar, inputs_policy);
            let seed = match inputs_policy {
                InputsPolicy::Raw | InputsPolicy::Oracle => serde_json::json!({
                    "decision_index": decision_idx,
                    "asset": asset,
                    "timestamp": bar.timestamp,
                    "market_data": {
                        "asset": asset,
                        "current_bar": current_bar_json,
                        "next_bar_open": next_bar_open,
                        "reference_price_usd": bar.close,
                        "reference_price_source": "eval_bar.close",
                        "bar_history": bar_history,
                    },
                    "portfolio_state": {
                        "position_size": position,
                        "equity": equity,
                        "mark_price": bar.close,
                    },
                }),
                InputsPolicy::Causal => serde_json::json!({
                    "asset": asset,
                    "market_data": {
                        "asset": asset,
                        "current_bar": current_bar_json,
                        "next_bar_open": next_bar_open,
                        "reference_price_usd": bar.close,
                        "reference_price_source": "eval_bar.close",
                        "bar_history": bar_history,
                    },
                    "portfolio_state": {
                        "position_size": position,
                        "equity": equity,
                        "mark_price": bar.close,
                    },
                }),
            };

            // eval-flat-degeneracy-early-stop (F-9): before paying the
            // LLM tax, check whether we should inherit this decision as
            // a flat. Two entry paths:
            //
            //   (a) `inherit_remaining > 0` — we're mid-skip-window from
            //       a prior trigger. Keep inheriting until the counter
            //       drains. No supervisor note here; the entry-row note
            //       was already written when the policy fired.
            //
            //   (b) Policy fires NOW based on the rolling history. Write
            //       ONE supervisor-note row, set `inherit_remaining`,
            //       then fall through into the inherit branch.
            //
            // Both paths short-circuit `run_pipeline`, write an
            // `eval_decisions` row with `action=flat, conviction=0.0,
            // justification="inherited from early-stop policy"`, record
            // equity (kept dense per bar so the chart series stays
            // continuous), and `continue` to the next bar.
            let policy_plan = if inherit_remaining == 0 {
                early_stop::should_skip_next_decision(
                    &recent_actions,
                    &recent_convictions,
                    position == prev_position,
                    &early_stop_cfg,
                )
            } else {
                None
            };
            if let Some(plan) = policy_plan.as_ref() {
                tracing::info!(
                    run_id = %run.id,
                    decision_index = decision_idx,
                    skip_count = plan.skip_count,
                    "early-stop policy fired — inheriting flat decisions"
                );
                store
                    .record_supervisor_note(&run.id, "guard", "info", &plan.reason)
                    .await?;
                inherit_remaining = plan.skip_count;
                // Flush the rolling buffer so the policy can't re-fire
                // on the next bar without a fresh streak rebuilding.
                recent_actions.clear();
                recent_convictions.clear();
            }
            if inherit_remaining > 0 {
                let inherited_row = DecisionRow {
                    run_id: run.id.clone(),
                    decision_index: decision_idx,
                    timestamp: bar.timestamp,
                    asset: asset.clone(),
                    action: "flat".into(),
                    conviction: Some(0.0),
                    justification: Some("inherited from early-stop policy".into()),
                    reasoning: None,
                    order_size: None,
                    fill_price: None,
                    fill_size: None,
                    fee: None,
                    pnl_realized: None,
                };
                store.record_decision(&inherited_row).await?;
                self.emit_chart(
                    &run.id,
                    RunChartEvent::Decision(LiveDecisionRow::from(&inherited_row)),
                )
                .await;
                self.emit(ProgressEvent::DecisionEmitted {
                    run_id: run.id.clone(),
                    action: "flat".into(),
                    asset: asset.clone(),
                    size: 0.0,
                    conviction: 0.0,
                });

                // Mark equity to next bar's open with no position change
                // — `flat` on an already-flat or held position is a
                // no-op fill. The existing `simulate_fill` semantics
                // already give us this when `pos == 0`; for `pos != 0`
                // an inherited flat would close the position, which
                // would BE a portfolio change. We don't want the
                // inherit branch to mutate position state, so we update
                // equity in-place from current state instead of going
                // through simulate_fill.
                equity = initial + realized_total + position * (next_bar_open - entry_price);
                store.record_equity(&run.id, bar.timestamp, equity).await?;
                self.emit_chart(
                    &run.id,
                    RunChartEvent::Equity(ChartEquityPoint {
                        time: bar.timestamp.timestamp(),
                        equity_usd: equity,
                    }),
                )
                .await;
                equity_curve.push(equity);

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

                inherit_remaining -= 1;
                prev_position = position;
                decision_idx += 1;
                continue;
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

            // Hard-limit breach check (cli-operator-safety-p0 slice 2/3).
            // Decisions counter uses `decision_idx + 1` here because this
            // bar's decision IS counted toward the cap — the next bar
            // shouldn't run if the cap has just been reached. `is_empty()`
            // short-circuits the call when no limits are configured.
            if let Some(limits) = self.limits.as_ref() {
                if !limits.is_empty() {
                    if let Some(breach) = limits.check_for_cancel(
                        decision_idx + 1,
                        total_input_tokens,
                        total_output_tokens,
                        run_started,
                    ) {
                        let reason = breach.reason();
                        let _ = store.cancel_active(&run.id, &reason).await;
                        anyhow::bail!(reason);
                    }
                }
            }

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

            // engine-trade-guardrails-pyramid-flip-block (F-7):
            // Server-side gate at the apply seam. The trader's emitted
            // action stays in `parsed.action` (preserved verbatim in
            // `eval_decisions.action` below); `applied_action` is what
            // drives `simulate_fill` / marker derivation. A `RewriteTo`
            // also writes a `supervisor_notes` row so the operator sees
            // the block.
            let original_action = GuardAction::parse(&parsed.action);
            let position_state = position_state_from_size(pre_fill_position);
            let decision = guardrails::classify(original_action, position_state, last_open_direction);
            let applied_action: String = match &decision {
                GuardrailDecision::Allow => parsed.action.clone(),
                GuardrailDecision::RewriteTo { action, reason } => {
                    let note =
                        supervisor_note_content(*reason, original_action, *action, &asset, decision_idx);
                    store
                        .record_supervisor_note(&run.id, "guard", "warn", &note)
                        .await?;
                    tracing::warn!(
                        run_id = %run.id,
                        decision_index = decision_idx,
                        asset = %asset,
                        reason = reason.as_str(),
                        original = original_action.as_str(),
                        applied = action.as_str(),
                        "eval guardrail rewrote trader action",
                    );
                    action.as_str().to_string()
                }
            };

            // `simulate_fill` treats any non-(long_open|short_open) action
            // as `want_flat` (closes any open position). That suits a
            // trader-emitted `flat` or the guardrail one-step-flip
            // rewrite, but a guardrail pyramid-block rewrites
            // `long_open` → `hold` and we MUST NOT close the existing
            // position in that case. Short-circuit a true no-op fill
            // for `hold` so the position survives untouched.
            let fill = if applied_action == "hold" {
                FillOutcome {
                    new_pos: pre_fill_position,
                    new_entry: entry_price,
                    fill_price: None,
                    fill_size: None,
                    fee: None,
                    realized_pnl: 0.0,
                }
            } else {
                simulate_fill(SimulateFillArgs {
                    pos: pre_fill_position,
                    entry: entry_price,
                    action: &applied_action,
                    next_open: next_bar_open,
                    slip_bps,
                    taker_bps,
                    equity,
                    risk_pct: strategy.risk.risk_pct_per_trade,
                })
            };
            position = fill.new_pos;
            entry_price = fill.new_entry;
            realized_total += fill.realized_pnl;
            let fill_happened = fill.fill_price.is_some();
            if fill_happened {
                n_trades += 1;

                // FillRecorded — only when an actionable decision actually
                // crossed the book. For close-to-flat decisions, side is
                // derived from the pre-fill position direction.
                // Side is derived from the APPLIED action so a
                // guardrail-rewritten `flat` (one-step flip block) shows
                // as a close, not a phantom short_open.
                let side = fill_side_for_action(&applied_action, pre_fill_position);
                self.emit(ProgressEvent::FillRecorded {
                    run_id: run.id.clone(),
                    side: side.into(),
                    price: fill.fill_price.unwrap_or(0.0),
                    qty: fill.fill_size.unwrap_or(0.0),
                    fee: fill.fee.unwrap_or(0.0),
                });
            }

            // DecisionEmitted fires for every cycle so subscribers see
            // flat/hold decisions too. Carries the ORIGINAL trader
            // action — subscribers correlate the emitted intent with the
            // matching `eval_decisions` row, which also stores the
            // original. The supervisor_notes table carries the rewrite.
            self.emit(ProgressEvent::DecisionEmitted {
                run_id: run.id.clone(),
                action: parsed.action.clone(),
                asset: asset.clone(),
                size: fill.fill_size.unwrap_or(0.0),
                conviction: parsed.conviction,
            });

            // Update the per-asset open-direction memory for the
            // guardrail's next-cycle flip detection. Driven by the
            // APPLIED action (a guardrail-rewritten `hold` keeps the
            // existing direction; a `flat` clears it).
            match GuardAction::parse(&applied_action) {
                GuardAction::LongOpen => last_open_direction = Some(GuardAction::LongOpen),
                GuardAction::ShortOpen => last_open_direction = Some(GuardAction::ShortOpen),
                GuardAction::Flat => last_open_direction = None,
                GuardAction::Hold | GuardAction::Other => {}
            }

            // Mark equity to the next bar's open.
            equity = initial + realized_total + position * (next_bar_open - entry_price);

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
            // Markers reflect what actually hit the portfolio — so they
            // use the APPLIED action, not the trader's original.
            // The audit-side trace still has the original in
            // `eval_decisions.action`.
            let marker_event = match applied_action.as_str() {
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
                    price: next_bar_open,
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

            // eval-flat-degeneracy-early-stop (F-9): roll the buffer and
            // apply reset triggers. A portfolio change (position size
            // delta — open, close, or resize) wipes the streak; so does
            // any non-flat/non-hold action. Otherwise we append and
            // truncate to the configured window.
            let portfolio_changed = position != prev_position;
            let cls = early_stop::Action::classify(&parsed.action);
            if portfolio_changed || !matches!(cls, early_stop::Action::Flat | early_stop::Action::Hold) {
                recent_actions.clear();
                recent_convictions.clear();
            } else {
                recent_actions.push(cls);
                recent_convictions.push(parsed.conviction);
                let cap = early_stop_cfg.window;
                if recent_actions.len() > cap {
                    let drop_n = recent_actions.len() - cap;
                    recent_actions.drain(0..drop_n);
                    recent_convictions.drain(0..drop_n);
                }
            }
            prev_position = position;

            decision_idx += 1;
        }

        if store.is_terminal(&run.id).await? {
            anyhow::bail!("eval run stopped");
        }

        let returns = equity_to_returns(&equity_curve);
        let cadence_minutes = strategy.manifest.decision_cadence_minutes;
        let periods_per_year = annualization_periods_per_year(cadence_minutes);
        let strategy_return_pct = total_return_pct(initial, equity);

        // Compute the four automatic baselines over the same cadence-gated bar
        // slice the strategy saw. `decision_bars` was populated by the loop
        // above — one push per cadence-gate pass, matching the strategy's
        // iteration exactly.
        let baselines = build_baselines_report(&decision_bars, initial, cadence_minutes, strategy_return_pct);

        let metrics = MetricsSummary {
            total_return_pct: strategy_return_pct,
            sharpe: sharpe_from_returns(&returns, periods_per_year),
            max_drawdown_pct: max_drawdown_pct(&equity_curve),
            win_rate: 0.0,
            n_trades,
            n_decisions: decision_idx,
            baselines: Some(baselines),
        };

        run.actual_input_tokens = Some(total_input_tokens);
        run.actual_output_tokens = Some(total_output_tokens);
        run.metrics = Some(metrics.clone());
        run.status = RunStatus::Completed;
        // F-8 stats: log the per-run cache-hint emit count. Counter
        // is process-wide; per-run delta is the right signal because
        // the launch-concurrency gate isolates the scope.
        let cache_hint_end =
            crate::agent::llm::CACHE_HINT_EMITTED_CALLS.load(std::sync::atomic::Ordering::Relaxed);
        let cache_hint_emitted_calls = cache_hint_end.saturating_sub(cache_hint_start);
        tracing::info!(
            target: "xvision::eval",
            run_id = %run.id,
            executor = "backtest",
            cache_hint_emitted_calls,
            "eval run finalize: provider prompt-cache stats"
        );
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

/// Convert `xvision_eval`'s baseline computation result into the engine's
/// `BaselinesReport` type (stored in `metrics_json`). The two structs have
/// identical shapes but live in different crates; this function bridges them
/// without requiring the eval crate to import the engine's types.
fn build_baselines_report(
    bars: &[Ohlcv],
    initial_equity: f64,
    cadence_minutes: u32,
    strategy_return_pct: f64,
) -> BaselinesReport {
    let computed =
        bar_baselines::compute_baselines(bars, initial_equity, cadence_minutes, strategy_return_pct);
    BaselinesReport {
        buy_hold: BaselineMetrics {
            return_pct: computed.buy_hold.return_pct,
            sharpe: computed.buy_hold.sharpe,
        },
        always_flat: BaselineMetrics {
            return_pct: computed.always_flat.return_pct,
            sharpe: computed.always_flat.sharpe,
        },
        simple_trend: BaselineMetrics {
            return_pct: computed.simple_trend.return_pct,
            sharpe: computed.simple_trend.sharpe,
        },
        simple_mean_reversion: BaselineMetrics {
            return_pct: computed.simple_mean_reversion.return_pct,
            sharpe: computed.simple_mean_reversion.sharpe,
        },
        relative_to: BaselineRelative {
            buy_hold: computed.relative_to.buy_hold,
            always_flat: computed.relative_to.always_flat,
            simple_trend: computed.relative_to.simple_trend,
            simple_mean_reversion: computed.relative_to.simple_mean_reversion,
        },
    }
}

/// Serialize an Ohlcv bar as the same JSON shape used for
/// `market_data.current_bar` so `bar_history` entries are
/// homogeneous with the current-bar shape the trader prompt already
/// knows about. F-6: under `Causal` we drop `timestamp` so the
/// trader LLM can't accidentally key on a wall-clock label.
fn ohlcv_to_json(bar: &Ohlcv, policy: InputsPolicy) -> serde_json::Value {
    match policy {
        InputsPolicy::Raw | InputsPolicy::Oracle => serde_json::json!({
            "timestamp": bar.timestamp,
            "open": bar.open,
            "high": bar.high,
            "low": bar.low,
            "close": bar.close,
            "volume": bar.volume,
        }),
        InputsPolicy::Causal => serde_json::json!({
            "open": bar.open,
            "high": bar.high,
            "low": bar.low,
            "close": bar.close,
            "volume": bar.volume,
        }),
    }
}

/// Build the `bar_history` slice. Mirror of `paper::build_bar_history`
/// — kept in lock-step so the two executor paths produce identical
/// JSON under the same policy.
fn build_bar_history(bars: &[&Ohlcv], policy: InputsPolicy) -> Vec<serde_json::Value> {
    match policy {
        InputsPolicy::Raw | InputsPolicy::Oracle => bars.iter().map(|b| ohlcv_to_json(b, policy)).collect(),
        InputsPolicy::Causal => bars
            .iter()
            .enumerate()
            .map(|(i, b)| {
                serde_json::json!({
                    "bar_index": i,
                    "open": b.open,
                    "high": b.high,
                    "low": b.low,
                    "close": b.close,
                    "volume": b.volume,
                })
            })
            .collect(),
    }
}

/// Mirror of `paper::resolve_inputs_policy`. Sourced from the
/// trader-role `ResolvedAgentSlot`; defaults to `Raw` so legacy
/// strategy shapes (no attached agents) keep today's behavior.
fn resolve_inputs_policy(agent_slots: &[ResolvedAgentSlot]) -> InputsPolicy {
    agent_slots
        .iter()
        .find(|r| canonical_role(&r.role) == "trader")
        .map(|r| r.inputs_policy)
        .unwrap_or(InputsPolicy::Raw)
}

/// Mirror of `paper::resolve_bar_history_limit`. F-8 per-trader cap.
fn resolve_bar_history_limit(agent_slots: &[ResolvedAgentSlot]) -> Option<u32> {
    agent_slots
        .iter()
        .find(|r| canonical_role(&r.role) == "trader")
        .and_then(|r| r.bar_history_limit)
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
            hypothesis: None,
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
            temperature: None,
            inputs_policy: crate::agents::InputsPolicy::Raw,
            bar_history_limit: None,
        }
    }

    #[test]
    fn trader_model_id_returns_canonical_trader_model() {
        // QA #7 — trader_model_id used `eq_ignore_ascii_case` without
        // trim, so padded role variants missed the reasoning-class
        // truncation hint. Canonical comparison fixes all variants.
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

    #[test]
    fn trader_model_id_returns_none_when_no_trader() {
        let strategy = empty_strategy();
        let slots = vec![resolved("regime", "claude-opus-4-7")];
        assert!(trader_model_id(&slots, &strategy).is_none());
    }
}
