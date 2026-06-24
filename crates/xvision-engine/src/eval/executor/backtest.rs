//! `Executor` — replays an OHLCV fixture in chronological order,
//! invoking the strategy's pipeline at each decision boundary and simulating
//! fills against the next bar's open with linear slippage + taker fees. No
//! broker is involved; positions and equity are tracked in-memory.
//!
//! This is the v1 demo path that doesn't require external broker keys.
//! Pair with `xvn eval run --mode backtest --strategy <id> --scenario <id>`.
//!
//! Out of scope (deferred):
//! - Multi-asset universes (uses `scenario.asset_universe[0]` only — v1
//!   constraint, same as paper-mode-executor-deleted).
//! - Indicator panel injection into the pipeline seed (matching what
//!   paper-mode-executor-deleted passes today, which is just portfolio_state).
//! - Win-rate is sourced from realized round-trip PnL: each time a position
//!   returns to flat (trader `flat`/flip OR a deterministic SL/TP exit) the
//!   `realized_count` denominator and, on a positive realized PnL, the `wins`
//!   numerator are incremented; `win_rate = wins / realized_count`. See the
//!   run-accounting counter doc at the counter declarations below (U4).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
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
use crate::eval::executor::live_source::MultiLiveStream;
use crate::eval::executor::real_broker_fills::RealBrokerFills;
use crate::eval::executor::trace_types::{AggressorSide, FillBranch};
use crate::eval::executor::traits::{
    eval_only_token, Clock, FillRecord, FillRequest, FillSink, InstantClock, SimulatedFills,
};
use crate::eval::executor::wall_clock::WallClock;
use crate::eval::executor::RunExecutor;
use crate::eval::findings::{make_volume_share_excess_finding, Finding, Severity};
use crate::eval::guardrails::{
    self as guardrails, position_state_from_size, supervisor_note_content, Action as GuardAction,
    GuardrailDecision,
};
use crate::eval::live_config::LiveConfig;
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
use crate::strategies::risk::RiskConfig;
use crate::strategies::{ClosePolicy, DecisionMode, MechanisticConfig, Strategy};
use crate::tools::ToolRegistry;

use super::trader_output::TraderOutput;
use xvision_execution::broker_surface::{BrokerErrorClass, BrokerSurface};

pub(crate) struct LiveRuntime {
    /// Multi-asset live bar fanout. A single active asset is a 1-element
    /// `MultiLiveStream` (== the single L1 `LiveStream`), preserving
    /// single-asset byte-identity; multiple actives merge their per-asset
    /// `LiveStream`s into one tagged-bar stream.
    pub(crate) bar_source: MultiLiveStream,
    pub(crate) clock: WallClock,
    pub(crate) fill_sink: RealBrokerFills,
    /// Run-terminating limits (time / bar / decision). The live loop
    /// evaluates whichever fires first; `LiveConfig::validate` guarantees
    /// at least one is set, so a live run always has a deterministic exit
    /// even when the bar stream never closes.
    pub(crate) stop_policy: crate::eval::live_config::StopPolicy,
}

#[derive(Default)]
pub struct Executor {
    /// Optional progress channel. When `None` the executor is silent
    /// (today's `api::eval::run_with_deps` callers); when `Some`, every
    /// significant action emits a `ProgressEvent`. Send-when-no-subscribers
    /// is a no-op via `send_event`. Mirrors PR #35's paper-mode-executor-deleted wiring
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
    live_runtime: Option<tokio::sync::Mutex<LiveRuntime>>,
    fill_sink_override: Option<tokio::sync::Mutex<Box<dyn FillSink>>>,
    /// Optional per-run narrowing of the strategy's `asset_universe`.
    /// `None` runs the full universe. CLI run-layer asset narrowing can
    /// thread a subset here without putting assets back on scenarios.
    asset_subset: Option<Vec<xvision_core::trading::AssetSymbol>>,
    /// Optional per-asset injected bars. When set, backtest execution
    /// builds an aligned multi-asset timeline from this map instead of
    /// mapping `injected_bars` to the first active asset.
    injected_asset_bars: Option<BTreeMap<xvision_core::trading::AssetSymbol, Vec<Ohlcv>>>,
    /// Optional run-level market-data context keyed by asset + timeframe.
    market_data: Option<crate::eval::market_data::MarketDataContext>,
    /// Stage 1 (Cline runtime unification) — which runtime drives the
    /// LLM-backed slots for this run. Defaults to `LlmDispatch`; the eval
    /// entry point sets it from `RuntimeConfig.agent_runtime` and pairs it
    /// with `cline` when `Cline` is selected. Threaded into every
    /// `run_pipeline` invocation via `PipelineInputs.runtime`.
    agent_runtime: xvision_core::config::AgentRuntime,
    /// The live sidecar context, present only when the eval entry point
    /// spawned a Cline client for this run. `None` keeps dispatch on
    /// `LlmDispatch` regardless of `agent_runtime`.
    cline: Option<crate::agent::dispatch_capability::ClineDispatchCtx>,
    /// F9 (2026-06-04): when `Some(variant)`, this run is an autooptimizer
    /// honesty-check (canary) executing a deliberately-sabotaged strategy
    /// (`kill-trades`, `remove-loss-limit`, `absurd-cadence`). The broker-rule
    /// rejection site demotes blocking violations to `debug` annotated as
    /// "expected (honesty-check sabotage)" rather than emitting a bare `WARN
    /// min_order_size_violation` that an operator cannot distinguish from a
    /// real fault. `None` (every real run) keeps the `WARN` intact.
    canary_sabotage: Option<String>,
    /// LANE byu — optional periodic attest sink for the LIVE loop. When
    /// `Some`, the executor invokes `maybe_attest` every
    /// `attest_every_n_trades` executed (filled) trades with a listed-
    /// performance snapshot. Dependency-inverted: the engine defines the
    /// trait + a no-op default; the concrete identity-backed impl is injected
    /// from the dashboard (which depends on both engine and identity), so NO
    /// hard `xvision-engine -> xvision-identity` Cargo edge is added. `None`
    /// (every backtest, every test that does not wire it) keeps the seam
    /// dormant. See `attest_hook.rs`.
    attest_hook: Option<Arc<dyn super::attest_hook::AttestHook>>,
    /// Trade interval for the attest hook (default 20). Clamped to at least 1
    /// at the call site so the boundary modulo never divides by zero. Only
    /// consulted when `attest_hook` is `Some`.
    attest_every_n_trades: u32,
}

/// LANE byu — default cadence (in executed trades) at which the live loop
/// fires the periodic [`super::attest_hook::AttestHook`]. The dashboard can
/// override per-run via [`Executor::with_attest_hook`].
pub const DEFAULT_ATTEST_EVERY_N_TRADES: u32 = 20;

impl Executor {
    /// Backtest constructor — wires `InjectedBars + InstantClock +
    /// SimulatedFills` under the hood. The trio is composed inside
    /// `run_inner` from the supplied bars; this constructor is the
    /// stable entry point the API dispatch site uses for
    /// [`RunMode::Backtest`].
    ///
    /// Mirrors [`Executor::with_bars`] today (which it delegates to).
    /// Future evolution: take a `CostModel` explicitly rather than
    /// reading it back off the scenario at `run_inner` time. Kept
    /// minimal here so the executor-collapse-paper-mode +
    /// executor-live-shell PR stays focused.
    pub fn backtest(bars: Vec<Ohlcv>) -> Self {
        Self::with_bars(bars)
    }

