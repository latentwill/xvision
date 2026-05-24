//! `BacktestExecutor` — replays an OHLCV fixture in chronological order,
//! invoking the strategy's pipeline at each decision boundary and simulating
//! fills against the next bar's open with linear slippage + taker fees. No
//! broker is involved; positions and equity are tracked in-memory.
//!
//! This is the v1 demo path that doesn't require external broker keys.
//! Pair with `xvn eval run --mode backtest --strategy <id> --scenario <id>`.
//!
//! Out of scope (deferred):
//! - Multi-asset fan-out (resolves a single asset from
//!   `strategy.manifest.asset_universe[0]` for now; per-asset fan-out is a
//!   later task, same staging as PaperExecutor).
//! - Indicator panel injection into the pipeline seed (matching what
//!   PaperExecutor passes today, which is just portfolio_state).
//! - Win-rate sourced from realized-PnL pairs across decisions (the
//!   `MetricsSummary.win_rate` is left at 0.0 the same way PaperExecutor
//!   leaves it — Phase 3.C work).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Utc;
use ulid::Ulid;
use xvision_core::market::Ohlcv;
use xvision_core::providers::Catalog;
use xvision_data::fixtures::load_ohlcv_fixture;

use xvision_eval::baselines::bar_baselines;

use crate::agent::llm::LlmDispatch;
use crate::agent::observability::ObsEmitter;
use crate::agent::pipeline::{run_pipeline, PipelineInputs, ResolvedAgentSlot};
use crate::agent::recovery::{
    is_malformed_json_recoverable, is_schema_missing_field_recoverable, try_repair_malformed_json,
    try_repair_schema_missing_field, TraderRepairContext,
};
use crate::agents::InputsPolicy;
use crate::api::chart::{
    ChartEquityPoint, HoldMarker, LiveDecisionRow, MarkerEvent, RunChartEvent, RunEventBus, TradeMarker,
    TradeSide,
};
use crate::eval::broker_rules::{
    rule_set_for_asset_class, BrokerRuleSet, BrokerViolationSeverity, OrderKind, PendingOrder, TimeInForce,
};
use crate::eval::cost_arrays::BarCostTable;
use crate::eval::early_stop::{self, EarlyStopConfig};
use crate::eval::executor::trace_types::{AggressorSide, FillBranch};
use crate::eval::executor::traits::{
    BarSource, Clock, FillRecord, FillRequest, FillSink, InjectedBars, InstantClock, SimulatedFills,
};
use crate::eval::executor::Executor;
use crate::eval::findings::{make_volume_share_excess_finding, Finding, Severity};
use crate::eval::guardrails::{
    self as guardrails, position_state_from_size, supervisor_note_content, Action as GuardAction,
    GuardrailDecision,
};
use crate::eval::metrics::{
    annualization_periods_per_year, equity_to_returns, max_drawdown_pct, sharpe_from_returns,
    total_return_pct,
};
#[cfg(test)]
use crate::eval::orders::OrderState;
use crate::eval::progress::{send_event, ProgressEvent, ProgressTx};
use crate::eval::run::{BaselineMetrics, BaselineRelative, BaselinesReport, MetricsSummary, Run, RunStatus};
use crate::eval::scenario::{FeeSource, FillProvenance, Scenario, SlippageModel, VenueOverride};
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
    /// V2D cortex-memory recorder. Built once at server start
    /// (`ApiContext.memory_recorder`) and threaded through
    /// `with_memory_recorder` here so every `run_pipeline` invocation
    /// can pass it down into `execute_slot` for recall/write. `None`
    /// keeps the dispatcher's memory seam dormant.
    memory_recorder: Option<std::sync::Arc<crate::agent::memory_recorder::MemoryRecorder>>,
    /// Cached provider catalogs for context-overflow recovery. The
    /// pipeline uses the slot provider to pick the matching catalog and
    /// select a cheap summarizer model. Empty by default for tests and
    /// legacy callers.
    provider_catalogs: HashMap<String, Arc<Catalog>>,
    /// Optional per-run hard caps. When set, the per-bar loop checks
    /// `EvalLimits::check_for_cancel` after each decision's tokens are
    /// counted; on breach the run is marked Cancelled with a stable
    /// reason string in `Run.error`. `None` (the default) preserves
    /// pre-limits behavior. See `crate::eval::limits`.
    limits: Option<super::super::limits::EvalLimits>,
    /// Phase D — unified `Recorder` (typically an `EvalRecorder` that
    /// mirrors rows into both a `TraceBuf` and the `xvn.db` recorder
    /// tables). When `None`, the executor keeps the legacy bus-driven
    /// emission path. The recorder-symmetry regression test wires this
    /// explicitly to assert F-11(f) closure.
    recorder: Option<Arc<dyn xvision_observability::Recorder>>,
    /// Multi-asset (B4): optional per-run narrowing of the strategy's
    /// `asset_universe`. `None` (the default) runs the full universe; a
    /// later CLI task sets `Some(subset)` to honor an `--assets` flag.
    /// Validated against the universe by `active_assets`.
    asset_subset: Option<Vec<xvision_core::trading::AssetSymbol>>,
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
            memory_recorder: None,
            provider_catalogs: HashMap::new(),
            limits: None,
            recorder: None,
            asset_subset: None,
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
            memory_recorder: None,
            provider_catalogs: HashMap::new(),
            limits: None,
            recorder: None,
            asset_subset: None,
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
            memory_recorder: None,
            provider_catalogs: HashMap::new(),
            limits: None,
            recorder: None,
            asset_subset: None,
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

    /// Phase D — attach a unified `Recorder` (typically an
    /// `EvalRecorder`). Once wired, every `dispatch_capability`
    /// invocation routes its row-typed writes through this recorder so
    /// the eval-executor surface produces rows symmetric to the
    /// harness path (F-11(f) closure).
    pub fn with_recorder(mut self, recorder: Arc<dyn xvision_observability::Recorder>) -> Self {
        self.recorder = Some(recorder);
        self
    }

    /// Attach the V2D cortex-memory recorder. When present, every
    /// `run_pipeline` invocation threads it into `SlotInput.memory` so
    /// slots whose `memory_mode != Off` actually consult / write the
    /// memory store. `None` (the default) leaves the seam dormant.
    pub fn with_memory_recorder(
        mut self,
        recorder: std::sync::Arc<crate::agent::memory_recorder::MemoryRecorder>,
    ) -> Self {
        self.memory_recorder = Some(recorder);
        self
    }

    /// Attach cached provider catalogs for context-overflow recovery.
    pub fn with_provider_catalogs(mut self, catalogs: HashMap<String, Arc<Catalog>>) -> Self {
        self.provider_catalogs = catalogs;
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

    /// Multi-asset (B4): narrow the run to a subset of the strategy's
    /// `asset_universe`. Builder-style; chains after `with_bars` etc.
    /// `None` (the default) runs the full universe. Validated against
    /// the universe by `active_assets` at run start.
    pub fn with_asset_subset(mut self, subset: Vec<xvision_core::trading::AssetSymbol>) -> Self {
        self.asset_subset = Some(subset);
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
    #[allow(clippy::too_many_arguments)]
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
        // Scenarios are asset-free; the asset a run trades comes from the
        // strategy's `asset_universe` (single-asset for now — a later task
        // adds the per-asset loop).
        use std::str::FromStr;
        let asset_sym = xvision_core::trading::AssetSymbol::from_str(
            strategy
                .manifest
                .asset_universe
                .first()
                .ok_or_else(|| anyhow!("strategy {} has empty asset_universe", strategy.manifest.id))?,
        )
        .map_err(|e| anyhow!("{e}"))?;
        let asset = asset_sym.as_alpaca_pair();

        let cadence_min = strategy.manifest.decision_cadence_minutes as i64;
        if cadence_min <= 0 {
            anyhow::bail!(
                "strategy {} has non-positive decision_cadence_minutes",
                strategy.manifest.id
            );
        }

        // Multi-asset (B4): v1 implements `execution_mode = PerAsset` +
        // `capital_mode = Pooled` only. Other modes parse + validate but
        // the executor returns a clear not-yet-implemented error so the
        // operator sees the limit instead of silently-wrong accounting.
        use crate::strategies::{CapitalMode, ExecutionMode};
        match &strategy.manifest.execution_mode {
            ExecutionMode::PerAsset => {}
            ExecutionMode::Portfolio => anyhow::bail!("execution_mode `portfolio` not yet implemented"),
            ExecutionMode::Custom(name) => {
                anyhow::bail!("execution_mode `custom:{name}` not yet implemented")
            }
        }
        if strategy.manifest.capital_mode != CapitalMode::Pooled {
            anyhow::bail!("capital_mode `per_asset` not yet implemented");
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

        // executor-trait-extraction (sub-track 1): the per-bar loop is
        // now driven by the BarSource trait, timestamp progression by
        // the Clock trait, and fill production by the FillSink trait.
        // Trait-object dispatch keeps codegen reasonable when sub-track
        // 3 lands the Live impl on the same `BacktestExecutor` shape.
        //
        // The BarSource owns its own copy of the bars (clone is cheap —
        // `Ohlcv` is `Copy`-like data); the executor keeps the original
        // `bars: Vec<Ohlcv>` for indexed T+1 look-ahead and for the
        // post-loop `compute_baselines` call. The two stay in lockstep
        // because the BarSource is constructed from the same vector and
        // is drained in step with the indexed iteration below.
        let mut bar_source: Box<dyn BarSource> = Box::new(InjectedBars::new(bars.clone()));
        let mut clock: Box<dyn Clock> = Box::new(InstantClock::new());
        let mut fill_sink: Box<dyn FillSink> = Box::new(SimulatedFills::new());

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

        // Scenario-default cost values. These are the fallbacks when no
        // per-bar array column and no per-asset override matches.
        let default_slip_bps = match &scenario.venue.slippage {
            SlippageModel::Linear { bps } => *bps as f64,
            SlippageModel::None => 0.0,
            SlippageModel::VolumeShare { .. } => 0.0, // VolumeShare computes dynamically
        };
        let default_taker_bps = scenario.venue.fees.taker_bps as f64;

        // Per-bar cost table. Built from the injected bars' Parquet source
        // when cost columns are present. An empty table means "no per-bar
        // overrides" — all bars fall back to scenario defaults.
        // For the legacy `load_ohlcv_fixture` path we don't have the raw
        // Parquet batches, so the table stays empty.
        let bar_cost_table = BarCostTable::default();

        // Accumulate `volume_share_excess` findings during the run; persist
        // them after the loop so we don't block the hot path on DB I/O.
        let mut volume_share_findings: Vec<Finding> = Vec::new();

        // eval-broker-rule-findings: build the asset-class-appropriate rule set
        // once per run. Currently Alpaca is the only venue; asset_class drives
        // Crypto vs Equity (no-op stub). The rule set is boxed so it can be
        // swapped per-venue in a future track without changing this function.
        let broker_rules: Box<dyn BrokerRuleSet> = rule_set_for_asset_class(scenario.asset_class);
        // Running count of orders rejected by broker-rule validation. Zero for
        // runs with no violations. Logged at finalization and surfaced as a
        // run-level finding when > 0.
        let mut broker_rejected_orders: u32 = 0;

        let mut equity = initial;
        let mut equity_curve: Vec<f64> = vec![initial];
        // Multi-asset (B4): pooled accounting moved from scalar
        // `position`/`entry_price`/`realized_total` to a `PortfolioBook`
        // keyed per asset. Single-asset preserved: with ONE asset the
        // pooled formula `initial + realized + Σ position·(mark−entry)`
        // reduces to the old scalar formula exactly. Stage-3 fan-out
        // adds the second key; this stage routes the single resolved
        // `asset_sym` through the book without changing any numbers.
        let mut book = crate::eval::executor::book::PortfolioBook::new(initial);
        let mut decision_idx = 0u32;
        // Phase C — per-eval-run signal cache owned by the executor.
        // Lifetime equals the run loop; dropped when the run completes.
        // Built once here so the cache survives across cycles and the
        // Minute / Decision granularity paths can re-fire prior signals.
        let mut signal_cache = crate::agent::signal_cache::SignalCache::new();
        let multi_filter_config = crate::agent::filter_dispatch::MultiFilterConfig::default();
        let bar_period_minutes = cadence_min.max(1) as u32;
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
        let mut prev_position: f64 = book.position(asset_sym);

        // track-plan-touches: build the per-run filter hook. Returns
        // `None` for `EveryBar` strategies (the default) — the loop
        // body short-circuits the hook call when `filter_hook.is_none()`.
        // Errors (CompiledRules, FilterGated without filter) surface
        // here and abort the run, matching the rest of the pre-loop
        // strategy invariants.
        let mut filter_hook = crate::eval::filter_hook::FilterHook::new(strategy)?;
        // Cache the pool handle for the hook's per-bar inserts. We
        // reach through `store` rather than threading it as a separate
        // parameter so the executor's surface area stays unchanged.
        let pool = store.pool().clone();

        // executor-trait-extraction: per-bar loop is now driven by the
        // BarSource trait. The indexed access (T+1 look-ahead,
        // decision_bars push, history slicing) still uses the local
        // `bars` slice, which is the same data the BarSource was built
        // from. Clock advancement is driven explicitly inside the loop.
        let mut i: usize = 0;
        while let Some(bar) = bar_source.next_bar().await {
            // Sync indexed view of the bar — the `bars` slice and the
            // BarSource share the same underlying data, so the same
            // index works for both.
            debug_assert!(i < bars.len(), "bar_source out of sync with bars vec");
            // Update the logical clock to this bar's timestamp before
            // any decision-side work. Live impls will ignore this
            // (their `WallClock::advance_to` will be a no-op).
            clock.advance_to(bar.timestamp);
            let bar = &bars[i];
            if store.is_terminal(&run.id).await? {
                anyhow::bail!("eval run stopped");
            }
            // Cadence gate: only fire on bars whose minute-aligned timestamp
            // is divisible by the strategy's cadence. With hourly bars and
            // 60-min cadence this always matches.
            if (bar.timestamp.timestamp() / 60) % cadence_min != 0 {
                i += 1;
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

            // track-plan-touches: per-bar filter evaluation. `None`
            // means EveryBar strategy (no gating). When the outcome's
            // decision is not `Active`, skip the agent pipeline for
            // this bar — record the evaluation, emit the event, keep
            // equity/metrics dense for chart continuity, then `continue`
            // past pipeline / decision / fill work. The bar is still
            // counted in `decision_bars` (already pushed above) so
            // post-loop baselines see the same bar slice the strategy
            // would have considered.
            if let Some(hook) = filter_hook.as_mut() {
                let in_position = book.position(asset_sym).abs() > f64::EPSILON;
                let evaluation = hook.evaluate(bar, in_position);
                hook.record(&pool, self.progress.as_ref(), &run.id, bar.timestamp, &evaluation)
                    .await?;
                if !evaluation.outcome.decision.is_active() {
                    equity = book.equity(&BTreeMap::from([(asset_sym, next_bar_open)]));
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
                    i += 1;
                    continue;
                }
            }

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
                        "position_size": book.position(asset_sym),
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
                        "position_size": book.position(asset_sym),
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
                    book.position(asset_sym) == prev_position,
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
                // F43: also expose the policy fire as an engine event
                // so the trace dock has a discrete bar tick to render.
                if let Some(obs) = self.obs_emitter.as_ref() {
                    let payload = serde_json::json!({
                        "decision_index": decision_idx,
                        "skip_count": plan.skip_count,
                        "reason": plan.reason,
                    });
                    obs.emit_engine_event("early_stop_triggered", None, Some(payload.to_string()))
                        .await;
                }
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
                equity = book.equity(&BTreeMap::from([(asset_sym, next_bar_open)]));
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
                prev_position = book.position(asset_sym);
                decision_idx += 1;
                i += 1;
                continue;
            }

            // F43 (`trace-dock-emitters`): open a per-decision span so
            // child model.call / tool.call / broker.call rows have a
            // visible decision parent on the trace dock. Closed at the
            // bottom of the iteration with span_finished_ok (or
            // _error on early bail). Also emits the `decision_started`
            // engine event so the dashboard timeline shows a per-bar
            // tick without diffing decision rows.
            let decision_span_id = crate::agent::observability::fresh_span_id();
            if let Some(obs) = self.obs_emitter.as_ref() {
                obs.emit_decision_span_started(&decision_span_id, None, decision_idx as i64, Some(&asset))
                    .await;
                let payload = serde_json::json!({
                    "decision_index": decision_idx,
                    "asset": asset,
                    "bar_ts": bar.timestamp.to_rfc3339(),
                });
                obs.emit_engine_event(
                    "decision_started",
                    Some(decision_span_id.clone()),
                    Some(payload.to_string()),
                )
                .await;
            }
            macro_rules! finish_decision_span_error {
                ($message:expr) => {
                    if let Some(obs) = self.obs_emitter.as_ref() {
                        obs.emit_span_finished_error(&decision_span_id, $message)
                            .await;
                    }
                };
            }

            // F-5 phase 2a: keep a copy of the seed so the
            // malformed-json repair path (below) can rebuild the
            // original user prompt byte-for-byte. The pipeline consumes
            // `seed_inputs` by value; cloning here is cheap relative to
            // the LLM dispatch and keeps the repair turn deterministic
            // for the A/B-cache pairing acceptance criterion.
            let seed_for_repair = seed.clone();
            let outs = run_pipeline(PipelineInputs {
                strategy,
                agent_slots,
                seed_inputs: seed,
                dispatch: dispatch.clone(),
                tools: tools.clone(),
                obs: self.obs_emitter.clone(),
                memory_recorder: self.memory_recorder.clone(),
                // V2D Phase 1.5 — backtest dispatches with the scenario
                // start so the recorder's Pattern recall can exclude
                // anything trained inside the replay window. Run/scenario
                // provenance flows down to Observation writes.
                scenario_start: Some(scenario.time_window.start),
                run_id: run.id.clone(),
                scenario_id: scenario.id.clone(),
                cycle_idx: decision_idx as i64,
                provider_catalogs: self.provider_catalogs.clone(),
                // Phase C — Filter capability runtime context. The
                // executor owns the cache for the run's lifetime; the
                // pipeline borrows it mutably for this cycle.
                filter_ctx: Some(crate::agent::pipeline::FilterPipelineCtx {
                    signal_cache: &mut signal_cache,
                    bar_period_minutes,
                    multi_filter_config,
                    bar_ts: bar.timestamp,
                    strategy_id: strategy.manifest.id.clone(),
                }),
                // Phase D — unified Recorder. Wired by callers that
                // construct an `EvalRecorder` and thread it via
                // `BacktestExecutor::with_recorder`. The default `None`
                // keeps the legacy bus-driven emission path untouched.
                recorder: self.recorder.as_deref(),
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
                        finish_decision_span_error!(reason.as_str());
                        anyhow::bail!(reason);
                    }
                }
            }

            if store.is_terminal(&run.id).await? {
                finish_decision_span_error!("eval run stopped");
                anyhow::bail!("eval run stopped");
            }

            let trader = match outs.trader.as_ref() {
                Some(t) => t,
                None => {
                    let err = TraderOutput::missing_response_error(&run.id, decision_idx);
                    finish_decision_span_error!(&err.to_string());
                    return Err(err.into());
                }
            };
            let trader_model_id = trader_model_id(agent_slots, strategy);
            let parsed = match TraderOutput::parse_response(trader, &run.id, decision_idx) {
                Ok(p) => p,
                Err(e) => {
                    // F-5 phase 2a (`harness-recovery-malformed-json`):
                    // single-shot repair attempt for the MalformedJson
                    // family (`InvalidJson` / `Truncated`). All other
                    // kinds bypass the repair and surface as today (their
                    // recovery families belong to sibling phase-2
                    // contracts or are intentionally non-recoverable per
                    // the audit). The repair propagates the ORIGINAL
                    // error on second-attempt failure so
                    // `eval_runs.error` keeps its wire-stable
                    // `[invalid_json]` / `[truncated]` prefix.
                    // F-5 phase 2b (`harness-recovery-schema-missing-field`)
                    // is checked FIRST: targeted-patch retry is cheaper
                    // than the full repair re-ask. The two families are
                    // disjoint per `FailureClass::family`, so each error
                    // walks exactly one branch — no double-repair.
                    if is_schema_missing_field_recoverable(&e) {
                        if let Some(ctx) = trader_repair_context(agent_slots, strategy) {
                            match try_repair_schema_missing_field(
                                trader,
                                e,
                                ctx,
                                &seed_for_repair,
                                dispatch.clone(),
                                self.obs_emitter.as_ref(),
                                &run.id,
                                decision_idx,
                            )
                            .await
                            {
                                Ok(repaired) => repaired,
                                Err(original) => {
                                    let err = original.with_model_hint(trader_model_id.as_deref());
                                    finish_decision_span_error!(&err.to_string());
                                    return Err(err.into());
                                }
                            }
                        } else {
                            let err = e.with_model_hint(trader_model_id.as_deref());
                            finish_decision_span_error!(&err.to_string());
                            return Err(err.into());
                        }
                    } else if is_malformed_json_recoverable(&e) {
                        if let Some(ctx) = trader_repair_context(agent_slots, strategy) {
                            match try_repair_malformed_json(
                                trader,
                                e,
                                ctx,
                                &seed_for_repair,
                                dispatch.clone(),
                                self.obs_emitter.as_ref(),
                                &run.id,
                                decision_idx,
                            )
                            .await
                            {
                                Ok(repaired) => repaired,
                                Err(original) => {
                                    let err = original.with_model_hint(trader_model_id.as_deref());
                                    finish_decision_span_error!(&err.to_string());
                                    return Err(err.into());
                                }
                            }
                        } else {
                            let err = e.with_model_hint(trader_model_id.as_deref());
                            finish_decision_span_error!(&err.to_string());
                            return Err(err.into());
                        }
                    } else {
                        let err = e.with_model_hint(trader_model_id.as_deref());
                        finish_decision_span_error!(&err.to_string());
                        return Err(err.into());
                    }
                }
            };

            if store.is_terminal(&run.id).await? {
                finish_decision_span_error!("eval run stopped");
                anyhow::bail!("eval run stopped");
            }

            let pre_fill_position = book.position(asset_sym);
            let pre_fill_entry = book.entry_price(asset_sym);

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
                    // F43: also emit a `guardrail_fired` engine event
                    // so the trace dock shows the rewrite as a
                    // discrete bar-level tick, not just a
                    // supervisor_notes entry.
                    if let Some(obs) = self.obs_emitter.as_ref() {
                        let payload = serde_json::json!({
                            "decision_index": decision_idx,
                            "asset": asset,
                            "reason": reason.as_str(),
                            "original": original_action.as_str(),
                            "applied": action.as_str(),
                        });
                        obs.emit_engine_event(
                            "guardrail_fired",
                            Some(decision_span_id.clone()),
                            Some(payload.to_string()),
                        )
                        .await;
                    }
                    // Per-decision warn demoted to debug (eval-guardrail-log-collapse):
                    // the supervisor_notes row is the durable record; a per-run
                    // summary warn is emitted at finalize by guardrail_summary::fire_guardrail_summary.
                    tracing::debug!(
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

            // eval-broker-rule-findings: validate new open orders against venue
            // rules before calling simulate_fill. Only `long_open` and
            // `short_open` generate new orders at the venue; `hold` and `flat`
            // do not (they are portfolio-state changes or no-ops).
            //
            // Severity-driven behavior (per `BrokerRuleSet::validate` contract):
            //   - `Critical` (e.g. unsupported_order_type, min_order_size):
            //     the venue would hard-reject. Order does NOT fill, the
            //     decision is recorded with no fill data, a finding is
            //     written, and `broker_rejected_orders` is incremented.
            //   - `Warning` (e.g. fractional_order_rounding): the venue
            //     would accept with a soft correction (precision truncation).
            //     A finding is still written for operator visibility, but the
            //     fill PROCEEDS — otherwise a benign precision warning would
            //     silently veto every fill on a crypto scenario where the
            //     `risk_pct_per_trade × equity / price` quotient has long
            //     decimal expansion.
            //
            // The rule set is built once per run from `scenario.asset_class`;
            // see the `rule_set_for_asset_class` call above the decision loop.
            let broker_rejected = if applied_action == "long_open" || applied_action == "short_open" {
                // Estimate order size using the same risk model as simulate_fill.
                // The qty estimate is approximate (simulate_fill may arrive at a
                // slightly different price); it is sufficient for notional /
                // precision checks.
                let estimated_qty = {
                    let usd_at_risk = equity * strategy.risk.risk_pct_per_trade;
                    (usd_at_risk / next_bar_open).max(0.0)
                };
                let pending = PendingOrder {
                    symbol: asset.clone(),
                    // v1 backtest always emits market orders with GTC TIF.
                    // Future tracks (intra-bar fill ordering, limit orders) will
                    // plumb the trader's expressed order kind / TIF through here.
                    kind: OrderKind::Market,
                    tif: TimeInForce::Gtc,
                    qty: estimated_qty,
                    price: next_bar_open,
                };
                match broker_rules.validate(&pending) {
                    Ok(()) => false, // order accepted; proceed to simulate_fill
                    Err(violation) => {
                        // Per `BrokerRuleSet::validate` contract: only
                        // `Critical` violations skip the fill (the venue would
                        // hard-reject). `Warning` violations record a finding
                        // for operator visibility but the fill still proceeds
                        // — the venue would accept the order after truncating
                        // precision, so the backtest must mirror that.
                        let is_blocking = matches!(violation.severity, BrokerViolationSeverity::Critical);
                        if is_blocking {
                            broker_rejected_orders += 1;
                        }
                        let finding_severity = match violation.severity {
                            BrokerViolationSeverity::Critical => Severity::Critical,
                            BrokerViolationSeverity::Warning => Severity::Warning,
                        };
                        let summary_lead = if is_blocking {
                            "Order rejected by broker rule"
                        } else {
                            "Broker rule warning"
                        };
                        let finding = Finding {
                            id: Ulid::new().to_string(),
                            run_id: run.id.clone(),
                            kind: "broker_rule_violation".into(),
                            severity: finding_severity,
                            summary: format!(
                                "{summary_lead} `{}`: {}",
                                violation.specific_rule, violation.message
                            ),
                            evidence: serde_json::json!({
                                "specific_rule": violation.specific_rule,
                                "message": violation.message,
                                "severity": violation.severity,
                                "asset": asset,
                                "action": applied_action,
                                "estimated_qty": estimated_qty,
                                "next_bar_open": next_bar_open,
                                "decision_index": decision_idx,
                            }),
                            extracted_at: Utc::now(),
                            schema_version: crate::eval::findings::FINDING_SCHEMA_VERSION.to_string(),
                            evidence_cycle_ids: Some(vec![decision_idx.to_string()]),
                            produced_by_check: Some(format!("broker:{}", violation.specific_rule)),
                            eval_review_id: None,
                            review_type: None,
                            confidence: None,
                            title: Some(format!("Broker rule violation: {}", violation.specific_rule)),
                            description: Some(violation.message.clone()),
                            recommendation: Some(
                                "Review strategy's order construction logic; \
                                 ensure order types, TIFs, and sizes are compatible \
                                 with the target venue."
                                    .into(),
                            ),
                            created_at: None,
                        };
                        if is_blocking {
                            tracing::warn!(
                                run_id = %run.id,
                                decision_index = decision_idx,
                                asset = %asset,
                                specific_rule = %violation.specific_rule,
                                action = %applied_action,
                                "broker rule rejected order — no fill this cycle",
                            );
                        } else {
                            tracing::debug!(
                                run_id = %run.id,
                                decision_index = decision_idx,
                                asset = %asset,
                                specific_rule = %violation.specific_rule,
                                action = %applied_action,
                                "broker rule warning — fill proceeds",
                            );
                        }
                        if let Err(e) = store.record_finding(&finding).await {
                            tracing::error!(
                                run_id = %run.id,
                                decision_index = decision_idx,
                                error = %e,
                                "failed to record broker_rule_violation finding",
                            );
                        }
                        // F43 (`trace-dock-emitters`): broker rule
                        // violations also surface as supervisor_notes +
                        // an engine event so the trace dock's notes /
                        // events tabs both reflect the broker push-back
                        // (the `findings` table is operator-facing only,
                        // not on the trace dock).
                        if let Some(obs) = self.obs_emitter.as_ref() {
                            let severity = if is_blocking { "error" } else { "warn" };
                            let note_msg = format!(
                                "broker rule {} `{}` at decision {decision_idx} ({asset}): {}",
                                if is_blocking { "rejected order" } else { "warning" },
                                violation.specific_rule,
                                violation.message,
                            );
                            obs.emit_supervisor_note("guard", severity, &note_msg).await;
                            let payload = serde_json::json!({
                                "decision_index": decision_idx,
                                "asset": asset,
                                "specific_rule": violation.specific_rule,
                                "severity": if is_blocking { "critical" } else { "warning" },
                                "action": applied_action,
                            });
                            obs.emit_engine_event(
                                "broker_rule_violation",
                                Some(decision_span_id.clone()),
                                Some(payload.to_string()),
                            )
                            .await;
                        }
                        is_blocking // Critical → skip simulate_fill; Warning → fill proceeds
                    }
                }
            } else {
                false // hold/flat: no venue order, skip broker check
            };

            // `simulate_fill` treats any non-(long_open|short_open) action
            // as `want_flat` (closes any open position). That suits a
            // trader-emitted `flat` or the guardrail one-step-flip
            // rewrite, but a guardrail pyramid-block rewrites
            // `long_open` → `hold` and we MUST NOT close the existing
            // position in that case. Short-circuit a true no-op fill
            // for `hold` so the position survives untouched.
            //
            // A broker-rejected order also skips simulate_fill: the order is
            // treated as if it never existed (fail-honest — the strategy sees
            // the decision in the trace but no fill in outcomes).
            let fill: FillRecord = if applied_action == "hold" || broker_rejected {
                FillRecord {
                    new_pos: pre_fill_position,
                    new_entry: pre_fill_entry,
                    fill_price: None,
                    fill_size: None,
                    fee: None,
                    realized_pnl: 0.0,
                    provenance: FillProvenance::default(),
                    fill_branch: None,
                    aggressor_side: None,
                    order_state: None,
                    volume_cap_hit: None,
                }
            } else {
                // Resolve per-bar and per-asset cost overrides.
                let bar_cost = bar_cost_table.lookup(&bar.timestamp);
                let asset_override = resolve_asset_override(&scenario.venue.overrides, &asset);

                // Override precedence: per-bar array > per-asset override > scenario default.
                let effective_slip_bps = bar_cost
                    .and_then(|c| c.slip_bps)
                    .or_else(|| {
                        asset_override
                            .and_then(|o| o.slippage.as_ref())
                            .and_then(|s| match s {
                                SlippageModel::Linear { bps } => Some(*bps as f64),
                                SlippageModel::None => Some(0.0),
                                SlippageModel::VolumeShare { .. } => None,
                            })
                    })
                    .unwrap_or(default_slip_bps);

                let effective_taker_bps = bar_cost
                    .and_then(|c| c.fee_bps)
                    .or_else(|| {
                        asset_override
                            .and_then(|o| o.fees.as_ref())
                            .map(|f| f.taker_bps as f64)
                    })
                    .unwrap_or(default_taker_bps);

                // Maker fee: per-bar override if present, else per-asset, else scenario default.
                let effective_maker_bps = bar_cost
                    .and_then(|c| c.fee_bps) // when per-bar fee is present, use it for both sides
                    .or_else(|| {
                        asset_override
                            .and_then(|o| o.fees.as_ref())
                            .map(|f| f.maker_bps as f64)
                    })
                    .unwrap_or(scenario.venue.fees.maker_bps as f64);

                let effective_spread_bps = bar_cost.and_then(|c| c.spread_bps).unwrap_or(0.0);

                // Determine the effective slippage model. When a per-bar
                // slip_bps column is present, treat it as a Linear model
                // (the value is in effective_slip_bps). Otherwise use the
                // asset override or scenario default.
                //
                // We store an owned fallback value so Rust doesn't reject
                // a reference to a temporary.
                let per_bar_slip_present = bar_cost.and_then(|c| c.slip_bps).is_some();
                let fallback_linear = SlippageModel::Linear { bps: 0 }; // bps ignored; value via effective_slip_bps
                let effective_slippage_model: &SlippageModel = if per_bar_slip_present {
                    &fallback_linear
                } else {
                    asset_override
                        .and_then(|o| o.slippage.as_ref())
                        .unwrap_or(&scenario.venue.slippage)
                };

                let per_bar_fee_present = bar_cost.and_then(|c| c.fee_bps).is_some();
                let per_asset_fee_present = asset_override.and_then(|o| o.fees.as_ref()).is_some();
                let fee_source = resolve_fee_source(per_bar_fee_present, per_asset_fee_present);

                // Fill bar is the next bar — `bars.get(i + 1)` if present,
                // else the current bar (terminal-bar fallback). We need its
                // O/H/L for intra-bar ordering. When bars[i+1] doesn't exist,
                // use current bar's close as a degenerate open and O==H==L.
                let fill_bar = bars.get(i + 1);
                let (fill_bar_open, fill_bar_high, fill_bar_low) = fill_bar
                    .map(|b| (b.open, b.high, b.low))
                    .unwrap_or((bar.close, bar.close, bar.close));

                // executor-trait-extraction: fill production now routes
                // through the FillSink trait. SimulatedFills::submit
                // runs the verbatim pre-refactor `simulate_fill` body;
                // sub-track 3's broker-backed impl will replace this
                // call with a forward-to-broker call without changing
                // the surrounding loop. The request is owned (no
                // borrowed references) so future async-broker impls
                // can hold it across `.await`.
                let fill_record: FillRecord = fill_sink
                    .submit(FillRequest {
                        pos: pre_fill_position,
                        entry: pre_fill_entry,
                        action: applied_action.clone(),
                        next_open: next_bar_open,
                        bar_volume: bar.volume,
                        slip_bps: effective_slip_bps,
                        spread_bps: effective_spread_bps,
                        taker_bps: effective_taker_bps,
                        maker_bps: effective_maker_bps,
                        equity,
                        risk_pct: strategy.risk.risk_pct_per_trade,
                        slippage_model: effective_slippage_model.clone(),
                        fee_source,
                        asset: asset.clone(),
                        bar_ts: bar.timestamp,
                        bar_open: fill_bar_open,
                        bar_high: fill_bar_high,
                        bar_low: fill_bar_low,
                    })
                    .await;

                // Collect volume_share_excess finding if the cap bound.
                if let Some((req_qty, bar_vol, cap_qty, fill_share)) = fill_record.volume_cap_hit {
                    volume_share_findings.push(make_volume_share_excess_finding(
                        &run.id,
                        decision_idx,
                        req_qty,
                        bar_vol,
                        cap_qty,
                        fill_share,
                    ));
                }

                fill_record
            };
            // Apply the fill to the pooled book keyed by this asset.
            // `set_position(_, 0.0, _)` clears the leg, so a flat fill
            // leaves `position`/`entry_price` reading 0.0 — identical to
            // the old scalar `entry_price = fill.new_entry (== 0.0)`.
            book.set_position(asset_sym, fill.new_pos, fill.new_entry);
            book.add_realized(fill.realized_pnl);
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

            // F43 (`trace-dock-emitters`): emit `fill_attempted` per
            // decision regardless of whether the fill crossed the book.
            // The payload distinguishes hold/no-op iterations
            // (`filled: false`) from real fills so the trace dock can
            // render a per-bar tick density indicator without joining
            // `eval_decisions`.
            if let Some(obs) = self.obs_emitter.as_ref() {
                let payload = serde_json::json!({
                    "decision_index": decision_idx,
                    "asset": asset,
                    "applied_action": applied_action,
                    "filled": fill_happened,
                    "fill_price": fill.fill_price,
                    "fill_size": fill.fill_size,
                });
                obs.emit_engine_event(
                    "fill_attempted",
                    Some(decision_span_id.clone()),
                    Some(payload.to_string()),
                )
                .await;
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
            equity = book.equity(&BTreeMap::from([(asset_sym, next_bar_open)]));

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
            let portfolio_changed = book.position(asset_sym) != prev_position;
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
            prev_position = book.position(asset_sym);

            // F43 (`trace-dock-emitters`): close the per-decision span
            // + emit the `decision_completed` engine event so the
            // trace dock can compute decision-scoped duration.
            if let Some(obs) = self.obs_emitter.as_ref() {
                obs.emit_span_finished_ok(&decision_span_id).await;
                let payload = serde_json::json!({
                    "decision_index": decision_idx,
                    "asset": asset,
                    "applied_action": applied_action,
                });
                obs.emit_engine_event(
                    "decision_completed",
                    Some(decision_span_id.clone()),
                    Some(payload.to_string()),
                )
                .await;
            }

            decision_idx += 1;
            i += 1;
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
            // inference_cost_quote_total + net_return_pct populated
            // post-finalize by api::eval::enrich_with_inference_cost.
            ..Default::default()
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
            broker_rejected_orders,
            "eval run finalize: provider prompt-cache stats"
        );
        // Persist volume_share_excess findings accumulated during the run.
        for finding in &volume_share_findings {
            if let Err(e) = store.record_finding(finding).await {
                tracing::warn!(
                    run_id = %run.id,
                    error = %e,
                    "failed to persist volume_share_excess finding; continuing"
                );
            }
        }

        // eval-broker-rule-findings: if any orders were rejected, emit an
        // aggregate run-level finding so the reviewer can see the count
        // without scanning the per-decision violations. The per-decision
        // findings are already in findings.jsonl.
        //
        // NOTE: `broker_rejected_orders` is intentionally not added to
        // `MetricsSummary` in this track to avoid touching ~25 struct literal
        // construction sites across parallel V2E tracks (which would cause
        // merge conflicts). The metric is surfaced here through the findings
        // JSONL and as a tracing::info field above. Adding it to
        // `MetricsSummary` is deferred to a follow-up coordination step — see
        // PR body "Coordination" section.
        if broker_rejected_orders > 0 {
            let summary_finding = Finding {
                id: Ulid::new().to_string(),
                run_id: run.id.clone(),
                kind: "broker_rule_violation".into(),
                severity: Severity::Warning,
                summary: format!(
                    "{broker_rejected_orders} order(s) rejected by broker-rule validation \
                     this run; see per-decision findings for details."
                ),
                evidence: serde_json::json!({
                    "broker_rejected_orders": broker_rejected_orders,
                    "note": "Per-decision findings carry specific_rule and evidence_cycle_ids.",
                }),
                extracted_at: Utc::now(),
                schema_version: crate::eval::findings::FINDING_SCHEMA_VERSION.to_string(),
                evidence_cycle_ids: Some(vec![]),
                produced_by_check: Some("broker:run_aggregate".to_string()),
                eval_review_id: None,
                review_type: None,
                confidence: None,
                title: Some(format!("{broker_rejected_orders} broker-rejected order(s)")),
                description: Some(
                    "One or more orders were rejected by offline broker-rule validation \
                     before reaching simulate_fill. The strategy's backtest P&L does not \
                     reflect these orders. Review per-decision findings for details."
                        .into(),
                ),
                recommendation: Some(
                    "Inspect per-decision `broker_rule_violation` findings. \
                     Fix the strategy's order construction to comply with \
                     Alpaca's supported order types, TIFs, and minimum sizes."
                        .into(),
                ),
                created_at: None,
            };
            if let Err(e) = store.record_finding(&summary_finding).await {
                tracing::error!(
                    run_id = %run.id,
                    broker_rejected_orders,
                    error = %e,
                    "failed to record broker_rule_violation aggregate finding",
                );
            }
        }

        store.finalize(&run.id, &metrics).await?;
        Ok(metrics)
    }
}

// executor-trait-extraction: `SimulateFillArgs`, `SimulateFillResult`,
// `FillOutcome`, and `simulate_fill` were the inline fill-simulation
// scaffolding before the FillSink trait absorbed the same logic. The
// trait-side equivalents are `FillRequest`, `FillRecord`, and
// `SimulatedFills` in `super::traits`. These local copies are kept
// (gated to test builds) because the inline unit tests at the bottom of
// this file exercise the pre-refactor signatures directly — pinning
// the byte-identical behavior between the inline code (the regression
// target) and the trait-side lift (the actual production path). When
// sub-track 2 or 3 needs to delete one or the other, this is where
// the cleanup happens.
#[cfg(test)]
struct SimulateFillArgs<'a> {
    pos: f64,
    entry: f64,
    action: &'a str,
    next_open: f64,
    /// Bar volume — required for `VolumeShare` model. Zero/NaN triggers
    /// a fallback to the scenario-default `Linear` model.
    bar_volume: f64,
    /// Effective slip_bps resolved via override precedence. For `VolumeShare`
    /// this is unused (impact computed from `price_impact * volume_share²`).
    slip_bps: f64,
    /// Half-spread in bps (0.0 when no `spread_bps` column present).
    spread_bps: f64,
    taker_bps: f64,
    /// Maker fee in bps — applied when `AggressorSide::Maker` is classified.
    maker_bps: f64,
    equity: f64,
    risk_pct: f64,
    /// The effective slippage model for this bar (after override resolution).
    slippage_model: &'a SlippageModel,
    /// Provenance tag for the fee source.
    fee_source: FeeSource,
    /// Asset venue symbol — used for fallback debug logging.
    asset: &'a str,
    /// Bar timestamp — used for fallback debug logging.
    bar_ts: chrono::DateTime<chrono::Utc>,
    /// Current bar's OHLC — used for NautilusTrader intra-bar fill ordering.
    /// `open` must equal `next_open` from the *decision* bar's perspective
    /// (i.e. this is the *next* bar that fills the order).
    bar_open: f64,
    /// High of the fill bar — used by `intra_bar_fill_branch` for the O→H→L→C
    /// path decision. Stored here for future limit/stop order paths; v1 market
    /// orders do not consult it (they always use `NextOpenOnly`).
    #[allow(dead_code)]
    bar_high: f64,
    /// Low of the fill bar — used by `intra_bar_fill_branch` for the O→L→H→C
    /// path decision. Stored here for future limit/stop order paths; v1 market
    /// orders do not consult it (they always use `NextOpenOnly`).
    #[allow(dead_code)]
    bar_low: f64,
}

/// Result wrapper that bundles the `FillOutcome` with volume-cap metadata.
#[cfg(test)]
#[allow(dead_code)]
struct SimulateFillResult {
    outcome: FillOutcome,
    /// When `Some`, the volume cap bound: `(requested_qty, bar_volume,
    /// cap_binding_qty, fill_share)`. The caller uses this to emit a
    /// `volume_share_excess` finding.
    volume_cap_hit: Option<(f64, f64, f64, f64)>,
}

#[cfg(test)]
#[allow(dead_code)]
struct FillOutcome {
    new_pos: f64,
    new_entry: f64,
    fill_price: Option<f64>,
    fill_size: Option<f64>,
    fee: Option<f64>,
    realized_pnl: f64,
    /// Fill provenance — describes how cost was resolved for this fill.
    /// Populated by `simulate_fill`; consumed by `eval-trace-surface-foundation`
    /// when it lands its trace column writes. Unused until that track merges.
    #[allow(dead_code)]
    provenance: FillProvenance,
    /// Which intra-bar branch triggered this fill. `None` for no-op (hold/flat-no-pos).
    #[allow(dead_code)]
    fill_branch: Option<FillBranch>,
    /// Maker vs taker classification. `None` for no-op fills.
    #[allow(dead_code)]
    aggressor_side: Option<AggressorSide>,
    /// Order lifecycle state after the fill attempt.
    #[allow(dead_code)]
    order_state: Option<OrderState>,
}

// ---------------------------------------------------------------------------
// Intra-bar fill ordering helpers (V2E eval-intra-bar-fill-ordering)
// ---------------------------------------------------------------------------

/// Corwin-Schultz (2012) bid-ask spread proxy from per-bar H/L data.
///
/// Formula: `2 * sqrt(max(0, log(H/L)² - 2*ln(2)*σ²))`
/// where σ² is estimated from the rolling window's `log(H/L)²` values.
///
/// Returns spread in basis points. Always returns a finite, non-negative value.
///
/// # Limitations
/// - Downward-biased on thinly-traded names (Corwin-Schultz known limitation).
/// - For liquid Alpaca symbols (BTC/USD, ETH/USD, large-cap equities) the
///   estimator is a reasonable default when no explicit spread column is present.
///
/// `hl_log_sq_window` is a small rolling window of `(ln(H/L))²` values from
/// recent bars — typically the last 5–20 bars. When the window is empty, σ²
/// falls back to 0, producing `spread ≈ log(H/L) * 2`.
pub fn corwin_schultz_spread_bps(bar_high: f64, bar_low: f64, hl_log_sq_window: &[f64]) -> f64 {
    // Guard against degenerate inputs.
    if bar_high <= 0.0
        || bar_low <= 0.0
        || bar_high < bar_low
        || !bar_high.is_finite()
        || !bar_low.is_finite()
    {
        return 0.0;
    }

    let ln2 = std::f64::consts::LN_2;
    let log_hl = (bar_high / bar_low).ln();
    let log_hl_sq = log_hl * log_hl;

    // Rolling variance proxy: mean of (ln(H/L))² over the window.
    let sigma_sq = if hl_log_sq_window.is_empty() {
        0.0
    } else {
        hl_log_sq_window.iter().sum::<f64>() / hl_log_sq_window.len() as f64
    };

    // Corwin-Schultz: spread = 2 * sqrt(max(0, S²)) where S² = log_hl² - 2*ln(2)*σ².
    let s_sq = (log_hl_sq - 2.0 * ln2 * sigma_sq).max(0.0);
    let spread_fraction = 2.0 * s_sq.sqrt();

    // Convert from fraction to bps, clamp to non-negative (sqrt ensures non-neg,
    // but guard against NaN from pathological inputs).
    let spread_bps = (spread_fraction * 10_000.0).max(0.0);
    if spread_bps.is_finite() {
        spread_bps
    } else {
        0.0
    }
}

/// NautilusTrader-style intra-bar fill branch determination.
///
/// Given a fill bar's O/H/L, determine which OHLC walk sequence applies
/// and whether the `trigger_price` would have been hit within that bar.
///
/// # Rules
///
/// 1. **Gap past trigger**: if the bar's open is already past the trigger
///    in the fill direction (e.g. stop at 100 but bar opens at 95 for a
///    long stop) → fill at open, `FillBranch::GapPast`. No price guarantee.
///
/// 2. **O→H→L→C** (high closer to open): if `|H - O| <= |L - O|`,
///    visit open → high → low → close. First crossing fills.
///
/// 3. **O→L→H→C** (low closer to open): if `|L - O| < |H - O|`,
///    visit open → low → high → close. First crossing fills.
///
/// `trigger_price`: the limit or stop price to test.
/// `is_buy`: true when the order fills on an upward crossing (stop-buy,
///   limit-buy above a falling price). false for sell-side.
///
/// Returns `(FillBranch, Option<fill_price>)`. `fill_price` is `None`
/// when the trigger is not reached within this bar — the caller should
/// leave the order `Open`.
pub fn intra_bar_fill_branch(
    bar_open: f64,
    bar_high: f64,
    bar_low: f64,
    trigger_price: f64,
    is_buy: bool,
) -> (FillBranch, Option<f64>) {
    // Rule 1: gap past trigger at open.
    // For a buy stop/limit: if open is already at or above the trigger, gap fill.
    // For a sell stop/limit: if open is already at or below the trigger, gap fill.
    let gap_filled = if is_buy {
        bar_open >= trigger_price
    } else {
        bar_open <= trigger_price
    };
    if gap_filled {
        return (FillBranch::GapPast, Some(bar_open));
    }

    // Rule 2/3: NautilusTrader heuristic — which extreme is closer to open?
    let high_dist = (bar_high - bar_open).abs();
    let low_dist = (bar_low - bar_open).abs();

    if high_dist <= low_dist {
        // O→H→L→C: high visited first.
        if is_buy {
            // Buy trigger crossed when high reaches trigger.
            if bar_high >= trigger_price {
                return (FillBranch::OhlcHighFirst, Some(trigger_price));
            }
        } else {
            // Sell trigger: not crossed in this sequence unless low is below trigger.
            if bar_low <= trigger_price {
                return (FillBranch::OhlcHighFirst, Some(trigger_price));
            }
        }
    } else {
        // O→L→H→C: low visited first.
        if is_buy {
            // Buy trigger: not crossed on the down leg; only on the up leg.
            if bar_high >= trigger_price {
                return (FillBranch::OhlcLowFirst, Some(trigger_price));
            }
        } else {
            // Sell trigger crossed when low reaches trigger.
            if bar_low <= trigger_price {
                return (FillBranch::OhlcLowFirst, Some(trigger_price));
            }
        }
    }

    // Trigger not reached within this bar.
    (FillBranch::NextOpenOnly, None)
}

/// Classify a fill as maker or taker.
///
/// A fill is **maker** when the fill price is within `spread/2` of the bar open
/// on the passive side (i.e. the order was resting and got hit, not crossing).
/// All other fills (market orders, or limits that cross the spread) are **taker**.
///
/// # Maker classification rule
///
/// `spread_bps_at_fill` is the effective spread at fill time (from the per-bar
/// column, per-asset override, Corwin-Schultz proxy, or scenario default).
///
/// For a long fill: `fill_price ≤ bar_open + spread_bps/10_000 / 2 * bar_open`
/// For a short fill: `fill_price ≥ bar_open - spread_bps/10_000 / 2 * bar_open`
///
/// This is a permissive heuristic — any resting limit inside half-spread of
/// the open is classified as maker. Tighten if backtests show unrealistic
/// maker rates.
///
/// Market orders always classify as taker.
pub fn classify_aggressor_side(
    action: &str,
    fill_price: f64,
    bar_open: f64,
    spread_bps: f64,
) -> AggressorSide {
    // Market orders are always taker.
    if action == "long_open" || action == "short_open" {
        // The v1 backtest emits only market-style orders — all current fills
        // at next_open are taker. This function is structured to support future
        // limit-order paths where maker classification would apply.
        //
        // Maker check: is the fill price within the passive half-spread of open?
        let half_spread = bar_open * (spread_bps / 10_000.0) / 2.0;
        let trade_long = action == "long_open";
        if trade_long {
            // Passive buy: fill at or below open + half_spread (resting bid).
            if fill_price <= bar_open + half_spread {
                return AggressorSide::Maker;
            }
        } else {
            // Passive sell: fill at or above open - half_spread (resting offer).
            if fill_price >= bar_open - half_spread {
                return AggressorSide::Maker;
            }
        }
    }
    AggressorSide::Taker
}

/// Find the trader slot's repair context — system prompt, model id,
/// max_tokens, temperature — for the F-5 phase-2a MalformedJson repair
/// path (`harness-recovery-malformed-json`). After `LLMSlot.prompt`
/// removal, only attached agent slots can supply this context; legacy
/// `strategy.trader_slot` entries have no prompt source and skip repair.
///
fn trader_repair_context<'a>(
    agent_slots: &'a [ResolvedAgentSlot],
    _strategy: &'a Strategy,
) -> Option<TraderRepairContext<'a>> {
    if let Some(resolved) = agent_slots.iter().find(|r| canonical_role(&r.role) == "trader") {
        let model = resolved.slot.effective_model();
        if !resolved.system_prompt.trim().is_empty() && !model.trim().is_empty() {
            return Some(TraderRepairContext {
                system_prompt: &resolved.system_prompt,
                model,
                max_tokens: resolved.max_tokens,
                temperature: resolved.temperature,
            });
        }
    }
    None
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