    /// Live constructor — wires `MultiLiveStream + WallClock +
    /// RealBrokerFills`. The `bar_source` is a multi-asset fanout; a
    /// single-asset live run hands in a 1-element `MultiLiveStream`, which
    /// is behaviourally identical to the L1 single `LiveStream`.
    pub fn live(
        live_config: &LiveConfig,
        broker: Arc<dyn BrokerSurface>,
        bar_source: MultiLiveStream,
        clock: WallClock,
        obs_emitter: Option<ObsEmitter>,
    ) -> anyhow::Result<Self> {
        live_config
            .validate()
            .map_err(|e| anyhow!("invalid LiveConfig: {e:?}"))?;
        Ok(Self {
            progress: None,
            injected_bars: None,
            warmup_bars: Vec::new(),
            event_bus: None,
            obs_emitter,
            memory_recorder: None,
            provider_catalogs: HashMap::new(),
            limits: None,
            recorder: None,
            live_runtime: Some(tokio::sync::Mutex::new(LiveRuntime {
                bar_source,
                clock,
                fill_sink: RealBrokerFills::new(broker),
                stop_policy: live_config.stop_policy.clone(),
            })),
            fill_sink_override: None,
            asset_subset: None,
            injected_asset_bars: None,
            market_data: None,
            agent_runtime: Default::default(),
            cline: None,
            canary_sabotage: None,
            attest_hook: None,
            attest_every_n_trades: DEFAULT_ATTEST_EVERY_N_TRADES,
        })
    }

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
            live_runtime: None,
            fill_sink_override: None,
            asset_subset: None,
            injected_asset_bars: None,
            market_data: None,
            agent_runtime: Default::default(),
            cline: None,
            canary_sabotage: None,
            attest_hook: None,
            attest_every_n_trades: DEFAULT_ATTEST_EVERY_N_TRADES,
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
            live_runtime: None,
            fill_sink_override: None,
            asset_subset: None,
            injected_asset_bars: None,
            market_data: None,
            agent_runtime: Default::default(),
            cline: None,
            canary_sabotage: None,
            attest_hook: None,
            attest_every_n_trades: DEFAULT_ATTEST_EVERY_N_TRADES,
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
            live_runtime: None,
            fill_sink_override: None,
            asset_subset: None,
            injected_asset_bars: None,
            market_data: None,
            agent_runtime: Default::default(),
            cline: None,
            canary_sabotage: None,
            attest_hook: None,
            attest_every_n_trades: DEFAULT_ATTEST_EVERY_N_TRADES,
        }
    }

    /// Attach a progress `ProgressTx` to an EXISTING executor, builder-style,
    /// without resetting the other fields. `with_progress` (the constructor)
    /// resets every field, so chaining it after `with_bars`/`with_event_bus`
    /// would wipe the injected bars/bus. This setter is the chainable form the
    /// optimizer paper-tester adapter (U5) uses:
    ///   `Executor::with_bars(bars).with_event_bus(bus).with_progress_tx(tx)`.
    pub fn with_progress_tx(mut self, progress: ProgressTx) -> Self {
        self.progress = Some(progress);
        self
    }

    /// Attach a live-stream event bus to an existing executor. Builder-style
    /// so callers can chain after `with_bars` / `with_progress`:
    ///   `Executor::with_bars(bars).with_event_bus(bus)`.
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
    ///   `Executor::with_bars(bars).with_warmup(warmup)`.
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

    /// Narrow a backtest run to a subset of `strategy.manifest.asset_universe`.
    pub fn with_asset_subset(mut self, subset: Vec<xvision_core::trading::AssetSymbol>) -> Self {
        self.asset_subset = Some(subset);
        self
    }

    /// Inject per-asset bar series for multi-asset backtests.
    pub fn with_asset_bars(mut self, bars: BTreeMap<xvision_core::trading::AssetSymbol, Vec<Ohlcv>>) -> Self {
        self.injected_asset_bars = Some(bars);
        self
    }

    /// Inject run-level asset/timeframe market-data context.
    pub fn with_market_data(mut self, market_data: crate::eval::market_data::MarketDataContext) -> Self {
        self.market_data = Some(market_data);
        self
    }

    /// F9 — mark this run as an autooptimizer honesty-check (canary) executing
    /// the named sabotage variant, so broker-rule rejections it provokes by
    /// design are relabeled as expected honesty-check noise rather than logged
    /// as bare `WARN min_order_size_violation`.
    pub fn with_canary_sabotage(mut self, variant: impl Into<String>) -> Self {
        self.canary_sabotage = Some(variant.into());
        self
    }

    #[doc(hidden)]
    pub fn with_fill_sink(mut self, sink: Box<dyn FillSink>) -> Self {
        self.fill_sink_override = Some(tokio::sync::Mutex::new(sink));
        self
    }

    /// LANE byu — attach a periodic attest sink to the LIVE loop, firing every
    /// `every_n` executed (filled) trades with a listed-performance snapshot.
    /// Builder-style so the dashboard can chain after `Executor::live(...)`:
    ///   `Executor::live(..)?.with_attest_hook(identity_hook, 20)`.
    ///
    /// `every_n` is clamped to at least 1 (a `0` becomes "every trade") so the
    /// boundary modulo can never divide by zero. The hook is invoked
    /// fire-and-forget from `run_inner_live` — a slow/failing attestation
    /// never blocks or aborts the trading loop. The concrete
    /// `xvision-identity` implementation is injected here from the dashboard,
    /// which keeps the engine free of a hard identity dependency.
    pub fn with_attest_hook(mut self, hook: Arc<dyn super::attest_hook::AttestHook>, every_n: u32) -> Self {
        self.attest_hook = Some(hook);
        self.attest_every_n_trades = super::attest_hook::clamp_every_n(every_n);
        self
    }

    /// WS-9 — set the attest boundary interval WITHOUT wiring a hook. The
    /// engine emits `attest_boundary_reached` at every `every_n`-trade boundary
    /// independently of the hook, so an observed live run can surface the
    /// boundary on the trace dock even before an identity-backed hook exists.
    /// `every_n` is clamped to at least 1 (a `0` becomes "every trade").
    pub fn with_attest_every_n_trades(mut self, every_n: u32) -> Self {
        self.attest_every_n_trades = super::attest_hook::clamp_every_n(every_n);
        self
    }

    /// Stage 1 (Cline runtime unification) — select the agent runtime and,
    /// for `Cline`, the live sidecar context. Builder-style so the eval
    /// entry point can chain after `with_bars` / `with_observability`:
    ///   `Executor::with_bars(bars).with_cline_runtime(runtime, Some(ctx))`.
    ///
    /// When `runtime == Cline` but `cline` is `None`, the dispatcher falls
    /// back to `LlmDispatch` (the `should_use_cline` guard), so a flag flip
    /// without a wired client never silently drops a decision.
    pub fn with_cline_runtime(
        mut self,
        runtime: xvision_core::config::AgentRuntime,
        cline: Option<crate::agent::dispatch_capability::ClineDispatchCtx>,
    ) -> Self {
        self.agent_runtime = runtime;
        self.cline = cline;
        self
    }

    /// Returns `true` when the executor has a live `ClineDispatchCtx`, meaning
    /// the trader slot will be dispatched via `execute_slot_cline` rather than
    /// falling back to `LlmDispatch`.
    ///
    /// **Test-only accessor.** Production code must not branch on this;
    /// use `should_use_cline` inside the pipeline dispatch path instead.
    /// `pub` (not `#[cfg(test)]`) so integration tests in `tests/` can reach it.
    pub fn cline_is_wired(&self) -> bool {
        self.cline.is_some()
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
impl RunExecutor for Executor {
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
                let _ = store.fail_active(&run.id, &reason, None).await;
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

impl Executor {
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
        // §3 (cline-live-followups): split backtest / live at the top.
        // When a `LiveRuntime` is present the run is driven by a streaming
        // loop (LiveStream + WallClock + RealBrokerFills) — see
        // `run_inner_live`. The entire backtest body below is unchanged and
        // stays byte-identical for `RunMode::Backtest` (no live_runtime).
        if self.live_runtime.is_some() {
            return self
                .run_inner_live(run, strategy, scenario, agent_slots, dispatch, tools, store)
                .await;
        }
        // Multi-asset (B4): scenarios are asset-free; the asset set a run
        // trades comes from the strategy's `asset_universe` (resolved /
        // validated by `active_assets`, optionally narrowed by an
        // `--assets` subset). `PerAsset` execution runs the pipeline once
        // per active asset each bar, sharing one pooled capital book.
        use crate::eval::executor::asset_set::active_assets;
        let active = active_assets(&strategy.manifest.asset_universe, self.asset_subset.as_deref())?;
        // The first active asset doubles as the single-asset fixture key
        // (the legacy `load_ohlcv_fixture` path loads by alpaca pair) and
        // as the default key for the single `injected_bars` vec.
        let asset_sym = *active.first().ok_or_else(|| {
            anyhow!(
                "strategy {} resolved an empty active asset set",
                strategy.manifest.id
            )
        })?;
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

        // Multi-asset (B4) — per-asset bars. Three sources, in precedence:
        // 1. `with_asset_bars` — explicit per-asset vecs (multi-asset test
        //    path; the production API/CLI layer threads resolved bars here).
        // 2. `with_bars` — a single pre-loaded vec (Task 8 DB-resolved path,
        //    single-asset). Mapped to the first active asset.
        // 3. Legacy fixture loader — the canonical-scenarios fallback reads
        //    `data/probes/<cache_key>.parquet` for the first active asset.
        //    Keeps pre-Task-8 tests working without a DB / Alpaca creds.
        //
        // Sources 2 and 3 are single-asset; they only key the first active
        // asset, so a single-asset run is byte-identical to the old path.
        let asset_bars: BTreeMap<xvision_core::trading::AssetSymbol, Vec<Ohlcv>> =
            if let Some(per_asset) = self.injected_asset_bars.clone() {
                per_asset
            } else {
                let single: Vec<Ohlcv> = if let Some(injected) = self.injected_bars.clone() {
                    injected
                } else {
                    let data_seed = &scenario.bar_cache_policy.cache_key;
                    load_ohlcv_fixture(data_seed, &asset, usize::MAX)
                        .map_err(|e| anyhow!("load fixture {}: {e}", data_seed))?
                };
                BTreeMap::from([(asset_sym, single)])
            };
        // An empty bar list is the only genuinely-uninterpretable case
        // (qa-decisions-30day-count). Anything narrower is a loader
        // contract bug, not a runtime input to silently tolerate.
        if asset_bars.values().all(|v| v.is_empty()) {
            anyhow::bail!("scenario {} has no bars; nothing to backtest", scenario.id,);
        }

        // The fan-out only iterates assets that actually have bars. An
        // active asset with no injected/loaded bars contributes no
        // decisions (and carries any open position untouched). For the
        // single-asset path this is exactly `[asset_sym]`.
        let active: Vec<xvision_core::trading::AssetSymbol> = active
            .into_iter()
            .filter(|a| asset_bars.get(a).is_some_and(|v| !v.is_empty()))
            .collect();
        if active.is_empty() {
            anyhow::bail!("scenario {} has no bars for any active asset", scenario.id,);
        }
        // Venue symbols of the active set, surfaced in each seed as
        // `active_assets` so the trader sees the cross-asset context.
        let active_venue_symbols: Vec<String> = active.iter().map(|a| a.as_alpaca_pair()).collect();

        // Aligned timeline: outer-join the per-asset bar series by
        // timestamp. `timeline[ts][asset] = bar_index` — the per-asset
        // index into `asset_bars[asset]` so the per-decision body can do
        // T+1 look-ahead and history slicing exactly as the single-asset
        // path did. An asset missing a bar at `ts` simply isn't present in
        // the inner map and gets no decision there. BTreeMap keys keep the
        // timestamp order ascending and the per-timestamp asset order
        // deterministic (AssetSymbol is `Ord`).
        let mut timeline: BTreeMap<
            chrono::DateTime<chrono::Utc>,
            BTreeMap<xvision_core::trading::AssetSymbol, usize>,
        > = BTreeMap::new();
        for a in &active {
            for (idx, bar) in asset_bars[a].iter().enumerate() {
                timeline.entry(bar.timestamp).or_default().insert(*a, idx);
            }
        }

        // executor-trait-extraction (sub-track 1): fill production routes
        // through the FillSink trait. The multi-asset fan-out drives the
        // aligned `timeline` directly (rather than a single BarSource), so
        // the per-asset T+1 look-ahead can index each asset's own vec; the
        // Clock + FillSink seams are preserved for the future Live impl.
        let mut clock: Box<dyn Clock> = Box::new(InstantClock::new());
        let mut fill_sink: Box<dyn FillSink> = Box::new(SimulatedFills::new(eval_only_token()));

        // Used by RunTick to report timeline progress. One tick per
        // distinct timestamp (the bar clock), independent of how many
        // assets decided at that timestamp.
        let total_decision_bars = timeline.len().max(1) as f64;

        // Per-decision rolling-history window. Warmup bars (from
        // `eval::bars::load_warmup_bars`) are concatenated in front of the
        // first active asset's bars so we can slice the last
        // `scenario.warmup_bars` bars at each decision and surface them in
        // the seed as `market_data.bar_history`. v1 warmup is single-asset
        // (the DB path resolves warmup per the single resolved asset); for
        // additional assets in a multi-asset run the history window is
        // built from that asset's own in-window bars with no warmup prefix.
        let warmup_count = self.warmup_bars.len();
        let history_window = scenario.warmup_bars as usize;
        // Per-asset combined `[warmup..., bars...]` views for history
        // slicing. Only the first active asset gets the warmup prefix
        // (warmup is single-asset in v1); the rest use their bars as-is.
        let combined_bars_by_asset: BTreeMap<xvision_core::trading::AssetSymbol, Vec<&Ohlcv>> = active
            .iter()
            .map(|a| {
                let combined: Vec<&Ohlcv> = if *a == asset_sym {
                    self.warmup_bars.iter().chain(asset_bars[a].iter()).collect()
                } else {
                    asset_bars[a].iter().collect()
                };
                (*a, combined)
            })
            .collect();

        // F-6: per-run seed-sanitization policy. Shared implementation used
        // by both the backtest and live executor paths; `Raw` (default)
        // reproduces the pre-F-6 JSON byte-for-byte so this branch is a
        // no-op for every existing scenario+strategy combination that didn't
        // opt into `Causal`.
        let inputs_policy = resolve_inputs_policy(agent_slots);
        // F-8: optional rolling-window cap; shared by both executor paths.
        // `None` keeps today's behavior; `Some(n)` trims the slice to
        // the most-recent `n` entries.
        let bar_history_limit = resolve_bar_history_limit(agent_slots);

        let supported_timeframes = self
            .market_data
            .as_ref()
            .map(|ctx| ctx.supported_timeframes(asset_sym))
            .unwrap_or_else(|| vec![strategy.native_timeframe().as_str().to_string()]);
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
        // Tracks bars held while short per asset for borrow-cost accrual.
        let mut short_bars_held: BTreeMap<xvision_core::trading::AssetSymbol, u32> = BTreeMap::new();
        // Advanced SL/TP per-position state (trailing, break-even, fading, time, ATR, partial TP).
        let mut sltp_state: BTreeMap<
            xvision_core::trading::AssetSymbol,
            crate::eval::executor::sltp::PositionRiskState,
        > = BTreeMap::new();
        // Run-local episodic memory store — accumulates structured decision
        // observations across bars for within-run recall. Scoped to this run;
        // dropped when the run completes. Not persisted to SQLite (R3).
        let mut episodic_store = crate::agent::episodic::EpisodicStore::new(500);
        let bar_secs = (cadence_min.max(1) as u64) * 60;
        let mut decision_idx = 0u32;
        // Phase C — per-eval-run signal cache owned by the executor.
        // Lifetime equals the run loop; dropped when the run completes.
        // Built once here so the cache survives across cycles and the
        // Minute / Decision granularity paths can re-fire prior signals.
        let mut signal_cache = crate::agent::signal_cache::SignalCache::new();
        let multi_filter_config = crate::agent::filter_dispatch::MultiFilterConfig::default();
        let bar_period_minutes = cadence_min.max(1) as u32;
        // Run-accounting counters (U4 — definitions; keep these in sync with
        // the doc on `MetricsSummary`). Each measures a DIFFERENT thing and
        // they intentionally do not all share a denominator:
        //
        //   * `n_trades`       — FILL LEGS that crossed the book: opens, closes,
        //                        SL/TP forced exits, and partial-TP1 slices each
        //                        count one. An open+close round-trip is therefore
        //                        2 here. This is the historical "trade count"
        //                        semantics asserted by existing tests
        //                        (eval_executor_paper, eval_executor_live_loop)
        //                        and is NOT changed.
        //   * `realized_count` — CLOSED ROUND-TRIPS: incremented once each time a
        //                        non-flat position returns to flat (via trader
        //                        `flat`/flip OR a deterministic SL/TP exit). This
        //                        is the `win_rate` denominator.
        //   * `wins`           — round-trips whose realized PnL was > 0; numerator
        //                        of `win_rate = wins / realized_count`.
        //
        // The decision/wake counters live elsewhere:
        //   * `decision_idx` (→ `MetricsSummary.n_decisions`) — LLM-pipeline
        //     decision slots, INCLUDING synthesized SL/TP exit rows; cadence-
        //     gated and filter-suppressed bars do NOT increment it (correct: no
        //     decision happened).
        //   * Filter wake/suppression counters (`FilterSummary.wakeups` = Trips,
        //     `suppressed_in_position` = in-position Holds, etc.) are aggregated
        //     separately in `xvision_filters::events::FilterSummary::from_events`.
        let mut n_trades = 0u32;
        let mut wins = 0u32;
        let mut realized_count = 0u32;
        // R3 daily-loss kill: the UTC day the realized-loss window currently
        // tracks, and the book's cumulative realized PnL at that day's start.
        // `realized_today = book.realized() - daily_realized_at_day_start`.
        let mut daily_loss_day: Option<chrono::NaiveDate> = None;
        let mut daily_realized_at_day_start: f64 = 0.0;
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        // Wall-clock anchor for `EvalLimits::max_wall_clock_secs`. Captured
        // at the start of the decision loop, NOT at `run.started_at` — the
        // CLI cap is measured against the executor's own running window so
        // a slow scenario-load doesn't burn the operator's budget.
        let run_started: Instant = Instant::now();
        // engine-trade-guardrails-pyramid-flip-block (F-7):
        // tracks the trader's most recent emitted open direction PER ASSET
        // so the guardrail can detect a same-bar flip even when the
        // executor's live position is momentarily flat between a close and
        // an opposite open. Cleared on emitted `flat`. Only updated from
        // the ORIGINAL trader action — a guardrail-rewritten `hold` does
        // not bump the direction state. Keyed per asset so a flip on BTC
        // doesn't leak into ETH's flip detection.
        let mut last_open_direction: BTreeMap<xvision_core::trading::AssetSymbol, GuardAction> =
            BTreeMap::new();
        // WS-15 (`trace-obs` market-context): per-asset last-seen regime
        // label. A market regime change is the highest-signal pre-decision
        // state shift; when the label for an asset differs from the prior
        // decision we emit a queryable `regime_transition` engine event
        // (see the per-decision emit below). Keyed per asset so a regime
        // flip on BTC doesn't masquerade as one on ETH. Purely
        // observational — never feeds back into regime/briefing/trading.
        let mut last_regime_label: BTreeMap<xvision_core::trading::AssetSymbol, String> = BTreeMap::new();
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
        //
        // Multi-asset (B4): this state is PER ASSET — each asset has its
        // own flat-degeneracy streak and skip window. A flat run on BTC
        // must not skip decisions on ETH. `EarlyStopState::default()` is
        // the empty single-asset starting point preserved byte-identically.
        #[derive(Default)]
        struct EarlyStopState {
            recent_actions: Vec<early_stop::Action>,
            recent_convictions: Vec<f64>,
            inherit_remaining: u32,
            prev_position: f64,
        }
        let early_stop_cfg = EarlyStopConfig::from_env_or_default();
        let mut early_stop_state: BTreeMap<xvision_core::trading::AssetSymbol, EarlyStopState> =
            active.iter().map(|a| (*a, EarlyStopState::default())).collect();

        // track-plan-touches: build the per-run filter hook. Returns
        // `None` for `EveryBar` strategies (the default) — the loop
        // body short-circuits the hook call when `filter_hook.is_none()`.
        // Errors (CompiledRules, FilterGated without filter) surface
        // here and abort the run, matching the rest of the pre-loop
        // strategy invariants.
        // trace-obs WS-6: thread the run's ObsEmitter into the hook so a
        // filter trip emits a `filter_fired` engine event onto the bus
        // (in addition to the per-bar `eval_filter_evaluations` row +
        // ProgressEvent). `None` for non-observed runs keeps the legacy
        // table-only path.
        let mut filter_hook = crate::eval::filter_hook::FilterHook::new(strategy)?
            .map(|hook| hook.with_obs(self.obs_emitter.clone()));
        // ERROR-1 (docs/QA/2026-06-14-eval-test-gemini-flash-churn-findings.md):
        // the documented `wake_when_in_position = Never` contract is "no
        // mid-position calls; exits rely ENTIRELY on the deterministic risk
        // gate (risk.stop_loss_atr_multiple)". A model that nonetheless emits a
        // protective bracket at open time plants a stop tighter than the config
        // ATR stop; because the trader is never re-woken in-position, the entry
        // bar's own intraday low trips it and the position re-opens next bar →
        // 1-bar churn / fee bleed. Honor the contract: under wake:never, ignore
        // the model's protective brackets so only the config ATR stop (applied
        // via the R1 fallback below) manages the exit.
        let wake_never = strategy
            .filter
            .as_ref()
            .map(|f| matches!(f.wake_when_in_position, xvision_filters::WakeInPosition::Never))
            .unwrap_or(false);
        // Cache the pool handle for the hook's per-bar inserts. We
        // reach through `store` rather than threading it as a separate
        // parameter so the executor's surface area stays unchanged.
        let pool = store.pool().clone();

        // Multi-asset (B4): the per-bar loop is now driven by the aligned
        // `timeline` (one outer step per distinct timestamp). At each
        // timestamp the inner loop fans out over the assets that have a
        // bar there, running the per-asset decision body for each. Equity
        // is marked + recorded ONCE per timestamp (pooled NAV) after all
        // that timestamp's assets have decided — `eval_equity_samples` is
        // keyed `(run_id, timestamp)`, so there is exactly one pooled
        // series, not one per asset. For a single-asset run this collapses
        // to one asset per timestamp and is byte-identical to the old
        // per-bar path. `timeline_idx` drives the RunTick progress %.
        let mut timeline_idx: usize = 0;
        // B25: equity samples are buffered in-memory during the backtest loop and
        // flushed in a single transaction at the end of the run. This collapses
        // ~2 000 auto-commit INSERTs (one per timestamp) into one fsync,
        // eliminating the WAL checkpoint stall that inflated the second eval
        // window by ~3×. The Vec is pre-allocated to the timeline length so no
        // reallocation occurs during iteration.
        let mut equity_samples_buf: Vec<(chrono::DateTime<chrono::Utc>, f64)> =
            Vec::with_capacity(timeline.len());
        // F36 (capture-on-interrupt): persist accumulated metrics+tokens every
        // PARTIAL_PERSIST_INTERVAL so a run that never reaches `finalize`
        // (cancelled / timed out / crashed) isn't left with NULL metrics. The
        // cancel checkpoint below also persists a final snapshot before bailing.
        let mut last_partial_persist = Instant::now();
        // U5: wall-clock heartbeat so a long backtest (e.g. the optimizer's
        // parent baseline eval) doesn't go silent for 10–20 minutes. Mirrors
        // the `PARTIAL_PERSIST_INTERVAL` pattern — a constant-time `Instant`
        // check at the TOP of the loop, independent of the cadence early-continue
        // below, so it fires on wall-clock time rather than per-decision.
        let mut last_heartbeat = Instant::now();
        for (&ts, assets_at_ts) in &timeline {
            // Update the logical clock to this timestamp before any
            // decision-side work. Live impls ignore this.
            clock.advance_to(ts);
            if last_heartbeat.elapsed() >= EVAL_HEARTBEAT_INTERVAL {
                self.emit(ProgressEvent::EvalHeartbeat {
                    run_id: run.id.clone(),
                    decisions: decision_idx as u64,
                    elapsed_s: run_started.elapsed().as_secs(),
                });
                last_heartbeat = Instant::now();
            }
            if store.is_terminal(&run.id).await? {
                // F36: capture the partial metrics+tokens accumulated up to the
                // interrupt before bailing.
                persist_partial_snapshot(
                    store,
                    &run.id,
                    &equity_curve,
                    initial,
                    equity,
                    strategy.manifest.decision_cadence_minutes,
                    realized_count,
                    wins,
                    n_trades,
                    decision_idx,
                    total_input_tokens,
                    total_output_tokens,
                )
                .await;
                anyhow::bail!("eval run stopped");
            }
            if last_partial_persist.elapsed() >= PARTIAL_PERSIST_INTERVAL {
                persist_partial_snapshot(
                    store,
                    &run.id,
                    &equity_curve,
                    initial,
                    equity,
                    strategy.manifest.decision_cadence_minutes,
                    realized_count,
                    wins,
                    n_trades,
                    decision_idx,
                    total_input_tokens,
                    total_output_tokens,
                )
                .await;
                last_partial_persist = Instant::now();
            }
            // Cadence gate: only fire on timestamps whose minute-aligned
            // value is divisible by the strategy's cadence. Timestamp-level
            // (shared across all assets at this ts).
            if (ts.timestamp() / 60) % cadence_min != 0 {
                timeline_idx += 1;
                continue;
            }
            // Track the first active asset's bar at this timestamp so
            // baselines (a single-series computation) replay the same bar
            // slice the strategy saw. Single-asset: identical to the old
            // `decision_bars.push(bar.clone())`.
            if let Some(&first_idx) = assets_at_ts.get(&asset_sym) {
                decision_bars.push(asset_bars[&asset_sym][first_idx].clone());
            }

            // RunTick fires before the per-timestamp pipeline work so
            // dashboards advance even when an LLM round-trip is slow.
            let scenario_progress_pct =
                ((timeline_idx as f64 / total_decision_bars) * 100.0).clamp(0.0, 100.0);
            self.emit(ProgressEvent::RunTick {
                run_id: run.id.clone(),
                scenario_progress_pct,
                current_ts: ts,
            });

            // track-plan-touches: per-bar filter evaluation. `None` means
            // EveryBar strategy (no gating). The filter is a STRATEGY-level
            // gate, evaluated once per timestamp; when not `Active` it skips
            // ALL assets' decisions this timestamp. `in_position` is true
            // when any leg is open.
            //
            // QA31 (filter-bypass on staggered multi-asset data): previously
            // this hard-coded `asset_sym` (the FIRST active asset, captured
            // outside the timestamp loop). If that first asset had a data
            // gap at this timestamp — common in multi-asset backtests with
            // different trading hours or holidays — the entire filter
            // evaluation was SKIPPED, leaving `filter_gated = false`. The
            // 6-fires/day cap and the cooldown were silently bypassed for
            // every other asset at that timestamp, and operators saw the
            // strategy fire every bar despite a strict filter being set.
            //
            // Fix: evaluate on ANY asset that has a bar at this timestamp.
            // `assets_at_ts` is exactly that set (its keys are the assets
            // with present bars), so picking the first entry from the
            // BTreeMap gives a deterministic representative bar. The
            // strategy filter is a STRATEGY-level signal — it doesn't care
            // which asset's bar it sees, only that the timestamp has one.
            //
            // The wakeup gate allows LLM dispatch on both Trip (first bar
            // the condition tree becomes true after Inactive/Cooldown) AND
            // Hold (subsequent bars the condition tree stays true). Level
            // operators (Gt/Lt/Gte/Lte) correctly fire every bar the level
            // holds; Cooldown, CappedForDay, SuppressedInPosition, Warming,
            // and Inactive still suppress dispatch.
            let mut filter_gated = false;
            let mut filter_trigger_context: Option<serde_json::Value> = None;
            if let Some(hook) = filter_hook.as_mut() {
                if let Some((&gate_asset_sym, &gate_idx)) = assets_at_ts.iter().next() {
                    let gate_bar = &asset_bars[&gate_asset_sym][gate_idx];
                    let in_position = active.iter().any(|a| book.position(*a).abs() > f64::EPSILON);
                    let evaluation = hook.evaluate(gate_bar, in_position);
                    hook.record(&pool, self.progress.as_ref(), &run.id, ts, &evaluation)
                        .await?;
                    if !evaluation.outcome.decision.is_active() {
                        filter_gated = true;
                        // U11: surface the specific "blocked because a position
                        // is open" case on the live progress stream so operators
                        // can tell it apart from "filter simply didn't fire". The
                        // suppressed_in_position reason is already recorded in the
                        // persisted FilterEventV1 ledger via `hook.record`; this
                        // additional event rides the same ProgressTx for live
                        // CLI/dashboard consumers.
                        if matches!(
                            evaluation.outcome.decision,
                            xvision_filters::runtime::ActivationDecision::SuppressedInPosition
                        ) {
                            self.emit(ProgressEvent::FilterBlocked {
                                run_id: run.id.clone(),
                                reason: "in_position".to_string(),
                            });
                        }
                    } else {
                        filter_trigger_context = evaluation.trigger_context.clone();
                    }
                }
            }

            // Per-asset fan-out. Each iteration runs the existing
            // per-decision body for one asset using that asset's own bar
            // vec + index (T+1 look-ahead, history slicing). `'asset`
            // labels the loop so the body's skip paths (early-stop inherit,
            // filter gate) advance to the next asset rather than the next
            // timestamp. Skipped via `continue 'asset` after recording the
            // per-asset decision row.
            'asset: for (&asset_sym, &i) in assets_at_ts.iter() {
                let asset = asset_sym.as_alpaca_pair();
                let bars = &asset_bars[&asset_sym];
                let bar = &bars[i];
                let combined_bars = &combined_bars_by_asset[&asset_sym];
                // Warmup prefix only precedes the first active asset's bars
                // (v1 warmup is single-asset); others have no warmup offset.
                let warmup_count = if asset_sym == *active.first().unwrap() {
                    warmup_count
                } else {
                    0
                };

                // A decision at bar T normally fills at T+1's open. For the
                // final bar of an asset's window there is no T+1, so the
                // fill source falls back to the same bar's close
                // (qa-decisions-30day-count).
                let next_bar_open = bars.get(i + 1).map(|b| b.open).unwrap_or(bar.close);

                let filter_gated_position_sltp_check = filter_gated
                    && book.position(asset_sym).abs() > f64::EPSILON
                    && sltp_state.contains_key(&asset_sym);
                if filter_gated && !filter_gated_position_sltp_check {
                    // Strategy filter gated this timestamp: skip the agent
                    // pipeline for this asset. No decision row is written
                    // (matches the single-asset filter-gate behavior, which
                    // recorded only the filter evaluation + dense equity).
                    // Equity is recorded once per timestamp below.
                    continue 'asset;
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
                // pre-022 wire shape; `Some(n)` trims to the most-recent `n`
                // entries. Shared with the live loop via `bar_history_limit_offset`.
                let history_slice: &[&Ohlcv] =
                    &history_slice[bar_history_limit_offset(history_slice.len(), bar_history_limit)..];
                let source_window_start = history_slice
                    .first()
                    .map(|b| b.timestamp)
                    .unwrap_or(bar.timestamp);
                let source_window_end = bar.timestamp;

                let last_closed_times = self
                    .market_data
                    .as_ref()
                    .map(|ctx| {
                        supported_timeframes
                            .iter()
                            .filter_map(|tf| {
                                let granularity = match tf.as_str() {
                                    "1m" => Some(xvision_data::alpaca::BarGranularity::Minute1),
                                    "5m" => Some(xvision_data::alpaca::BarGranularity::Minute5),
                                    "15m" => Some(xvision_data::alpaca::BarGranularity::Minute15),
                                    "30m" => xvision_data::alpaca::BarGranularity::new(
                                        30,
                                        xvision_data::alpaca::BarGranularityUnit::Minute,
                                    )
                                    .ok(),
                                    "1h" => Some(xvision_data::alpaca::BarGranularity::Hour1),
                                    "2h" => xvision_data::alpaca::BarGranularity::new(
                                        2,
                                        xvision_data::alpaca::BarGranularityUnit::Hour,
                                    )
                                    .ok(),
                                    "4h" => Some(xvision_data::alpaca::BarGranularity::Hour4),
                                    "1d" => Some(xvision_data::alpaca::BarGranularity::Day1),
                                    _ => None,
                                }?;
                                ctx.last_closed_at(asset_sym, granularity, bar.timestamp)
                                    .map(|ts| (tf.clone(), ts))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let (seed_sl_price, seed_tp_price, seed_bars_held) =
                    if let Some(sltp) = sltp_state.get(&asset_sym) {
                        (
                            sltp.get_effective_sl_price(),
                            sltp.get_effective_tp_price(),
                            sltp.bars_held,
                        )
                    } else {
                        (0.0, 0.0, 0)
                    };
                // Shared seed-context prologue: position/entry/mark are derived
                // inside `build_seed_context` (single source of truth), so this
                // path can't drift from the live one.
                let seed = build_decision_seed(DecisionSeedInput::from_context(build_seed_context(
                    &book,
                    asset_sym,
                    bar,
                    SeedContextParams {
                        decision_idx,
                        asset: &asset,
                        active_assets: &active_venue_symbols,
                        history_slice,
                        inputs_policy,
                        supported_timeframes: &supported_timeframes,
                        last_closed_times,
                        equity,
                        next_bar_open,
                        reference_price_source: "eval_bar.close",
                        bars_held: seed_bars_held,
                        stop_loss_price: seed_sl_price,
                        take_profit_price: seed_tp_price,
                        risk_config: &strategy.risk,
                        // Backtest is spot-only: no perps context (deferred).
                        perps: PerpsContext::default(),
                    },
                )));
                // When the DSL filter fired this bar, inject its trigger context
                // (indicator snapshot + fire.reason/priority/tags) into the seed
                // so the trader's briefing includes the values that caused the
                // filter to trip.
                let mut seed = seed;
                if let Some(ctx) = &filter_trigger_context {
                    if let Some(obj) = seed.as_object_mut() {
                        obj.insert("filter_context".to_string(), ctx.clone());
                    }
                }
                // WU2 Pine import: inject briefing_indicators latest values.
                // When a Pine-imported Agentic strategy has briefing_indicators,
                // compute each indicator over the history slice and inject the
                // latest value into the seed under "briefing_indicators" so the
                // trader LLM sees them without computing from raw bars.
                // Mirrors the filter_context post-build insertion pattern.
                if !strategy.briefing_indicators.is_empty() {
                    inject_briefing_indicators_into_seed(
                        &mut seed,
                        &strategy.briefing_indicators,
                        bar,
                        history_slice,
                    );
                }
                // U6: inject top-5 episodic recall observations when the store
                // has relevant history. Query vector is derived from the
                // filter_trigger_context indicators; falls back to zero-vector
                // when no filter context is present (acknowledged risk R9).
                // Indicator values may be at the top level or nested under
                // "context" depending on the filter implementation.
                {
                    let get_ind = |ctx: &serde_json::Value, key: &str| -> Option<f64> {
                        ctx.get(key)
                            .or_else(|| ctx.get("context").and_then(|c| c.get(key)))
                            .and_then(|v| v.as_f64())
                    };
                    let query_vec = filter_trigger_context
                        .as_ref()
                        .map(|ctx| {
                            let snap = crate::agent::episodic::IndicatorSnapshot {
                                rsi: get_ind(ctx, "rsi_14"),
                                macd_hist: get_ind(ctx, "macd_hist"),
                                ema_cross: get_ind(ctx, "ema_cross"),
                                volume_zscore: get_ind(ctx, "volume_zscore"),
                            };
                            snap.feature_vector()
                        })
                        .unwrap_or([0.0_f64; 4]);
                    if let Some(episodes_json) = episodic_store.to_seed_json(query_vec, 5) {
                        // P1: For causal runs, strip temporal identifiers so
                        // prior_episodes cannot leak bar timestamps or decision
                        // indices that causal sanitization was designed to hide.
                        let sanitized = if inputs_policy == InputsPolicy::Causal {
                            if let serde_json::Value::Array(arr) = episodes_json {
                                let cleaned: Vec<serde_json::Value> = arr
                                    .into_iter()
                                    .map(|mut ep| {
                                        if let serde_json::Value::Object(ref mut m) = ep {
                                            m.remove("bar_timestamp");
                                            m.remove("decision_idx");
                                        }
                                        ep
                                    })
                                    .collect();
                                serde_json::Value::Array(cleaned)
                            } else {
                                serde_json::Value::Array(vec![])
                            }
                        } else {
                            episodes_json
                        };
                        if let Some(obj) = seed.as_object_mut() {
                            obj.insert("prior_episodes".to_string(), sanitized);
                        }
                    }
                }

                // Advanced SL/TP check — fires before the LLM pipeline.
                // If a trigger fires, we execute the exit, record the
                // decision row, and `continue 'asset` so the LLM is not
                // consulted for this bar. This fires even during early-stop
                // skip windows (capital protection trumps efficiency).
                if book.position(asset_sym).abs() > f64::EPSILON {
                    if let Some(sltp) = sltp_state.get_mut(&asset_sym) {
                        use crate::eval::executor::sltp::SltpTrigger;
                        let sltp_position = book.position(asset_sym);
                        let sltp_entry = book.entry_price(asset_sym);
                        match crate::eval::executor::sltp::check_and_update(sltp, bar) {
                            Some(SltpTrigger::FullExit { reason }) => {
                                // WS-14: capture the effective bracket prices the
                                // exit fired against BEFORE the state is removed
                                // (line below clears `sltp_state`). These are the
                                // values the deterministic exit already computed —
                                // we only read them for the `position_exit` trace
                                // event, never recompute or change exit logic.
                                let exit_effective_sl_price = sltp.get_effective_sl_price();
                                let exit_effective_tp_price = sltp.get_effective_tp_price();
                                let (sltp_pnl, sltp_fee) = apply_sltp_full_exit(
                                    sltp_position,
                                    sltp_entry,
                                    next_bar_open,
                                    default_slip_bps,
                                    default_taker_bps,
                                );
                                // Apply borrow cost for short exits.
                                let borrow_cost = if sltp_position < -f64::EPSILON {
                                    let held = short_bars_held.remove(&asset_sym).unwrap_or(0);
                                    let borrow_bps =
                                        resolve_asset_override(&scenario.venue.overrides, &asset)
                                            .and_then(|o| o.borrow_bps_per_day)
                                            .unwrap_or(scenario.venue.borrow_bps_per_day);
                                    compute_borrow_cost(
                                        sltp_position.abs(),
                                        sltp_entry,
                                        borrow_bps,
                                        held,
                                        bar_secs,
                                    )
                                } else {
                                    0.0
                                };
                                let net_sltp_pnl = sltp_pnl - borrow_cost;
                                book.add_realized(net_sltp_pnl);
                                if net_sltp_pnl > 0.0 {
                                    wins += 1;
                                }
                                realized_count += 1;
                                book.set_position(asset_sym, 0.0, 0.0);
                                sltp_state.remove(&asset_sym);
                                last_open_direction.remove(&asset_sym);
                                n_trades += 1;
                                let fill_price = next_bar_open
                                    * (1.0
                                        - default_slip_bps / 10_000.0
                                            * if sltp_position > 0.0 { 1.0 } else { -1.0 });
                                let sltp_row = crate::eval::store::DecisionRow {
                                    run_id: run.id.clone(),
                                    decision_index: decision_idx,
                                    timestamp: bar.timestamp,
                                    asset: asset.clone(),
                                    action: reason.to_string(),
                                    conviction: Some(1.0),
                                    justification: Some(format!("sltp: {reason}")),
                                    reasoning: None,
                                    order_size: Some(sltp_position.abs()),
                                    fill_price: Some(fill_price),
                                    fill_size: Some(sltp_position.abs()),
                                    fee: Some(sltp_fee),
                                    pnl_realized: Some(sltp_pnl - borrow_cost),
                                    delayed: Some(false),
                                };
                                store.record_decision(&sltp_row).await?;
                                self.emit_chart(
                                    &run.id,
                                    RunChartEvent::Decision(LiveDecisionRow::from(&sltp_row)),
                                )
                                .await;
                                self.emit(ProgressEvent::FillRecorded {
                                    run_id: run.id.clone(),
                                    side: fill_side_for_action(reason, sltp_position).into(),
                                    price: fill_price,
                                    qty: sltp_position.abs(),
                                    fee: sltp_fee,
                                });
                                // U5-SLTP: record deterministic exits so later
                                // decisions can recall stop-out/TP events.
                                {
                                    let sltp_obs = crate::agent::episodic::EpisodicObservation::new(
                                        bar.timestamp.to_rfc3339(),
                                        decision_idx,
                                        reason.to_string(),
                                        1.0,
                                        Some(sltp_entry),
                                        Some(reason.to_string()),
                                        format!("sltp: {reason}"),
                                        crate::agent::episodic::IndicatorSnapshot::default(),
                                    );
                                    episodic_store.push(sltp_obs);
                                }
                                // WS-14: emit a `position_exit` engine event so the
                                // deterministic SL/TP/trailing/time exit — often THE
                                // realized-PnL event — is first-class in the trace,
                                // not just a DB row + chart blip. This exit site runs
                                // BEFORE the LLM-decision span is opened (and bails via
                                // `continue 'asset`), so it gets its own short-lived
                                // `agent.decision` span scoped to the exit, mirroring
                                // how an LLM decision carries its outcome events.
                                // Emit-only: every value below is already computed for
                                // the exit above — no recompute, no logic change.
                                if let Some(obs) = self.obs_emitter.as_ref() {
                                    let exit_span_id = crate::agent::observability::fresh_span_id();
                                    obs.emit_decision_span_started(
                                        &exit_span_id,
                                        None,
                                        decision_idx as i64,
                                        Some(&asset),
                                        Some(bar.timestamp),
                                        Some(bar.close),
                                        Some(sltp_position),
                                        // decision_input (WS-10): a deterministic SL/TP exit
                                        // carries no agent briefing, so there is no
                                        // market-context snapshot to attach here.
                                        None,
                                    )
                                    .await;
                                    let payload = serde_json::json!({
                                        "asset": asset,
                                        "exit_reason": reason,
                                        "effective_sl_price": exit_effective_sl_price,
                                        "effective_tp_price": exit_effective_tp_price,
                                        "realized_pnl": net_sltp_pnl,
                                        "exit_price": fill_price,
                                    });
                                    obs.emit_engine_event(
                                        "position_exit",
                                        Some(exit_span_id.clone()),
                                        Some(payload.to_string()),
                                    )
                                    .await;
                                    obs.emit_span_finished_ok(&exit_span_id).await;
                                }
                                decision_idx += 1;
                                continue 'asset;
                            }
                            Some(SltpTrigger::PartialTp1 { fraction }) => {
                                let close_units = sltp_position.abs() * fraction;
                                let fill_price = next_bar_open;
                                let fee_rate = default_taker_bps / 10_000.0;
                                let fee = close_units * fill_price * fee_rate;
                                let pnl = sltp_position * fraction * (fill_price - sltp_entry) - fee;
                                let remaining = sltp_position * (1.0 - fraction);
                                book.add_realized(pnl);
                                book.set_position(asset_sym, remaining, sltp_entry);
                                sltp.tp1_taken = true;
                                n_trades += 1;
                                let pt1_row = crate::eval::store::DecisionRow {
                                    run_id: run.id.clone(),
                                    decision_index: decision_idx,
                                    timestamp: bar.timestamp,
                                    asset: asset.clone(),
                                    action: "partial_tp1".to_string(),
                                    conviction: Some(1.0),
                                    justification: Some(format!(
                                        "sltp: partial TP1 ({:.0}%)",
                                        fraction * 100.0
                                    )),
                                    reasoning: None,
                                    order_size: Some(close_units),
                                    fill_price: Some(fill_price),
                                    fill_size: Some(close_units),
                                    fee: Some(fee),
                                    pnl_realized: Some(pnl),
                                    delayed: Some(false),
                                };
                                store.record_decision(&pt1_row).await?;
                                self.emit_chart(
                                    &run.id,
                                    RunChartEvent::Decision(LiveDecisionRow::from(&pt1_row)),
                                )
                                .await;
                                // U5-SLTP: record partial TP1 as an episodic event.
                                {
                                    let pt1_obs = crate::agent::episodic::EpisodicObservation::new(
                                        bar.timestamp.to_rfc3339(),
                                        decision_idx,
                                        "partial_tp1",
                                        1.0,
                                        None::<f64>,
                                        None::<String>,
                                        "partial take-profit 1",
                                        crate::agent::episodic::IndicatorSnapshot::default(),
                                    );
                                    episodic_store.push(pt1_obs);
                                }
                                decision_idx += 1;
                                continue 'asset;
                            }
                            None => {}
                        }
                    }
                }

                if filter_gated_position_sltp_check {
                    // PF-17: filter-gated in-position bars still need the
                    // deterministic SL/TP check above, but a non-triggering
                    // bar must continue to skip the agent pipeline.
                    continue 'asset;
                }

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
                // Per-asset early-stop streak state. Each asset has its own
                // skip window so a flat run on one asset can't suppress
                // decisions on another.
                let policy_plan = {
                    let es = early_stop_state
                        .get(&asset_sym)
                        .expect("early_stop_state seeded for every active asset");
                    if es.inherit_remaining == 0 {
                        early_stop::should_skip_next_decision(
                            &es.recent_actions,
                            &es.recent_convictions,
                            book.position(asset_sym) == es.prev_position,
                            &early_stop_cfg,
                        )
                    } else {
                        None
                    }
                };
                if let Some(plan) = policy_plan.as_ref() {
                    tracing::info!(
                        run_id = %run.id,
                        decision_index = decision_idx,
                        asset = %asset,
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
                            "asset": asset,
                            "skip_count": plan.skip_count,
                            "reason": plan.reason,
                        });
                        obs.emit_engine_event("early_stop_triggered", None, Some(payload.to_string()))
                            .await;
                    }
                    let es = early_stop_state.get_mut(&asset_sym).unwrap();
                    es.inherit_remaining = plan.skip_count;
                    // Flush the rolling buffer so the policy can't re-fire
                    // on the next bar without a fresh streak rebuilding.
                    es.recent_actions.clear();
                    es.recent_convictions.clear();
                }
                if early_stop_state.get(&asset_sym).unwrap().inherit_remaining > 0 {
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
                        delayed: Some(false),
                    };
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

                    // No position change — `flat` on an already-flat or held
                    // position is a no-op fill. The inherit branch must NOT
                    // mutate position state, so the book is left untouched and
                    // the pooled equity is marked + recorded ONCE at the end of
                    // this timestamp (after all assets), not here.
                    let es = early_stop_state.get_mut(&asset_sym).unwrap();
                    es.inherit_remaining -= 1;
                    es.prev_position = book.position(asset_sym);
                    decision_idx += 1;
                    continue 'asset;
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
                    // WS-10 (`trace-obs-decision-input`): capture the
                    // structured market context the trader saw this bar
                    // (indicator panel, current-bar OHLCV, regime,
                    // briefing mode, bounded bar_history summary) from
                    // the seed (briefing) already assembled above, and
                    // attach it to the `agent.decision` span attributes
                    // so it's queryable and lands in the export. The
                    // backtest dispatch path sends the FULL briefing
                    // (delta-briefing is a per-slot dispatch opt-in not
                    // threaded through this executor), so `prev`/opt-in
                    // are `None`/`false` here — the recorded mode is
                    // truthfully "full".
                    let decision_input = build_decision_input(&seed, None, false);
                    obs.emit_decision_span_started(
                        &decision_span_id,
                        None,
                        decision_idx as i64,
                        Some(&asset),
                        Some(bar.timestamp),
                        Some(bar.close),
                        Some(book.position(asset_sym)),
                        Some(decision_input),
                    )
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

                    // WS-15 (`trace-obs` market-context): emit a
                    // `regime_transition` engine event when the regime
                    // label for THIS asset changed from the prior
                    // decision. The label is COMPUTED here per decision
                    // (deterministic + cheap, no LLM) by running the shared
                    // `derive_regime_labels` heuristic over the executor's
                    // OWN trailing bar-history window for this asset — the
                    // last `history_window` bars plus the current bar
                    // (`combined_bars[history_start..=combined_idx]`). The
                    // empty `seed.regime` field the backtest emits is never
                    // a usable source here, so reading it would make this
                    // event dead in backtests. Purely observational: the
                    // computed label feeds ONLY this trace event — it is
                    // NEVER injected into the trader seed / briefing /
                    // prompt, so regime/briefing/trading logic is untouched.
                    // No event on the first observation (no prior) or when
                    // the regime is stable.
                    let regime_window = &combined_bars[history_start..=combined_idx];
                    let curr_regime = compute_regime_label(regime_window);
                    let prev_regime = last_regime_label.get(&asset_sym).cloned();
                    if let Some((from, to)) = regime_changed(prev_regime.as_deref(), curr_regime.as_deref()) {
                        let payload = serde_json::json!({
                            "asset": asset,
                            "from": from,
                            "to": to,
                            "decision_index": decision_idx,
                        });
                        obs.emit_engine_event(
                            "regime_transition",
                            Some(decision_span_id.clone()),
                            Some(payload.to_string()),
                        )
                        .await;
                    }
                    if let Some(c) = curr_regime {
                        last_regime_label.insert(asset_sym, c);
                    }
                }
                macro_rules! finish_decision_span_error {
                    ($message:expr) => {
                        if let Some(obs) = self.obs_emitter.as_ref() {
                            obs.emit_span_finished_error(&decision_span_id, $message)
                                .await;
                        }
                    };
                }

                // GUARDRAIL(invalid_output_schema): emit the Phase 4.2 typed
                // short-circuit onto the obs/event stream when the trader's
                // schema-recovery is EXHAUSTED (all repair attempts failed, or
                // the failure class is non-recoverable). Without this the
                // failed prerequisite was only a free-text decision-span error
                // string; the typed `invalid_output_schema` event makes the
                // recorded failure machine-readable rather than degrading to a
                // silent/opaque error. Pair with `finish_decision_span_error!`
                // at every recovery-exhausted branch (it does NOT close the
                // span — the caller still does).
                macro_rules! emit_schema_short_circuit {
                    ($detail:expr) => {
                        if let Some(obs) = self.obs_emitter.as_ref() {
                            let role = trader_role_label(agent_slots);
                            let sc = crate::guardrails::ShortCircuit::InvalidOutputSchema {
                                role: role.clone(),
                                expected: "TraderOutput".to_string(),
                                detail: $detail,
                            };
                            let payload = sc.to_typed_error();
                            let payload_json = serde_json::to_string(&payload).ok();
                            obs.emit_engine_event(sc.code(), Some(decision_span_id.clone()), payload_json)
                                .await;
                        }
                    };
                }

                // Publish the current asset + bar timestamp into the shared
                // dispatch handles so the tool chokepoint can (a) reject
                // cross-asset market-data fetches (asset guard) and (b) anchor
                // Nansen backtest calls to the simulated clock (`as_of`).
                // No-op when `cline` is absent (non-sidecar run).
                if let Some(cline) = self.cline.as_ref() {
                    publish_decision_context(
                        &cline.tool_asset_guard,
                        &cline.as_of_guard,
                        &asset,
                        bar.timestamp,
                    )
                    .await;
                }

                // F-5 phase 2a: keep a copy of the seed so the
                // malformed-json repair path (below) can rebuild the
                // original user prompt byte-for-byte. The pipeline consumes
                // `seed_inputs` by value; cloning here is cheap relative to
                // the LLM dispatch and keeps the repair turn deterministic
                // for the A/B-cache pairing acceptance criterion.
                let parsed: TraderOutput = if strategy.decision_mode == DecisionMode::Mechanistic {
                    if store.is_terminal(&run.id).await? {
                        // F36: capture partial metrics+tokens before bailing.
                        persist_partial_snapshot(
                            store,
                            &run.id,
                            &equity_curve,
                            initial,
                            equity,
                            strategy.manifest.decision_cadence_minutes,
                            realized_count,
                            wins,
                            n_trades,
                            decision_idx,
                            total_input_tokens,
                            total_output_tokens,
                        )
                        .await;
                        finish_decision_span_error!("eval run stopped");
                        anyhow::bail!("eval run stopped");
                    }
                    let cfg = strategy
                        .mechanistic_config
                        .as_ref()
                        .expect("validate_strategy ensures mechanistic_config with has_rules");
                    mechanistic_action(
                        cfg,
                        book.position(asset_sym),
                        book.entry_price(asset_sym),
                        bar.close,
                    )
                } else {
                    let seed_for_repair = seed.clone();
                    // WS-17: open a `decision.model` span as a CHILD of this
                    // bar's `agent.decision` span BEFORE running the pipeline,
                    // and thread its id down so the Cline trader's captured
                    // chain-of-thought (`decision.reasoning`) nests under it.
                    // The span is the single per-cycle record of the model
                    // invocation that produced the trade decision (the
                    // trader/regime/filter slot roles were retired — it's one
                    // decision-model call now). Closed after the pipeline
                    // returns, carrying the input/output token counts from
                    // `PipelineOutputs`. `None` emitter ⇒ no-op + `None` id.
                    let decision_model_span_id = self
                        .obs_emitter
                        .as_ref()
                        .map(|_| crate::agent::observability::fresh_span_id());
                    if let (Some(obs), Some(dm_span_id)) =
                        (self.obs_emitter.as_ref(), decision_model_span_id.as_ref())
                    {
                        let provider = trader_provider(agent_slots, strategy).unwrap_or_default();
                        let model = trader_model_id(agent_slots, strategy).unwrap_or_default();
                        obs.emit_model_call_started(
                            dm_span_id,
                            Some(decision_span_id.clone()),
                            &provider,
                            &model,
                            Some("trader"),
                            None,
                            None,
                        )
                        .await;
                    }
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
                        source_window_start: Some(source_window_start),
                        source_window_end: Some(source_window_end),
                        run_id: run.id.clone(),
                        scenario_id: scenario.id.clone(),
                        cycle_idx: decision_idx as i64,
                        trace_attrs: None,
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
                            // Multi-asset (B4): scope each asset's filter signals
                            // to `Asset(asset)` so two assets at the same bar keep
                            // independent signal-cache entries. Single-asset runs
                            // simply key everything under the one asset.
                            scope: crate::agent::dispatch_capability::SignalScope::Asset(asset_sym),
                        }),
                        // Phase D — unified Recorder. Wired by callers that
                        // construct an `EvalRecorder` and thread it via
                        // `BacktestExecutor::with_recorder`. The default `None`
                        // keeps the legacy bus-driven emission path untouched.
                        recorder: self.recorder.as_deref(),
                        // Stage 1 — Cline runtime selection. `LlmDispatch` by
                        // default; `Cline` + the spawned sidecar ctx when the
                        // eval entry point selected it.
                        runtime: self.agent_runtime,
                        cline: self.cline.clone(),
                        // WS-17: parent for the captured chain-of-thought.
                        model_call_span_id: decision_model_span_id.clone(),
                    })
                    .await;
                    // WS-17: close the `decision.model` span on BOTH arms
                    // before propagating — an Err would otherwise leave the
                    // span dangling open on the trace dock.
                    if let (Some(obs), Some(dm_span_id)) =
                        (self.obs_emitter.as_ref(), decision_model_span_id.as_ref())
                    {
                        match &outs {
                            Ok(_) => obs.emit_span_finished_ok(dm_span_id).await,
                            Err(e) => obs.emit_span_finished_error(dm_span_id, &e.to_string()).await,
                        }
                    }
                    let outs = outs?;
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
                        // F36: capture partial metrics+tokens before bailing.
                        persist_partial_snapshot(
                            store,
                            &run.id,
                            &equity_curve,
                            initial,
                            equity,
                            strategy.manifest.decision_cadence_minutes,
                            realized_count,
                            wins,
                            n_trades,
                            decision_idx,
                            total_input_tokens,
                            total_output_tokens,
                        )
                        .await;
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
                    match TraderOutput::parse_response(trader, &run.id, decision_idx) {
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
                                            // schema-missing-field recovery exhausted.
                                            let err = original.with_model_hint(trader_model_id.as_deref());
                                            emit_schema_short_circuit!(err.to_string());
                                            finish_decision_span_error!(&err.to_string());
                                            return Err(err.into());
                                        }
                                    }
                                } else {
                                    // No repair context → recovery cannot run.
                                    let err = e.with_model_hint(trader_model_id.as_deref());
                                    emit_schema_short_circuit!(err.to_string());
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
                                            // malformed-json repair exhausted.
                                            let err = original.with_model_hint(trader_model_id.as_deref());
                                            emit_schema_short_circuit!(err.to_string());
                                            finish_decision_span_error!(&err.to_string());
                                            return Err(err.into());
                                        }
                                    }
                                } else {
                                    // No repair context → recovery cannot run.
                                    let err = e.with_model_hint(trader_model_id.as_deref());
                                    emit_schema_short_circuit!(err.to_string());
                                    finish_decision_span_error!(&err.to_string());
                                    return Err(err.into());
                                }
                            } else {
                                // Non-recoverable failure class: recovery never applies.
                                let err = e.with_model_hint(trader_model_id.as_deref());
                                emit_schema_short_circuit!(err.to_string());
                                finish_decision_span_error!(&err.to_string());
                                return Err(err.into());
                            }
                        }
                    }
                }; // closes if decision_mode == Mechanistic { ... } else { match ... }

                if store.is_terminal(&run.id).await? {
                    // F36: capture partial metrics+tokens before bailing.
                    persist_partial_snapshot(
                        store,
                        &run.id,
                        &equity_curve,
                        initial,
                        equity,
                        strategy.manifest.decision_cadence_minutes,
                        realized_count,
                        wins,
                        n_trades,
                        decision_idx,
                        total_input_tokens,
                        total_output_tokens,
                    )
                    .await;
                    finish_decision_span_error!("eval run stopped");
                    anyhow::bail!("eval run stopped");
                }

                let pre_fill_position = book.position(asset_sym);
                let pre_fill_entry = book.entry_price(asset_sym);

                // Borrow accrual: count each bar a short is open.
                if pre_fill_position < -f64::EPSILON {
                    *short_bars_held.entry(asset_sym).or_insert(0) += 1;
                }

                // engine-trade-guardrails-pyramid-flip-block (F-7):
                // Server-side gate at the apply seam. The trader's emitted
                // action stays in `parsed.action` (preserved verbatim in
                // `eval_decisions.action` below); `applied_action` is what
                // drives `simulate_fill` / marker derivation. A `RewriteTo`
                // also writes a `supervisor_notes` row so the operator sees
                // the block.
                // WS-13 (`trace-obs-risk-gate`): open a `risk.gate` span
                // around the engine's REAL risk window — the guardrail
                // rewrite below + the deterministic risk-config vetoes
                // that follow. The span is a child of this decision's
                // `agent.decision` span and is closed (with a verdict
                // derived read-only from what actually happened) once the
                // final `applied_action` is known. This is emit-only: the
                // guardrail/risk-config/veto LOGIC and the trade outcome
                // are unchanged. Snapshot the trader's pre-risk action so
                // the verdict can tell "approved" (unchanged) apart from
                // "modified"/"vetoed" (rewritten) without re-deriving it.
                let risk_gate_span_id = crate::agent::observability::fresh_span_id();
                let trader_action_pre_risk = parsed.action.clone();
                if let Some(obs) = self.obs_emitter.as_ref() {
                    obs.emit_risk_gate_started(&risk_gate_span_id, Some(decision_span_id.clone()))
                        .await;
                }
                // Reason carried out of the risk-config veto block so the
                // span finish can report it (`daily_loss_kill` /
                // `max_concurrent_positions`). `None` when nothing vetoed.
                let mut risk_veto_reason: Option<&'static str> = None;

                let original_action = GuardAction::parse(&parsed.action);
                let position_state = position_state_from_size(pre_fill_position);
                let decision = guardrails::classify(
                    original_action,
                    position_state,
                    last_open_direction.get(&asset_sym).copied(),
                );
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

                // R3: deterministic risk-config vetoes on NEW opens. These run
                // after the guardrail rewrite and before the broker/fill seam,
                // and only constrain opening orders — `hold`/`flat` and exits
                // are never vetoed (capital protection must always be able to
                // close). A vetoed open is rewritten to `hold` (position
                // survives untouched; for a flat book that is a no-op) and a
                // supervisor note records the reason.
                //
                //   * daily_loss_kill_pct  — once cumulative realized loss for
                //     the current UTC day exceeds this fraction of starting
                //     capital, no further opens are admitted for the rest of
                //     that day. (0.0 disables.)
                //   * max_concurrent_positions — caps the number of distinct
                //     assets holding an open position; a new open that would
                //     exceed the cap is vetoed. Re-opening / adjusting an asset
                //     that is already in-position is not blocked.
                let applied_action: String = {
                    let is_new_open = applied_action == "long_open" || applied_action == "short_open";
                    if !is_new_open {
                        applied_action
                    } else {
                        // Daily-loss kill: roll the realized-loss accumulator on
                        // a UTC-day boundary, then compare today's realized loss
                        // against the configured fraction of starting capital.
                        let bar_day = bar.timestamp.date_naive();
                        if daily_loss_day != Some(bar_day) {
                            daily_loss_day = Some(bar_day);
                            daily_realized_at_day_start = book.realized();
                        }
                        let kill_pct = strategy.risk.daily_loss_kill_pct;
                        let realized_today = book.realized() - daily_realized_at_day_start;
                        let daily_loss_breached = kill_pct > 0.0 && realized_today <= -(kill_pct * initial);

                        // Max concurrent positions: count distinct assets that
                        // currently hold a non-flat position. The asset being
                        // opened only consumes a NEW slot if it is currently
                        // flat.
                        let max_positions = strategy.risk.max_concurrent_positions;
                        let open_positions = book.open_position_count();
                        let already_open = book.position(asset_sym).abs() > f64::EPSILON;
                        let max_positions_breached =
                            max_positions > 0 && !already_open && open_positions >= max_positions as usize;

                        // Perps entry veto (venue-gated). Backtest is spot-only
                        // so `is_perp_venue=false` keeps this permanently inert
                        // here. Funding data is not plumbed in the backtest path
                        // (PerpsContext::default() → all None). Liq-distance is
                        // not yet plumbed into the engine book (follow-on track);
                        // pass None → that check no-ops.
                        let is_perp_venue = false;
                        let perps_funding_rate: Option<f64> = None;
                        let direction = if applied_action == "short_open" {
                            xvision_core::trading::Direction::Short
                        } else {
                            xvision_core::trading::Direction::Long
                        };
                        let perps_veto = crate::strategies::risk::perps::perps_entry_veto(
                            &strategy.risk,
                            is_perp_venue,
                            true, // is_new_open: this branch only runs for new opens
                            direction,
                            perps_funding_rate,
                            None,
                        );

                        let exposure_breached = {
                            let cap = strategy.risk.max_total_exposure_pct;
                            if cap > 0.0 {
                                let existing: f64 = book
                                    .open_legs()
                                    .iter()
                                    .map(|(_, pos, _entry, mark)| pos.abs() * mark)
                                    .sum();
                                let new_notional = {
                                    let usd_at_risk = equity * strategy.risk.risk_pct_per_trade;
                                    usd_at_risk.max(0.0)
                                };
                                crate::strategies::risk::perps::exceeds_total_exposure(
                                    cap,
                                    equity,
                                    existing,
                                    new_notional,
                                )
                            } else {
                                false
                            }
                        };

                        let breach_reason: Option<&str> = if daily_loss_breached {
                            Some("daily_loss_kill")
                        } else if max_positions_breached {
                            Some("max_concurrent_positions")
                        } else if exposure_breached {
                            Some("max_total_exposure")
                        } else {
                            match perps_veto {
                                Some(xvision_core::trading::VetoReason::PunitiveFunding) => {
                                    Some("punitive_funding")
                                }
                                Some(xvision_core::trading::VetoReason::NearLiquidation) => {
                                    Some("near_liquidation")
                                }
                                _ => None,
                            }
                        };

                        if let Some(reason) = breach_reason {
                            // WS-13: surface the veto reason to the
                            // enclosing `risk.gate` span finish (read-only
                            // — does not change the veto behavior below).
                            risk_veto_reason = Some(reason);
                            let note = format!(
                                "risk veto `{reason}` at decision {decision_idx} ({asset}): \
                                 open {applied_action} rewritten to hold \
                                 (realized_today={realized_today:.2}, open_positions={open_positions})"
                            );
                            store
                                .record_supervisor_note(&run.id, "risk", "warn", &note)
                                .await?;
                            if let Some(obs) = self.obs_emitter.as_ref() {
                                let payload = serde_json::json!({
                                    "decision_index": decision_idx,
                                    "asset": asset,
                                    "reason": reason,
                                    "original": applied_action.as_str(),
                                    "applied": "hold",
                                });
                                obs.emit_engine_event(
                                    "risk_veto",
                                    Some(decision_span_id.clone()),
                                    Some(payload.to_string()),
                                )
                                .await;
                            }
                            "hold".to_string()
                        } else {
                            applied_action
                        }
                    }
                };

                // WS-13 (`trace-obs-risk-gate`): close the `risk.gate`
                // span with a verdict derived READ-ONLY from what the
                // guardrail + risk-config checks above already did. The
                // mapping (checked in this order — a veto also rewrites
                // the action, so the veto case must win over "modified"):
                //   * `vetoed`   — the risk-config block rewrote a new
                //                  open to `hold` (reason carried).
                //   * `modified` — the guardrail rewrote the trader's
                //                  action (action changed, no veto).
                //   * `approved` — risk ran, the action is unchanged.
                // Emitted for ALL THREE verdicts so `risk.gate` shows
                // every decision, not only the bars where risk fired.
                if let Some(obs) = self.obs_emitter.as_ref() {
                    let (verdict, veto_reason): (&str, Option<&str>) = if let Some(reason) = risk_veto_reason
                    {
                        ("vetoed", Some(reason))
                    } else if applied_action != trader_action_pre_risk {
                        ("modified", None)
                    } else {
                        ("approved", None)
                    };
                    obs.emit_risk_gate_finished(&risk_gate_span_id, verdict, veto_reason, None)
                        .await;
                }

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
                            if is_blocking && self.canary_sabotage.is_some() {
                                // F9: this is an autooptimizer honesty-check
                                // (canary) running a deliberately-sabotaged
                                // strategy. Its broker-rule rejections are
                                // expected (e.g. `kill-trades` zero-sizes every
                                // order → min-order-notional). Demote to debug
                                // and label so the operator does not mistake
                                // them for a real broker fault.
                                tracing::debug!(
                                    run_id = %run.id,
                                    decision_index = decision_idx,
                                    asset = %asset,
                                    specific_rule = %violation.specific_rule,
                                    action = %applied_action,
                                    sabotage_variant = %self
                                        .canary_sabotage
                                        .as_deref()
                                        .unwrap_or("unknown"),
                                    "honesty-check canary: broker rule rejected order — expected (honesty-check sabotage)",
                                );
                            } else if is_blocking {
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
                //
                // A1 per-run pause: when the run is paused (an ADDITIVE per-run
                // gate alongside the global SafetyManager pause), skip the fill
                // submit for this cycle and emit a no-op fill — the run keeps
                // iterating (decisions still record), it just doesn't trade.
                // Re-read per cycle so a pause issued mid-run via
                // `POST /api/eval/runs/:id/pause` takes effect on the next cycle.
                //
                // FAIL OPEN on the BACKTEST/simulated path (`simulate_fill`): no
                // real money rides on this fill, so a transient read error
                // degrades to "not paused" (`unwrap_or(false)`) rather than
                // silently freezing a backtest. The LIVE path
                // (`decide_one_live` → `RealBrokerFills`) fails CLOSED instead.
                let run_paused = store.is_paused(&run.id).await.unwrap_or(false);
                let fill: FillRecord = if applied_action == "hold" || broker_rejected || run_paused {
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
                        broker_error: None,
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
                    let (fill_bar_open, fill_bar_high, fill_bar_low, fill_bar_close) = fill_bar
                        .map(|b| (b.open, b.high, b.low, b.close))
                        .unwrap_or((bar.close, bar.close, bar.close, bar.close));

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
                            bar_close: fill_bar_close,
                            decision_to_fill_ms: scenario.venue.latency.decision_to_fill_ms,
                            bar_duration_ms: bar_secs * 1_000,
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

                if fill.fill_price.is_some() {
                    if fill.new_pos.abs() > f64::EPSILON
                        && (applied_action == "long_open" || applied_action == "short_open")
                    {
                        let direction = if fill.new_pos > 0.0 {
                            xvision_core::trading::Direction::Long
                        } else {
                            xvision_core::trading::Direction::Short
                        };
                        // ERROR-1: under wake:never the model's protective
                        // brackets are ignored (see `wake_never` above) — only
                        // the config ATR stop manages the exit. Neutralizing the
                        // model's `*_pct` / `*_atr_mult` / trailing / breakeven /
                        // fade / TP1-2 / time-exit inputs here leaves the R1
                        // config-ATR fallback as the sole stop source.
                        let sl_pct = if wake_never {
                            0.0
                        } else {
                            parsed.stop_loss_pct.map(|v| v as f64).unwrap_or(0.0)
                        };
                        let tp_pct = if wake_never {
                            0.0
                        } else {
                            parsed.take_profit_pct.map(|v| v as f64).unwrap_or(0.0)
                        };
                        let model_sl_atr_mult = if wake_never { None } else { parsed.sl_atr_mult };
                        let model_tp_atr_mult = if wake_never { None } else { parsed.tp_atr_mult };
                        // R1: enforce the strategy's configured protective stop
                        // even when the model emits no bracket of its own. If
                        // the trader supplied neither a percent SL nor an ATR
                        // SL multiple, fall back to `risk.stop_loss_atr_multiple`
                        // so a held position cannot ride an unbounded adverse
                        // move (the -14.5% no-stop-out repro). The model's own
                        // `sl_atr_mult` wins when present; otherwise the
                        // strategy config supplies a deterministic ATR stop.
                        let config_atr_mult = strategy.risk.stop_loss_atr_multiple;
                        let effective_sl_atr_mult = model_sl_atr_mult.or_else(|| {
                            if sl_pct <= 0.0 && config_atr_mult > 0.0 {
                                Some(config_atr_mult)
                            } else {
                                None
                            }
                        });
                        // Compute ATR whenever any ATR-based level (model or
                        // config fallback) needs it. When warmup leaves ATR
                        // unavailable, `atr_sl_price`/`atr_tp_price` no-op, so
                        // no spurious stop fires.
                        let entry_atr = if effective_sl_atr_mult.is_some() || model_tp_atr_mult.is_some() {
                            crate::eval::executor::sltp::compute_atr14(history_slice)
                        } else {
                            None
                        };
                        // Model-supplied management brackets, suppressed wholesale
                        // under wake:never so the config ATR stop is authoritative.
                        let (
                            trailing_stop_pct,
                            breakeven_trigger_pct,
                            breakeven_offset_pct,
                            fade_sl_bars,
                            fade_sl_start_pct,
                            fade_sl_end_pct,
                            max_bars_held,
                            tp1_pct,
                            tp1_close_fraction,
                            tp2_pct,
                        ) = if wake_never {
                            (None, None, None, None, None, None, None, None, None, None)
                        } else {
                            (
                                parsed.trailing_stop_pct,
                                parsed.breakeven_trigger_pct,
                                parsed.breakeven_offset_pct,
                                parsed.fade_sl_bars,
                                parsed.fade_sl_start_pct,
                                parsed.fade_sl_end_pct,
                                parsed.max_bars_held,
                                parsed.tp1_pct,
                                parsed.tp1_close_fraction,
                                parsed.tp2_pct,
                            )
                        };
                        sltp_state.insert(
                            asset_sym,
                            crate::eval::executor::sltp::PositionRiskState::new(
                                direction,
                                fill.new_entry,
                                sl_pct,
                                tp_pct,
                                entry_atr,
                                trailing_stop_pct,
                                breakeven_trigger_pct,
                                breakeven_offset_pct,
                                fade_sl_bars,
                                fade_sl_start_pct,
                                fade_sl_end_pct,
                                max_bars_held,
                                effective_sl_atr_mult,
                                model_tp_atr_mult,
                                tp1_pct,
                                tp1_close_fraction,
                                tp2_pct,
                            ),
                        );
                    } else if fill.new_pos.abs() <= f64::EPSILON {
                        sltp_state.remove(&asset_sym);
                    }
                }

                book.add_realized(fill.realized_pnl);
                if pre_fill_position != 0.0 && fill.new_pos.abs() <= f64::EPSILON {
                    // Closing a position — count for win_rate
                    realized_count += 1;
                    if fill.realized_pnl > 0.0 {
                        wins += 1;
                    }
                }

                // Borrow cost: when a short is closed, subtract accumulated
                // cost from realized PnL. Long positions accrue nothing.
                if pre_fill_position < -f64::EPSILON && fill.fill_price.is_some() {
                    let held = short_bars_held.remove(&asset_sym).unwrap_or(0);
                    let borrow_bps = resolve_asset_override(&scenario.venue.overrides, &asset)
                        .and_then(|o| o.borrow_bps_per_day)
                        .unwrap_or(scenario.venue.borrow_bps_per_day);
                    let cost = compute_borrow_cost(
                        pre_fill_position.abs(),
                        pre_fill_entry,
                        borrow_bps,
                        held,
                        bar_secs,
                    );
                    if cost > 0.0 {
                        book.add_realized(-cost);
                    }
                }

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

                    // WS-14: emit a typed `broker.call` span around the simulated
                    // fill so backtest fills are auditable on the trace dock the
                    // same way live fills are (live emits these via
                    // `RealBrokerFills`; only the simulated path was missing them).
                    // Stamped with the `backtest` venue so a reader can tell a
                    // simulated fill from a real one at a glance. Emit-only: the
                    // fill geometry (side/qty/price/fee) is read from the
                    // already-produced `FillRecord` — no change to fill logic.
                    if let Some(obs) = self.obs_emitter.as_ref() {
                        let broker_side = if side == "buy" {
                            xvision_observability::BrokerSide::Buy
                        } else {
                            xvision_observability::BrokerSide::Sell
                        };
                        let qty = fill.fill_size.unwrap_or(0.0);
                        let broker_span_id = crate::agent::observability::fresh_span_id();
                        obs.emit_broker_call_started(
                            &broker_span_id,
                            Some(decision_span_id.clone()),
                            broker_side,
                            asset.clone(),
                            qty,
                            Some(next_bar_open),
                            "market",
                            "backtest",
                            Some(format!("backtest-{}-{}", asset, bar.timestamp.timestamp())),
                        )
                        .await;
                        obs.emit_broker_call_finished(
                            &broker_span_id,
                            xvision_observability::BrokerCallOutcome::Filled,
                            fill.fill_price,
                            fill.fill_size,
                            fill.fee,
                            None,
                            None,
                            None,
                            None,
                        )
                        .await;
                    }
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
                    GuardAction::LongOpen => {
                        last_open_direction.insert(asset_sym, GuardAction::LongOpen);
                    }
                    GuardAction::ShortOpen => {
                        last_open_direction.insert(asset_sym, GuardAction::ShortOpen);
                    }
                    GuardAction::Flat => {
                        last_open_direction.remove(&asset_sym);
                    }
                    GuardAction::Hold | GuardAction::Other => {}
                }

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
                    delayed: Some(false),
                };
                store.record_decision(&decision_row).await?;
                self.emit_chart(
                    &run.id,
                    RunChartEvent::Decision(LiveDecisionRow::from(&decision_row)),
                )
                .await;

                // U5: write structured episodic observation for state-changing
                // decisions. Flat and hold produce no signal worth recalling.
                if parsed.action != "flat" && parsed.action != "hold" {
                    let indicators = filter_trigger_context
                        .as_ref()
                        .map(|ctx| {
                            let get_ind = |key: &str| -> Option<f64> {
                                ctx.get(key)
                                    .or_else(|| ctx.get("context").and_then(|c| c.get(key)))
                                    .and_then(|v| v.as_f64())
                            };
                            crate::agent::episodic::IndicatorSnapshot {
                                rsi: get_ind("rsi_14"),
                                macd_hist: get_ind("macd_hist"),
                                ema_cross: get_ind("ema_cross"),
                                volume_zscore: get_ind("volume_zscore"),
                            }
                        })
                        .unwrap_or_default();
                    let ep_entry_price = if book.position(asset_sym).abs() > f64::EPSILON {
                        Some(book.entry_price(asset_sym))
                    } else {
                        fill.fill_price
                    };
                    let exit_reason: Option<String> = if parsed.action == "flat"
                        || parsed.action == "short_close"
                        || parsed.action == "long_close"
                    {
                        Some(parsed.action.clone())
                    } else {
                        None
                    };
                    let obs = crate::agent::episodic::EpisodicObservation::new(
                        bar.timestamp.to_rfc3339(),
                        decision_idx,
                        &parsed.action,
                        parsed.conviction,
                        ep_entry_price,
                        exit_reason,
                        &parsed.justification,
                        indicators,
                    );
                    episodic_store.push(obs);
                }

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

                // Equity is the pooled NAV across all assets and is recorded
                // ONCE per timestamp (after this inner loop), not per asset —
                // `eval_equity_samples` is keyed `(run_id, timestamp)`.

                // eval-flat-degeneracy-early-stop (F-9): roll this asset's
                // buffer and apply reset triggers. A portfolio change (position
                // size delta — open, close, or resize) wipes the streak; so
                // does any non-flat/non-hold action. Otherwise we append and
                // truncate to the configured window. Per-asset state.
                let portfolio_changed =
                    book.position(asset_sym) != early_stop_state.get(&asset_sym).unwrap().prev_position;
                let cls = early_stop::Action::classify(&parsed.action);
                {
                    let es = early_stop_state.get_mut(&asset_sym).unwrap();
                    if portfolio_changed
                        || !matches!(cls, early_stop::Action::Flat | early_stop::Action::Hold)
                    {
                        es.recent_actions.clear();
                        es.recent_convictions.clear();
                    } else {
                        es.recent_actions.push(cls);
                        es.recent_convictions.push(parsed.conviction);
                        let cap = early_stop_cfg.window;
                        if es.recent_actions.len() > cap {
                            let drop_n = es.recent_actions.len() - cap;
                            es.recent_actions.drain(0..drop_n);
                            es.recent_convictions.drain(0..drop_n);
                        }
                    }
                    es.prev_position = book.position(asset_sym);
                }

                // F43 (`trace-dock-emitters`): close the per-decision span
                // + emit the `decision_completed` engine event so the
                // trace dock can compute decision-scoped duration.
                //
                // QA30: enrich the payload so the SpanInspector renders a
                // useful summary when an operator clicks an `agent.decision`
                // span — previously the trace dock showed an "empty"
                // decision span with no action / price / position context.
                // The shape mirrors `eval_decisions` row fields plus the
                // pre-fill position so a reader can see what the agent saw
                // entering the cycle.
                if let Some(obs) = self.obs_emitter.as_ref() {
                    obs.emit_span_finished_ok(&decision_span_id).await;
                    let post_fill_position = book.position(asset_sym);
                    // WS-14: surface this decision's PnL/position arc on the
                    // engine event so the per-decision outcome lives in the trace,
                    // not only on the chart SSE. Every value is read from state
                    // already mutated by the fill above — no extra computation
                    // pass, no change to fill/book logic.
                    //
                    // `mark` is this asset's next-bar open (the same per-asset mark
                    // the pooled-NAV step below uses) so `unrealized_pnl` /
                    // `equity_delta` agree with the recorded equity series. The
                    // decision entered with `equity` (last set at the prior
                    // timestamp's NAV step); `equity_post` re-marks the book after
                    // this decision's realized + position change.
                    let mark = next_bar_open;
                    let cumulative_realized = book.realized();
                    let unrealized_pnl = if post_fill_position.abs() > f64::EPSILON {
                        post_fill_position * (mark - book.entry_price(asset_sym))
                    } else {
                        0.0
                    };
                    let mut decision_marks: BTreeMap<xvision_core::trading::AssetSymbol, f64> =
                        BTreeMap::new();
                    decision_marks.insert(asset_sym, mark);
                    let equity_post = book.equity(&decision_marks);
                    let equity_delta = equity_post - equity;
                    let payload = serde_json::json!({
                        "decision_index": decision_idx,
                        "asset": asset,
                        "bar_ts": bar.timestamp.to_rfc3339(),
                        "mark_price": bar.close,
                        "action": parsed.action,
                        "applied_action": applied_action,
                        "conviction": parsed.conviction,
                        "justification": parsed.justification,
                        "filled": fill_happened,
                        "fill_price": fill.fill_price,
                        "fill_size": fill.fill_size,
                        "fee": fill.fee,
                        "realized_pnl": fill.realized_pnl,
                        "cumulative_realized": cumulative_realized,
                        "unrealized_pnl": unrealized_pnl,
                        "equity_delta": equity_delta,
                        "position_pre": pre_fill_position,
                        "position_post": post_fill_position,
                    });
                    obs.emit_engine_event(
                        "decision_completed",
                        Some(decision_span_id.clone()),
                        Some(payload.to_string()),
                    )
                    .await;
                }

                decision_idx += 1;
            } // end 'asset inner loop

            // Pooled NAV mark for this timestamp. Each active asset is
            // valued at its next-bar open (T+1) when it has a bar at this
            // timestamp, falling back to its bar close on the terminal bar
            // — the same per-asset mark price the decision body used. An
            // asset with no bar at this timestamp keeps its prior leg and
            // is simply absent from `marks` (the book treats absent marks
            // as zero unrealized for that leg this tick). Recorded once so
            // there is a single pooled equity series.
            let mut marks: BTreeMap<xvision_core::trading::AssetSymbol, f64> = BTreeMap::new();
            for (&a, &idx) in assets_at_ts.iter() {
                let abars = &asset_bars[&a];
                let mark = abars.get(idx + 1).map(|b| b.open).unwrap_or(abars[idx].close);
                // Carry this asset's last seen mark on its open leg (if any)
                // so a later timestamp where the asset has no bar falls back
                // to this value instead of marking to entry. Single-asset:
                // the one asset is always present, so `mark` only ever sets
                // the same value `marks` already carries — equity unchanged.
                book.mark(a, mark);
                marks.insert(a, mark);
            }
            equity = book.equity(&marks);
            // B25: buffer instead of per-row INSERT; flushed in one tx below.
            equity_samples_buf.push((ts, equity));
            self.emit_chart(
                &run.id,
                RunChartEvent::Equity(ChartEquityPoint {
                    time: ts.timestamp(),
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
                // CT5: backtests are NOT deployments — leave the capital fields
                // None so the backtest path is behaviorally unchanged and no
                // live-money number is ever emitted from a backtest.
                deployed_capital_usd: None,
                unrealized_pnl_usd: None,
                realized_pnl_usd: None,
                daily_loss_limit_remaining_usd: None,
            });

            timeline_idx += 1;
        }

        // B25: flush all buffered equity samples in a single transaction now
        // that the timeline loop is complete. This is the sole DB write for
        // the entire equity series, replacing ~2 000 auto-commit INSERTs.
        store.record_equity_batch(&run.id, &equity_samples_buf).await?;

        if store.is_terminal(&run.id).await? {
            // F36: capture the (now near-complete) accumulators before bailing.
            let partial = compute_run_metrics(
                &equity_curve,
                initial,
                equity,
                strategy.manifest.decision_cadence_minutes,
                realized_count,
                wins,
                n_trades,
                decision_idx,
                None,
            );
            let _ = store
                .persist_partial(&run.id, &partial, total_input_tokens, total_output_tokens)
                .await;
            anyhow::bail!("eval run stopped");
        }

        // Mark-to-market: close all open positions at their last-seen mark
        // price before computing metrics. Applied at NORMAL COMPLETION only
        // (all bars consumed). Early-stop/cancel bails above this point, so
        // this block is unreachable on a cancelled run.
        for (_asset, pnl) in book.close_all_at_mark() {
            realized_count += 1;
            n_trades += 1;
            if pnl > 0.0 {
                wins += 1;
            }
            equity += pnl;
        }

        let cadence_minutes = strategy.manifest.decision_cadence_minutes;
        let strategy_return_pct = total_return_pct(initial, equity);

        // Compute the five automatic baselines over the same cadence-gated bar
        // slice the strategy saw. `decision_bars` was populated by the loop
        // above — one push per cadence-gate pass, matching the strategy's
        // iteration exactly.
        let baselines = build_baselines_report(&decision_bars, initial, cadence_minutes, strategy_return_pct);

        // inference_cost_quote_total + net_return_pct populated post-finalize by
        // api::eval::enrich_with_inference_cost.
        let metrics = compute_run_metrics(
            &equity_curve,
            initial,
            equity,
            cadence_minutes,
            realized_count,
            wins,
            n_trades,
            decision_idx,
            Some(baselines),
        );

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

    /// Live run loop (§3 L1 single-asset, §4 L2 multi-asset fanout).
    ///
    /// Drives a streaming `MultiLiveStream.next_tagged()` loop instead of
    /// the backtest's aligned timeline. Each arriving `(asset, bar)` runs
    /// ONE per-asset decision cycle through the shared `decide_one_live`
    /// body, submits the resulting order through `RealBrokerFills`
    /// (decision → broker market order → broker-reported fill) on a
    /// `WallClock`, applies the broker-reported fill to the SHARED pooled
    /// `PortfolioBook`, and records the decision + a pooled-equity sample.
    /// No injected backtest bars are required — bars come from the stream.
    ///
    /// **Per-asset isolation (§4 item 2):** the rolling `history`,
    /// `signal_cache`, and `last_open_direction` are keyed per asset
    /// (`BTreeMap<AssetSymbol, _>`) so a signal/position on BTC never
    /// bleeds into ETH. Only the `PortfolioBook` (pooled NAV) and the
    /// monotonic `decision_idx` are shared — matching the multi-asset
    /// backtest, where the decision index is a single run-wide counter.
    ///
    /// **Equity-PK keying (§4 item 3):** `eval_equity_samples` is keyed
    /// `(run_id, timestamp)`. Live bars are not pre-aligned into a
    /// timeline, so two assets can arrive at the same bar timestamp. We
    /// therefore upsert the pooled equity sample per arriving bar
    /// (`record_equity_upsert`): the latest pooled NAV at a given
    /// timestamp wins, yielding exactly one pooled-equity row per
    /// timestamp without a PK collision. A single-asset run never repeats
    /// a timestamp, so the upsert behaves like the L1 plain INSERT.
    ///
    /// Exit conditions:
    ///   (a) stream end — `next_tagged()` returns `None` (ALL sub-streams
    ///       closed);
    ///   (b) `StopPolicy` limit hit — time / bar / decision;
    ///   (c) cancellation — `store.is_terminal()` flips (cancel/stop);
    ///   (d) broker error — a `RealBrokerFills` rejection surfaces as a
    ///       `Rejected` `FillRecord` carrying a `broker_error`, which is
    ///       lifted into a classified run failure.
    ///
    /// The per-(asset, bar) body is factored into `decide_one_live`, called
    /// once per arriving tagged bar.
    #[allow(clippy::too_many_arguments)]
    async fn run_inner_live(
        &self,
        run: &mut Run,
        strategy: &Strategy,
        scenario: &Scenario,
        agent_slots: &[ResolvedAgentSlot],
        dispatch: Arc<dyn LlmDispatch>,
        tools: Arc<ToolRegistry>,
        store: &RunStore,
    ) -> Result<MetricsSummary> {
        use crate::eval::executor::asset_set::active_assets;
        use std::collections::BTreeMap;

        // Resolve the active asset set. The `MultiLiveStream` in
        // `live_runtime` was built (in `build_live_executor`) over this
        // same active set, so the per-asset state maps below cover exactly
        // the assets the stream can yield. A single active asset reduces to
        // the L1 path (one map entry, one sub-stream).
        let active = active_assets(&strategy.manifest.asset_universe, self.asset_subset.as_deref())?;
        if active.is_empty() {
            anyhow::bail!(
                "strategy {} resolved an empty active asset set",
                strategy.manifest.id
            );
        }
        // Venue symbols of the active set, surfaced in each seed as
        // `active_assets` so the trader sees the cross-asset context
        // (mirrors the multi-asset backtest path).
        let active_venue_symbols: Vec<String> = active.iter().map(|a| a.as_alpaca_pair()).collect();
        // Per-asset venue-symbol strings, resolved once so the hot loop
        // doesn't reallocate them per bar.
        let asset_pairs: BTreeMap<xvision_core::trading::AssetSymbol, String> =
            active.iter().map(|a| (*a, a.as_alpaca_pair())).collect();

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

        let inputs_policy = resolve_inputs_policy(agent_slots);

        let supported_timeframes = self
            .market_data
            .as_ref()
            .map(|ctx| ctx.supported_timeframes(*active.first().expect("active asset")))
            .unwrap_or_else(|| vec![strategy.native_timeframe().as_str().to_string()]);
        let bar_history_limit = resolve_bar_history_limit(agent_slots);
        let history_window = scenario.warmup_bars as usize;

        let initial = scenario.capital.initial;
        // Pooled book + NAV are SHARED across all assets (PortfolioBook
        // carries per-asset legs + a pooled equity formula).
        let mut book = crate::eval::executor::book::PortfolioBook::new(initial);
        let mut equity = initial;
        let mut equity_curve: Vec<f64> = vec![initial];
        let mut peak_equity = initial.max(0.0);
        // Single monotonic decision counter, shared across assets — exactly
        // like the multi-asset backtest. Each arriving (asset, bar) gets a
        // unique index so the `(run_id, decision_index)` PK never collides.
        let mut decision_idx = 0u32;
        let mut n_trades = 0u32;
        let wins = 0u32;
        let realized_count = 0u32;
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        let run_started: Instant = Instant::now();
        // PER-ASSET guardrail flip memory: the trader's last emitted open
        // direction, keyed per asset so a flip on BTC doesn't leak into
        // ETH's flip detection (mirrors the backtest's per-asset map).
        let mut last_open_direction: BTreeMap<xvision_core::trading::AssetSymbol, Option<GuardAction>> =
            active.iter().map(|a| (*a, None)).collect();
        // PER-ASSET rolling bar-history window. Each asset's live bars
        // (including its warmup prefix, which the `LiveStream` drains
        // first) accumulate in their own buffer so the trader seed for one
        // asset never sees another asset's bars.
        let mut history: BTreeMap<xvision_core::trading::AssetSymbol, Vec<Ohlcv>> =
            active.iter().map(|a| (*a, Vec::new())).collect();
        // PER-ASSET signal cache so filter signals stay scoped to the asset
        // they were computed for. `SignalScope::Asset(asset)` keys the
        // filter dispatch; an isolated cache per asset keeps the scopes
        // from sharing entries.
        let mut signal_cache: BTreeMap<
            xvision_core::trading::AssetSymbol,
            crate::agent::signal_cache::SignalCache,
        > = active
            .iter()
            .map(|a| (*a, crate::agent::signal_cache::SignalCache::new()))
            .collect();
        // Last pooled-equity timestamp recorded — drives the upsert path so
        // a single-asset run keeps the L1 one-INSERT-per-bar shape while a
        // multi-asset run collapses same-timestamp bars to one pooled row.
        let multi_filter_config = crate::agent::filter_dispatch::MultiFilterConfig::default();
        let cadence_min = strategy.manifest.decision_cadence_minutes.max(1);
        let bar_period_minutes = cadence_min;
        // B25: buffer equity samples during the live loop; flushed in one
        // transaction after the loop so we don't issue one auto-commit INSERT
        // per bar (WAL checkpoint stall, B25).
        let mut equity_samples_buf: Vec<(chrono::DateTime<chrono::Utc>, f64)> = Vec::new();
        // R3 risk-veto: run-level daily-loss accumulator state (mirrors the
        // backtest path at lines ~817–818). These are NOT per-asset — the
        // daily kill check applies to the whole run's realized PnL.
        let mut daily_loss_day: Option<chrono::NaiveDate> = None;
        let mut daily_realized_at_day_start: f64 = 0.0;
        // CT5 (§6): in-memory per-session execution-layer tracker. The
        // authoritative source for the deployment's drawdown_pct +
        // daily_loss_limit_remaining_usd, kept in lock-step with the loop-local
        // peak / day-start above. NOT a persisted snapshot table.
        let mut session_tracker = crate::eval::executor::live_session::LiveSessionTracker::new(initial);
        // CT5 (§6.3 option A): the most recent per-run unrealized PnL, persisted
        // to `eval_runs.unrealized_pnl_usd` in the buffered equity flush so the
        // poll path has an honest number between SSE ticks. `None` pre-first-mark.
        let mut latest_unrealized_pnl: Option<f64> = None;

        // CT5: per-bar live_run_state upsert. One SQLite row per live run,
        // written best-effort after each bar so the dashboard can display
        // capital-risk state without waiting for the run to finish.
        let mut risk_veto_count: i64 = 0;
        let live_state = crate::eval::live_run_state::LiveStateStore::new(store.pool().clone());
        let deployed_capital = initial;
        let strategy_id = run.live_config.as_ref().map(|c| c.strategy_id.clone());
        let strategy_name = run.live_config.as_ref().map(|c| c.display_name.clone());

        // Pull the runtime out of the executor for the duration of the
        // loop. The `Mutex` is held across `.await`s on the stream + fills;
        // a live run has a single driver so there is no contention.
        let mut runtime = self
            .live_runtime
            .as_ref()
            .expect("run_inner_live invoked without a live_runtime")
            .lock()
            .await;
        let stop_policy = runtime.stop_policy.clone();

        // Per-bar filter hook for strategy-level gating. `None` for
        // `EveryBar` strategies (the default); when `Some`, `decide_one_live`
        // evaluates it before `run_pipeline` so cooldown_bars,
        // max_wakeups_per_day, and wake_when_in_position are honored in
        // live mode.
        let mut filter_hook = crate::eval::filter_hook::FilterHook::new(strategy)?
            .map(|hook| hook.with_obs(self.obs_emitter.clone()));

        // SLTP parity: per-asset stop-loss / take-profit state.
        let mut sltp_state: std::collections::BTreeMap<
            xvision_core::trading::AssetSymbol,
            crate::eval::executor::sltp::PositionRiskState,
        > = std::collections::BTreeMap::new();
        let mut short_bars_held: std::collections::BTreeMap<xvision_core::trading::AssetSymbol, u32> =
            std::collections::BTreeMap::new();
        let wake_never = strategy
            .filter
            .as_ref()
            .map(|f| matches!(f.wake_when_in_position, xvision_filters::WakeInPosition::Never))
            .unwrap_or(false);
        let default_slip_bps = match &scenario.venue.slippage {
            SlippageModel::Linear { bps } => *bps as f64,
            SlippageModel::None => 0.0,
            SlippageModel::VolumeShare { .. } => 0.0,
        };
        let default_taker_bps = scenario.venue.fees.taker_bps as f64;
        let bar_secs = (cadence_min.max(1) as u64) * 60;

        // Early-stop: per-asset flat-degeneracy detection.
        let early_stop_cfg = EarlyStopConfig::from_env_or_default();
        let mut early_stop_state: std::collections::BTreeMap<
            xvision_core::trading::AssetSymbol,
            EarlyStopState,
        > = active.iter().map(|a| (*a, EarlyStopState::default())).collect();

        // In-memory episodic store for per-decision recall.
        let mut episodic_store = crate::agent::episodic::EpisodicStore::new(500);
        // Graceful LLM delay: track skipped dispatches and flag delayed
        // decisions. The live loop currently calls decide_one_live
        // synchronously, so these counters are infrastructure for the
        // eventual async-dispatch upgrade. When the agent takes multiple
        // bars to respond, skipped_dispatches counts bars that arrived
        // during agent think-time, and delayed_decisions counts decisions
        // whose bar age exceeds the cadence period.
        let mut skipped_dispatches: u64 = 0;
        let mut delayed_decisions: u64 = 0;
        /// Count of agents force-cancelled via --max-agent-ms.
        let mut forced_cancels: u64 = 0;
        // Track the bar timestamp this agent was dispatched for.
        // Used to compute staleness when the decision arrives.
        let mut current_agent_bar: Option<chrono::DateTime<chrono::Utc>> = None;

        tracing::info!(
            target: "xvision_engine::live_executor",
            run_id = %run.id,
            strategy = %strategy.manifest.id,
            active_assets = ?active.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            filter = %filter_hook.as_ref().map_or("none", |_| "active"),
            bar_limit = ?stop_policy.bar_limit,
            "live loop: entering bar stream",
        );

        // Drain warmup history from the bar source into per-asset buffers so
        // the first tradable bar has a complete lookback window.  Without
        // this, `MultiLiveStream::new()` collects warmup via `take_warmup()`
        // but `run_inner_live` never consumes it — indicators see zero
        // history and the first several bars are wasted.
        let warmup = runtime.bar_source.take_warmup_history();
        let mut bar_count: u32 = 0;
        if !warmup.is_empty() {
            let counts: Vec<String> = warmup
                .iter()
                .map(|(a, bars)| format!("{}:{} bars", a, bars.len()))
                .collect();
            tracing::info!(
                target: "xvision_engine::live_executor",
                "live loop: seeding per-asset history from warmup ({} assets: {})",
                warmup.len(),
                counts.join(", "),
            );
            for (asset, bars) in warmup {
                // Emit initial-equity chart points for warmup bars so the
                // dashboard chart shows the full lookback window.
                for bar in &bars {
                    equity_samples_buf.push((bar.timestamp, equity));
                    self.emit_chart(
                        &run.id,
                        RunChartEvent::Equity(ChartEquityPoint {
                            time: bar.timestamp.timestamp(),
                            equity_usd: equity,
                        }),
                    )
                    .await;
                    // Emit bar event for SSE subscribers and record in
                    // eval_run_bars so the chart endpoint can render candles
                    // even before the first decision.
                    self.emit_chart(
                        &run.id,
                        RunChartEvent::Bar(crate::api::chart::ChartBar {
                            time: bar.timestamp.timestamp(),
                            open: bar.open,
                            high: bar.high,
                            low: bar.low,
                            close: bar.close,
                            volume: bar.volume,
                        }),
                    )
                    .await;
                    store
                        .record_bar(
                            &run.id,
                            &asset.as_alpaca_pair(),
                            bar_count,
                            bar.timestamp,
                            bar.open,
                            bar.high,
                            bar.low,
                            bar.close,
                            bar.volume,
                        )
                        .await
                        .with_context(|| format!("record warmup bar idx={bar_count}"))?;
                    bar_count += 1;
                }
                // Push warmup bars through the filter hook so the
                // indicator engine accumulates the bars it needs to exit
                // the Warming state. Must run BEFORE hist.extend() which
                // consumes `bars`.
                if let Some(hook) = filter_hook.as_mut() {
                    for bar in &bars {
                        hook.evaluate(bar, false);
                    }
                }
                // Seed per-asset history for the LLM seed context.
                if let Some(hist) = history.get_mut(&asset) {
                    hist.extend(bars);
                }
            }
        }

        tracing::info!(
            target: "xvision_engine::live_executor",
            run_id = %run.id,
            strategy = %strategy.manifest.id,
            active_assets = ?active.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            filter = %filter_hook.as_ref().map_or("none", |_| "active"),
            bar_limit = ?stop_policy.bar_limit,
            "live loop: entering bar stream",
        );

        // F36: capture-on-interrupt for the live loop too — a cancelled or
        // crashed live/real-money run must record the metrics+tokens it
        // accumulated, not NULL.
        let mut last_partial_persist = Instant::now();
        let mut live_bar_count: u32 = 0;
        loop {
            // (c) cancellation / external stop.
            if store.is_terminal(&run.id).await? {
                // A2: close-in-loop. A cancelled/terminated LIVE run can still
                // hold real broker positions opened earlier in the loop. Before
                // we finish, flatten every open leg through the broker (the same
                // `RealBrokerFills` submit path normal fills use) so the run does
                // not leave dangling exposure, and the closing fills settle
                // realized PnL / equity. Best-effort and panic-free: a per-asset
                // failure is logged + noted, then we still bail. This is the
                // LIVE path only — the backtest/simulated cancel path never
                // reaches here and keeps its no-broker-call behavior. `equity`
                // is recomputed below from the post-flatten book before the
                // partial snapshot so the cancelled run records its settled NAV.
                let flatten = self
                    .close_open_positions_on_cancel(
                        store,
                        run,
                        strategy,
                        scenario,
                        &mut book,
                        &mut runtime.fill_sink,
                        equity,
                        &mut decision_idx,
                    )
                    .await;
                // Recompute equity on ANY settled close fill (full OR partial):
                // a partial close still settles realized PnL on the book, so the
                // cancelled run's partial metrics must use the refreshed NAV,
                // not a stale pre-close value. Gating on the full-close count
                // alone (`fully_closed > 0`) skipped this on a `flat_partial`.
                if flatten.any_fill {
                    equity = book.equity(&std::collections::BTreeMap::new());
                    equity_curve.push(equity);
                }
                let partial = compute_run_metrics(
                    &equity_curve,
                    initial,
                    equity,
                    strategy.manifest.decision_cadence_minutes,
                    realized_count,
                    wins,
                    n_trades,
                    decision_idx,
                    None,
                );
                let _ = store
                    .persist_partial(&run.id, &partial, total_input_tokens, total_output_tokens)
                    .await;
                anyhow::bail!("eval run stopped");
            }

            // (c2) A3 one-shot flatten: the cockpit's [Flatten positions]
            // action (spec §2.7) sets `eval_runs.flatten_requested` so the
            // operator can close all open positions at market WITHOUT
            // terminating the run (it typically stays paused/alive). Honored
            // here, alongside the cancel checkpoint above: when the flag is
            // set, flatten every open leg through the SAME close path A2 uses
            // on cancel, then CLEAR the flag (one-shot) and CONTINUE the loop.
            // The run is NOT terminated. `flatten_requested` shares the
            // missing-column tolerance `is_paused` uses (a pre-062 schema is
            // inert → Ok(false)); a non-missing-column read error propagates
            // via `?` rather than silently skipping the flatten.
            if store.flatten_requested(&run.id).await? {
                let flatten = self
                    .flatten_open_positions(
                        FlattenReason::Flatten,
                        store,
                        run,
                        strategy,
                        scenario,
                        &mut book,
                        &mut runtime.fill_sink,
                        equity,
                        &mut decision_idx,
                    )
                    .await;
                // Recompute equity on ANY settled close fill (full OR partial):
                // a partial close settles realized PnL on the book, so the live
                // run must continue from the refreshed NAV. Gating on the
                // full-close count alone skipped this on a `flat_partial`.
                if flatten.any_fill {
                    equity = book.equity(&std::collections::BTreeMap::new());
                    equity_curve.push(equity);
                }
                // Clear the request UNCONDITIONALLY (even when some legs failed
                // to close or the book was already flat): the flag is a
                // one-shot request, and re-flattening every cycle would trap a
                // run that hit a partial/rejected close. Failures were already
                // logged + recorded as supervisor notes inside the helper.
                // Best-effort: a clear failure is logged but does not abort the
                // run (the run stays alive; worst case the operator's next
                // cycle re-attempts the flatten, which is safe — a flat book is
                // a no-op).
                if let Err(e) = store.clear_flatten(&run.id).await {
                    tracing::warn!(
                        target: "xvision_engine::live_executor",
                        run_id = %run.id,
                        error = %e,
                        "live flatten: failed to clear flatten_requested after flattening; \
                         the run continues and a stale flag may re-trigger one more flatten next cycle"
                    );
                }
            }

            if last_partial_persist.elapsed() >= PARTIAL_PERSIST_INTERVAL {
                let partial = compute_run_metrics(
                    &equity_curve,
                    initial,
                    equity,
                    strategy.manifest.decision_cadence_minutes,
                    realized_count,
                    wins,
                    n_trades,
                    decision_idx,
                    None,
                );
                let _ = store
                    .persist_partial(&run.id, &partial, total_input_tokens, total_output_tokens)
                    .await;
                // CT5 (§6.3 option A): persist the latest per-run unrealized so
                // the `LiveDeploymentSummary` poll path is fresh between SSE
                // ticks. `None` (pre-first-mark) writes NULL, never a faked 0.
                let _ = store.set_unrealized_pnl(&run.id, latest_unrealized_pnl).await;
                last_partial_persist = Instant::now();
            }

            // (a) stream end — `next_tagged()` returns `None` only once ALL
            // sub-streams have closed. A lagging or closed sub-stream does
            // not stop the merged stream (§4 item 4); `select_all` drops a
            // closed sub-stream and keeps yielding from the live ones.
            let (asset_sym, bar) = match runtime.bar_source.next_tagged().await {
                Some(tagged) => tagged,
                None => break,
            };
            // Venue-symbol string for this asset (e.g. "BTC/USD"). Resolved
            // from the pre-built map for the active set; an asset the stream
            // yields outside that set still gets a valid pair computed on
            // demand (any `AssetSymbol` has one), so a config/universe drift
            // doesn't drop bars.
            let asset = asset_pairs
                .get(&asset_sym)
                .cloned()
                .unwrap_or_else(|| asset_sym.as_alpaca_pair());
            bar_count += 1;
            live_bar_count += 1;
            // Emit bar event for SSE subscribers and persist in eval_run_bars.
            self.emit_chart(
                &run.id,
                RunChartEvent::Bar(crate::api::chart::ChartBar {
                    time: bar.timestamp.timestamp(),
                    open: bar.open,
                    high: bar.high,
                    low: bar.low,
                    close: bar.close,
                    volume: bar.volume,
                }),
            )
            .await;
            store
                .record_bar(
                    &run.id,
                    &asset,
                    bar_count,
                    bar.timestamp,
                    bar.open,
                    bar.high,
                    bar.low,
                    bar.close,
                    bar.volume,
                )
                .await
                .with_context(|| format!("record live bar idx={bar_count}"))?;
            // the live stream is yielding bars without bumping to debug.
            // Throttled to every 50th bar (roughly every 50 min at 1m
            // granularity) to avoid spamming container logs.
            if live_bar_count % 50 == 1 {
                tracing::info!(
                    target: "xvision_engine::live_executor",
                    live_bar_count,
                    asset = %asset_sym,
                    "live bar received"
                );
            }

            // (b) bar-count stop limit. Checked AFTER the decision is
            // recorded (at the loop bottom via `live_stop_reason`) so
            // `bar_limit = N` yields exactly N decisions / bars.

            // Logical clock = wall clock for live (`advance_to` is a no-op
            // on `WallClock`; we still call it to mirror the backtest shape).
            // `wall_now` is the run's logical "now" — surfaced in the
            // RunTick so subscribers see progress against real time.
            runtime.clock.advance_to(bar.timestamp);
            let wall_now = runtime.clock.now();
            // Persisted decision rows are keyed by `(run_id, decision_index)`
            // (a single monotonic counter, unique per arriving bar) and the
            // equity sample by the BAR's timestamp. Bar timestamps align the
            // chart series to the market data the trader saw; the pooled
            // equity write below upserts on `(run_id, timestamp)` so two
            // assets sharing a timestamp collapse to one pooled row rather
            // than colliding on the PK (§4 item 3).
            let decision_ts = bar.timestamp;

            self.emit(ProgressEvent::RunTick {
                run_id: run.id.clone(),
                // Live runs are open-ended; surface bar progress against the
                // bar_limit when one is set, else hold at 0 (indeterminate).
                scenario_progress_pct: stop_policy
                    .bar_limit
                    .map(|lim| ((live_bar_count as f64 / lim as f64) * 100.0).clamp(0.0, 100.0))
                    .unwrap_or(0.0),
                current_ts: wall_now,
            });

            // Run one decision cycle for THIS (asset, bar). The pooled book
            // + the shared `equity` are passed in; the per-asset rolling
            // history, signal cache, and open-direction memory are pulled
            // from their per-asset maps so BTC's state never bleeds into
            // ETH (§4 item 2). `decide_one_live` is the shared body — its
            // signature is unchanged from L1.
            // Per-asset state is pre-seeded for the strategy's active set,
            // but the stream is built over the LiveConfig's asset list; if
            // those ever diverge we lazily create state for the arriving
            // asset rather than panicking, so a config/universe mismatch
            // degrades to "this asset just starts cold" instead of a crash.
            let asset_history = history.get(&asset_sym).map(|v| v.as_slice()).unwrap_or(&[]);
            let asset_last_open = last_open_direction.get(&asset_sym).copied().flatten();
            let asset_signal_cache = signal_cache
                .entry(asset_sym)
                .or_insert_with(crate::agent::signal_cache::SignalCache::new);
            // Graceful LLM delay: determine if this decision is stale.
            // Compare the bar this agent was dispatched for against the
            // current decision timestamp. In async mode, the agent may
            // have taken several bars to complete — when it does, the
            // decision is flagged as delayed.
            let delayed = if let Some(agent_bar) = current_agent_bar.take() {
                let bar_age_ms = (decision_ts - agent_bar).num_milliseconds();
                let cadence_ms = strategy.manifest.decision_cadence_minutes as i64 * 60_000;
                if bar_age_ms >= cadence_ms {
                    delayed_decisions += 1;
                    true
                } else {
                    false
                }
            } else {
                false
            };
            current_agent_bar = Some(decision_ts);
            let dispatch_start = Instant::now();

            let outcome = match self
                .decide_one_live(
                    DecideOneLiveCtx {
                        run,
                        strategy,
                        scenario,
                        agent_slots,
                        dispatch: dispatch.clone(),
                        tools: tools.clone(),
                        store,
                        asset_sym,
                        asset: &asset,
                        active_venue_symbols: &active_venue_symbols,
                        bar: &bar,
                        decision_ts,
                        decision_idx,
                        equity,
                        inputs_policy,
                        bar_history_limit,
                        history_window,
                        supported_timeframes: &supported_timeframes,
                        history: asset_history,
                        last_open_direction: asset_last_open,
                        bar_period_minutes,
                        signal_cache: asset_signal_cache,
                        multi_filter_config,
                        daily_loss_day,
                        daily_realized_at_day_start,
                        filter_hook: filter_hook.as_mut(),
                        episodic_store: &mut episodic_store,
                        sltp_state: &mut sltp_state,
                        wake_never,
                        short_bars_held: &mut short_bars_held,
                        default_slip_bps,
                        default_taker_bps,
                        bar_secs,
                        early_stop_state: &mut early_stop_state,
                        early_stop_cfg: &early_stop_cfg,
                        delayed,
                    },
                    &mut runtime.fill_sink,
                    &mut book,
                )
                .await
            {
                Ok(o) => o,
                Err(e) => {
                    tracing::warn!(
                        target: "xvision_engine::live",
                        error = %e,
                        live_bar_count,
                        "live agent dispatch failed; skipping bar, will retry next cycle"
                    );
                    skipped_dispatches += 1;
                    let limits = self.limits.as_ref();
                    let max_skips = limits.map(|l| l.max_consecutive_skips).unwrap_or(5);
                    if skipped_dispatches > 0 && skipped_dispatches % max_skips as u64 == 0 {
                        self.emit(ProgressEvent::MetricsUpdated {
                            run_id: run.id.clone(),
                            equity,
                            drawdown_pct: 0.0,
                            n_trades,
                            deployed_capital_usd: None,
                            unrealized_pnl_usd: None,
                            realized_pnl_usd: None,
                            daily_loss_limit_remaining_usd: None,
                        });
                        tracing::warn!(
                            target: "xvision_engine::live",
                            consecutive_skips = skipped_dispatches,
                            "max-consecutive-skips threshold reached \u{2014} agent may be stuck"
                        );
                    }
                    // Still mark-to-market so equity stays current.
                    book.mark(asset_sym, bar.close);
                    let marks = std::collections::BTreeMap::from([(asset_sym, bar.close)]);
                    equity = book.equity(&marks);
                    equity_samples_buf.push((decision_ts, equity));
                    decision_idx += 1;
                    continue;
                }
            };
            // Hang belt: check dispatch elapsed against --max-agent-ms
            let dispatch_elapsed = dispatch_start.elapsed();
            if let Some(max_ms) = self.limits.as_ref().and_then(|l| l.max_agent_ms) {
                if dispatch_elapsed.as_millis() as u64 > max_ms {
                    forced_cancels += 1;
                    tracing::warn!(
                        target: "xvision_engine::live",
                        elapsed_ms = dispatch_elapsed.as_millis(),
                        max_agent_ms = max_ms,
                        "agent dispatch exceeded max-agent-ms; decision accepted but flagged as forced-cancel"
                    );
                }
            }

            // Always mark-to-market and record an equity sample, even
            // when the filter gate suppresses dispatch.  Without this,
            // `eval_equity_samples` stayed at the initial equity for
            // every filtered bar and the equity chart showed flat $0/nil
            // until the first filter pass — operators had no signal that
            // the run was alive and tracking.
            book.mark(asset_sym, bar.close);
            let marks = std::collections::BTreeMap::from([(asset_sym, bar.close)]);
            equity = book.equity(&marks);
            equity_samples_buf.push((decision_ts, equity));
            self.emit_chart(
                &run.id,
                RunChartEvent::Equity(ChartEquityPoint {
                    time: decision_ts.timestamp(),
                    equity_usd: equity,
                }),
            )
            .await;
            equity_curve.push(equity);
            if equity > peak_equity {
                peak_equity = equity;
            }
            session_tracker.observe_equity(equity);
            // R3 risk-veto: write back the (possibly updated) daily-loss
            // state from the outcome so the session tracker sees current
            // values on every bar, gated or not.
            daily_loss_day = outcome.daily_loss_day;
            daily_realized_at_day_start = outcome.daily_realized_at_day_start;
            if let Some(day) = daily_loss_day {
                session_tracker.roll_day(day, daily_realized_at_day_start);
            }

            // Filter gate: when the DSL filter suppressed this bar,
            // skip token counting, fill tracking, and history push.
            // Continue to the next bar so the loop stays alive.

            // Stop-policy time limit: checked BEFORE the filter gate so a
            // time-limited run always terminates, even when the filter
            // blocks every bar.  Bar/decision/trade limits stay after the
            // gate — they count only bars that pass the filter.
            if let Some(secs) = stop_policy.time_limit_secs {
                if run_started.elapsed().as_secs() >= secs {
                    tracing::info!(
                        target: "xvision_engine::live_executor",
                        run_id = %run.id,
                        live_bar_count,
                        "live run reached time_limit_secs {secs}; ending stream loop"
                    );
                    break;
                }
            }
            if outcome.filter_gated {
                continue;
            }

            total_input_tokens += outcome.input_tokens;
            total_output_tokens += outcome.output_tokens;
            run.actual_input_tokens = Some(total_input_tokens);
            run.actual_output_tokens = Some(total_output_tokens);
            store
                .update_token_usage(&run.id, total_input_tokens, total_output_tokens)
                .await?;

            if outcome.fill_happened {
                n_trades += 1;
            }
            // Per-asset open-direction memory: write back THIS asset's
            // updated direction only.
            last_open_direction.insert(asset_sym, outcome.last_open_direction);

            // Push this bar onto THIS asset's rolling history AFTER the
            // decision (so the seed for bar T sees only bars strictly before
            // T, matching the backtest's history-slice semantics). Keyed per
            // asset so the window never mixes assets.
            let asset_hist = history.entry(asset_sym).or_default();
            asset_hist.push(bar.clone());
            if history_window > 0 && asset_hist.len() > history_window {
                let drop_n = asset_hist.len() - history_window;
                asset_hist.drain(0..drop_n);
            }

            // (d) broker error — RealBrokerFills surfaced a rejection.
            // Structural limitations (e.g. Alpaca crypto is long-only so
            // short_open is permanently unsupported) are logged and skipped so
            // the run continues. All other rejection classes terminate the run
            // so the operator sees the failure immediately.
            if let Some((class, msg)) = outcome.broker_error {
                if class == BrokerErrorClass::UnsupportedAsset {
                    tracing::warn!(
                        target: "xvision_engine::live_executor",
                        error_class = class.as_tag(),
                        error_message = %msg,
                        "live broker: unsupported trade skipped (no-fill, run continues)"
                    );
                } else {
                    anyhow::bail!("[{}] live broker submit failed: {}", class.as_tag(), msg);
                }
            }
            // CT5 honesty (§6.1): source drawdown from the ONE session-tracker
            // authority instead of re-deriving it with a literal-0 fallback. The
            // tracker returns `None` only when there is genuinely no positive
            // peak yet; `observe_equity(equity)` ran just above, so a peak always
            // exists here and the `unwrap_or(0.0)` is never the fabricated "no
            // data" path. The `MetricsUpdated.drawdown_pct` field is `f64` (the
            // backtest path shares it), so per-tick `None` propagation over the
            // event is a deferred follow-up tied to widening that field.
            let drawdown_pct = session_tracker.drawdown_pct(equity).unwrap_or(0.0);
            // CT5 capital fields derived in-loop from the book + tracker. They
            // are emitted on BOTH buses:
            //  - the engine `ProgressBus` (`ProgressEvent::MetricsUpdated`), for
            //    CLI / optimizer / progress-bar subscribers, and
            //  - the `RunEventBus` as `RunChartEvent::DeploymentMetrics` (CT5
            //    §4, delivered below), which the dashboard deployment SSE already
            //    subscribes to. The SSE now streams the full capital block
            //    per-tick (`event: metrics`); consumers (`n0k`/`awm`/`8s4`) no
            //    longer have to wait on the 5s poll for live capital values.
            // HONESTY MANDATE: a realized figure with no fill history surfaces as
            // None, never a faked 0.
            let realized_now = book.realized();
            let realized_pnl_usd = if n_trades > 0 { Some(realized_now) } else { None };
            // Deployed capital = Σ |position| * mark over open legs.
            let deployed_capital_usd = {
                let notional: f64 = book
                    .open_legs()
                    .iter()
                    .map(|(_, position, _entry, mark)| position.abs() * *mark)
                    .sum();
                if book.open_position_count() > 0 {
                    Some(notional)
                } else {
                    None
                }
            };
            // Unrealized = equity - initial - realized (book mark-to-market).
            let unrealized_pnl_usd = Some(equity - initial - realized_now);
            let daily_loss_limit_remaining_usd = session_tracker
                .daily_loss_limit_remaining_usd(strategy.risk.daily_loss_kill_pct, realized_now);
            // CT5 §6.3 option A: persist the per-run unrealized so the poll path
            // (`LiveDeploymentSummary` from `GET /api/live/deployments`) has an
            // honest number — this is the source for the deployment's unrealized
            // P&L, since the SSE does not stream the capital block.
            if let Some(u) = unrealized_pnl_usd {
                latest_unrealized_pnl = Some(u);
            }
            self.emit(ProgressEvent::MetricsUpdated {
                run_id: run.id.clone(),
                equity,
                drawdown_pct,
                n_trades,
                deployed_capital_usd,
                unrealized_pnl_usd,
                realized_pnl_usd,
                daily_loss_limit_remaining_usd,
            });
            // CT5 §4: project the SAME capital block onto the RunEventBus the
            // dashboard deployment SSE reads. `drawdown_pct` carries the honest
            // `Option` from the session tracker (the `MetricsUpdated.drawdown_pct`
            // f64 above uses `unwrap_or(0.0)` only because the backtest path
            // shares that field; here we keep `None` when there is no peak).
            self.emit_chart(
                &run.id,
                RunChartEvent::DeploymentMetrics(crate::api::chart::DeploymentMetricsTick {
                    time: decision_ts.timestamp(),
                    equity_usd: equity,
                    drawdown_pct: session_tracker.drawdown_pct(equity),
                    deployed_capital_usd,
                    unrealized_pnl_usd,
                    realized_pnl_usd,
                    daily_loss_limit_remaining_usd,
                    n_trades,
                }),
            )
            .await;

            // CT5: per-bar capital-risk snapshot upserted into live_run_state.
            // Best-effort — a snapshot write failure must never abort the live
            // loop (real money may be riding on it).
            if outcome.risk_vetoed {
                risk_veto_count += 1;
            }
            {
                let realized_today = book.realized() - daily_realized_at_day_start;
                let kill_pct = strategy.risk.daily_loss_kill_pct;
                let daily_loss_remaining = (kill_pct * initial + realized_today).max(0.0);
                let unrealized: f64 = book
                    .open_legs()
                    .iter()
                    .map(|(_, pos, entry, last_mark)| pos * (last_mark - entry))
                    .sum();
                let snap = crate::eval::live_run_state::LiveRunState {
                    run_id: run.id.clone(),
                    strategy_id: strategy_id.clone(),
                    strategy_name: strategy_name.clone(),
                    deployed_capital_usd: deployed_capital,
                    equity_usd: Some(equity),
                    unrealized_pnl_usd: Some(unrealized),
                    realized_pnl_usd: Some(book.realized()),
                    realized_today_usd: Some(realized_today),
                    daily_loss_remaining_usd: Some(daily_loss_remaining),
                    drawdown_pct: Some(drawdown_pct),
                    peak_equity_usd: Some(peak_equity),
                    risk_veto_count,
                    last_decision_at: Some(decision_ts.to_rfc3339()),
                    updated_at: chrono::Utc::now().to_rfc3339(),
                    daily_loss_budget_usd: Some(kill_pct * initial),
                    stop_at: run
                        .live_config
                        .as_ref()
                        .and_then(|c| c.stop_policy.time_limit_secs)
                        .map(|secs| (run.started_at + chrono::Duration::seconds(secs as i64)).to_rfc3339()),
                };
                let _ = live_state.upsert(&snap).await;
                // CT5 Task 8: broadcast the per-bar state snapshot over SSE so
                // the live-deployments stream handler can forward it to clients.
                self.emit_chart(
                    &run.id,
                    RunChartEvent::LiveRunState(crate::api::chart::LiveRunStatePayload {
                        equity_usd: snap.equity_usd,
                        unrealized_pnl_usd: snap.unrealized_pnl_usd,
                        realized_today_usd: snap.realized_today_usd,
                        daily_loss_remaining_usd: snap.daily_loss_remaining_usd,
                        drawdown_pct: snap.drawdown_pct,
                        risk_veto_count: snap.risk_veto_count,
                        last_decision_at: snap.last_decision_at.clone(),
                    }),
                )
                .await;
            }

            decision_idx += 1;

            // LANE byu — periodic auto-attest. Only a FILLED trade advances
            // `n_trades`, so checking the boundary here fires the injected
            // attest sink exactly once each time the cumulative trade count
            // crosses an `attest_every_n_trades` boundary (20, 40, …) and
            // never between. The hook is fire-and-forget: we `.await` it but a
            // conforming impl returns promptly (spawning its own background
            // chain submit) so a slow/failing attestation never stalls or
            // aborts the loop while we hold the runtime mutex. No hook (every
            // backtest, every test that does not wire one) is a no-op.
            if super::attest_hook::is_attest_boundary(n_trades, self.attest_every_n_trades) {
                // WS-9: emit the chain-adjacent boundary event the ENGINE owns,
                // regardless of which hook (if any) is wired. This is the one
                // attestation-flow event that genuinely fires in a live run
                // today, so it lands on the trace dock + export even with the
                // no-op hook. The DOWNSTREAM lifecycle events (verdict / chain
                // submit / posted) are the HOOK's to emit through the same
                // emitter seam — the engine never adds an identity edge.
                if let Some(obs) = self.obs_emitter.as_ref() {
                    let payload = serde_json::json!({
                        "agent_id": strategy.manifest.id,
                        "n_trades": n_trades,
                        "run_id": run.id,
                    });
                    obs.emit_engine_event("attest_boundary_reached", None, Some(payload.to_string()))
                        .await;
                }
                if let Some(hook) = self.attest_hook.as_ref() {
                    let summary = super::attest_hook::AttestSummary {
                        run_id: run.id.clone(),
                        agent_id: strategy.manifest.id.clone(),
                        n_trades,
                        n_decisions: decision_idx,
                        realized_count,
                        wins,
                        gross_return_pct: if initial != 0.0 {
                            (equity - initial) / initial * 100.0
                        } else {
                            0.0
                        },
                        equity,
                    };
                    // Thread the run's ObsEmitter into the hook so a future
                    // identity-backed impl can emit `attest_verdict` /
                    // `chain_submit_started` / `chain_submit_finished` /
                    // `attestation_posted` onto the same bus. `None` for
                    // non-observed runs keeps the legacy quiet path.
                    hook.maybe_attest(summary, self.obs_emitter.clone()).await;
                }
            }

            // (b) StopPolicy — evaluate after the decision is fully
            // recorded so a limit of N yields N decisions. Whichever fires
            // first terminates the loop cleanly (not an error).
            if let Some(stop) =
                live_stop_reason(&stop_policy, live_bar_count, decision_idx, n_trades, run_started)
            {
                tracing::info!(
                    run_id = %run.id,
                    reason = %stop,
                    live_bar_count,
                    decision_idx,
                    "live run reached stop policy; ending stream loop"
                );
                break;
            }
        }
        drop(runtime);

        // B25: flush all buffered equity samples in a single upsert transaction.
        // The upsert variant is used here (not plain batch insert) because the
        // live loop can have two assets land at the same timestamp, making the
        // last-writer-wins ON CONFLICT semantics necessary — identical to the
        // per-row `record_equity_upsert` it replaces.
        store
            .record_equity_upsert_batch(&run.id, &equity_samples_buf)
            .await?;
        // CT5 (§6.3 option A): final persist of the per-run unrealized PnL so a
        // run that ends keeps its last honest mark-to-market for the poll path.
        let _ = store.set_unrealized_pnl(&run.id, latest_unrealized_pnl).await;

        if store.is_terminal(&run.id).await? {
            let partial = compute_run_metrics(
                &equity_curve,
                initial,
                equity,
                strategy.manifest.decision_cadence_minutes,
                realized_count,
                wins,
                n_trades,
                decision_idx,
                None,
            );
            let _ = store
                .persist_partial(&run.id, &partial, total_input_tokens, total_output_tokens)
                .await;
            anyhow::bail!("eval run stopped");
        }

        // Live runs do not compute the four backtest baselines (they replay a
        // single forward stream, not a fixed window).
        let mut metrics = compute_run_metrics(
            &equity_curve,
            initial,
            equity,
            strategy.manifest.decision_cadence_minutes,
            realized_count,
            wins,
            n_trades,
            decision_idx,
            None,
        );
        // Set live-only counters on the metrics summary.
        metrics.skipped_dispatches = skipped_dispatches;
        metrics.delayed_decisions = delayed_decisions;
        metrics.forced_cancels = forced_cancels;

        run.actual_input_tokens = Some(total_input_tokens);
        run.actual_output_tokens = Some(total_output_tokens);
        run.metrics = Some(metrics.clone());
        run.status = RunStatus::Completed;
        tracing::info!(
            target: "xvision::eval",
            run_id = %run.id,
            executor = "live",
            n_decisions = decision_idx,
            n_trades,
            "live eval run finalize"
        );
        store.finalize(&run.id, &metrics).await?;
        Ok(metrics)
    }

    /// One per-(asset, bar) decision cycle for the live loop. Shared body
    /// §4 will call per asset per bar. Builds the seed, runs the agent
    /// pipeline, parses the trader output, applies the pyramid-flip
    /// guardrail, submits the order through the live `FillSink`
    /// (`RealBrokerFills`), applies the broker-reported fill to the book,
    /// and records the decision row + chart events.
    ///
    /// Returns a [`LiveDecisionOutcome`] carrying token counts, the trade
    /// flag, the updated open-direction memory, and any broker error so the
    /// caller can fail the run on a rejected order.
    async fn decide_one_live(
        &self,
        ctx: DecideOneLiveCtx<'_>,
        fill_sink: &mut RealBrokerFills,
        book: &mut crate::eval::executor::book::PortfolioBook,
    ) -> Result<LiveDecisionOutcome> {
        let DecideOneLiveCtx {
            run,
            strategy,
            scenario,
            agent_slots,
            dispatch,
            tools,
            store,
            asset_sym,
            asset,
            active_venue_symbols,
            bar,
            decision_ts,
            decision_idx,
            equity,
            inputs_policy,
            bar_history_limit,
            history_window,
            history,
            supported_timeframes,
            mut last_open_direction,
            bar_period_minutes,
            signal_cache,
            multi_filter_config,
            mut daily_loss_day,
            mut daily_realized_at_day_start,
            mut filter_hook,
            episodic_store,
            sltp_state,
            wake_never,
            short_bars_held: _,
            default_slip_bps: _,
            default_taker_bps: _,
            bar_secs: _,
            early_stop_state,
            early_stop_cfg,
            delayed,
        } = ctx;

        // History slice: last `history_window` bars strictly before this
        // bar (the live loop pushes the current bar AFTER the decision).
        let history_slice: Vec<&Ohlcv> = {
            let start = history.len().saturating_sub(history_window);
            let slice = &history[start..];
            // F-8 cap shared with the backtest loop via `bar_history_limit_offset`.
            slice[bar_history_limit_offset(slice.len(), bar_history_limit)..]
                .iter()
                .collect()
        };
        let source_window_start = history_slice
            .first()
            .map(|b| b.timestamp)
            .unwrap_or(bar.timestamp);
        let source_window_end = bar.timestamp;

        // For live the next-open reference is the current bar's close — we
        // don't have a T+1 bar yet (the broker fills at the live market
        // price; `next_open` is only the reference price for sizing).
        let next_open = bar.close;
        // LIVE PERPS ATTACH POINT: extracted to a named binding so the R3 veto
        // below can read `perps_ctx.funding_rate` without duplicating the
        // default. A follow-on track populates funding/OI here from an
        // out-of-band poller (xvision_data::perp_feed::fetch_perp_snapshot).
        // Until that poller exists all fields are None → the funding-carry veto
        // no-ops regardless of `is_perp_venue`.
        let perps_ctx = PerpsContext::default();

        let last_closed_times = supported_timeframes
            .iter()
            .map(|tf| (tf.clone(), bar.timestamp))
            .collect();
        // Shared seed-context prologue: position/entry/mark are derived inside
        // `build_seed_context` (single source of truth), so this path can't
        // drift from the backtest one.
        let mut seed = build_decision_seed(DecisionSeedInput::from_context(build_seed_context(
            book,
            asset_sym,
            bar,
            SeedContextParams {
                decision_idx,
                asset,
                active_assets: active_venue_symbols,
                history_slice: &history_slice,
                inputs_policy,
                supported_timeframes,
                last_closed_times,
                equity,
                next_bar_open: next_open,
                reference_price_source: "live_bar.close",
                bars_held: sltp_state.get(&asset_sym).map(|s| s.bars_held).unwrap_or(0),
                stop_loss_price: sltp_state
                    .get(&asset_sym)
                    .map(|s| s.get_effective_sl_price())
                    .unwrap_or(0.0),
                take_profit_price: sltp_state
                    .get(&asset_sym)
                    .map(|s| s.get_effective_tp_price())
                    .unwrap_or(0.0),
                risk_config: &strategy.risk,
                perps: perps_ctx,
            },
        )));

        // Publish the current asset + bar timestamp into the shared dispatch
        // handles (mirrors the backtest write at the top of `let parsed`).
        if let Some(cline) = self.cline.as_ref() {
            publish_decision_context(&cline.tool_asset_guard, &cline.as_of_guard, &asset, bar.timestamp)
                .await;
        }

        // Filter gate: match the backtest path at lines ~1146-1175.
        // Evaluate the DSL filter hook before `run_pipeline` so that
        // cooldown_bars, max_wakeups_per_day, and wake_when_in_position
        // are honored in live mode — not just backtest.
        // Also record every evaluation via hook.record() so the UI shows
        // filter events (Warming, Inactive, Cooldown, Suppressed, Active)
        // even when the filter suppresses dispatch.
        let mut filter_gated = false;
        let mut filter_trigger_context: Option<serde_json::Value> = None;
        if let Some(hook) = filter_hook.as_mut() {
            let in_position = book.position(asset_sym).abs() > f64::EPSILON;
            let evaluation = hook.evaluate(bar, in_position);
            hook.record(
                store.pool(),
                self.progress.as_ref(),
                &run.id,
                decision_ts,
                &evaluation,
            )
            .await?;
            if !evaluation.outcome.decision.is_active() {
                filter_gated = true;
                if matches!(
                    evaluation.outcome.decision,
                    xvision_filters::runtime::ActivationDecision::SuppressedInPosition
                ) {
                    self.emit(ProgressEvent::FilterBlocked {
                        run_id: run.id.clone(),
                        reason: "in_position".to_string(),
                    });
                }
            } else {
                filter_trigger_context = evaluation.trigger_context.clone();
            }
        }

        // Inject filter context into the seed (parity with backtest L1307-1311).
        if let Some(ctx) = &filter_trigger_context {
            if let Some(obj) = seed.as_object_mut() {
                obj.insert("filter_context".to_string(), ctx.clone());
            }
        }
        // Inject briefing indicators for Pine-imported strategies (parity with backtest).
        if !strategy.briefing_indicators.is_empty() {
            inject_briefing_indicators_into_seed(
                &mut seed,
                &strategy.briefing_indicators,
                bar,
                &history_slice,
            );
        }

        // Inject episodic memory recall (parity with backtest L1330-1376).
        {
            let get_ind = |ctx: &serde_json::Value, key: &str| -> Option<f64> {
                ctx.get(key)
                    .or_else(|| ctx.get("context").and_then(|c| c.get(key)))
                    .and_then(|v| v.as_f64())
            };
            let query_vec = filter_trigger_context
                .as_ref()
                .map(|ctx| {
                    let snap = crate::agent::episodic::IndicatorSnapshot {
                        rsi: get_ind(ctx, "rsi_14"),
                        macd_hist: get_ind(ctx, "macd_hist"),
                        ema_cross: get_ind(ctx, "ema_cross"),
                        volume_zscore: get_ind(ctx, "volume_zscore"),
                    };
                    snap.feature_vector()
                })
                .unwrap_or([0.0_f64; 4]);
            if let Some(episodes_json) = episodic_store.to_seed_json(query_vec, 5) {
                let sanitized = sanitize_prior_episodes_for_policy(episodes_json, inputs_policy);
                if let Some(obj) = seed.as_object_mut() {
                    obj.insert("prior_episodes".to_string(), sanitized);
                }
            }
        }

        // When the filter gate is closed and there is no open SLTP-managed
        // position, skip the LLM pipeline entirely. Open positions still flow
        // through the deterministic SLTP check below before returning gated.
        let filter_gated_position_sltp_check = filter_gated
            && book.position(asset_sym).abs() > f64::EPSILON
            && sltp_state.contains_key(&asset_sym);
        if live_filter_gate_should_short_circuit(
            filter_gated,
            book.position(asset_sym).abs() > f64::EPSILON,
            sltp_state.contains_key(&asset_sym),
        ) {
            return Ok(LiveDecisionOutcome {
                input_tokens: 0,
                output_tokens: 0,
                fill_happened: false,
                last_open_direction,
                broker_error: None,
                daily_loss_day,
                daily_realized_at_day_start,
                risk_vetoed: false,
                filter_gated: true,
            });
        }

        // SLTP check: deterministic exits before LLM pipeline (parity with backtest).
        if book.position(asset_sym).abs() > f64::EPSILON {
            if let Some(sltp) = sltp_state.get_mut(&asset_sym) {
                use crate::eval::executor::sltp::SltpTrigger;
                match crate::eval::executor::sltp::check_and_update(sltp, bar) {
                    Some(SltpTrigger::FullExit { reason }) => {
                        let sltp_pos = book.position(asset_sym);
                        let sltp_entry = book.entry_price(asset_sym);
                        let mut req = live_sltp_close_request(
                            asset,
                            sltp_pos,
                            sltp_entry,
                            next_open,
                            scenario.venue.fees.taker_bps as f64,
                            equity,
                            u64::from(strategy.manifest.decision_cadence_minutes.max(1)),
                        );
                        req.bar_ts = bar.timestamp;
                        req.bar_open = bar.open;
                        req.bar_high = bar.high;
                        req.bar_low = bar.low;
                        req.bar_close = bar.close;
                        let fill = fill_sink.submit(req).await;
                        let broker_error = fill.broker_error.clone();
                        let fill_happened = fill.fill_price.is_some();
                        if fill_happened {
                            book.set_position(asset_sym, fill.new_pos, fill.new_entry);
                            book.add_realized(fill.realized_pnl);
                            sltp_state.remove(&asset_sym);
                        }
                        let row = crate::eval::store::DecisionRow {
                            run_id: run.id.clone(),
                            decision_index: decision_idx,
                            timestamp: bar.timestamp,
                            asset: asset.to_string(),
                            action: reason.to_string(),
                            conviction: Some(1.0),
                            justification: Some(format!("sltp: {reason}")),
                            reasoning: None,
                            order_size: fill.fill_size,
                            fill_price: fill.fill_price,
                            fill_size: fill.fill_size,
                            fee: fill.fee,
                            pnl_realized: if fill.realized_pnl != 0.0 {
                                Some(fill.realized_pnl)
                            } else {
                                None
                            },
                            delayed: Some(delayed),
                        };
                        store.record_decision(&row).await?;
                        self.emit_chart(&run.id, RunChartEvent::Decision(LiveDecisionRow::from(&row)))
                            .await;
                        if fill_happened {
                            self.emit(ProgressEvent::FillRecorded {
                                run_id: run.id.clone(),
                                side: fill_side_for_action(reason, sltp_pos).into(),
                                price: fill.fill_price.unwrap_or(0.0),
                                qty: fill.fill_size.unwrap_or(0.0),
                                fee: fill.fee.unwrap_or(0.0),
                            });
                        }
                        let mut outcome = live_sltp_exit_outcome(
                            fill_happened,
                            false,
                            broker_error,
                            if fill_happened { None } else { last_open_direction },
                        );
                        outcome.daily_loss_day = daily_loss_day;
                        outcome.daily_realized_at_day_start = daily_realized_at_day_start;
                        return Ok(outcome);
                    }
                    Some(SltpTrigger::PartialTp1 { fraction }) => {
                        let pos = book.position(asset_sym);
                        let entry = book.entry_price(asset_sym);
                        let close_units = live_sltp_close_units(pos, fraction);
                        let mut req = live_sltp_close_request(
                            asset,
                            close_units.copysign(pos),
                            entry,
                            next_open,
                            scenario.venue.fees.taker_bps as f64,
                            equity,
                            u64::from(strategy.manifest.decision_cadence_minutes.max(1)),
                        );
                        req.bar_ts = bar.timestamp;
                        req.bar_open = bar.open;
                        req.bar_high = bar.high;
                        req.bar_low = bar.low;
                        req.bar_close = bar.close;
                        let fill = fill_sink.submit(req).await;
                        let broker_error = fill.broker_error.clone();
                        let fill_happened = fill.fill_price.is_some();
                        if fill_happened {
                            let filled_units = fill.fill_size.unwrap_or(close_units);
                            let remaining = live_sltp_remaining_position(pos, filled_units);
                            book.add_realized(fill.realized_pnl);
                            book.set_position(asset_sym, remaining, entry);
                            sltp.tp1_taken = true;
                        }
                        let row = crate::eval::store::DecisionRow {
                            run_id: run.id.clone(),
                            decision_index: decision_idx,
                            timestamp: bar.timestamp,
                            asset: asset.to_string(),
                            action: "partial_tp1".to_string(),
                            conviction: Some(1.0),
                            justification: Some(format!("sltp: partial_tp1 {:.2}%", fraction * 100.0)),
                            reasoning: None,
                            order_size: fill.fill_size,
                            fill_price: fill.fill_price,
                            fill_size: fill.fill_size,
                            fee: fill.fee,
                            pnl_realized: if fill.realized_pnl != 0.0 {
                                Some(fill.realized_pnl)
                            } else {
                                None
                            },
                            delayed: Some(delayed),
                        };
                        store.record_decision(&row).await?;
                        self.emit_chart(&run.id, RunChartEvent::Decision(LiveDecisionRow::from(&row)))
                            .await;
                        if fill_happened {
                            self.emit(ProgressEvent::FillRecorded {
                                run_id: run.id.clone(),
                                side: fill_side_for_action("flat", pos).into(),
                                price: fill.fill_price.unwrap_or(0.0),
                                qty: fill.fill_size.unwrap_or(0.0),
                                fee: fill.fee.unwrap_or(0.0),
                            });
                        }
                        let mut outcome =
                            live_sltp_exit_outcome(fill_happened, false, broker_error, last_open_direction);
                        outcome.daily_loss_day = daily_loss_day;
                        outcome.daily_realized_at_day_start = daily_realized_at_day_start;
                        return Ok(outcome);
                    }
                    None => {}
                }
            }
        }

        if filter_gated_position_sltp_check {
            return Ok(LiveDecisionOutcome {
                input_tokens: 0,
                output_tokens: 0,
                fill_happened: false,
                last_open_direction,
                broker_error: None,
                daily_loss_day,
                daily_realized_at_day_start,
                risk_vetoed: false,
                filter_gated: true,
            });
        }

        // Early-stop: skip pipeline during flat-degeneracy streaks.
        {
            let es = early_stop_state
                .get(&asset_sym)
                .expect("early_stop_state seeded for every active asset");
            if es.inherit_remaining == 0 {
                let actions: Vec<crate::eval::early_stop::Action> = es.recent_actions.clone();
                if let Some(plan) = crate::eval::early_stop::should_skip_next_decision(
                    &actions,
                    &es.recent_convictions,
                    book.position(asset_sym) == es.prev_position,
                    early_stop_cfg,
                ) {
                    store
                        .record_supervisor_note(&run.id, "guard", "info", &plan.reason)
                        .await?;
                    let es = early_stop_state.get_mut(&asset_sym).unwrap();
                    es.inherit_remaining = plan.skip_count;
                    es.recent_actions.clear();
                    es.recent_convictions.clear();
                }
            }
            if early_stop_state.get(&asset_sym).unwrap().inherit_remaining > 0 {
                let row = inherited_early_stop_row(&run.id, decision_idx, bar.timestamp, asset);
                store.record_decision(&row).await?;
                self.emit_chart(&run.id, RunChartEvent::Decision(LiveDecisionRow::from(&row)))
                    .await;
                self.emit(ProgressEvent::DecisionEmitted {
                    run_id: run.id.clone(),
                    action: "flat".into(),
                    asset: asset.to_string(),
                    size: 0.0,
                    conviction: 0.0,
                });
                let es = early_stop_state.get_mut(&asset_sym).unwrap();
                es.inherit_remaining -= 1;
                es.prev_position = book.position(asset_sym);
                return Ok(LiveDecisionOutcome {
                    input_tokens: 0,
                    output_tokens: 0,
                    fill_happened: false,
                    last_open_direction,
                    broker_error: None,
                    daily_loss_day,
                    daily_realized_at_day_start,
                    risk_vetoed: false,
                    filter_gated: false,
                });
            }
        }

        // Decision source: mechanistic rules or LLM pipeline.
        let pre_fill_position = book.position(asset_sym);
        let pre_fill_entry = book.entry_price(asset_sym);

        let (input_tokens, output_tokens, parsed) =
            if strategy.decision_mode == crate::strategies::DecisionMode::Mechanistic {
                let cfg = strategy
                    .mechanistic_config
                    .as_ref()
                    .expect("validate_strategy ensures mechanistic_config");
                let parsed = mechanistic_action(cfg, pre_fill_position, pre_fill_entry, bar.close);
                (0u64, 0u64, parsed)
            } else {
                // WS-17: open decision span for live observability.
                let span_id = crate::agent::observability::fresh_span_id();
                if let Some(obs) = self.obs_emitter.as_ref() {
                    obs.emit_decision_span_started(
                        &span_id,
                        None,
                        decision_idx as i64,
                        Some(asset),
                        Some(bar.timestamp),
                        Some(bar.close),
                        Some(pre_fill_position),
                        None,
                    )
                    .await;
                    let payload = serde_json::json!({
                        "decision_index": decision_idx,
                        "asset": asset,
                        "bar_ts": bar.timestamp.to_rfc3339(),
                    });
                    obs.emit_engine_event(
                        "decision_started",
                        Some(span_id.clone()),
                        Some(payload.to_string()),
                    )
                    .await;
                }

                let decision_model_span_id = self
                    .obs_emitter
                    .as_ref()
                    .map(|_| crate::agent::observability::fresh_span_id());
                if let (Some(obs), Some(dm_span_id)) =
                    (self.obs_emitter.as_ref(), decision_model_span_id.as_ref())
                {
                    let provider = trader_provider(agent_slots, strategy).unwrap_or_default();
                    let model = trader_model_id(agent_slots, strategy).unwrap_or_default();
                    obs.emit_model_call_started(
                        dm_span_id,
                        Some(span_id.clone()),
                        &provider,
                        &model,
                        Some("trader"),
                        None,
                        None,
                    )
                    .await;
                }

                let outs = run_pipeline(PipelineInputs {
                    strategy,
                    agent_slots,
                    seed_inputs: seed,
                    dispatch: dispatch.clone(),
                    tools: tools.clone(),
                    obs: self.obs_emitter.clone(),
                    memory_recorder: self.memory_recorder.clone(),
                    scenario_start: None,
                    source_window_start: Some(source_window_start),
                    source_window_end: Some(source_window_end),
                    run_id: run.id.clone(),
                    scenario_id: scenario.id.clone(),
                    cycle_idx: decision_idx as i64,
                    trace_attrs: None,
                    provider_catalogs: self.provider_catalogs.clone(),
                    filter_ctx: Some(crate::agent::pipeline::FilterPipelineCtx {
                        signal_cache,
                        bar_period_minutes,
                        multi_filter_config,
                        bar_ts: bar.timestamp,
                        strategy_id: strategy.manifest.id.clone(),
                        scope: crate::agent::dispatch_capability::SignalScope::Asset(asset_sym),
                    }),
                    recorder: self.recorder.as_deref(),
                    runtime: self.agent_runtime,
                    cline: self.cline.clone(),
                    model_call_span_id: decision_model_span_id.clone(),
                })
                .await;
                if let (Some(obs), Some(dm_span_id)) =
                    (self.obs_emitter.as_ref(), decision_model_span_id.as_ref())
                {
                    match &outs {
                        Ok(_) => obs.emit_span_finished_ok(dm_span_id).await,
                        Err(e) => obs.emit_span_finished_error(dm_span_id, &e.to_string()).await,
                    }
                }
                let outs = match outs {
                    Ok(outs) => outs,
                    Err(e) => {
                        if let Some(obs) = self.obs_emitter.as_ref() {
                            obs.emit_span_finished_error(&span_id, &e.to_string()).await;
                        }
                        return Err(e);
                    }
                };

                let input_tokens = outs.total_input_tokens as u64;
                let output_tokens = outs.total_output_tokens as u64;

                let trader = match outs.trader.as_ref() {
                    Some(t) => t,
                    None => {
                        if let Some(obs) = self.obs_emitter.as_ref() {
                            obs.emit_span_finished_error(&span_id, "missing trader response")
                                .await;
                        }
                        let err = TraderOutput::missing_response_error(&run.id, decision_idx);
                        return Err(err.into());
                    }
                };
                let trader_model_id = trader_model_id(agent_slots, strategy);
                let parsed = match TraderOutput::parse_response(trader, &run.id, decision_idx) {
                    Ok(p) => {
                        if let Some(obs) = self.obs_emitter.as_ref() {
                            obs.emit_span_finished_ok(&span_id).await;
                        }
                        p
                    }
                    Err(e) => {
                        if let Some(obs) = self.obs_emitter.as_ref() {
                            obs.emit_span_finished_error(&span_id, &e.to_string()).await;
                        }
                        let err = e.with_model_hint(trader_model_id.as_deref());
                        return Err(err.into());
                    }
                };
                (input_tokens, output_tokens, parsed)
            };

        // Pyramid-flip guardrail (F-7), shared with backtest.
        let original_action = GuardAction::parse(&parsed.action);
        let position_state = position_state_from_size(pre_fill_position);
        let decision = guardrails::classify(original_action, position_state, last_open_direction);
        let applied_action: String = match &decision {
            GuardrailDecision::Allow => parsed.action.clone(),
            GuardrailDecision::RewriteTo { action, reason } => {
                let note = supervisor_note_content(*reason, original_action, *action, asset, decision_idx);
                store
                    .record_supervisor_note(&run.id, "guard", "warn", &note)
                    .await?;
                action.as_str().to_string()
            }
        };

        // R3 risk-veto block (ported from backtest path).
        //
        // Only new opens (`long_open` / `short_open`) are subject to the
        // veto. Holds, flats, and guardrail-rewritten holds pass through
        // unchanged.
        //
        //   * daily_loss_kill_pct — once cumulative realized loss for the
        //     current UTC day exceeds this fraction of starting capital, no
        //     further opens are admitted for the rest of that day.
        //     (0.0 disables.)
        //   * max_concurrent_positions — caps the number of distinct assets
        //     holding an open position; a new open that would exceed the cap
        //     is vetoed. Re-opening / adjusting an asset that is already
        //     in-position is not blocked.
        // CT5: track whether this cycle fired a risk veto so the loop driver
        // can increment the monotonic `risk_veto_count` in `live_run_state`.
        let mut risk_vetoed = false;
        let applied_action: String = {
            let is_new_open = applied_action == "long_open" || applied_action == "short_open";
            if !is_new_open {
                applied_action
            } else {
                let initial = scenario.capital.initial;
                // Roll the realized-loss accumulator on a UTC-day boundary.
                let bar_day = bar.timestamp.date_naive();
                if daily_loss_day != Some(bar_day) {
                    daily_loss_day = Some(bar_day);
                    daily_realized_at_day_start = book.realized();
                }
                let kill_pct = strategy.risk.daily_loss_kill_pct;
                let realized_today = book.realized() - daily_realized_at_day_start;
                let daily_loss_breached = kill_pct > 0.0 && realized_today <= -(kill_pct * initial);

                let max_positions = strategy.risk.max_concurrent_positions;
                let open_positions = book.open_position_count();
                let already_open = book.position(asset_sym).abs() > f64::EPSILON;
                let max_positions_breached =
                    max_positions > 0 && !already_open && open_positions >= max_positions as usize;

                // Perps entry veto (venue-gated). `fill_sink.is_perp_venue()`
                // returns true only for directional-perps brokers (Orderly,
                // byreal/Hyperliquid, Bybit linear) — false on Alpaca and all
                // other spot venues, keeping the perps guards permanently inert
                // for spot runs. Funding rate comes from the named `perps_ctx`
                // binding above (all None until the out-of-band poller lands).
                // Liq-distance is not yet plumbed into the engine book
                // (follow-on track); pass None → that check no-ops.
                let is_perp_venue = fill_sink.is_perp_venue();
                let perps_funding_rate = perps_ctx.funding_rate;
                let direction = if applied_action == "short_open" {
                    xvision_core::trading::Direction::Short
                } else {
                    xvision_core::trading::Direction::Long
                };
                let perps_veto = crate::strategies::risk::perps::perps_entry_veto(
                    &strategy.risk,
                    is_perp_venue,
                    true, // is_new_open: this branch only runs for new opens
                    direction,
                    perps_funding_rate,
                    None,
                );

                let exposure_breached = {
                    let cap = strategy.risk.max_total_exposure_pct;
                    if cap > 0.0 {
                        let existing: f64 = book
                            .open_legs()
                            .iter()
                            .map(|(_, pos, _entry, mark)| pos.abs() * mark)
                            .sum();
                        let new_notional = {
                            let usd_at_risk = equity * strategy.risk.risk_pct_per_trade;
                            usd_at_risk.max(0.0)
                        };
                        crate::strategies::risk::perps::exceeds_total_exposure(
                            cap,
                            equity,
                            existing,
                            new_notional,
                        )
                    } else {
                        false
                    }
                };

                let breach_reason: Option<&str> = if daily_loss_breached {
                    Some("daily_loss_kill")
                } else if max_positions_breached {
                    Some("max_concurrent_positions")
                } else if exposure_breached {
                    Some("max_total_exposure")
                } else {
                    match perps_veto {
                        Some(xvision_core::trading::VetoReason::PunitiveFunding) => Some("punitive_funding"),
                        Some(xvision_core::trading::VetoReason::NearLiquidation) => Some("near_liquidation"),
                        _ => None,
                    }
                };

                if let Some(reason) = breach_reason {
                    let note = format!(
                        "risk veto `{reason}` at decision {decision_idx} ({asset}): \
                         open {applied_action} rewritten to hold \
                         (realized_today={realized_today:.2}, open_positions={open_positions})"
                    );
                    store
                        .record_supervisor_note(&run.id, "risk", "warn", &note)
                        .await?;
                    if let Some(obs) = self.obs_emitter.as_ref() {
                        let payload = serde_json::json!({
                            "decision_index": decision_idx,
                            "asset": asset,
                            "reason": reason,
                            "original": applied_action.as_str(),
                            "applied": "hold",
                        });
                        obs.emit_engine_event("risk_veto", None, Some(payload.to_string()))
                            .await;
                    }
                    risk_vetoed = true;
                    "hold".to_string()
                } else {
                    applied_action
                }
            }
        };

        // Submit through the live FillSink — UNLESS the applied action is
        // `hold`. A `hold` (including a guardrail pyramid-block rewrite of
        // `long_open`/`short_open` → `hold`) must leave the existing
        // position untouched. We CANNOT forward `hold` to the sink:
        // `RealBrokerFills` (like `simulate_fill_inner`) only no-ops `hold`
        // when already flat — with an open position it classifies `hold`
        // as `want_flat` and would CLOSE the position. The backtest path
        // guards this identically (`if applied_action == "hold" { no-op }`
        // before calling the sink), so we mirror it here.
        //
        // A1 per-run pause: when the run is paused (an ADDITIVE per-run gate
        // alongside the global SafetyManager pause), skip the broker submit
        // for this cycle and emit a no-op fill so the live run keeps iterating
        // without placing an order. Re-read per cycle so a pause issued
        // mid-run via `POST /api/eval/runs/:id/pause` is honored next cycle.
        //
        // FAIL CLOSED on the LIVE path: this is the real-broker (`RealBrokerFills`)
        // dispatch — a read error here (lock contention, pool exhaustion, I/O)
        // means we CANNOT confirm the run is unpaused. Submitting a real order
        // we couldn't clear is unsafe, so treat an unconfirmed state as paused
        // (`unwrap_or(true)`) and skip the submit. `is_paused` already
        // propagates transient errors (only the inert pre-061 missing-column
        // case returns Ok(false)), so this only trips on genuine read failures.
        let run_paused = store.is_paused(&run.id).await.unwrap_or(true);
        let fill: FillRecord = if applied_action == "hold" || run_paused {
            FillRecord {
                new_pos: pre_fill_position,
                new_entry: pre_fill_entry,
                fill_price: None,
                fill_size: None,
                fee: None,
                realized_pnl: 0.0,
                provenance: crate::eval::scenario::FillProvenance::default(),
                fill_branch: None,
                aggressor_side: None,
                order_state: None,
                broker_error: None,
                volume_cap_hit: None,
            }
        } else {
            fill_sink
                .submit(FillRequest {
                    pos: pre_fill_position,
                    entry: pre_fill_entry,
                    action: applied_action.clone(),
                    next_open,
                    bar_volume: bar.volume,
                    slip_bps: 0.0,
                    spread_bps: 0.0,
                    taker_bps: scenario.venue.fees.taker_bps as f64,
                    maker_bps: scenario.venue.fees.maker_bps as f64,
                    equity,
                    risk_pct: strategy.risk.risk_pct_per_trade,
                    slippage_model: scenario.venue.slippage.clone(),
                    fee_source: crate::eval::scenario::FeeSource::Default,
                    asset: asset.to_string(),
                    bar_ts: bar.timestamp,
                    bar_open: bar.open,
                    bar_high: bar.high,
                    bar_low: bar.low,
                    bar_close: bar.close,
                    decision_to_fill_ms: scenario.venue.latency.decision_to_fill_ms,
                    bar_duration_ms: u64::from(strategy.manifest.decision_cadence_minutes.max(1)) * 60_000,
                })
                .await
        };

        let broker_error = fill.broker_error.clone();

        // Apply the broker-reported fill to the pooled book.
        book.set_position(asset_sym, fill.new_pos, fill.new_entry);

        // Build SLTP state when a new position opens (parity with backtest L2808-2829).
        if fill.new_pos.abs() > f64::EPSILON && pre_fill_position.abs() <= f64::EPSILON {
            let direction = if fill.new_pos > 0.0 {
                xvision_core::trading::Direction::Long
            } else {
                xvision_core::trading::Direction::Short
            };
            let entry_atr = crate::eval::executor::sltp::compute_atr14(&history_slice);
            let state = build_live_sltp_state(
                direction,
                fill.new_entry,
                &parsed,
                wake_never,
                strategy.risk.stop_loss_atr_multiple,
                entry_atr,
            );
            sltp_state.insert(asset_sym, state);
        } else if fill.new_pos.abs() <= f64::EPSILON {
            sltp_state.remove(&asset_sym);
        }

        book.add_realized(fill.realized_pnl);
        let fill_happened = fill.fill_price.is_some();
        if fill_happened {
            let side = fill_side_for_action(&applied_action, pre_fill_position);
            self.emit(ProgressEvent::FillRecorded {
                run_id: run.id.clone(),
                side: side.into(),
                price: fill.fill_price.unwrap_or(0.0),
                qty: fill.fill_size.unwrap_or(0.0),
                fee: fill.fee.unwrap_or(0.0),
            });
        }

        self.emit(ProgressEvent::DecisionEmitted {
            run_id: run.id.clone(),
            action: parsed.action.clone(),
            asset: asset.to_string(),
            size: fill.fill_size.unwrap_or(0.0),
            conviction: parsed.conviction,
        });

        match GuardAction::parse(&applied_action) {
            GuardAction::LongOpen => last_open_direction = Some(GuardAction::LongOpen),
            GuardAction::ShortOpen => last_open_direction = Some(GuardAction::ShortOpen),
            GuardAction::Flat => last_open_direction = None,
            GuardAction::Hold | GuardAction::Other => {}
        }

        let decision_row = DecisionRow {
            run_id: run.id.clone(),
            decision_index: decision_idx,
            timestamp: decision_ts,
            asset: asset.to_string(),
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
            delayed: Some(delayed),
        };
        store.record_decision(&decision_row).await?;
        self.emit_chart(
            &run.id,
            RunChartEvent::Decision(LiveDecisionRow::from(&decision_row)),
        )
        .await;

        // Marker event (mirrors the backtest mapping).
        let t = decision_ts.timestamp();
        let marker_event = match applied_action.as_str() {
            "long_open" => fill.fill_price.zip(fill.fill_size).map(|(price, size)| {
                MarkerEvent::Trade(make_trade_marker(
                    TradeSide::Buy,
                    t,
                    price,
                    size,
                    fill.fee,
                    fill.realized_pnl,
                    decision_idx,
                    &parsed.justification,
                ))
            }),
            "short_open" | "flat" => fill.fill_price.zip(fill.fill_size).map(|(price, size)| {
                MarkerEvent::Trade(make_trade_marker(
                    TradeSide::Sell,
                    t,
                    price,
                    size,
                    fill.fee,
                    fill.realized_pnl,
                    decision_idx,
                    &parsed.justification,
                ))
            }),
            "hold" => Some(MarkerEvent::Hold(HoldMarker {
                time: t,
                price: next_open,
                conviction: Some(parsed.conviction),
                decision_index: decision_idx,
            })),
            _ => None,
        };
        if let Some(marker) = marker_event {
            self.emit_chart(&run.id, RunChartEvent::Marker(marker)).await;
        }

        // Push this decision as an episodic observation (parity with backtest).
        {
            let obs = crate::agent::episodic::EpisodicObservation::new(
                bar.timestamp.to_rfc3339(),
                decision_idx,
                parsed.action.clone(),
                parsed.conviction,
                Some(pre_fill_entry),
                Some(applied_action.clone()),
                parsed.justification.clone(),
                crate::agent::episodic::IndicatorSnapshot::default(),
            );
            episodic_store.push(obs);
        }

        // Update early-stop state with this decision's action and conviction.
        {
            let action = match parsed.action.as_str() {
                "flat" => crate::eval::early_stop::Action::Flat,
                "hold" => crate::eval::early_stop::Action::Hold,
                _ => crate::eval::early_stop::Action::Other,
            };
            let es = early_stop_state.get_mut(&asset_sym).unwrap();
            es.recent_actions.push(action);
            es.recent_convictions.push(parsed.conviction);
            es.prev_position = fill.new_pos;
            // Keep rolling window bounded
            let max_window = early_stop_cfg.window * 2;
            while es.recent_actions.len() > max_window {
                es.recent_actions.remove(0);
                es.recent_convictions.remove(0);
            }
        }
        Ok(LiveDecisionOutcome {
            input_tokens,
            output_tokens,
            fill_happened,
            last_open_direction,
            broker_error,
            daily_loss_day,
            daily_realized_at_day_start,
            risk_vetoed, // CT5: propagate to loop driver for live_run_state counter
            filter_gated: false,
        })
    }
    /// LIVE-only thin wrapper: flatten every open broker position before a
    /// cancelled run finishes. Delegates to [`flatten_open_positions`] with the
    /// `Cancel` reason and returns the [`FlattenOutcome`] (legs fully closed +
    /// whether any close fill landed); the caller recomputes equity on any fill
    /// and then `bail!`s to end the run as `Cancelled`. Behavior is unchanged
    /// from the original A2 implementation — the close logic now lives in the
    /// shared helper so the A3 one-shot flatten path can reuse it.
    #[allow(clippy::too_many_arguments)]
    async fn close_open_positions_on_cancel(
        &self,
        store: &RunStore,
        run: &Run,
        strategy: &Strategy,
        scenario: &Scenario,
        book: &mut crate::eval::executor::book::PortfolioBook,
        fill_sink: &mut RealBrokerFills,
        equity: f64,
        decision_idx: &mut u32,
    ) -> FlattenOutcome {
        self.flatten_open_positions(
            FlattenReason::Cancel,
            store,
            run,
            strategy,
            scenario,
            book,
            fill_sink,
            equity,
            decision_idx,
        )
        .await
    }

    /// LIVE-only: flatten every open broker position at market.
    ///
    /// Shared close path used by BOTH the A2 cancel-time flatten (via
    /// [`close_open_positions_on_cancel`], which calls this then `bail!`s the
    /// run to `Cancelled`) and the A3 one-shot "flatten positions" cockpit
    /// action (which calls this then CLEARS the request flag and CONTINUES the
    /// run). The only difference between the two callers is the `reason` (which
    /// shapes log/note/decision labels) and what the caller does AFTER — this
    /// helper itself never terminates the run.
    ///
    /// It closes each open leg through the SAME `RealBrokerFills` submit path
    /// that normal fills use (a `"flat"` `FillRequest` per asset, sized at
    /// `|pos|`), applies the broker-reported closing fill to the pooled `book`
    /// (settling realized PnL), and records a `flat` decision row + an equity
    /// sample — so realized PnL / equity settle consistently with a
    /// strategy-driven close.
    ///
    /// **Scope.** This is reachable ONLY from `run_inner_live`; the
    /// backtest / simulated path never calls it and keeps its no-broker-call
    /// behavior.
    ///
    /// **Robustness.** Best-effort and panic-free: if one asset's close errors
    /// (broker rejection surfaced as a `Rejected` `FillRecord`, or a store
    /// write failure) it is logged AND recorded as a run-level supervisor note
    /// (severity `warn`), then the routine continues flattening the remaining
    /// legs. The caller proceeds regardless — for cancel the run still ends
    /// `Cancelled`; for flatten the flag is still cleared (so a partial/failed
    /// close does NOT trap the loop re-flattening every cycle) — but the
    /// failure is visible, not swallowed. The asset set is the book's actual
    /// open legs (`PortfolioBook::open_legs`); flat legs are never submitted,
    /// and `RealBrokerFills` additionally no-ops a zero-position flat. A flat
    /// book is a no-op (returns 0, no broker calls).
    ///
    /// Returns a [`FlattenOutcome`] distinguishing "any close fill landed on
    /// the book" (`any_fill`, true for full OR partial closes) from "N legs
    /// fully flattened" (`fully_closed`). Callers recompute equity whenever a
    /// fill landed — a PARTIAL close settles realized PnL on the book just like
    /// a full close, so the equity/equity_curve must be refreshed even when no
    /// leg reached flat. `decision_idx` is advanced once per recorded closing
    /// decision so the `(run_id, decision_index)` PK never collides with the
    /// loop's rows.
    #[allow(clippy::too_many_arguments)]
    async fn flatten_open_positions(
        &self,
        reason: FlattenReason,
        store: &RunStore,
        run: &Run,
        strategy: &Strategy,
        scenario: &Scenario,
        book: &mut crate::eval::executor::book::PortfolioBook,
        fill_sink: &mut RealBrokerFills,
        equity: f64,
        decision_idx: &mut u32,
    ) -> FlattenOutcome {
        let open = book.open_legs();
        if open.is_empty() {
            return FlattenOutcome::default();
        }
        tracing::info!(
            target: "xvision_engine::live_executor",
            run_id = %run.id,
            open_legs = open.len(),
            reason = reason.tag(),
            "live flatten: flattening open broker positions"
        );

        let now = Utc::now();
        let mut closed = 0usize;
        let mut any_fill = false;
        for (asset_sym, pos, entry, last_mark) in open {
            let asset = asset_sym.as_alpaca_pair();
            // Reference price: the leg's last mark (falls back to entry inside
            // the book). A flat close is sized at |pos| regardless, and the
            // broker reports the true fill price; this only seeds the
            // OrderRequest reference + fallback.
            let reference = if last_mark > 0.0 && last_mark.is_finite() {
                last_mark
            } else {
                entry
            };
            let fill: FillRecord = fill_sink
                .submit(FillRequest {
                    pos,
                    entry,
                    action: "flat".to_string(),
                    next_open: reference,
                    bar_volume: 0.0,
                    slip_bps: 0.0,
                    spread_bps: 0.0,
                    taker_bps: scenario.venue.fees.taker_bps as f64,
                    maker_bps: scenario.venue.fees.maker_bps as f64,
                    equity,
                    risk_pct: strategy.risk.risk_pct_per_trade,
                    slippage_model: scenario.venue.slippage.clone(),
                    fee_source: crate::eval::scenario::FeeSource::Default,
                    asset: asset.clone(),
                    bar_ts: now,
                    bar_open: reference,
                    bar_high: reference,
                    bar_low: reference,
                    bar_close: reference,
                    decision_to_fill_ms: scenario.venue.latency.decision_to_fill_ms,
                    bar_duration_ms: u64::from(strategy.manifest.decision_cadence_minutes.max(1)) * 60_000,
                })
                .await;

            // A broker rejection surfaces as a no-fill `Rejected` record — the
            // position did NOT close. Log + record a run-level note and keep
            // flattening the other legs; the leg stays in the book so the
            // dangling exposure remains visible in the finished run.
            if let Some((class, msg)) = &fill.broker_error {
                tracing::error!(
                    target: "xvision_engine::live_executor",
                    run_id = %run.id,
                    asset = %asset,
                    reason = reason.tag(),
                    error_class = class.as_tag(),
                    error_message = %msg,
                    "live flatten: broker REJECTED position close; exposure may remain open"
                );
                let _ = store
                    .record_supervisor_note(
                        &run.id,
                        "executor",
                        "warn",
                        &format!(
                            "{}-close failed for {asset} [{}]: {msg} — position may remain open at the broker",
                            reason.tag(),
                            class.as_tag()
                        ),
                    )
                    .await;
                continue;
            }

            // Apply the broker-reported close to the pooled book (mirrors the
            // normal fill path: set the new position + settle realized PnL).
            book.set_position(asset_sym, fill.new_pos, fill.new_entry);
            book.add_realized(fill.realized_pnl);

            // A close that did NOT take the position to ~flat is a PARTIAL
            // fill: the broker reported a fill, but residual exposure remains
            // open. Use the same epsilon convention the book/loop use to
            // distinguish "flat" from "still holding". A partial must not be
            // labeled `flat` (it isn't), must not count toward `closed`, and
            // leaves the (reduced) leg in the book so the residual exposure
            // stays visible in the finished run.
            let fully_closed = fill.new_pos.abs() <= f64::EPSILON;
            let action_label = if fully_closed { "flat" } else { "flat_partial" };

            if fill.fill_price.is_some() {
                // ANY close fill — full OR partial — settled realized PnL on
                // the book above, so the caller must refresh equity. A partial
                // close does not increment `closed` (no leg reached flat) but
                // still flips `any_fill`.
                any_fill = true;
                if fully_closed {
                    closed += 1;
                }
                self.emit(ProgressEvent::FillRecorded {
                    run_id: run.id.clone(),
                    side: fill_side_for_action("flat", pos).into(),
                    price: fill.fill_price.unwrap_or(0.0),
                    qty: fill.fill_size.unwrap_or(0.0),
                    fee: fill.fee.unwrap_or(0.0),
                });
            }

            // For a partial, surface the residual exposure as a run-level warn
            // note (mirrors the rejection branch's "exposure may remain open"
            // signal) so the cancelled run's ledger is honest about it.
            if !fully_closed {
                tracing::warn!(
                    target: "xvision_engine::live_executor",
                    run_id = %run.id,
                    asset = %asset,
                    reason = reason.tag(),
                    residual_pos = fill.new_pos,
                    "live flatten: close only PARTIALLY filled; residual exposure remains open"
                );
                let _ = store
                    .record_supervisor_note(
                        &run.id,
                        "executor",
                        "warn",
                        &format!(
                            "{}-close for {asset} only partially filled (residual position {:.8}) — exposure remains open at the broker",
                            reason.tag(),
                            fill.new_pos
                        ),
                    )
                    .await;
            }

            // Record the closing decision row + chart event so the cancelled
            // run's ledger settles consistently with a strategy-driven flat.
            // A partial is recorded as `flat_partial` (distinct from a full
            // `flat`) with the realized PnL on the portion that DID close.
            let decision_row = DecisionRow {
                run_id: run.id.clone(),
                decision_index: *decision_idx,
                timestamp: now,
                asset: asset.clone(),
                action: action_label.to_string(),
                conviction: None,
                justification: Some(if fully_closed {
                    format!("{}: flatten open position", reason.tag())
                } else {
                    format!("{}: partial flatten (residual exposure remains)", reason.tag())
                }),
                reasoning: Some(if fully_closed {
                    format!("{}: flatten open position", reason.tag())
                } else {
                    format!("{}: partial flatten (residual exposure remains)", reason.tag())
                }),
                order_size: fill.fill_size,
                fill_price: fill.fill_price,
                fill_size: fill.fill_size,
                fee: fill.fee,
                pnl_realized: if fill.realized_pnl != 0.0 {
                    Some(fill.realized_pnl)
                } else {
                    None
                },
                delayed: Some(false),
            };
            if let Err(e) = store.record_decision(&decision_row).await {
                // The broker DID close the position (book already settled);
                // only the local audit row failed. Surface it but do not
                // unwind the close.
                tracing::error!(
                    target: "xvision_engine::live_executor",
                    run_id = %run.id,
                    asset = %asset,
                    reason = reason.tag(),
                    error = %e,
                    "live flatten: closed position but failed to persist the closing decision row"
                );
                let _ = store
                    .record_supervisor_note(
                        &run.id,
                        "executor",
                        "warn",
                        &format!(
                            "{}-close for {asset} filled but its decision row failed to persist: {e}",
                            reason.tag()
                        ),
                    )
                    .await;
            } else {
                self.emit_chart(
                    &run.id,
                    RunChartEvent::Decision(LiveDecisionRow::from(&decision_row)),
                )
                .await;
            }
            *decision_idx += 1;
        }

        // Record one final equity sample at the post-flatten NAV so the
        // cancelled run's equity curve ends on the settled value.
        let marks = std::collections::BTreeMap::new();
        let settled_equity = book.equity(&marks);
        if let Err(e) = store.record_equity_upsert(&run.id, now, settled_equity).await {
            tracing::warn!(
                target: "xvision_engine::live_executor",
                run_id = %run.id,
                reason = reason.tag(),
                error = %e,
                "live flatten: failed to record post-flatten equity sample"
            );
        }
        self.emit_chart(
            &run.id,
            RunChartEvent::Equity(ChartEquityPoint {
                time: now.timestamp(),
                equity_usd: settled_equity,
            }),
        )
        .await;

        FlattenOutcome {
            fully_closed: closed,
            any_fill,
        }
    }
}

/// Outcome of [`Executor::flatten_open_positions`]. Distinguishes "a close
/// fill landed on the book" from "a leg reached flat" so callers refresh
/// equity on ANY settled fill (full or partial), not only on a full flatten.
///
/// `any_fill` is true whenever at least one broker close fill was applied to
/// the book — including a PARTIAL fill that left residual exposure (the
/// `flat_partial` path), which still settles realized PnL. `fully_closed`
/// counts only legs that reached flat. The A2 cancel-close wrapper exposes
/// `fully_closed` as its leg count; both A2 and A3 callers recompute
/// equity + push to the equity curve whenever `any_fill` is true.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FlattenOutcome {
    /// Number of legs that produced a FULL closing fill (reached flat).
    fully_closed: usize,
    /// True iff at least one broker close fill (full OR partial) was applied
    /// to the book, so realized PnL changed and equity must be recomputed.
    any_fill: bool,
}

/// Why the live executor is flattening open positions. Shapes the log /
/// supervisor-note / decision-row labels emitted by
/// [`Executor::flatten_open_positions`]; it does NOT change the close
/// mechanics. `Cancel` is the A2 cancel-time flatten (caller `bail!`s after);
/// `Flatten` is the A3 one-shot cockpit "flatten positions" action (caller
/// clears the request flag and keeps the run running).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlattenReason {
    /// A2: the run is being cancelled — flatten then end the run.
    Cancel,
    /// A3: a one-shot flatten was requested — flatten then keep running.
    Flatten,
}