/// Simulate a market-order fill at the next bar's open, applying the
/// configured slippage model and taker fee.
///
/// # Slippage models
/// - `Linear { bps }`: flat basis-point slippage on `next_open` regardless
///   of order size. Identical to the pre-V2E behavior.
/// - `None`: zero slippage; fills at `next_open` verbatim.
/// - `VolumeShare { price_impact, volume_limit }`: zipline-canonical quadratic
///   model. `volume_share = min(order_qty / bar_volume, volume_limit)`.
///   `fill_price = next_open * (1 ± price_impact * volume_share²)`.
///   Falls back to the scenario-default `Linear` and emits a debug log when
///   bar volume is missing or zero.
///
/// # Override precedence
/// Resolved by the caller before invocation:
///   per-bar array > per-asset override > scenario default.
///
/// Action semantics (matches the v1 trader-output schema):
/// - `long_open`: hold long, reverse short → long, or open long from flat.
/// - `short_open`: hold short, reverse long → short, or open short from flat.
/// - `flat` (or any unknown action): close any open position; otherwise no-op.
#[cfg(test)]
fn simulate_fill(a: SimulateFillArgs<'_>) -> SimulateFillResult {
    let want_long = a.action == "long_open";
    let want_short = a.action == "short_open";
    let want_flat = !want_long && !want_short;

    // No-op when target direction matches current position.
    if (want_long && a.pos > 0.0) || (want_short && a.pos < 0.0) || (want_flat && a.pos == 0.0) {
        return SimulateFillResult {
            outcome: FillOutcome {
                new_pos: a.pos,
                new_entry: a.entry,
                fill_price: None,
                fill_size: None,
                fee: None,
                realized_pnl: 0.0,
                provenance: FillProvenance::default(),
                fill_branch: None,
                aggressor_side: None,
                order_state: None,
            },
            volume_cap_hit: None,
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

    // Compute the initial position size so we know the order quantity for
    // the VolumeShare model. For no-op paths this is already handled above.
    // We need `new_pos_units` to compute `order_qty` for VolumeShare, but
    // `new_pos_units` depends on `fill_price`, which depends on slippage,
    // which for VolumeShare depends on `order_qty`. We resolve this by
    // first computing order size at `next_open` (the mid), then applying
    // VolumeShare impact.
    let approx_units = if want_flat {
        a.pos.abs()
    } else {
        let usd_at_risk = a.equity * a.risk_pct;
        let units = (usd_at_risk / a.next_open).max(0.0);
        if a.pos != 0.0 {
            // Reversing: pay close leg + open leg.
            a.pos.abs() + units
        } else {
            units
        }
    };

    // Resolve slip fraction and volume-cap state.
    let mut volume_share = 0.0_f64;
    let mut volume_cap_bound = false;
    let mut volume_cap_hit: Option<(f64, f64, f64, f64)> = None;

    let effective_slip_fraction: f64 = match a.slippage_model {
        SlippageModel::None => 0.0,

        SlippageModel::Linear { bps } => {
            // When a per-bar column was present, `a.slip_bps` already holds
            // that value (override precedence resolved by caller). Otherwise
            // `a.slip_bps` came from the asset override or scenario default.
            // In both cases the `bps` field on `Linear` may be stale vs the
            // resolved value — we always use `a.slip_bps` here.
            let _ = bps; // resolved value via a.slip_bps
            a.slip_bps / 10_000.0
        }

        SlippageModel::VolumeShare {
            price_impact,
            volume_limit,
        } => {
            // Fallback when bar volume is zero or missing.
            if a.bar_volume <= 0.0 || !a.bar_volume.is_finite() {
                tracing::debug!(
                    asset = a.asset,
                    bar_ts = %a.bar_ts,
                    "VolumeShare: bar volume missing or zero; falling back to Linear slip_bps={}",
                    a.slip_bps
                );
                a.slip_bps / 10_000.0
            } else {
                let raw_share = approx_units / a.bar_volume;
                volume_cap_bound = raw_share > *volume_limit;
                volume_share = raw_share.min(*volume_limit);

                if volume_cap_bound {
                    // Quantity that would consume exactly volume_limit of bar volume.
                    let cap_qty = *volume_limit * a.bar_volume;
                    volume_cap_hit = Some((approx_units, a.bar_volume, cap_qty, volume_share));
                }

                price_impact * volume_share * volume_share
            }
        }
    };

    // Apply spread (half-spread widening on both sides of mid).
    let spread_fraction = a.spread_bps / 10_000.0 / 2.0;

    let fill_price = if trade_long {
        a.next_open * (1.0 + effective_slip_fraction + spread_fraction)
    } else {
        a.next_open * (1.0 - effective_slip_fraction - spread_fraction)
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

    // Maker/taker classification.
    // The v1 backtest emits market-style orders (long_open / short_open / flat).
    // `classify_aggressor_side` applies the half-spread test: if the fill price
    // is within passive half-spread of the bar open, it's maker; otherwise taker.
    // For market orders the fill price is open + slip + spread, which is OUTSIDE
    // the passive half-spread, so they correctly classify as taker.
    // The flat (close) path fills at open - slip - spread (for longs), which is
    // also outside the passive half-spread, so it's taker too.
    let aggressor_side = classify_aggressor_side(a.action, fill_price, a.bar_open, a.spread_bps);

    // Fee in bps depends on aggressor side.
    let fee_bps_applied = match aggressor_side {
        AggressorSide::Maker => a.maker_bps,
        AggressorSide::Taker => a.taker_bps,
    };

    let notional = traded_units * fill_price;
    let fee = notional * (fee_bps_applied / 10_000.0);

    let new_entry = if new_pos_units == 0.0 { 0.0 } else { fill_price };

    // All current (v1) fills are market orders — NextOpenOnly.
    // The intra-bar ordering helpers (`intra_bar_fill_branch`) are available
    // for future limit/stop order paths; the v1 path always uses next-bar open.
    let fill_branch = FillBranch::NextOpenOnly;

    // Order state: volume-cap-bound fills are PartiallyFilled; all other fills
    // are Filled.
    let order_state = if volume_cap_bound {
        OrderState::PartiallyFilled
    } else {
        OrderState::Filled
    };

    let provenance = FillProvenance {
        slip_bps_applied: effective_slip_fraction * 10_000.0,
        spread_bps_applied: spread_fraction * 2.0 * 10_000.0, // round-trip bps
        fee_bps_applied,
        fee_source: a.fee_source,
        volume_share,
        volume_cap_bound,
    };

    SimulateFillResult {
        outcome: FillOutcome {
            new_pos: new_pos_units,
            new_entry,
            fill_price: Some(fill_price),
            fill_size: Some(traded_units),
            fee: Some(fee),
            realized_pnl: realized - fee,
            provenance,
            fill_branch: Some(fill_branch),
            aggressor_side: Some(aggressor_side),
            order_state: Some(order_state),
        },
        volume_cap_hit,
    }
}

/// Resolve the first matching `VenueOverride` for the given venue symbol.
/// Returns `None` when no pattern matches (caller falls through to defaults).
fn resolve_asset_override<'a>(overrides: &'a [VenueOverride], symbol: &str) -> Option<&'a VenueOverride> {
    overrides.iter().find(|o| o.matches(symbol))
}

/// Determine the `FeeSource` provenance tag based on which source won.
fn resolve_fee_source(per_bar_won: bool, per_asset_won: bool) -> FeeSource {
    if per_bar_won {
        FeeSource::PerBarArray
    } else if per_asset_won {
        FeeSource::PerAssetOverride
    } else {
        FeeSource::Default
    }
}

/// Build a `TradeMarker` from fill-level data. Extracted to avoid duplicating
/// the identical field construction across the `long_open` and
/// `short_open`/`flat` arms of the marker-event match.
#[allow(clippy::too_many_arguments)]
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
    } else if action == "short_open" || pre_fill_position > 0.0 {
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

    // Updated because <reason>: `SimulateFillArgs` gained new fields for the
    // V2E cost-model rewrite (bar_volume, spread_bps, slippage_model,
    // fee_source, asset, bar_ts). The `simulate_fill` return type changed
    // from `FillOutcome` to `SimulateFillResult { outcome, volume_cap_hit }`.
    // Updated for eval-intra-bar-fill-ordering: added maker_bps, bar_open,
    // bar_high, bar_low fields. Existing behavioural assertions still pass.
    fn args(pos: f64, action: &'static str) -> SimulateFillArgs<'static> {
        SimulateFillArgs {
            pos,
            entry: 50_000.0,
            action,
            next_open: 60_000.0,
            bar_volume: 1_000.0, // large enough that VolumeShare never caps
            slip_bps: 10.0,      // 0.1%
            spread_bps: 0.0,
            taker_bps: 25.0, // 0.25%
            maker_bps: 10.0, // 0.10%
            equity: 10_000.0,
            risk_pct: 0.02, // 2%
            slippage_model: &SlippageModel::Linear { bps: 10 },
            fee_source: FeeSource::Default,
            asset: "BTC/USD",
            bar_ts: chrono::Utc::now(),
            bar_open: 60_000.0,
            bar_high: 61_000.0,
            bar_low: 59_000.0,
        }
    }

    #[test]
    fn flat_when_already_flat_is_noop() {
        // Updated because <reason>: simulate_fill now returns SimulateFillResult;
        // unwrap .outcome to access fill fields.
        let out = simulate_fill(args(0.0, "flat")).outcome;
        assert_eq!(out.new_pos, 0.0);
        assert!(out.fill_price.is_none());
        assert_eq!(out.realized_pnl, 0.0);
    }

    #[test]
    fn long_open_from_flat_opens_long_at_slipped_up_price() {
        // Updated because <reason>: simulate_fill now returns SimulateFillResult;
        // unwrap .outcome to access fill fields.
        let out = simulate_fill(args(0.0, "long_open")).outcome;
        assert!(out.new_pos > 0.0);
        let fp = out.fill_price.unwrap();
        assert!(fp > 60_000.0); // slip adds for buys
        assert!((fp - 60_060.0).abs() < 1e-6); // 60_000 * 1.001
    }

    #[test]
    fn flat_closes_long_and_books_realized() {
        // Updated because <reason>: simulate_fill now returns SimulateFillResult;
        // unwrap .outcome to access fill fields.
        // pos=0.001 BTC bought at 50_000, close at 60_000-slip
        let out = simulate_fill(args(0.001, "flat")).outcome;
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
        // Updated because <reason>: simulate_fill now returns SimulateFillResult;
        // unwrap .outcome to access fill fields.
        let out = simulate_fill(args(0.001, "long_open")).outcome;
        assert_eq!(out.new_pos, 0.001);
        assert!(out.fill_price.is_none());
    }

    #[test]
    fn short_open_from_long_reverses_and_books_realized() {
        // Updated because <reason>: simulate_fill now returns SimulateFillResult;
        // unwrap .outcome to access fill fields.
        let out = simulate_fill(args(0.001, "short_open")).outcome;
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
                attested_with: vec!["m".into()],
                required_tools: vec!["ohlcv".into()],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
                color: None,
                execution_mode: Default::default(),
                capital_mode: Default::default(),
            },
            hypothesis: None,
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({}),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
        acknowledge_no_filter: false,
        }
    }

    fn resolved(role: &str, model: &str) -> ResolvedAgentSlot {
        ResolvedAgentSlot {
            role: role.into(),
            slot: LLMSlot {
                role: role.into(),
                attested_with: model.into(),
                allowed_tools: Vec::new(),
                provider: None,
                model: Some(model.into()),
            },
            system_prompt: "p".into(),
            max_tokens: None,
            temperature: None,
            inputs_policy: crate::agents::InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            capabilities: std::collections::BTreeSet::new(),
            noop_skip: true,
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