impl FlattenReason {
    /// Short tag used in logs, supervisor notes, and the closing decision
    /// row's justification/reasoning (e.g. `"cancel: flatten open position"`
    /// vs `"flatten: flatten open position"`).
    fn tag(self) -> &'static str {
        match self {
            FlattenReason::Cancel => "cancel",
            FlattenReason::Flatten => "flatten",
        }
    }
}

/// Reason the live loop terminated on a [`StopPolicy`] limit, or `None`
/// when no limit has fired. `decision_count` is the number of decisions
/// recorded so far; `bar_count` the number of bars consumed; `trade_count`
/// the number of completed fills; `started` the wall-clock anchor for
/// `time_limit_secs`.
fn live_stop_reason(
    policy: &crate::eval::live_config::StopPolicy,
    bar_count: u32,
    decision_count: u32,
    trade_count: u32,
    started: Instant,
) -> Option<String> {
    if let Some(lim) = policy.bar_limit {
        if bar_count >= lim {
            return Some(format!("bar_limit {lim} reached"));
        }
    }
    if let Some(lim) = policy.decision_limit {
        if decision_count >= lim {
            return Some(format!("decision_limit {lim} reached"));
        }
    }
    if let Some(lim) = policy.trade_limit {
        if trade_count >= lim {
            return Some(format!("trade_limit {lim} reached"));
        }
    }
    if let Some(secs) = policy.time_limit_secs {
        if started.elapsed().as_secs() >= secs {
            return Some(format!("time_limit_secs {secs} reached"));
        }
    }
    None
}

/// Inputs to one live per-(asset, bar) decision cycle. Bundled into a
/// struct because the body needs a wide-but-borrowed context and Clippy
/// (rightly) rejects a ~20-argument async fn.
struct DecideOneLiveCtx<'a> {
    run: &'a Run,
    strategy: &'a Strategy,
    scenario: &'a Scenario,
    agent_slots: &'a [ResolvedAgentSlot],
    dispatch: Arc<dyn LlmDispatch>,
    tools: Arc<ToolRegistry>,
    store: &'a RunStore,
    asset_sym: xvision_core::trading::AssetSymbol,
    asset: &'a str,
    active_venue_symbols: &'a [String],
    bar: &'a Ohlcv,
    decision_ts: chrono::DateTime<chrono::Utc>,
    decision_idx: u32,
    equity: f64,
    inputs_policy: InputsPolicy,
    bar_history_limit: Option<u32>,
    history_window: usize,
    history: &'a [Ohlcv],
    supported_timeframes: &'a [String],
    last_open_direction: Option<GuardAction>,
    bar_period_minutes: u32,
    signal_cache: &'a mut crate::agent::signal_cache::SignalCache,
    multi_filter_config: crate::agent::filter_dispatch::MultiFilterConfig,
    /// R3 risk-veto: UTC day the daily-loss accumulator was last rolled (None
    /// = not yet seen). Passed in from the loop driver's run-level state and
    /// returned (possibly updated) via `LiveDecisionOutcome`.
    daily_loss_day: Option<chrono::NaiveDate>,
    /// R3 risk-veto: the book's realized-PnL snapshot taken at the start of
    /// the current UTC day.  `realized_today = book.realized() - this`.
    daily_realized_at_day_start: f64,
    /// Per-bar filter hook for strategy-level gating. `None` for `EveryBar`
    /// strategies (the default); when `Some`, `decide_one_live` evaluates
    /// it before `run_pipeline` and skips when the filter says "not active".
    filter_hook: Option<&'a mut crate::eval::filter_hook::FilterHook>,
    /// In-memory episodic store shared across all decisions in this live run.
    episodic_store: &'a mut crate::agent::episodic::EpisodicStore,
    /// Per-asset SLTP state for deterministic stop-loss / take-profit.
    sltp_state: &'a mut std::collections::BTreeMap<
        xvision_core::trading::AssetSymbol,
        crate::eval::executor::sltp::PositionRiskState,
    >,
    /// True when the decision's bar age exceeds the cadence period
    /// (agent took >1 bar to respond). Only set in live/forward-test
    /// mode; always false in backtest.
    delayed: bool,
    wake_never: bool,
    short_bars_held: &'a mut std::collections::BTreeMap<xvision_core::trading::AssetSymbol, u32>,
    default_slip_bps: f64,
    default_taker_bps: f64,
    bar_secs: u64,
    /// Per-asset early-stop state for flat-degeneracy detection.
    early_stop_state: &'a mut std::collections::BTreeMap<xvision_core::trading::AssetSymbol, EarlyStopState>,
    /// Early-stop policy configuration (env-driven, read once at run start).
    early_stop_cfg: &'a EarlyStopConfig,
}

/// What `decide_one_live` returns to the loop driver.
struct LiveDecisionOutcome {
    input_tokens: u64,
    output_tokens: u64,
    fill_happened: bool,
    last_open_direction: Option<GuardAction>,
    broker_error: Option<(xvision_execution::broker_surface::BrokerErrorClass, String)>,
    /// R3 risk-veto: updated daily-loss state, written back to the loop
    /// driver so the accumulator persists across consecutive `decide_one_live`
    /// calls on the same run.
    daily_loss_day: Option<chrono::NaiveDate>,
    daily_realized_at_day_start: f64,
    /// CT5: true when the risk gate vetoed an open this decision cycle.
    /// The loop driver increments `risk_veto_count` and persists it to
    /// `live_run_state` each bar.
    pub(crate) risk_vetoed: bool,
    /// True when the DSL filter gate suppressed this bar (cooldown,
    /// daily cap, or position suppression). The loop driver skips
    /// token counting, fill tracking, and mark-to-market for gated bars.
    pub(crate) filter_gated: bool,
}

/// Per-asset early-stop state for flat-degeneracy detection.
/// Each asset has its own streak buffer and skip window so a flat
/// run on one asset cannot suppress decisions on another.
#[derive(Default)]
struct EarlyStopState {
    recent_actions: Vec<crate::eval::early_stop::Action>,
    recent_convictions: Vec<f64>,
    inherit_remaining: u32,
    prev_position: f64,
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

/// Apply mechanistic close policies to produce a `TraderOutput` without any LLM
/// call. Returns `flat` when a StopLoss or TakeProfit threshold is breached;
/// returns `hold` when flat or no policy triggers.
fn mechanistic_action(
    cfg: &MechanisticConfig,
    position: f64,
    entry_price: f64,
    mark_price: f64,
) -> TraderOutput {
    if position.abs() < f64::EPSILON || entry_price <= 0.0 {
        return TraderOutput {
            action: "hold".into(),
            conviction: 0.0,
            justification: "mechanistic: no open position".into(),
            ..Default::default()
        };
    }
    let pnl_pct = if position > 0.0 {
        (mark_price - entry_price) / entry_price * 100.0
    } else {
        (entry_price - mark_price) / entry_price * 100.0
    };
    for policy in &cfg.close_policies {
        match policy {
            ClosePolicy::StopLoss { pct } if pnl_pct <= -*pct => {
                return TraderOutput {
                    action: "flat".into(),
                    conviction: 1.0,
                    justification: format!("mechanistic: stop-loss ({pnl_pct:.2}% <= -{pct:.2}%)"),
                    ..Default::default()
                };
            }
            ClosePolicy::TakeProfit { pct } if pnl_pct >= *pct => {
                return TraderOutput {
                    action: "flat".into(),
                    conviction: 1.0,
                    justification: format!("mechanistic: take-profit ({pnl_pct:.2}% >= {pct:.2}%)"),
                    ..Default::default()
                };
            }
            _ => {}
        }
    }
    TraderOutput {
        action: "hold".into(),
        conviction: 0.0,
        justification: "mechanistic: no close policy triggered".into(),
        ..Default::default()
    }
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

/// Find the trader slot's configured provider for the WS-17
/// `decision.model` span attributes. Mirrors [`trader_model_id`]'s
/// resolution order (attached agent with canonical role `trader`, then
/// the legacy `strategy.trader_slot`). Returns `None` when the slot
/// left `provider` unset — the span then records no provider, matching
/// the `model_call`-span behaviour on the retired LlmDispatch path.
fn trader_provider(agent_slots: &[ResolvedAgentSlot], strategy: &Strategy) -> Option<String> {
    if let Some(resolved) = agent_slots.iter().find(|r| canonical_role(&r.role) == "trader") {
        if let Some(p) = resolved.slot.provider.as_deref() {
            let p = p.trim();
            if !p.is_empty() {
                return Some(p.to_string());
            }
        }
    }
    if let Some(slot) = strategy.trader_slot.as_ref() {
        if let Some(p) = slot.provider.as_deref() {
            let p = p.trim();
            if !p.is_empty() {
                return Some(p.to_string());
            }
        }
    }
    None
}

/// Role label for the trader position, used to attribute the
/// `invalid_output_schema` guardrail short-circuit to a slot. Prefers the
/// attached agent with canonical role `trader`; falls back to the literal
/// `"trader"` when no attached slot matches (legacy strategies).
fn trader_role_label(agent_slots: &[ResolvedAgentSlot]) -> String {
    agent_slots
        .iter()
        .find(|r| canonical_role(&r.role) == "trader")
        .map(|r| r.role.clone())
        .unwrap_or_else(|| "trader".to_string())
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

/// Compute the borrow cost for a closed short position.
///
/// `abs_pos * entry * borrow_bps_per_day / 10000 / bars_per_day * bars_held`
///
/// Returns 0.0 for any zero-value input (no bars held, zero entry, etc.).
/// Deterministic: pure function of its inputs, no side effects.
fn compute_borrow_cost(
    abs_pos: f64,
    entry: f64,
    borrow_bps_per_day: f64,
    bars_held: u32,
    bar_secs: u64,
) -> f64 {
    if bars_held == 0 || abs_pos == 0.0 || entry == 0.0 || bar_secs == 0 {
        return 0.0;
    }
    let bars_per_day = 86_400.0 / bar_secs as f64;
    let daily_cost = abs_pos * entry * borrow_bps_per_day / 10_000.0;
    daily_cost * bars_held as f64 / bars_per_day
}

fn live_filter_gate_should_short_circuit(
    filter_gated: bool,
    in_position: bool,
    has_sltp_state: bool,
) -> bool {
    filter_gated && !(in_position && has_sltp_state)
}

fn sanitize_prior_episodes_for_policy(
    episodes_json: serde_json::Value,
    inputs_policy: InputsPolicy,
) -> serde_json::Value {
    if inputs_policy != InputsPolicy::Causal {
        return episodes_json;
    }
    if let serde_json::Value::Array(arr) = episodes_json {
        let cleaned: Vec<serde_json::Value> = arr
            .into_iter()
            .map(|mut ep| {
                if let serde_json::Value::Object(ref mut m) = ep {
                    m.remove("bar_timestamp");
                    m.remove("decision_idx");
                }
                ep
            })
            .collect();
        serde_json::Value::Array(cleaned)
    } else {
        episodes_json
    }
}

fn live_sltp_close_units(pos: f64, fraction: f64) -> f64 {
    pos.abs() * fraction.clamp(0.0, 1.0)
}

fn live_sltp_remaining_position(pos: f64, close_units: f64) -> f64 {
    let remaining_abs = (pos.abs() - close_units).max(0.0);
    remaining_abs.copysign(pos)
}

fn live_sltp_close_request(
    asset: &str,
    pos: f64,
    entry: f64,
    next_open: f64,
    taker_bps: f64,
    equity: f64,
    bar_duration_minutes: u64,
) -> FillRequest {
    FillRequest {
        pos,
        entry,
        action: "flat".to_string(),
        next_open,
        bar_volume: f64::INFINITY,
        slip_bps: 0.0,
        spread_bps: 0.0,
        taker_bps,
        maker_bps: 0.0,
        equity,
        risk_pct: 0.0,
        slippage_model: crate::eval::scenario::SlippageModel::None,
        fee_source: crate::eval::scenario::FeeSource::Default,
        asset: asset.to_string(),
        bar_ts: chrono::Utc::now(),
        bar_open: next_open,
        bar_high: next_open,
        bar_low: next_open,
        bar_close: next_open,
        decision_to_fill_ms: 0,
        bar_duration_ms: bar_duration_minutes.max(1) * 60_000,
    }
}

fn live_sltp_exit_outcome(
    fill_happened: bool,
    risk_vetoed: bool,
    broker_error: Option<(xvision_execution::broker_surface::BrokerErrorClass, String)>,
    last_open_direction: Option<GuardAction>,
) -> LiveDecisionOutcome {
    LiveDecisionOutcome {
        input_tokens: 0,
        output_tokens: 0,
        fill_happened,
        last_open_direction,
        broker_error,
        daily_loss_day: None,
        daily_realized_at_day_start: 0.0,
        risk_vetoed,
        filter_gated: false,
    }
}

fn build_live_sltp_state(
    direction: xvision_core::trading::Direction,
    entry_price: f64,
    parsed: &TraderOutput,
    wake_never: bool,
    config_atr_mult: f64,
    entry_atr: Option<f64>,
) -> crate::eval::executor::sltp::PositionRiskState {
    let sl_pct = if wake_never {
        0.0
    } else {
        parsed.stop_loss_pct.map(|v| v as f64).unwrap_or(0.0)
    };
    let tp_pct = if wake_never {
        0.0
    } else {
        parsed.take_profit_pct.map(|v| v as f64).unwrap_or(0.0)
    };
    let model_sl_atr_mult = if wake_never { None } else { parsed.sl_atr_mult };
    let model_tp_atr_mult = if wake_never { None } else { parsed.tp_atr_mult };
    let effective_sl_atr_mult = model_sl_atr_mult.or_else(|| {
        if sl_pct <= 0.0 && config_atr_mult > 0.0 {
            Some(config_atr_mult)
        } else {
            None
        }
    });
    let (
        trailing_stop_pct,
        breakeven_trigger_pct,
        breakeven_offset_pct,
        fade_sl_bars,
        fade_sl_start_pct,
        fade_sl_end_pct,
        max_bars_held,
        tp1_pct,
        tp1_close_fraction,
        tp2_pct,
    ) = if wake_never {
        (None, None, None, None, None, None, None, None, None, None)
    } else {
        (
            parsed.trailing_stop_pct,
            parsed.breakeven_trigger_pct,
            parsed.breakeven_offset_pct,
            parsed.fade_sl_bars,
            parsed.fade_sl_start_pct,
            parsed.fade_sl_end_pct,
            parsed.max_bars_held,
            parsed.tp1_pct,
            parsed.tp1_close_fraction,
            parsed.tp2_pct,
        )
    };
    crate::eval::executor::sltp::PositionRiskState::new(
        direction,
        entry_price,
        sl_pct,
        tp_pct,
        entry_atr,
        trailing_stop_pct,
        breakeven_trigger_pct,
        breakeven_offset_pct,
        fade_sl_bars,
        fade_sl_start_pct,
        fade_sl_end_pct,
        max_bars_held,
        effective_sl_atr_mult,
        model_tp_atr_mult,
        tp1_pct,
        tp1_close_fraction,
        tp2_pct,
    )
}

fn inherited_early_stop_row(
    run_id: &str,
    decision_idx: u32,
    timestamp: chrono::DateTime<chrono::Utc>,
    asset: &str,
) -> DecisionRow {
    DecisionRow {
        run_id: run_id.to_string(),
        decision_index: decision_idx,
        timestamp,
        asset: asset.to_string(),
        action: "flat".into(),
        conviction: Some(0.0),
        justification: Some("inherited from early-stop policy".into()),
        reasoning: None,
        order_size: None,
        fill_price: None,
        fill_size: None,
        fee: None,
        pnl_realized: None,
        delayed: Some(false),
    }
}

/// Compute fill price, fee, and realized PnL for a full SLTP exit.
///
/// Returns `(pnl, fee)`. Slippage is adverse to the exiting side:
/// long exits fill slightly below `next_open`, short exits slightly above.
fn apply_sltp_full_exit(
    position: f64,
    entry_price: f64,
    next_open: f64,
    slip_bps: f64,
    taker_bps: f64,
) -> (f64, f64) {
    debug_assert!(
        position.abs() > 0.0,
        "apply_sltp_full_exit called with zero position"
    );
    let direction_sign = if position > 0.0 { 1.0_f64 } else { -1.0 };
    let fill_price = next_open * (1.0 - slip_bps / 10_000.0 * direction_sign);
    let fee = position.abs() * fill_price * taker_bps / 10_000.0;
    let pnl = position * (fill_price - entry_price) - fee;
    (pnl, fee)
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
/// F36: how often the executor flushes partial metrics+tokens to the DB during
/// a run, so an interrupt loses at most this much progress. Bounds the periodic
/// recompute cost regardless of run length.
const PARTIAL_PERSIST_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

/// U5: how often the backtest decision loop emits a `ProgressEvent::EvalHeartbeat`
/// so a live subscriber (CLI watch, dashboard SSE, optimizer cycle re-emit
/// bridge) sees forward progress during a long, otherwise-silent backtest.
/// Checked on wall-clock time at the top of the loop, independent of the
/// strategy cadence early-continue.
const EVAL_HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// F36: snapshot the accumulators into a partial [`MetricsSummary`] and persist
/// it (best-effort, no status change) so a cancelled/timed-out/crashed run keeps
/// the metrics+tokens it had so far instead of leaving `metrics_json = NULL`.
/// Called right before each cancel-bail and on the periodic timer. `baselines`
/// is `None` here (recomputed only at clean finish — keeps the snapshot cheap).
#[allow(clippy::too_many_arguments)]
async fn persist_partial_snapshot(
    store: &RunStore,
    run_id: &str,
    equity_curve: &[f64],
    initial: f64,
    equity: f64,
    cadence_minutes: u32,
    realized_count: u32,
    wins: u32,
    n_trades: u32,
    decision_idx: u32,
    input_tokens: u64,
    output_tokens: u64,
) {
    let metrics = compute_run_metrics(
        equity_curve,
        initial,
        equity,
        cadence_minutes,
        realized_count,
        wins,
        n_trades,
        decision_idx,
        None,
    );
    let _ = store
        .persist_partial(run_id, &metrics, input_tokens, output_tokens)
        .await;
}

/// Compute a [`MetricsSummary`] from the live accumulators. Extracted (F36) so
/// the same computation feeds both the final `finalize` write and the periodic
/// "capture-on-interrupt" partial persists — a cancelled/timed-out run records
/// the metrics it accumulated instead of `NULL`. `baselines` is passed by the
/// caller: the backtest finalize supplies the four computed baselines; live runs
/// and the cheap periodic snapshots pass `None` (baselines are recomputed only
/// at clean finish).
#[allow(clippy::too_many_arguments)]
fn compute_run_metrics(
    equity_curve: &[f64],
    initial: f64,
    equity: f64,
    cadence_minutes: u32,
    realized_count: u32,
    wins: u32,
    n_trades: u32,
    decision_idx: u32,
    baselines: Option<BaselinesReport>,
) -> MetricsSummary {
    let returns = equity_to_returns(equity_curve);
    let periods_per_year = annualization_periods_per_year(cadence_minutes);
    MetricsSummary {
        total_return_pct: total_return_pct(initial, equity),
        sharpe: sharpe_from_returns(&returns, periods_per_year),
        max_drawdown_pct: max_drawdown_pct(equity_curve),
        win_rate: if realized_count > 0 {
            wins as f64 / realized_count as f64
        } else {
            0.0
        },
        n_trades,
        n_decisions: decision_idx,
        baselines,
        ..Default::default()
    }
}

fn build_baselines_report(
    bars: &[Ohlcv],
    initial_equity: f64,
    cadence_minutes: u32,
    strategy_return_pct: f64,
) -> BaselinesReport {
    let computed =
        bar_baselines::compute_baselines(bars, initial_equity, cadence_minutes, strategy_return_pct, 42);
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
        random_direction: BaselineMetrics {
            return_pct: computed.random_direction.return_pct,
            sharpe: computed.random_direction.sharpe,
        },
        relative_to: BaselineRelative {
            buy_hold: computed.relative_to.buy_hold,
            always_flat: computed.relative_to.always_flat,
            simple_trend: computed.relative_to.simple_trend,
            simple_mean_reversion: computed.relative_to.simple_mean_reversion,
            random_direction: computed.relative_to.random_direction,
        },
    }
}

/// Perpetual-futures context threaded into the trader's `market_data`.
/// All fields optional — only the populated ones are emitted, and the whole
/// `perps` object is omitted (`null`) when nothing is set. Backtest passes
/// the default (all `None`); the live path attaches an out-of-band perps
/// feed reading (see `xvision_data::perp_feed`).
#[derive(Debug, Clone, Copy, Default)]
pub struct PerpsContext {
    pub funding_rate: Option<f64>,
    pub open_interest: Option<f64>,
    pub mark_index_basis: Option<f64>,
    pub long_short_ratio: Option<f64>,
}

impl PerpsContext {
    fn is_empty(&self) -> bool {
        self.funding_rate.is_none()
            && self.open_interest.is_none()
            && self.mark_index_basis.is_none()
            && self.long_short_ratio.is_none()
    }

    /// JSON object with only the populated fields, or `null` when empty so the
    /// trader prompt builder can skip it (mirrors the indicator-panel pattern).
    fn to_json(self) -> serde_json::Value {
        if self.is_empty() {
            return serde_json::Value::Null;
        }
        let mut obj = serde_json::Map::new();
        if let Some(v) = self.funding_rate {
            obj.insert("funding_rate".into(), serde_json::json!(v));
        }
        if let Some(v) = self.open_interest {
            obj.insert("open_interest".into(), serde_json::json!(v));
        }
        if let Some(v) = self.mark_index_basis {
            obj.insert("mark_index_basis".into(), serde_json::json!(v));
        }
        if let Some(v) = self.long_short_ratio {
            obj.insert("long_short_ratio".into(), serde_json::json!(v));
        }
        serde_json::Value::Object(obj)
    }
}

/// Input for [`build_decision_seed`], the production seed payload builder
/// shared by backtest/live execution and integration tests.
pub struct DecisionSeedInput<'a> {
    pub decision_idx: u32,
    pub asset: &'a str,
    pub active_assets: &'a [String],
    pub bar: &'a Ohlcv,
    pub next_bar_open: f64,
    pub reference_price_source: &'a str,
    pub position_size: f64,
    pub equity: f64,
    pub mark_price: f64,
    pub history_slice: &'a [&'a Ohlcv],
    pub inputs_policy: InputsPolicy,
    pub supported_timeframes: &'a [String],
    pub last_closed_times: std::collections::BTreeMap<String, chrono::DateTime<chrono::Utc>>,
    /// Entry price of the current open position; `0.0` when flat.
    pub entry_price: f64,
    /// Unrealised PnL as a percentage of entry; `0.0` when flat.
    pub unrealized_pnl_pct: f64,
    /// Number of bars the current position has been held; `0` when flat.
    pub bars_held: u32,
    /// Effective stop-loss price from the SLTP state; `0.0` when none active.
    pub stop_loss_price: f64,
    /// Effective take-profit price from the SLTP state; `0.0` when none active.
    pub take_profit_price: f64,
    /// The strategy's live, typed risk configuration. Injected into the seed so
    /// the trader/risk agents read the authoritative params from typed config
    /// rather than hand-written prompt text that drifts when the optimizer
    /// mutates `risk.*` (xvision-yzk).
    pub risk_config: &'a RiskConfig,
    /// Perps context (funding/OI/basis/long-short). Default (all `None`) on the
    /// spot/backtest path; populated live from `xvision_data::perp_feed`.
    pub perps: PerpsContext,
}

/// Raw per-decision executor state, *before* seed derivations. Both the
/// backtest and live decision loops build this and route through
/// [`DecisionSeedInput::from_context`], so the unrealized-PnL and
/// entry-price-when-flat derivations live in exactly one place — the live and
/// backtest paths cannot silently drift on them.
///
/// Fields here are the raw values the executor knows (signed book position,
/// raw book entry price, mark, equity, …). The derived `unrealized_pnl_pct`
/// and the flat-zeroed `entry_price` that the trader actually sees are computed
/// in `from_context`, never at the call-site.
pub struct SeedContext<'a> {
    pub decision_idx: u32,
    pub asset: &'a str,
    pub active_assets: &'a [String],
    pub bar: &'a Ohlcv,
    pub history_slice: &'a [&'a Ohlcv],
    pub inputs_policy: InputsPolicy,
    pub supported_timeframes: &'a [String],
    pub last_closed_times: std::collections::BTreeMap<String, chrono::DateTime<chrono::Utc>>,
    pub equity: f64,
    /// Signed position size from the book (`>0` long, `<0` short, `~0` flat).
    pub position_size: f64,
    /// Raw entry price from the book; may be stale when flat (the derivation
    /// zeroes it for the trader's view).
    pub entry_price: f64,
    pub mark_price: f64,
    pub next_bar_open: f64,
    /// Label distinguishing the reference-price origin (`"eval_bar.close"` vs
    /// `"live_bar.close"`). One of the two intentionally-divergent seed fields.
    pub reference_price_source: &'a str,
    pub bars_held: u32,
    pub stop_loss_price: f64,
    pub take_profit_price: f64,
    pub risk_config: &'a RiskConfig,
    pub perps: PerpsContext,
}

impl<'a> DecisionSeedInput<'a> {
    /// Build the seed input from raw executor state, deriving
    /// `unrealized_pnl_pct` and the flat-zeroed `entry_price` once. This is the
    /// single shared constructor both decision paths route through.
    pub fn from_context(ctx: SeedContext<'a>) -> Self {
        // Unrealized PnL %: flat (or no valid entry) → 0; long → (mark-entry);
        // short → (entry-mark). Single source of truth for both paths.
        let unrealized_pnl_pct = if ctx.position_size.abs() < f64::EPSILON || ctx.entry_price <= 0.0 {
            0.0
        } else if ctx.position_size > f64::EPSILON {
            (ctx.mark_price - ctx.entry_price) / ctx.entry_price * 100.0
        } else {
            (ctx.entry_price - ctx.mark_price) / ctx.entry_price * 100.0
        };
        // The trader sees entry_price 0 when flat (no open position).
        let entry_price = if ctx.position_size.abs() > f64::EPSILON {
            ctx.entry_price
        } else {
            0.0
        };
        DecisionSeedInput {
            decision_idx: ctx.decision_idx,
            asset: ctx.asset,
            active_assets: ctx.active_assets,
            bar: ctx.bar,
            next_bar_open: ctx.next_bar_open,
            reference_price_source: ctx.reference_price_source,
            position_size: ctx.position_size,
            equity: ctx.equity,
            mark_price: ctx.mark_price,
            history_slice: ctx.history_slice,
            inputs_policy: ctx.inputs_policy,
            supported_timeframes: ctx.supported_timeframes,
            last_closed_times: ctx.last_closed_times,
            entry_price,
            unrealized_pnl_pct,
            bars_held: ctx.bars_held,
            stop_loss_price: ctx.stop_loss_price,
            take_profit_price: ctx.take_profit_price,
            risk_config: ctx.risk_config,
            perps: ctx.perps,
        }
    }
}

/// Path-specific inputs to [`build_seed_context`] — the fields the shared
/// prologue cannot derive from the book + current bar. The backtest and live
/// loops legitimately differ on these (history-slice source, next-open
/// reference, the reference-price label, and — until the live SLTP gap is
/// closed — the stop/target/age triple); everything else is derived once, in
/// one place, so the two `SeedContext`s cannot silently drift.
struct SeedContextParams<'a> {
    decision_idx: u32,
    asset: &'a str,
    active_assets: &'a [String],
    history_slice: &'a [&'a Ohlcv],
    inputs_policy: InputsPolicy,
    supported_timeframes: &'a [String],
    last_closed_times: std::collections::BTreeMap<String, chrono::DateTime<chrono::Utc>>,
    equity: f64,
    next_bar_open: f64,
    reference_price_source: &'a str,
    bars_held: u32,
    stop_loss_price: f64,
    take_profit_price: f64,
    risk_config: &'a RiskConfig,
    perps: PerpsContext,
}

/// Shared seed-context prologue for both decision loops. Derives the
/// position/entry/mark snapshot from the book + current bar — `position_size`,
/// `entry_price`, and `mark_price` now have a single source of truth and can no
/// longer differ between the backtest and live paths — then assembles the
/// [`SeedContext`]. Path-specific values arrive via [`SeedContextParams`]. The
/// resulting context still routes through [`DecisionSeedInput::from_context`]
/// (upnl/flat-entry derivation) and [`build_decision_seed`] (JSON), so all
/// three shared seed stages are now reached by exactly one construction site.
fn build_seed_context<'a>(
    book: &crate::eval::executor::book::PortfolioBook,
    asset_sym: xvision_core::trading::AssetSymbol,
    bar: &'a Ohlcv,
    params: SeedContextParams<'a>,
) -> SeedContext<'a> {
    SeedContext {
        decision_idx: params.decision_idx,
        asset: params.asset,
        active_assets: params.active_assets,
        bar,
        history_slice: params.history_slice,
        inputs_policy: params.inputs_policy,
        supported_timeframes: params.supported_timeframes,
        last_closed_times: params.last_closed_times,
        equity: params.equity,
        position_size: book.position(asset_sym),
        entry_price: book.entry_price(asset_sym),
        mark_price: bar.close,
        next_bar_open: params.next_bar_open,
        reference_price_source: params.reference_price_source,
        bars_held: params.bars_held,
        stop_loss_price: params.stop_loss_price,
        take_profit_price: params.take_profit_price,
        risk_config: params.risk_config,
        perps: params.perps,
    }
}

/// Leading entries to drop from an already-windowed history slice to honor the
/// optional F-8 rolling-window cap (`Some(n)` keeps the most-recent `n`;
/// `None` keeps all). Shared by the backtest and live decision loops so this
/// truncation math can't drift between them. Returns an offset (zero-alloc) so
/// each loop slices its own container — backtest holds `&[&Ohlcv]`, the live
/// loop owns `&[Ohlcv]` and collects, and neither is forced to allocate here.
fn bar_history_limit_offset(len: usize, limit: Option<u32>) -> usize {
    match limit {
        Some(n) if (n as usize) < len => len - n as usize,
        _ => 0,
    }
}

/// Build the trader seed JSON for one decision cycle. F-6: `Causal`
/// drops `decision_index` and top-level `timestamp`, while Raw/Oracle
/// keep the pre-F-6 shape.
pub fn build_decision_seed(input: DecisionSeedInput<'_>) -> serde_json::Value {
    let bar_history = build_bar_history(input.history_slice, input.inputs_policy);
    let current_bar_json = ohlcv_to_json(input.bar, input.inputs_policy);
    match input.inputs_policy {
        InputsPolicy::Raw | InputsPolicy::Oracle => serde_json::json!({
            "decision_index": input.decision_idx,
            "asset": input.asset,
            "active_assets": input.active_assets,
            "timestamp": input.bar.timestamp,
            "market_data": {
                "asset": input.asset,
                "current_bar": current_bar_json,
                "next_bar_open": input.next_bar_open,
                "reference_price_usd": input.bar.close,
                "reference_price_source": input.reference_price_source,
                "bar_history": bar_history,
                "available_timeframes": input.supported_timeframes,
                "last_closed_times": input.last_closed_times,
                "perps": input.perps.to_json(),
            },
            "portfolio_state": {
                "position_size": input.position_size,
                "equity": input.equity,
                "mark_price": input.mark_price,
                "entry_price": input.entry_price,
                "unrealized_pnl_pct": input.unrealized_pnl_pct,
                "bars_held": input.bars_held,
                "stop_loss_price": input.stop_loss_price,
                "take_profit_price": input.take_profit_price,
            },
            "risk_config": risk_config_json(input.risk_config),
        }),
        InputsPolicy::Causal => serde_json::json!({
            "asset": input.asset,
            "active_assets": input.active_assets,
            "market_data": {
                "asset": input.asset,
                "current_bar": current_bar_json,
                "next_bar_open": input.next_bar_open,
                "reference_price_usd": input.bar.close,
                "reference_price_source": input.reference_price_source,
                "bar_history": bar_history,
                "available_timeframes": input.supported_timeframes,
                "last_closed_times": input.last_closed_times,
                "perps": input.perps.to_json(),
            },
            "portfolio_state": {
                "position_size": input.position_size,
                "equity": input.equity,
                "mark_price": input.mark_price,
                "entry_price": input.entry_price,
                "unrealized_pnl_pct": input.unrealized_pnl_pct,
                "bars_held": input.bars_held,
                "stop_loss_price": input.stop_loss_price,
                "take_profit_price": input.take_profit_price,
            },
            "risk_config": risk_config_json(input.risk_config),
        }),
    }
}

/// Render the strategy's live, typed [`RiskConfig`] as the seed's
/// authoritative `risk_config` block. xvision-yzk: the trader/risk agents
/// read these values instead of hand-written prompt text that silently
/// drifts when the optimizer mutates `risk.*`. Serialization is infallible
/// for this plain-data struct; fall back to an empty object defensively
/// rather than panic inside the per-cycle hot path.
fn risk_config_json(risk: &RiskConfig) -> serde_json::Value {
    serde_json::to_value(risk).unwrap_or_else(|_| serde_json::json!({}))
}

/// WU2 Pine import: inject `briefing_indicators` latest values into the
/// decision seed under the `"briefing_indicators"` key.
///
/// Mirrors the `filter_context` post-build insertion pattern at the call-site
/// (after `build_decision_seed`). For each `BriefingIndicator` in the strategy,
/// we spin up an `IndicatorEngine`, push the history bars + current bar, and
/// read the latest computed value. The result is a flat JSON object keyed by
/// the `source_token` (Pine variable name).
///
/// This is additive — existing seed fields are not modified. Skipped entirely
/// when `briefing_indicators` is empty (no overhead on non-pine-import strategies).
pub fn inject_briefing_indicators_into_seed(
    seed: &mut serde_json::Value,
    briefing_indicators: &[crate::strategies::BriefingIndicator],
    current_bar: &Ohlcv,
    history_slice: &[&Ohlcv],
) {
    use xvision_filters::{Bar as FilterBar, IndicatorEngine, IndicatorRef};

    if briefing_indicators.is_empty() {
        return;
    }

    // Build IndicatorRefs from the BriefingIndicator list.
    let refs: Vec<IndicatorRef> = briefing_indicators
        .iter()
        .filter_map(|bi| {
            let period = bi.params.first().map(|p| *p as u32);
            Some(IndicatorRef {
                name: bi.name,
                period,
                bar_offset: None,
            })
        })
        .collect();

    // Instantiate the engine for just these refs.
    let mut engine = IndicatorEngine::new(refs.iter());

    // Push history bars (oldest first).
    for bar in history_slice {
        let fb = FilterBar::with_volume(bar.open, bar.high, bar.low, bar.close, bar.volume);
        engine.push(&fb);
    }
    // Push current bar.
    let fb = FilterBar::with_volume(
        current_bar.open,
        current_bar.high,
        current_bar.low,
        current_bar.close,
        current_bar.volume,
    );
    engine.push(&fb);

    // Collect computed values keyed by source_token.
    let mut values = serde_json::Map::new();
    for (bi, ind_ref) in briefing_indicators.iter().zip(refs.iter()) {
        if let Some(v) = engine.value(ind_ref) {
            values.insert(bi.source_token.clone(), serde_json::json!(v));
        }
        // If not yet warmed up (None), skip — the LLM sees the key absent.
    }

    if !values.is_empty() {
        if let Some(obj) = seed.as_object_mut() {
            obj.insert(
                "briefing_indicators".to_string(),
                serde_json::Value::Object(values),
            );
        }
    }
}

/// WS-10 (`trace-obs-decision-input`): build the structured snapshot of
/// the market context the strategy agent saw this bar, from the trader
/// `seed` (briefing) the executor already assembled. The result is
/// attached to the `agent.decision` span attributes (under the
/// `decision_input` key) so the indicator panel, current bar, regime,
/// briefing mode, and a bounded `bar_history` summary are queryable and
/// land in the run export — instead of surviving only inside the opaque
/// model-call prompt.
///
/// Shape:
///
/// ```json
/// {
///   "indicators": { <indicator panel map> },
///   "current_bar": { <OHLCV> },
///   "regime": <label/value> | null,
///   "briefing_mode": "full" | "delta",
///   "changed_indicators": { ... },   // ONLY when "delta"
///   "bar_history": { "count": N, "first_ts": <ts|null>, "last_ts": <ts|null> }
/// }
/// ```
///
/// Kept SMALL/structured: the full `bar_history` window is NEVER inlined
/// — only a bounded `{count, first_ts, last_ts}` summary (full-window
/// capture is a separate concern / a blob writer's job).
///
/// `briefing_mode` is `"delta"` only when delta was opted in AND a
/// previous briefing was cached AND the indicator panel actually moved
/// between the two bars; otherwise `"full"` (including the first bar of
/// a run, where there's no prior briefing to diff against). When
/// `"delta"`, `changed_indicators` carries the moved/new entries of the
/// indicator panel (unchanged entries omitted; a key present last bar
/// but gone this bar surfaces as `null`) — consistent with the
/// `indicators` panel this snapshot reports. When `"full"`, no
/// `changed_indicators` key is emitted.
/// Compute the current market regime LABEL for an asset from the trailing
/// window of bars up to and including the current decision bar.
///
/// WS-15 (`trace-obs` market-context): the regime label is **computed**
/// here per decision (deterministic, cheap, no LLM) by delegating to the
/// shared [`derive_regime_labels`](crate::eval::regime::derive_regime_labels)
/// heuristic over the executor's own bar-history window. This is purely
/// observational — the label feeds ONLY the `regime_transition` trace
/// event; it is never injected into the trader seed / briefing / prompt,
/// so it changes nothing about what the agent sees or decides.
///
/// `window` is the trailing slice of bars (oldest first) ending at the
/// current decision bar. `derive_regime_labels` requires at least two bars;
/// with fewer it returns `None` (no transition can fire on the first bar).
/// The `&Ohlcv` view is converted to the `MarketBar` shape the heuristic
/// expects (the two structs carry identical OHLCV fields).
fn compute_regime_label(window: &[&Ohlcv]) -> Option<String> {
    let bars: Vec<xvision_data::alpaca::MarketBar> = window
        .iter()
        .map(|b| xvision_data::alpaca::MarketBar {
            timestamp: b.timestamp,
            open: b.open,
            high: b.high,
            low: b.low,
            close: b.close,
            volume: b.volume,
        })
        .collect();
    crate::eval::regime::derive_regime_labels(&bars).regime_label
}

/// Detect a regime transition between two consecutive decisions on the
/// same asset.
///
/// Returns `Some((from, to))` ONLY when a `prev` label exists AND it
/// differs from `curr` AND `curr` is present — i.e. a concrete
/// from→to label change. Returns `None` on the first observation
/// (`prev` is `None`), when the regime is unchanged, or when the
/// current label is absent. This is the predicate the executor uses to
/// decide whether to emit a `regime_transition` engine event; it never
/// fires on the first bar or on a stable regime.
pub fn regime_changed(prev: Option<&str>, curr: Option<&str>) -> Option<(String, String)> {
    match (prev, curr) {
        (Some(p), Some(c)) if p != c => Some((p.to_string(), c.to_string())),
        _ => None,
    }
}

pub fn build_decision_input(
    seed: &serde_json::Value,
    prev_seed: Option<&serde_json::Value>,
    delta_opt_in: bool,
) -> serde_json::Value {
    use serde_json::{Map, Value};

    // ---- Indicator panel ------------------------------------------
    // The executor injects indicators under `filter_context` (DSL filter
    // trigger snapshot) and/or `briefing_indicators` (Pine import). Merge
    // both into one panel; `briefing_indicators` wins on key collision
    // (it's the strategy-authored set).
    let indicator_panel = |s: &Value| -> Map<String, Value> {
        let mut panel = Map::new();
        if let Some(fc) = s.get("filter_context").and_then(Value::as_object) {
            for (k, v) in fc {
                panel.insert(k.clone(), v.clone());
            }
        }
        if let Some(bi) = s.get("briefing_indicators").and_then(Value::as_object) {
            for (k, v) in bi {
                panel.insert(k.clone(), v.clone());
            }
        }
        panel
    };
    let indicators = indicator_panel(seed);

    // ---- Current bar OHLCV ----------------------------------------
    let current_bar = seed
        .get("market_data")
        .and_then(|m| m.get("current_bar"))
        .cloned()
        .unwrap_or(Value::Null);

    // ---- Regime ---------------------------------------------------
    // Prefer a top-level `regime` label; fall back to one carried inside
    // the filter trigger context. `null` when neither is present.
    let regime = seed
        .get("regime")
        .or_else(|| seed.get("filter_context").and_then(|fc| fc.get("regime")))
        .cloned()
        .unwrap_or(Value::Null);

    // ---- Bounded bar_history summary ------------------------------
    // NEVER inline the full window — only count + first/last ts.
    let bar_history = bar_history_summary(seed);

    // ---- Briefing mode (full vs delta) ----------------------------
    // Honest about what the agent saw: "delta" only when delta was
    // opted in, a prior briefing exists, AND the indicator panel
    // actually moved between the two bars.
    let mut out = Map::new();
    out.insert("indicators".to_string(), Value::Object(indicators));
    out.insert("current_bar".to_string(), current_bar);
    out.insert("regime".to_string(), regime);
    out.insert("bar_history".to_string(), Value::Object(bar_history));

    let changed = if delta_opt_in {
        prev_seed.and_then(|prev| {
            let prev_panel = indicator_panel(prev);
            let curr_panel = indicator_panel(seed);
            let diff = diff_indicator_panel(&prev_panel, &curr_panel);
            if diff.is_empty() {
                None
            } else {
                Some(diff)
            }
        })
    } else {
        None
    };

    if let Some(diff) = changed {
        out.insert("briefing_mode".to_string(), Value::String("delta".into()));
        out.insert("changed_indicators".to_string(), Value::Object(diff));
    } else {
        out.insert("briefing_mode".to_string(), Value::String("full".into()));
    }

    Value::Object(out)
}

/// Indicator-panel diff: entries in `curr` that are new or changed vs
/// `prev`, plus entries that disappeared (surfaced as `null`). Mirrors
/// the dispatch-path indicator diff semantics
/// ([`crate::agent::briefing`]) but over the eval seed's actual
/// indicator panel (`filter_context` + `briefing_indicators`).
fn diff_indicator_panel(
    prev: &serde_json::Map<String, serde_json::Value>,
    curr: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    use serde_json::{Map, Value};
    let mut changed = Map::new();
    for (k, v) in curr {
        match prev.get(k) {
            Some(prev_v) if prev_v == v => {}
            _ => {
                changed.insert(k.clone(), v.clone());
            }
        }
    }
    for k in prev.keys() {
        if !curr.contains_key(k) {
            changed.insert(k.clone(), Value::Null);
        }
    }
    changed
}

/// Bounded summary of the seed's `market_data.bar_history` window:
/// `{ count, first_ts, last_ts }`. The full window is intentionally
/// never inlined — this keeps the decision-span attribute small.
fn bar_history_summary(seed: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    use serde_json::{Map, Value};
    let bars = seed
        .get("market_data")
        .and_then(|m| m.get("bar_history"))
        .and_then(Value::as_array);
    let mut summary = Map::new();
    let count = bars.map(|b| b.len()).unwrap_or(0);
    summary.insert("count".to_string(), Value::from(count));
    let ts_of = |entry: Option<&Value>| -> Value {
        entry
            .and_then(|e| e.get("timestamp"))
            .cloned()
            .unwrap_or(Value::Null)
    };
    summary.insert("first_ts".to_string(), ts_of(bars.and_then(|b| b.first())));
    summary.insert("last_ts".to_string(), ts_of(bars.and_then(|b| b.last())));
    summary
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

/// Build the `bar_history` JSON array for the trader seed. The backtest and
/// live decision loops both call this one function, so their `bar_history`
/// payloads are identical under the same policy by construction (there is no
/// separate `paper` module anymore — it was folded into this module).
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

/// Resolve the trader's `InputsPolicy` from the trader-role
/// `ResolvedAgentSlot`; defaults to `Raw` so legacy strategy shapes (no
/// attached agents) keep today's behavior. Used by both decision loops.
fn resolve_inputs_policy(agent_slots: &[ResolvedAgentSlot]) -> InputsPolicy {
    agent_slots
        .iter()
        .find(|r| canonical_role(&r.role) == "trader")
        .map(|r| r.inputs_policy)
        .unwrap_or(InputsPolicy::Raw)
}

/// Resolve the F-8 per-trader `bar_history_limit` cap from the trader-role
/// `ResolvedAgentSlot`. Used by both decision loops.
fn resolve_bar_history_limit(agent_slots: &[ResolvedAgentSlot]) -> Option<u32> {
    agent_slots
        .iter()
        .find(|r| canonical_role(&r.role) == "trader")
        .and_then(|r| r.bar_history_limit)
}

/// Publish the current decision's asset + simulated-clock timestamp into the
/// shared dispatch handles so the tool chokepoint can (a) reject cross-asset
/// market-data fetches and (b) anchor Nansen backtest calls. No-op when a
/// handle is `None` (non-sidecar run).
async fn publish_decision_context(
    asset_guard: &Option<std::sync::Arc<tokio::sync::RwLock<Option<String>>>>,
    as_of_guard: &Option<std::sync::Arc<tokio::sync::RwLock<Option<chrono::DateTime<chrono::Utc>>>>>,
    asset: &str,
    as_of: chrono::DateTime<chrono::Utc>,
) {
    if let Some(g) = asset_guard {
        *g.write().await = Some(asset.to_string());
    }
    if let Some(g) = as_of_guard {
        *g.write().await = Some(as_of);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// The F-8 truncation math is shared by the backtest and live decision
    /// loops via `bar_history_limit_offset`; pin it so the two can't drift.
    #[test]
    fn live_stop_reason_uses_first_configured_limit_order() {
        let started = Instant::now();
        let policy = crate::eval::live_config::StopPolicy {
            bar_limit: Some(2),
            decision_limit: Some(3),
            trade_limit: Some(1),
            time_limit_secs: None,
        };

        assert_eq!(
            live_stop_reason(&policy, 2, 3, 1, started).as_deref(),
            Some("bar_limit 2 reached")
        );
    }

    #[test]
    fn live_stop_reason_counts_trades_separately_from_decisions() {
        let started = Instant::now();
        let policy = crate::eval::live_config::StopPolicy {
            bar_limit: None,
            decision_limit: None,
            trade_limit: Some(2),
            time_limit_secs: None,
        };

        assert_eq!(live_stop_reason(&policy, 10, 10, 1, started), None);
        assert_eq!(
            live_stop_reason(&policy, 10, 10, 2, started).as_deref(),
            Some("trade_limit 2 reached")
        );
    }

    #[test]
    fn bar_history_limit_offset_shared_truncation() {
        assert_eq!(bar_history_limit_offset(10, None), 0); // no cap → keep all
        assert_eq!(bar_history_limit_offset(3, Some(5)), 0); // cap >= len → keep all
        assert_eq!(bar_history_limit_offset(5, Some(5)), 0); // cap == len → keep all
        assert_eq!(bar_history_limit_offset(10, Some(4)), 6); // keep most-recent 4
        assert_eq!(bar_history_limit_offset(10, Some(0)), 10); // cap 0 → drop all
    }

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
                execution_mode: Default::default(),
                capital_mode: Default::default(),
                decision_cadence_minutes: 15,
                timeframe_requirements: Default::default(),
                attested_with: vec!["m".into()],
                required_tools: vec!["ohlcv".into()],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
                color: None,
            },
            hypothesis: None,
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            decision_mode: Default::default(),
            mechanistic_config: None,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
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
            max_wall_ms: None,
            temperature: None,
            inputs_policy: crate::agents::InputsPolicy::Raw,
            bar_history_limit: None,
            memory_mode: xvision_memory::types::MemoryMode::Off,
            agent_id: String::new(),
            noop_skip: true,
            nano: None,
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

    #[test]
    fn live_filter_gate_does_not_skip_open_position_sltp_checks() {
        assert!(live_filter_gate_should_short_circuit(true, false, false));
        assert!(!live_filter_gate_should_short_circuit(true, true, true));
        assert!(live_filter_gate_should_short_circuit(true, true, false));
        assert!(!live_filter_gate_should_short_circuit(false, true, true));
    }

    #[test]
    fn live_sltp_state_preserves_trader_brackets() {
        let parsed = crate::eval::executor::trader_output::TraderOutput::parse_strict(
            r#"{
                "action":"long_open",
                "conviction":0.7,
                "justification":"breakout",
                "stop_loss_pct":2.0,
                "take_profit_pct":6.0,
                "trailing_stop_pct":1.5,
                "breakeven_trigger_pct":3.0,
                "breakeven_offset_pct":0.2,
                "fade_sl_bars":4,
                "fade_sl_start_pct":2.0,
                "fade_sl_end_pct":0.5,
                "max_bars_held":9,
                "sl_atr_mult":1.2,
                "tp_atr_mult":3.4,
                "tp1_pct":4.0,
                "tp1_close_fraction":0.5,
                "tp2_pct":8.0
            }"#,
            "01TEST",
            0,
        )
        .expect("valid trader brackets parse");

        let state = build_live_sltp_state(
            xvision_core::trading::Direction::Long,
            100.0,
            &parsed,
            false,
            2.5,
            Some(1.75),
        );

        assert_eq!(state.stop_loss_pct, 2.0);
        assert_eq!(state.take_profit_pct, 6.0);
        assert_eq!(state.entry_atr, Some(1.75));
        assert_eq!(state.trailing_stop_pct, Some(1.5));
        assert_eq!(state.breakeven_trigger_pct, Some(3.0));
        assert_eq!(state.breakeven_offset_pct, Some(0.2));
        assert_eq!(state.fade_sl_bars, Some(4));
        assert_eq!(state.fade_sl_start_pct, Some(2.0));
        assert_eq!(state.fade_sl_end_pct, Some(0.5));
        assert_eq!(state.max_bars_held, Some(9));
        assert_eq!(state.sl_atr_mult, Some(1.2));
        assert_eq!(state.tp_atr_mult, Some(3.4));
        assert_eq!(state.tp1_pct, Some(4.0));
        assert_eq!(state.tp1_close_fraction, Some(0.5));
        assert_eq!(state.tp2_pct, Some(8.0));
    }

    #[test]
    fn causal_prior_episodes_strip_temporal_ids() {
        let episodes = serde_json::json!([
            {
                "bar_timestamp": "2024-01-01T00:00:00Z",
                "decision_idx": 7,
                "action": "flat"
            }
        ]);

        let sanitized = sanitize_prior_episodes_for_policy(episodes, InputsPolicy::Causal);
        let first = sanitized.as_array().unwrap()[0].as_object().unwrap();
        assert!(!first.contains_key("bar_timestamp"));
        assert!(!first.contains_key("decision_idx"));
        assert_eq!(first.get("action").and_then(|v| v.as_str()), Some("flat"));
    }

    #[test]
    fn live_early_stop_inherited_row_is_persistable_flat_decision() {
        let ts = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let row = inherited_early_stop_row("run-1", 42, ts, "BTC/USD");

        assert_eq!(row.run_id, "run-1");
        assert_eq!(row.decision_index, 42);
        assert_eq!(row.asset, "BTC/USD");
        assert_eq!(row.action, "flat");
        assert_eq!(row.conviction, Some(0.0));
        assert_eq!(
            row.justification.as_deref(),
            Some("inherited from early-stop policy")
        );
        assert!(row.fill_price.is_none());
        assert!(row.pnl_realized.is_none());
    }

    #[test]
    fn live_sltp_full_exit_uses_broker_flat_request() {
        let req = live_sltp_close_request("BTC/USD", 0.75, 100.0, 101.0, 25.0, 100_000.0, 5);

        assert_eq!(req.action, "flat");
        assert_eq!(req.pos, 0.75);
        assert_eq!(req.entry, 100.0);
        assert_eq!(req.asset, "BTC/USD");
        assert_eq!(req.next_open, 101.0);
    }

    #[test]
    fn live_sltp_partial_tp_closes_fraction_and_returns() {
        let pos = 0.75;
        let fraction = 0.4;
        let close_units = live_sltp_close_units(pos, fraction);
        let remaining = live_sltp_remaining_position(pos, close_units);
        let outcome = live_sltp_exit_outcome(true, false, None, None);

        assert_eq!(close_units, 0.30000000000000004);
        assert_eq!(remaining, 0.44999999999999996);
        assert!(outcome.fill_happened);
        assert_eq!(outcome.last_open_direction, None);
        assert_eq!(outcome.broker_error, None);
    }

    #[tokio::test]
    async fn decision_context_write_populates_asset_and_clock() {
        use chrono::{TimeZone, Utc};
        use std::sync::Arc;
        use tokio::sync::RwLock;
        let asset_guard = Some(Arc::new(RwLock::new(None)));
        let as_of_guard = Some(Arc::new(RwLock::new(None)));
        let ts = Utc.with_ymd_and_hms(2024, 3, 15, 14, 0, 0).unwrap();

        publish_decision_context(&asset_guard, &as_of_guard, "BTC/USD", ts).await;

        assert_eq!(
            asset_guard.as_ref().unwrap().read().await.as_deref(),
            Some("BTC/USD")
        );
        assert_eq!(*as_of_guard.as_ref().unwrap().read().await, Some(ts));
    }

    #[tokio::test]
    async fn decision_context_write_is_noop_when_guards_absent() {
        use chrono::Utc;
        let asset_guard: Option<std::sync::Arc<tokio::sync::RwLock<Option<String>>>> = None;
        let as_of_guard: Option<std::sync::Arc<tokio::sync::RwLock<Option<chrono::DateTime<chrono::Utc>>>>> =
            None;
        // Both None (non-sidecar run) must not panic.
        publish_decision_context(&asset_guard, &as_of_guard, "BTC/USD", Utc::now()).await;
    }
}

#[cfg(test)]
mod live_shell_tests {
    #[test]
    fn live_shell_is_wired_by_api_builder() {
        // Live construction now needs broker + stream handles; API-level tests
        // cover validation without requiring real Alpaca credentials here.
        assert!(true);
    }
}

#[cfg(test)]
mod mark_to_market_tests {
    use crate::eval::executor::book::PortfolioBook;
    use xvision_core::trading::AssetSymbol;
    #[allow(non_upper_case_globals)]
    const Btc: AssetSymbol = AssetSymbol::Btc;

    #[test]
    fn mark_to_market_adds_to_win_count() {
        // A LONG entered at 100, marked at 150 — should be a win.
        let mut book = PortfolioBook::new(10_000.0);
        book.set_position(Btc, 1.0, 100.0);
        book.mark(Btc, 150.0);

        let mut realized_count = 0u32;
        let mut wins = 0u32;
        let mut equity = 10_000.0f64;

        for (_asset, pnl) in book.close_all_at_mark() {
            realized_count += 1;
            if pnl > 0.0 {
                wins += 1;
            }
            equity += pnl;
        }

        assert_eq!(realized_count, 1);
        assert_eq!(wins, 1);
        assert!(equity > 10_000.0);
    }

    #[test]
    fn mark_to_market_loss_not_win() {
        let mut book = PortfolioBook::new(10_000.0);
        book.set_position(Btc, 1.0, 100.0);
        book.mark(Btc, 80.0); // price fell

        let mut wins = 0u32;
        let mut realized_count = 0u32;

        for (_asset, pnl) in book.close_all_at_mark() {
            realized_count += 1;
            if pnl > 0.0 {
                wins += 1;
            }
        }

        assert_eq!(realized_count, 1);
        assert_eq!(wins, 0, "loss should not count as win");
    }
}
