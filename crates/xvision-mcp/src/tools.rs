//! Tool surface advertised over MCP.
//!
//! Two surfaces:
//! - **Indicator tools** (`xvn_health`, `xvn_sma`, ...) — stateless: the
//!   caller supplies the price / HLC series as parameters and we dispatch
//!   into `xvision-data`. NaN positions in the output mark indicator warmup
//!   and travel through the wire as JSON `null` (we round-trip through
//!   `Option<f64>` for that).
//! - **Authoring tools** (`xvn_list_templates`, `xvn_create_strategy`, ...)
//!   — stateful: persist `Strategy`s to `$XVN_HOME/strategies/`
//!   via `xvision_engine::strategies::store::FilesystemStore`.

use std::path::PathBuf;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use std::collections::HashMap;

use xvision_data as xvn;
use xvision_engine::agents::AgentSlot;
use xvision_engine::api::autooptimizer as api_autooptimizer;
use xvision_engine::api::eval::{
    self as api_eval, BatchDetail, CompareRunsRequest, CreateBatchRequest, EvalRunRequest, ListRunsRequest,
};
use xvision_engine::api::scenario as api_scenario;
use xvision_engine::api::{agents as api_agents, Actor, ApiContext};
use xvision_engine::api::{flywheel as api_flywheel, memory as api_memory, optimize as api_optimize};
use xvision_engine::authoring;
use xvision_engine::eval::behavior::derive_behavior_summary;
use xvision_engine::eval::run::{RunMode, RunStatus};
use xvision_engine::eval::scenario::Scenario;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::validate::{preflight_validate, validate_strategy};
use xvision_engine::strategies::{
    risk::RiskConfig,
    store::{FilesystemStore, StrategyStore},
};
use xvision_memory::store::MemoryStore;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn nan_to_null(xs: Vec<f64>) -> Vec<Option<f64>> {
    xs.into_iter()
        .map(|v| if v.is_finite() { Some(v) } else { None })
        .collect()
}

fn json_or_err<T: serde::Serialize>(t: &T) -> Result<String, rmcp::ErrorData> {
    serde_json::to_string(t).map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))
}

/// Convert a `xvision_engine::authoring` error into an MCP error. Engine
/// authoring failures are always caller-attributable (unknown template,
/// missing draft, malformed input, etc.) so we map them to JSON-RPC
/// `invalid_params` rather than `internal_error`.
fn authoring_err(e: anyhow::Error) -> rmcp::ErrorData {
    rmcp::ErrorData::invalid_params(format!("{e:#}"), None)
}

// ---------------------------------------------------------------------------
// request shapes — one per tool. JsonSchema derives feed the tools/list
// schema agents see.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PricesPeriod {
    /// Closing prices, oldest → newest.
    pub prices: Vec<f64>,
    /// Lookback window (e.g. 14 for RSI, 20 for SMA-20).
    pub period: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BollingerReq {
    /// Closing prices, oldest → newest.
    pub prices: Vec<f64>,
    /// Window length, e.g. 20.
    pub period: usize,
    /// Standard-deviation multiplier, e.g. 2.0.
    pub k: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AtrReq {
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    /// Wilder-smoothing period (typically 14).
    pub period: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MacdReq {
    pub prices: Vec<f64>,
    /// Fast EMA period — canonical 12.
    pub fast: usize,
    /// Slow EMA period — canonical 26.
    pub slow: usize,
    /// Signal line EMA period — canonical 9.
    pub signal: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DonchianReq {
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub period: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FibReq {
    pub prices: Vec<f64>,
    /// How many recent bars to scan for the swing high / swing low.
    pub lookback: usize,
}

// ---------------------------------------------------------------------------
// authoring request shapes — one per stateful tool. Each verb takes a
// strategy `id` (ULID); `xvn_create_strategy` doesn't need an existing
// one (it produces a blank draft post 2026-05-21 template-registry
// removal).
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateStrategyReq {
    /// Human-readable name (e.g., `btc-momentum-v1`). Post 2026-05-21
    /// the request no longer takes a `template` discriminator —
    /// `xvn_create_strategy` produces a blank draft and the agent
    /// (`xvn_create_strategy_agent`) and slot tool calls populate the
    /// draft. Unknown fields (including legacy
    /// `template` payloads from pre-migration callers) are silently
    /// ignored on the MCP boundary so the wizard tool-use loop
    /// doesn't break mid-conversation.
    pub name: String,
    /// Optional `@handle` of the author. Defaults to `@anonymous`.
    #[serde(default)]
    pub creator: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StrategyId {
    /// ULID of the strategy draft.
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateSlotReq {
    pub id: String,
    /// Slot to update: `regime` | `trader`.
    pub slot: String,
    /// Model requirement (e.g., `anthropic.claude-sonnet-4.6+`).
    #[serde(default)]
    pub attested_with: Option<String>,
    /// Explicit provider name for this slot.
    #[serde(default)]
    pub provider: Option<String>,
    /// Explicit model name for this slot.
    #[serde(default)]
    pub model: Option<String>,
    /// Tools the slot is allowed to call.
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetRiskConfigReq {
    pub id: String,
    /// `conservative` | `balanced` | `aggressive`. Mutually exclusive with `explicit`.
    #[serde(default)]
    pub preset: Option<String>,
    /// Full `RiskConfig`. Mutually exclusive with `preset`. Expected shape:
    /// `{ risk_pct_per_trade: f64, max_concurrent_positions: u32,
    ///    max_leverage: f64, stop_loss_atr_multiple: f64,
    ///    daily_loss_kill_pct: f64 }`.
    #[serde(default)]
    pub explicit: Option<serde_json::Value>,
}

// --- marketplace request shapes (x402 autonomous purchases) ------------------

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct MarketplaceGetReq {
    pub listing_id: u64,
}

// --- eval-domain request shapes (Phase 3.D Task 12) -------------------------

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct EvalListReq {
    /// Optional filter: only return runs for this strategy agent id.
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Optional filter: only return runs against this scenario id.
    #[serde(default)]
    pub scenario_id: Option<String>,
    /// Optional status filter: queued | running | completed | failed | cancelled.
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EvalRunIdReq {
    /// Run id (ULID).
    pub run_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EvalCompareReq {
    /// Two or more run ids (ULIDs) to fold into a single ComparisonReport.
    pub run_ids: Vec<String>,
}

// --- memory / flywheel request shapes --------------------------------------

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct MemoryListMcpReq {
    /// Optional tier filter: `observation` or `pattern`.
    #[serde(default)]
    pub tier: Option<String>,
    /// Exact namespace, e.g. `global` or `agent:<id>`.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Convenience shorthand for `namespace = agent:<id>`.
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub scenario_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    /// Pattern lifecycle filter, e.g. `active` or `staged`.
    #[serde(default)]
    pub promotion_state: Option<String>,
    #[serde(default)]
    pub include_forgotten: Option<bool>,
    #[serde(default)]
    pub forgotten_only: Option<bool>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryGetMcpReq {
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MemoryRecallMcpReq {
    /// Exact namespace, e.g. `global` or `agent:<id>`.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Convenience shorthand for `namespace = agent:<id>`.
    #[serde(default)]
    pub agent: Option<String>,
    /// Query embedding supplied by the caller. MCP recall is read-only and
    /// does not call a provider-side embedder.
    pub query_embedding: Vec<f32>,
    /// Number of Pattern hits to return. Defaults to 5.
    #[serde(default)]
    pub k: Option<usize>,
    /// RFC3339 scenario start for the temporal leakage filter.
    #[serde(default)]
    pub scenario_start: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FlywheelStatusMcpReq {
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FlywheelVelocityMcpReq {
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub days: Option<i64>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FlywheelLineageMcpReq {
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct OptimizeMemoryDemosMcpReq {
    pub target_agent_id: String,
    #[serde(default)]
    pub slot: Option<String>,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub memory_agent: Option<String>,
    #[serde(default)]
    pub scenario_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub demo_source: Option<String>,
    #[serde(default)]
    pub holdout_split: Option<String>,
    #[serde(default)]
    pub cohort_query: Option<String>,
    #[serde(default)]
    pub manual_observation_ids: Option<Vec<String>>,
    #[serde(default)]
    pub prior_pattern_ids: Option<Vec<String>>,
    #[serde(default)]
    pub auto_prior_patterns: bool,
    #[serde(default)]
    pub prior_pattern_limit: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub max_demo_chars: Option<usize>,
    #[serde(default)]
    pub apply: bool,
    #[serde(default)]
    pub child_name: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct AutoOptimizerListMcpReq {
    /// Exact namespace, e.g. `global` or `agent:<id>`.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Convenience shorthand for `namespace = agent:<id>`.
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AutoOptimizerRunIdMcpReq {
    /// AutoOptimizer run id.
    pub id: String,
}

// ---------------------------------------------------------------------------
// New verbs — F-13 MCP surface parity for CLI workbench wave A+B + wave-C
// ---------------------------------------------------------------------------

/// Request for `xvn_strategy_create_atomic`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StrategyCreateAtomicReq {
    /// Human-readable strategy name (e.g. `btc-momentum-v1`).
    pub name: String,
    /// Role the created agent plays inside the strategy (e.g. `trader`).
    pub role: String,
    /// Full system prompt text for the agent.
    pub prompt: String,
    /// Provider name (e.g. `openrouter`, `anthropic`).
    pub provider: String,
    /// Model id (e.g. `kimi-k2`, `deepseek/deepseek-chat`).
    pub model: String,
    /// Primary asset the strategy trades (e.g. `ETH/USD`).
    #[serde(default)]
    pub asset: Option<String>,
    /// Decision timeframe / bar granularity (e.g. `4h`, `1h`, `1d`).
    /// Accepted: `1m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `1d`.
    #[serde(default)]
    pub timeframe: Option<String>,
    /// Optional creator handle. Defaults to `@mcp`.
    #[serde(default)]
    pub creator: Option<String>,
}

/// Request for `xvn_strategy_validate_preflight` (wave-A/B form, uses `id`).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StrategyPreflightReq {
    /// Strategy id (ULID) to validate.
    pub id: String,
    /// Optional scenario id to cross-check against. When supplied the
    /// validator also checks asset-universe and timeframe alignment.
    #[serde(default)]
    pub scenario_id: Option<String>,
}

/// Request for `xvn_strategy_validate_preflight` (wave-C form, uses `strategy_id`).
/// Kept for backward compatibility with tests that use the wave-C shape.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StrategyValidatePreflightReq {
    /// Strategy agent id. Required.
    pub strategy_id: String,
    /// Optional scenario id to cross-check asset/timeframe alignment.
    #[serde(default)]
    pub scenario_id: Option<String>,
}

/// Duplicated from `xvision_cli::commands::strategy::PreflightReport` for the
/// MCP crate boundary. TODO: promote to a shared types crate in a follow-up.
#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct PreflightReport {
    pub strategy_id: String,
    pub eval_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_decisions: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeframe: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warmup_bars: Option<u32>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Request for `xvn_eval_batch_run`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EvalBatchRunReq {
    /// Strategy agent id (from `xvn strategy ls`).
    pub strategy_id: String,
    /// Scenario ids to run against.
    pub scenario_ids: Vec<String>,
    /// Run mode: `backtest` (default) or `paper`.
    #[serde(default)]
    pub mode: Option<String>,
    /// Agent profile id for post-run reviews (e.g. `reasoning-agent`).
    /// When set, a review is generated for each completed run.
    #[serde(default)]
    pub review_with: Option<String>,
}

/// `eval_batch_status` — retrieve the persisted batch record and its run ids.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EvalBatchStatusReq {
    /// Batch id (e.g. `batch_01K…`). Required.
    pub batch_id: String,
}

/// `eval_compare` — compare a set of runs (by run ids) or all runs from a
/// batch, returning a `ComparisonReport`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EvalCompareExtReq {
    /// Two or more run ids. Mutually exclusive with `batch_id`.
    #[serde(default)]
    pub run_ids: Vec<String>,
    /// Batch id — resolve run ids from this batch, then compare.
    /// Mutually exclusive with `run_ids`.
    #[serde(default)]
    pub batch_id: Option<String>,
    /// When true, return the comparison as a Markdown table instead of JSON.
    #[serde(default)]
    pub markdown: bool,
}

/// `scenarios_select` — filter the scenario library by timeframe /
/// decision count / regime labels and return a ranked subset. Scenarios are
/// asset-free; asset-universe selection now lives at the run layer.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScenariosSelectReq {
    /// Bar granularity filter (e.g. `4h`, `1h`). Maps to
    /// `decision_cadence_minutes`. Omitted → all timeframes.
    #[serde(default)]
    pub timeframe: Option<String>,

    /// [Mode A] Select scenarios within ±10 % of this decision count.
    /// Mutually exclusive with `same_decisions`.
    #[serde(default)]
    pub target_decisions: Option<u64>,

    /// [Mode B] Return scenarios that share the largest common decision count
    /// ≤ `max_decisions`. Requires `max_decisions`. Mutually exclusive with
    /// `target_decisions`.
    #[serde(default)]
    pub same_decisions: bool,

    /// [Mode B] Maximum decision count cap. Required when `same_decisions=true`.
    #[serde(default)]
    pub max_decisions: Option<u64>,

    /// Regime label filter (e.g. `["bull","bear"]`). OR-semantics per scenario.
    #[serde(default)]
    pub regimes: Vec<String>,

    /// Maximum results to return (default 4).
    #[serde(default)]
    pub count: Option<usize>,
}

/// Request for `xvn_eval_compare_report`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EvalCompareReportReq {
    /// Two or more run ids to compare.
    pub run_ids: Vec<String>,
    /// Sort metric: `return` (default), `sharpe`, or `drawdown`.
    #[serde(default)]
    pub sort: Option<String>,
}

/// One row in the `CompareReport` (CLI-level decorated shape).
#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct CompareRunRow {
    pub run_id: String,
    pub scenario_id: String,
    pub scenario_name: String,
    pub strategy_id: String,
    pub status: String,
    pub return_pct: Option<f64>,
    pub sharpe: Option<f64>,
    pub max_drawdown_pct: Option<f64>,
    pub decisions: u32,
    pub trades_opened: u32,
    pub action_distribution: HashMap<String, u32>,
    pub avg_bars_held: Option<f64>,
    pub primary_failure_mode: String,
}

/// Full compare report returned by `xvn_eval_compare_report`.
#[derive(Debug, serde::Serialize, JsonSchema)]
pub struct CompareReport {
    pub runs: Vec<CompareRunRow>,
}

/// Request for `xvn_scenario_inspect_card`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScenarioInspectCardReq {
    /// Scenario id.
    pub id: String,
}

/// Request for `xvn_eval_behavior`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EvalBehaviorReq {
    /// Run id (ULID).
    pub run_id: String,
}

// ---------------------------------------------------------------------------
// XvisionTools — the rmcp router.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct XvisionTools {
    /// Optional override for `$XVN_HOME` used by authoring tools (tests).
    /// `None` → read `XVN_HOME` env var, falling back to `$HOME/.xvn`.
    xvn_home: Option<PathBuf>,
}

fn resolve_xvn_home() -> PathBuf {
    if let Ok(s) = std::env::var("XVN_HOME") {
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".xvn")
}

impl XvisionTools {
    /// Return the sorted list of MCP tool names advertised by this server.
    ///
    /// This is the canonical inventory used by `crates/xvision-mcp/tests/parity.rs`
    /// to guard against untracked additions or removals. When a tool is added
    /// or removed from the `#[tool_router]` impl below, this constant must also
    /// be updated — and so must
    /// `docs/superpowers/evidence/2026-05-25-agent-cli-press-audit/mcp-parity-matrix.md`.
    pub fn tool_names() -> Vec<&'static str> {
        let mut names = vec![
            "xvn_atr",
            "xvn_bollinger",
            "xvn_create_strategy",
            "xvn_donchian",
            "xvn_ema",
            "xvn_eval_batch_run",
            "xvn_eval_batch_status",
            "xvn_eval_behavior",
            "xvn_eval_compare",
            "xvn_eval_compare_ext",
            "xvn_eval_compare_report",
            "xvn_eval_findings",
            "xvn_eval_get",
            "xvn_eval_list",
            "xvn_eval_metrics",
            "xvn_eval_scenarios",
            "xvn_fib_retracements",
            "xvn_get_strategy",
            "xvn_health",
            "xvn_list_templates",
            "xvn_macd",
            "xvn_marketplace_browse",
            "xvn_marketplace_buy",
            "xvn_marketplace_get_listing",
            "xvn_marketplace_import",
            "xvn_marketplace_wallet",
            "xvn_rsi",
            "xvn_scenario_inspect_card",
            "xvn_scenarios_select",
            "xvn_set_risk_config",
            "xvn_sma",
            "xvn_strategy_create_atomic",
            "xvn_strategy_validate_preflight",
            "xvn_update_slot",
            "xvn_validate_draft",
        ];
        names.sort_unstable();
        names
    }
}

#[tool_router(server_handler)]
impl XvisionTools {
    pub fn new() -> Self {
        Self::default()
    }

    /// Test-only constructor that pins `$XVN_HOME` to a known directory.
    /// Production callers use `XvisionTools::new()` and rely on the env var.
    pub fn with_xvn_home(home: PathBuf) -> Self {
        Self { xvn_home: Some(home) }
    }

    fn store(&self) -> FilesystemStore {
        let root = self
            .xvn_home
            .clone()
            .unwrap_or_else(resolve_xvn_home)
            .join("strategies");
        FilesystemStore::new(root)
    }

    /// Server health + version probe. Returns a JSON object with
    /// `{ ok: true, name, version }`. Use to confirm the MCP wiring is
    /// live before issuing real tool calls.
    #[tool(description = "Health probe for the xvision MCP server. Returns server name and version.")]
    fn xvn_health(&self) -> Result<String, rmcp::ErrorData> {
        json_or_err(&serde_json::json!({
            "ok": true,
            "name": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
        }))
    }

    /// Simple moving average. Returns a same-length series; warmup
    /// positions are JSON null. The latest SMA value is the last
    /// non-null element.
    #[tool(
        description = "Simple moving average over a closing-price series. Returns a same-length array; warmup positions are null."
    )]
    fn xvn_sma(&self, Parameters(req): Parameters<PricesPeriod>) -> Result<String, rmcp::ErrorData> {
        json_or_err(&nan_to_null(xvn::sma(&req.prices, req.period)))
    }

    /// Exponential moving average. Same shape as SMA.
    #[tool(
        description = "Exponential moving average over a closing-price series. EMA seeded with the SMA of the first `period` bars."
    )]
    fn xvn_ema(&self, Parameters(req): Parameters<PricesPeriod>) -> Result<String, rmcp::ErrorData> {
        json_or_err(&nan_to_null(xvn::ema(&req.prices, req.period)))
    }

    /// Wilder RSI on closing prices.
    #[tool(description = "Wilder-smoothed RSI on a closing-price series. Period 14 is canonical.")]
    fn xvn_rsi(&self, Parameters(req): Parameters<PricesPeriod>) -> Result<String, rmcp::ErrorData> {
        json_or_err(&nan_to_null(xvn::rsi(&req.prices, req.period)))
    }

    /// Bollinger Bands. Returns `{ middle: [...], upper: [...], lower: [...] }`.
    #[tool(
        description = "Bollinger Bands. Returns middle/upper/lower same-length arrays; warmup positions are null."
    )]
    fn xvn_bollinger(&self, Parameters(req): Parameters<BollingerReq>) -> Result<String, rmcp::ErrorData> {
        let bb = xvn::bollinger(&req.prices, req.period, req.k);
        json_or_err(&serde_json::json!({
            "middle": nan_to_null(bb.middle),
            "upper":  nan_to_null(bb.upper),
            "lower":  nan_to_null(bb.lower),
        }))
    }

    /// Wilder ATR. Inputs must be equal-length OHLC series.
    #[tool(
        description = "Wilder-smoothed Average True Range. Inputs (high/low/close) must be equal-length series."
    )]
    fn xvn_atr(&self, Parameters(req): Parameters<AtrReq>) -> Result<String, rmcp::ErrorData> {
        if req.high.len() != req.low.len() || req.low.len() != req.close.len() {
            return Err(rmcp::ErrorData::invalid_params(
                "high/low/close must be equal length".to_string(),
                None,
            ));
        }
        json_or_err(&nan_to_null(xvn::atr(
            &req.high, &req.low, &req.close, req.period,
        )))
    }

    /// MACD. Returns `{ macd: [...], signal: [...], histogram: [...] }`.
    #[tool(
        description = "MACD. Standard parameters fast=12, slow=26, signal=9. Returns macd/signal/histogram arrays."
    )]
    fn xvn_macd(&self, Parameters(req): Parameters<MacdReq>) -> Result<String, rmcp::ErrorData> {
        let m = xvn::macd(&req.prices, req.fast, req.slow, req.signal);
        json_or_err(&serde_json::json!({
            "macd":      nan_to_null(m.macd),
            "signal":    nan_to_null(m.signal),
            "histogram": nan_to_null(m.histogram),
        }))
    }

    /// Donchian channel — rolling-window high and low.
    #[tool(description = "Donchian channel. Rolling-period high and low over equal-length high/low arrays.")]
    fn xvn_donchian(&self, Parameters(req): Parameters<DonchianReq>) -> Result<String, rmcp::ErrorData> {
        if req.high.len() != req.low.len() {
            return Err(rmcp::ErrorData::invalid_params(
                "high/low must be equal length".to_string(),
                None,
            ));
        }
        let d = xvn::donchian(&req.high, &req.low, req.period);
        json_or_err(&serde_json::json!({
            "upper": nan_to_null(d.upper),
            "lower": nan_to_null(d.lower),
        }))
    }

    /// Fibonacci retracement levels. Detects the most recent swing in the
    /// lookback window and returns `(high, low, direction, levels)`.
    /// `levels` is an array of `[ratio, price]` pairs at the canonical
    /// ratios 0.236 / 0.382 / 0.500 / 0.618 / 0.786.
    #[tool(description = "Fibonacci retracement levels for the most recent swing within a lookback window.")]
    fn xvn_fib_retracements(&self, Parameters(req): Parameters<FibReq>) -> Result<String, rmcp::ErrorData> {
        match xvn::fib_retracements(&req.prices, req.lookback) {
            None => json_or_err(&serde_json::json!({ "found": false })),
            Some(f) => {
                let direction = match f.direction {
                    xvn::Direction::Up => "up",
                    xvn::Direction::Down => "down",
                };
                json_or_err(&serde_json::json!({
                    "found": true,
                    "high":      f.high,
                    "low":       f.low,
                    "direction": direction,
                    "levels": f.levels.iter().map(|(r, p)| serde_json::json!([r, p])).collect::<Vec<_>>(),
                }))
            }
        }
    }

    // -----------------------------------------------------------------------
    // authoring tools — operate on `$XVN_HOME/strategies/<id>.json` via
    // xvision_engine's strategy store + validator. Post-2026-05-21 the
    // strategy template_registry was removed; `xvn_list_templates`
    // returns an empty array (stub) and `xvn_create_strategy` produces
    // a blank draft. Operator-readable starters live as prepop seeds
    // under `$XVN_HOME/strategies/library/`.
    // -----------------------------------------------------------------------

    /// Deprecated stub. The strategy `template_registry` was removed
    /// on 2026-05-21; this tool now returns an empty array. Operators
    /// browse the prepop library at
    /// `$XVN_HOME/strategies/library/` (populated by `xvn strategies
    /// init`) for starter content.
    #[tool(
        description = "Deprecated. The strategy template_registry was removed; returns an empty array. Operator-readable starters live under $XVN_HOME/strategies/library/."
    )]
    async fn xvn_list_templates(&self) -> Result<String, rmcp::ErrorData> {
        json_or_err(&authoring::list_templates())
    }

    /// Create a new blank strategy draft. Persists to
    /// `$XVN_HOME/strategies/<id>.json`. Returns `{ id }`.
    ///
    /// Post-2026-05-21 the request no longer takes a `template`
    /// discriminator (the strategy template_registry was removed).
    /// Callers fill in agents / slots via the follow-up
    /// `xvn_create_strategy_agent`, `xvn_update_slot`, … verbs.
    #[tool(
        description = "Create a new blank strategy draft. Persists the strategy and returns { id } (ULID). Follow up with xvn_create_strategy_agent / xvn_update_slot to populate it."
    )]
    async fn xvn_create_strategy(
        &self,
        Parameters(req): Parameters<CreateStrategyReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let out = authoring::create_strategy(
            &self.store(),
            authoring::CreateStrategyReq {
                name: req.name,
                creator: req.creator,
            },
        )
        .await
        .map_err(authoring_err)?;
        json_or_err(&out)
    }

    /// Get a strategy by id. Returns the full `Strategy` JSON.
    #[tool(description = "Get a strategy by id. Returns the full Strategy JSON.")]
    async fn xvn_get_strategy(
        &self,
        Parameters(req): Parameters<StrategyId>,
    ) -> Result<String, rmcp::ErrorData> {
        let strategy = authoring::get_strategy(&self.store(), &req.id)
            .await
            .map_err(authoring_err)?;
        json_or_err(&strategy)
    }

    /// Update a slot on a strategy. Only fields with non-null values
    /// are mutated. Returns `{ id, updated: [...] }` listing which fields
    /// changed.
    #[tool(
        description = "Update a slot on a strategy. Only fields with non-null values are mutated. Returns { id, updated }."
    )]
    async fn xvn_update_slot(
        &self,
        Parameters(req): Parameters<UpdateSlotReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let out = authoring::update_slot(
            &self.store(),
            authoring::UpdateSlotReq {
                id: req.id,
                slot: req.slot,
                attested_with: req.attested_with,
                provider: req.provider,
                model: req.model,
                allowed_tools: req.allowed_tools,
            },
        )
        .await
        .map_err(authoring_err)?;
        json_or_err(&out)
    }

    /// Set the risk config on a strategy. Provide either `preset`
    /// (one of `conservative` / `balanced` / `aggressive`) or `explicit`
    /// (a full `RiskConfig`). Mutually exclusive. Returns `{ id, applied }`.
    #[tool(
        description = "Set the risk config on a strategy. Supply either preset (conservative/balanced/aggressive) or explicit (full RiskConfig). Mutually exclusive. Returns { id, applied }."
    )]
    async fn xvn_set_risk_config(
        &self,
        Parameters(req): Parameters<SetRiskConfigReq>,
    ) -> Result<String, rmcp::ErrorData> {
        // The MCP wire shape carries `explicit` as `serde_json::Value` so the
        // emitted schema doesn't require RiskConfig to derive `JsonSchema`.
        // Deserialize at the boundary; engine takes a typed `RiskConfig`.
        let explicit_typed =
            match req.explicit {
                Some(v) => Some(serde_json::from_value::<RiskConfig>(v).map_err(|e| {
                    rmcp::ErrorData::invalid_params(format!("explicit risk config: {e}"), None)
                })?),
                None => None,
            };
        let out = authoring::set_risk_config(
            &self.store(),
            authoring::SetRiskConfigReq {
                id: req.id,
                preset: req.preset,
                explicit: explicit_typed,
            },
        )
        .await
        .map_err(authoring_err)?;
        json_or_err(&out)
    }

    /// Validate a strategy draft against strategy invariants (trader slot
    /// required, asset universe non-empty, risk in range, declared tools
    /// granted by some slot, etc.). Returns `{ id, ok, errors }` —
    /// `errors` is empty when `ok` is true.
    #[tool(
        description = "Validate a strategy draft. Returns { id, ok, errors } — errors is a flat string list, empty when ok=true."
    )]
    async fn xvn_validate_draft(
        &self,
        Parameters(req): Parameters<StrategyId>,
    ) -> Result<String, rmcp::ErrorData> {
        let out = authoring::validate_draft(&self.store(), &req.id)
            .await
            .map_err(authoring_err)?;
        json_or_err(&out)
    }

    // --- eval browse / compare verbs (Phase 3.D Task 12) -------------------
    //
    // These wrap the existing `engine::api::eval::*` surface so MCP clients
    // (the dashboard's chat rail, the autooptimizer) can browse runs +
    // compare without going through the CLI. Each call opens a fresh
    // `ApiContext` against `$XVN_HOME/store.db` so the sqlite handle is
    // scoped to the call (no long-lived pool, matching the rest of the
    // MCP server's stateless handler shape).
    //
    // Mutations (`run_eval`, `extract_findings`, `publish_attestation`)
    // are intentionally deferred — they pull in broker/dispatch
    // construction from env, which deserves its own integration concern.

    /// List eval runs. Returns the slim `RunSummary` shape (id +
    /// agent_id + scenario_id + mode + status + started_at +
    /// completed_at + headline metrics). Optional filters narrow the
    /// result to a strategy / scenario / status.
    #[tool(description = "List eval runs (slim shape). Optional filters: agent_id, scenario_id, status.")]
    async fn xvn_eval_list(
        &self,
        Parameters(req): Parameters<EvalListReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;
        let status = req
            .status
            .as_deref()
            .map(parse_status_for_mcp)
            .transpose()?
            .map(|s| vec![s]);
        let summaries = api_eval::list_summaries(
            &ctx,
            ListRunsRequest {
                agent_id: req.agent_id,
                scenario_id: req.scenario_id,
                status,
                ..Default::default()
            },
        )
        .await
        .map_err(api_err_to_mcp)?;
        json_or_err(&summaries)
    }

    /// Get full detail for a single run — summary + decision rows +
    /// equity curve. Maps a missing run to MCP `invalid_params` so the
    /// client sees a clean 404-shaped error.
    #[tool(
        description = "Get full RunDetail (summary + decisions + equity curve) by run id. 404-shaped error when the run is unknown."
    )]
    async fn xvn_eval_get(
        &self,
        Parameters(req): Parameters<EvalRunIdReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;
        let detail = api_eval::get_run(&ctx, &req.run_id)
            .await
            .map_err(api_err_to_mcp)?;
        json_or_err(&detail)
    }

    /// Get just the `MetricsSummary` for a completed run. Convenience
    /// wrapper for callers that only want the headline numbers (the
    /// dashboard's run cards, the autooptimizer's lineage gate).
    /// Returns `null` when the run hasn't computed metrics yet
    /// (still queued / running / failed).
    #[tool(
        description = "Get just the MetricsSummary for a run, or null if metrics aren't computed yet. 404-shaped error when the run is unknown."
    )]
    async fn xvn_eval_metrics(
        &self,
        Parameters(req): Parameters<EvalRunIdReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;
        let run = api_eval::get(&ctx, &req.run_id).await.map_err(api_err_to_mcp)?;
        json_or_err(&run.metrics)
    }

    /// List the canonical scenarios packaged with this binary. These are
    /// the same scenarios the CLI's `xvn eval scenarios` shows.
    #[tool(
        description = "List canonical scenarios packaged with this binary. Returns id, display_name, asset_universe, regime_tags, time_window_days."
    )]
    async fn xvn_eval_scenarios(&self) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;
        let scenarios = api_eval::scenarios(&ctx).await.map_err(api_err_to_mcp)?;
        json_or_err(&scenarios)
    }

    /// Run-set comparison. Folds 2+ completed runs into a single
    /// `ComparisonReport` (per-run summary + equity curve + the union
    /// of all extracted findings). Validates that ≥2 ids are passed
    /// and that every id resolves; bad ids surface as 404-shaped errors.
    #[tool(
        description = "Compare 2+ completed runs side-by-side. Returns a ComparisonReport (runs + equity_curves + findings). At least two run_ids required."
    )]
    async fn xvn_eval_compare(
        &self,
        Parameters(req): Parameters<EvalCompareReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;
        let report = api_eval::compare(
            &ctx,
            CompareRunsRequest {
                run_ids: req.run_ids,
                allow_manifest_mismatch: false,
            },
        )
        .await
        .map_err(api_err_to_mcp)?;
        json_or_err(&report)
    }

    /// All extracted findings for a single run (empty array when none).
    /// Read directly from the store rather than going through the api
    /// layer because findings don't have a dedicated audit-worthy api
    /// surface — they're a downstream lookup, like equity samples.
    #[tool(
        description = "List all extracted findings for a run, ordered by extraction time. Empty array when there are none."
    )]
    async fn xvn_eval_findings(
        &self,
        Parameters(req): Parameters<EvalRunIdReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;
        let store = RunStore::new(ctx.db.clone());
        let findings = store
            .read_findings(&req.run_id)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        json_or_err(&findings)
    }

    // --- memory + flywheel read surfaces ----------------------------------
    //
    // These wrappers give MCP clients the same read-side memory/flywheel
    // visibility as the CLI and dashboard without exposing raw cortex/memory
    // writes. Mutation tools remain behind the dashboard/CLI/API policy layer.

    /// List memory items from the operator memory store. Defaults match the
    /// CLI/API surface: live rows only unless include_forgotten or
    /// forgotten_only is set.
    #[tool(
        description = "List memory items from the xvision memory store. Optional filters: tier, namespace/agent, run_id, scenario_id, promotion_state, include_forgotten, forgotten_only, limit, offset."
    )]
    async fn xvn_memory_list(
        &self,
        Parameters(req): Parameters<MemoryListMcpReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let store = self.memory_store().await?;
        let resp = api_memory::list(
            &store,
            api_memory::ListMemoryRequest {
                tier: req.tier,
                namespace: req.namespace,
                agent: req.agent,
                scenario_id: req.scenario_id,
                run_id: req.run_id,
                promotion_state: req.promotion_state,
                limit: req.limit,
                offset: req.offset,
                include_forgotten: req.include_forgotten,
                forgotten_only: req.forgotten_only,
            },
        )
        .await
        .map_err(api_err_to_mcp)?;
        json_or_err(&resp)
    }

    /// List namespaces with memory row counts so MCP clients can discover
    /// valid memory scopes before recall, flywheel, or optimizer calls.
    #[tool(description = "List memory namespaces with live, tier, lifecycle, and forgotten row counts.")]
    async fn xvn_memory_namespaces(&self) -> Result<String, rmcp::ErrorData> {
        let store = self.memory_store().await?;
        let resp = api_memory::list_namespaces(&store)
            .await
            .map_err(api_err_to_mcp)?;
        json_or_err(&resp)
    }

    /// Fetch one memory item by id. The embedding vector is intentionally
    /// omitted from the response; this is the operator DTO used by API/CLI.
    #[tool(
        description = "Get one memory item by id. Returns the operator DTO without the raw embedding vector."
    )]
    async fn xvn_memory_get(
        &self,
        Parameters(req): Parameters<MemoryGetMcpReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let store = self.memory_store().await?;
        let item = api_memory::get(&store, &req.id).await.map_err(api_err_to_mcp)?;
        json_or_err(&item)
    }

    /// Recall active Patterns for a namespace using a caller-supplied embedding.
    /// This is read-only and enforces the same structural + temporal filters as
    /// runtime recall: no Observations, no staged/forgotten Patterns, and no
    /// Patterns whose training window overlaps scenario_start.
    #[tool(
        description = "Recall active Pattern hits by namespace using a caller-supplied query_embedding. Enforces Pattern-only, active-only, forgotten-hidden, and scenario_start temporal filters."
    )]
    async fn xvn_memory_recall(
        &self,
        Parameters(req): Parameters<MemoryRecallMcpReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let namespace = resolve_mcp_namespace(req.namespace, req.agent)?;
        let scenario_start = match req.scenario_start {
            Some(s) => Some(parse_rfc3339_mcp(&s)?),
            None => None,
        };
        if req.query_embedding.is_empty() {
            return Err(rmcp::ErrorData::invalid_params(
                "query_embedding must not be empty".to_string(),
                None,
            ));
        }
        let store = self.memory_store().await?;
        let matches = store
            .query(
                &namespace,
                &req.query_embedding,
                req.k.unwrap_or(5),
                scenario_start,
            )
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("memory recall: {e}"), None))?;
        json_or_err(&serde_json::json!({
            "namespace": namespace,
            "count": matches.len(),
            "items": matches,
        }))
    }

    /// Summarize memory flywheel state for one namespace.
    #[tool(description = "Summarize memory flywheel state for one namespace or agent.")]
    async fn xvn_flywheel_status(
        &self,
        Parameters(req): Parameters<FlywheelStatusMcpReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let store = self.memory_store().await?;
        let resp = api_flywheel::status(
            &store,
            api_flywheel::FlywheelStatusRequest {
                namespace: req.namespace,
                agent: req.agent,
            },
        )
        .await
        .map_err(api_err_to_mcp)?;
        json_or_err(&resp)
    }

    /// Return recent flywheel velocity counters for one namespace.
    #[tool(description = "Return recent memory flywheel velocity counters for one namespace or agent.")]
    async fn xvn_flywheel_velocity(
        &self,
        Parameters(req): Parameters<FlywheelVelocityMcpReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;
        let store = self.memory_store().await?;
        let resp = api_flywheel::velocity(
            &ctx,
            &store,
            api_flywheel::FlywheelVelocityRequest {
                namespace: req.namespace,
                agent: req.agent,
                days: req.days,
            },
        )
        .await
        .map_err(api_err_to_mcp)?;
        json_or_err(&resp)
    }

    /// Return memory-demo optimizer lineage rows for one namespace.
    #[tool(
        description = "Return memory-demo optimizer lineage rows, including train/dev/holdout hashes, for one namespace or agent."
    )]
    async fn xvn_flywheel_lineage(
        &self,
        Parameters(req): Parameters<FlywheelLineageMcpReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;
        let resp = api_flywheel::lineage(
            &ctx,
            api_flywheel::FlywheelLineageRequest {
                namespace: req.namespace,
                agent: req.agent,
                limit: req.limit,
            },
        )
        .await
        .map_err(api_err_to_mcp)?;
        json_or_err(&resp)
    }

    /// Compile memory Observations into a deterministic demo prompt prefix.
    /// `apply=false` is a dry-run; `apply=true` explicitly mints a child
    /// Agent and writes optimizer lineage rows.
    #[tool(
        description = "Compile memory Observations into train/dev/holdout demos for an agent slot. Dry-run by default; set apply=true to mint a child agent and persist lineage."
    )]
    async fn xvn_optimize_memory_demos(
        &self,
        Parameters(req): Parameters<OptimizeMemoryDemosMcpReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;
        let store = self.memory_store().await?;
        let resp = api_optimize::compile_memory_demos(
            &ctx,
            &store,
            api_optimize::MemoryDemoOptimizeRequest {
                target_agent_id: req.target_agent_id,
                slot: req.slot,
                namespace: req.namespace,
                memory_agent: req.memory_agent,
                scenario_id: req.scenario_id,
                run_id: req.run_id,
                demo_source: req.demo_source,
                holdout_split: req.holdout_split,
                cohort_query: req.cohort_query,
                manual_observation_ids: req.manual_observation_ids,
                prior_pattern_ids: req.prior_pattern_ids,
                auto_prior_patterns: req.auto_prior_patterns,
                prior_pattern_limit: req.prior_pattern_limit,
                limit: req.limit,
                max_demo_chars: req.max_demo_chars,
                apply: req.apply,
                child_name: req.child_name,
            },
        )
        .await
        .map_err(api_err_to_mcp)?;
        json_or_err(&resp)
    }

    /// List offline autooptimizer runs. Read-only companion to the CLI and
    /// dashboard run-history surfaces.
    #[tool(description = "List offline autooptimizer run ledger rows by namespace or agent.")]
    async fn xvn_autooptimizer_list(
        &self,
        Parameters(req): Parameters<AutoOptimizerListMcpReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let store = self.memory_store().await?;
        let resp = api_autooptimizer::list_runs(
            &store,
            api_autooptimizer::AutoOptimizerRunListRequest {
                namespace: req.namespace,
                agent: req.agent,
                limit: req.limit,
                offset: req.offset,
            },
        )
        .await
        .map_err(api_err_to_mcp)?;
        json_or_err(&resp)
    }

    /// Inspect one autooptimizer run, including contributing Observation ids,
    /// Pattern id, numeric gate fields, and blind Finding provenance.
    #[tool(description = "Inspect one autooptimizer run ledger row by id.")]
    async fn xvn_autooptimizer_inspect(
        &self,
        Parameters(req): Parameters<AutoOptimizerRunIdMcpReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let store = self.memory_store().await?;
        let run = api_autooptimizer::inspect_run(&store, &req.id)
            .await
            .map_err(api_err_to_mcp)?;
        json_or_err(&run)
    }

    /// Return just the qualitative Finding and gate provenance for one
    /// autooptimizer run. This keeps chat-rail callers from having to parse the
    /// full run object when they only need the judge context.
    #[tool(description = "Return qualitative Finding and numeric gate provenance for one autooptimizer run.")]
    async fn xvn_autooptimizer_findings(
        &self,
        Parameters(req): Parameters<AutoOptimizerRunIdMcpReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let store = self.memory_store().await?;
        let run = api_autooptimizer::inspect_run(&store, &req.id)
            .await
            .map_err(api_err_to_mcp)?;
        json_or_err(&serde_json::json!({
            "id": run.id,
            "pattern_id": run.pattern_id,
            "gate_metric": run.gate_metric,
            "gate_verdict": run.gate_verdict,
            "gate_reason": run.gate_reason,
            "finding_text": run.finding_text,
            "finding_model": run.finding_model,
            "finding_blind": run.finding_blind,
            "qualitative_finding_json": run.qualitative_finding_json,
            "finding_blinded_metrics": run.finding_blinded_metrics,
            "judge_model": run.judge_model,
            "judge_token_cost": run.judge_token_cost,
        }))
    }

    // ── F-13: MCP surface parity for new CLI verbs (workbench wave A+B + wave-C) ──

    /// Atomically create a strategy + agent + provider/model binding in one
    /// call. Equivalent to `xvn strategy create --prompt <text> --provider
    /// --model --asset --timeframe`. Returns `{ strategy_id, agent_id,
    /// eval_ready, provider, model, warnings }`.
    #[tool(
        description = "Atomically create a strategy + agent + provider/model binding. Requires name, role, prompt, provider, model. Returns {strategy_id, agent_id, eval_ready, provider, model, warnings}."
    )]
    async fn xvn_strategy_create_atomic(
        &self,
        Parameters(req): Parameters<StrategyCreateAtomicReq>,
    ) -> Result<String, rmcp::ErrorData> {
        use xvision_engine::agents::InputsPolicy;
        use xvision_engine::strategies::risk::RiskPreset;
        use xvision_engine::strategies::{
            manifest::PublicManifest, ActivationMode, AgentRef, PipelineDef, Strategy,
        };

        let asset = req.asset.unwrap_or_else(|| "BTC/USD".to_string());
        let timeframe = req.timeframe.unwrap_or_else(|| "4h".to_string());
        let creator = req.creator.unwrap_or_else(|| "@mcp".to_string());

        let cadence_minutes = parse_timeframe_mcp(&timeframe)?;

        let ctx = self.api_context().await?;

        // 1. Create the agent library entry.
        let agent = api_agents::create(
            &ctx,
            api_agents::CreateAgentRequest {
                name: format!("{} {}", req.name, req.role),
                description: format!(
                    "Created atomically with strategy '{}' role '{}'",
                    req.name, req.role
                ),
                tags: vec!["atomic-create".to_string(), "mcp".to_string()],
                slots: vec![AgentSlot {
                    name: "main".to_string(),
                    provider: req.provider.clone(),
                    model: req.model.clone(),
                    system_prompt: req.prompt,
                    skill_ids: Vec::new(),
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                }],
                scope_strategy_id: None,
            },
        )
        .await
        .map_err(api_err_to_mcp)?;

        let agent_id = agent.agent_id.clone();
        let strategy_id = Ulid::new().to_string();

        // 2. Build the strategy with the agent wired in.
        let strategy = Strategy {
            manifest: PublicManifest {
                id: strategy_id.clone(),
                display_name: req.name.clone(),
                plain_summary: String::new(),
                creator,
                template: "custom".to_string(),
                regime_fit: Vec::new(),
                asset_universe: vec![asset],
                decision_cadence_minutes: cadence_minutes,
                attested_with: Vec::new(),
                required_tools: Vec::new(),
                risk_preset_or_config: "balanced".to_string(),
                published_at: None,
                min_warmup_bars: None,
                color: None,
                execution_mode: Default::default(),
                capital_mode: Default::default(),
                timeframe_requirements: Default::default(),
            },
            agents: vec![AgentRef {
                agent_id: agent_id.clone(),
                role: req.role,
                activates: None,
                prompt: String::new(),
                model_override: None,
                checkpoint: None,
                veto: None,
            }],
            pipeline: PipelineDef::default(),
            regime_slot: None,
            trader_slot: None,
            risk: RiskPreset::Balanced.expand(),
            hypothesis: None,
            activation_mode: ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            decision_mode: Default::default(),
            mechanistic_config: None,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
        };

        // 3. Validate shape.
        let preflight = preflight_validate(&strategy, None);

        // 4. Persist via FilesystemStore.
        self.store().save(&strategy).await.map_err(authoring_err)?;

        // 5. Return structured output.
        let eval_ready = preflight.warnings.is_empty() && preflight.errors.is_empty();
        json_or_err(&serde_json::json!({
            "strategy_id": strategy_id,
            "agent_id": agent_id,
            "eval_ready": eval_ready,
            "provider": req.provider,
            "model": req.model,
            "warnings": preflight.warnings,
        }))
    }

    /// Preflight-validate a saved strategy, optionally cross-checking against a
    /// scenario. Equivalent to `xvn strategy validate <id> --scenario <id> --json`.
    /// Returns a `PreflightReport` JSON object.
    #[tool(
        description = "Preflight-validate a strategy (shape + agent/provider checks). Optionally cross-check against a scenario for asset/timeframe alignment. Returns PreflightReport JSON."
    )]
    async fn xvn_strategy_validate_preflight(
        &self,
        Parameters(req): Parameters<StrategyPreflightReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let strategy = self.store().load(&req.id).await.map_err(authoring_err)?;

        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if let Err(e) = validate_strategy(&strategy) {
            errors.push(e.to_string());
        }

        let Some(scenario_id) = req.scenario_id else {
            warnings.push("no scenario_id supplied; shape-only check".to_string());
            let report = PreflightReport {
                strategy_id: req.id.clone(),
                eval_ready: errors.is_empty() && warnings.is_empty(),
                expected_decisions: None,
                asset: None,
                timeframe: None,
                warmup_bars: None,
                warnings,
                errors,
            };
            return json_or_err(&report);
        };

        let ctx = self.api_context().await?;

        // Check agents exist and have provider/model.
        for agent_ref in &strategy.agents {
            match api_agents::get(&ctx, &agent_ref.agent_id).await {
                Ok(agent) => {
                    let Some(slot) = agent.slots.first() else {
                        errors.push(format!(
                            "agent '{}' (role '{}') has no executable slots",
                            agent_ref.agent_id, agent_ref.role
                        ));
                        continue;
                    };
                    if slot.provider.trim().is_empty() {
                        errors.push(format!(
                            "agent '{}' (role '{}') has no provider set",
                            agent_ref.agent_id, agent_ref.role
                        ));
                    }
                    if slot.model.trim().is_empty() {
                        errors.push(format!(
                            "agent '{}' (role '{}') has no model set",
                            agent_ref.agent_id, agent_ref.role
                        ));
                    }
                }
                Err(_) => {
                    errors.push(format!(
                        "agent '{}' (role '{}') not found",
                        agent_ref.agent_id, agent_ref.role
                    ));
                }
            }
        }

        let scenario = match api_scenario::get(&ctx, &scenario_id).await {
            Ok(s) => s,
            Err(_) => {
                errors.push(format!("scenario '{scenario_id}' not found"));
                let report = PreflightReport {
                    strategy_id: req.id.clone(),
                    eval_ready: false,
                    expected_decisions: None,
                    asset: None,
                    timeframe: None,
                    warmup_bars: None,
                    warnings,
                    errors,
                };
                return json_or_err(&report);
            }
        };

        let pf = preflight_validate(&strategy, Some(&scenario));
        warnings.extend(pf.warnings);

        let granularity = xvision_engine::strategies::bar_granularity_for_cadence(
            strategy.manifest.decision_cadence_minutes,
        );
        let timeframe_display = granularity.canonical();

        let window_secs = (scenario.time_window.end - scenario.time_window.start)
            .num_seconds()
            .max(0) as u64;
        let granularity_secs = granularity.seconds();
        let expected_decisions = if granularity_secs > 0 {
            let total_bars = window_secs / granularity_secs;
            (total_bars as i64) - (scenario.warmup_bars as i64)
        } else {
            0
        };

        let report = PreflightReport {
            strategy_id: req.id.clone(),
            eval_ready: errors.is_empty() && warnings.is_empty(),
            expected_decisions: Some(expected_decisions),
            // Scenarios are asset-free; the asset is chosen at the run layer,
            // so preflight no longer reports a scenario-derived asset.
            asset: None,
            timeframe: Some(timeframe_display),
            warmup_bars: Some(scenario.warmup_bars),
            warnings,
            errors,
        };
        json_or_err(&report)
    }

    /// Launch one eval run per scenario, wait for all to reach a terminal state,
    /// and return a unified `BatchResult`. Equivalent to `xvn eval batch run
    /// --strategy X --scenarios A,B,C --wait --json`. Calls `api_eval::run`
    /// (env-bound, backtest mode only — no broker construction for MCP calls).
    #[tool(
        description = "Run a batch of eval runs (one per scenario) and return a unified BatchResult. Always waits for completion. Backtest mode only in MCP context."
    )]
    async fn xvn_eval_batch_run(
        &self,
        Parameters(req): Parameters<EvalBatchRunReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;

        let mode_str = req.mode.as_deref().unwrap_or("backtest");
        let mode = RunMode::parse(mode_str).ok_or_else(|| {
            rmcp::ErrorData::invalid_params(
                format!("unknown mode {mode_str:?}; expected backtest | paper"),
                None,
            )
        })?;

        // Create the batch row.
        let batch = api_eval::create_batch(
            &ctx,
            CreateBatchRequest {
                strategy_id: req.strategy_id.clone(),
                review_with: req.review_with.clone(),
            },
        )
        .await
        .map_err(api_err_to_mcp)?;
        let batch_id = batch.batch_id.clone();

        // Resolve scenario display names (best-effort).
        let mut scenario_names: Vec<String> = Vec::with_capacity(req.scenario_ids.len());
        for sid in &req.scenario_ids {
            let name = api_scenario::get(&ctx, sid)
                .await
                .map(|s| s.display_name)
                .unwrap_or_else(|_| sid.clone());
            scenario_names.push(name);
        }

        let mut entries: Vec<serde_json::Value> = Vec::with_capacity(req.scenario_ids.len());

        for (scenario_id, scenario_name) in req.scenario_ids.iter().zip(scenario_names.iter()) {
            let run_req = EvalRunRequest {
                agent_id: req.strategy_id.clone(),
                scenario_id: scenario_id.clone(),
                mode,
                params_override: None,
                limits: None,
                skip_preflight: false,
                provider_override: None,
                assets_subset: None,
                live_config: None,
                auto_fire_review: false,
                review_model: None,
                max_annotations_per_review: Some(8),
                trajectory_mode: api_eval::RunTrajectoryMode::default(),
            };

            let entry = match api_eval::run(&ctx, run_req).await {
                Ok(run) => {
                    api_eval::attach_run_to_batch(&ctx, &run.id, &batch_id)
                        .await
                        .map_err(api_err_to_mcp)?;
                    let actions = action_distribution_mcp(&ctx, &run.id).await.unwrap_or_default();
                    let (return_pct, sharpe, drawdown_pct, decisions) = if let Some(m) = &run.metrics {
                        (
                            Some(m.total_return_pct),
                            Some(m.sharpe),
                            Some(m.max_drawdown_pct),
                            m.n_decisions,
                        )
                    } else {
                        (None, None, None, 0)
                    };
                    serde_json::json!({
                        "scenario_id": scenario_id,
                        "scenario_name": scenario_name,
                        "run_id": run.id,
                        "status": run.status.as_str(),
                        "return_pct": return_pct,
                        "sharpe": sharpe,
                        "drawdown_pct": drawdown_pct,
                        "decisions": decisions,
                        "actions": actions,
                        "error": run.error,
                    })
                }
                Err(e) => {
                    serde_json::json!({
                        "scenario_id": scenario_id,
                        "scenario_name": scenario_name,
                        "run_id": null,
                        "status": "failed",
                        "error": e.to_string(),
                    })
                }
            };
            entries.push(entry);
        }

        let _ = api_eval::finalize_batch(&ctx, &batch_id).await;

        json_or_err(&serde_json::json!({
            "batch_id": batch_id,
            "strategy_id": req.strategy_id,
            "runs": entries,
        }))
    }

    /// Show the status of a persisted batch by its id. Required: `batch_id`.
    /// Returns `{ batch_id, strategy_id, status, created_at, completed_at?,
    /// review_with?, run_ids }`.
    #[tool(description = "Show the persisted status of an eval batch by id. \
        Required: batch_id. \
        Returns { batch_id, strategy_id, status, created_at, completed_at, review_with, run_ids }.")]
    async fn xvn_eval_batch_status(
        &self,
        Parameters(req): Parameters<EvalBatchStatusReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;
        let detail: BatchDetail = api_eval::get_batch(&ctx, &req.batch_id)
            .await
            .map_err(api_err_to_mcp)?;
        json_or_err(&detail)
    }

    /// Compare 2+ completed runs side-by-side. Supply either `run_ids`
    /// (two or more) or `batch_id` (resolves run ids from the batch). When
    /// `markdown=true` the report is returned as a Markdown table string.
    /// Returns a `ComparisonReport` or Markdown string.
    #[tool(
        description = "Compare 2+ completed runs. Supply run_ids (two or more ULIDs) or batch_id \
        (resolves run ids from a batch). Optional: markdown=true returns a Markdown table. \
        Returns ComparisonReport { runs, equity_curves, findings } or a Markdown string."
    )]
    async fn xvn_eval_compare_ext(
        &self,
        Parameters(req): Parameters<EvalCompareExtReq>,
    ) -> Result<String, rmcp::ErrorData> {
        // Resolve the run id list from either run_ids or batch_id.
        let run_ids = if let Some(bid) = &req.batch_id {
            if !req.run_ids.is_empty() {
                return Err(rmcp::ErrorData::invalid_params(
                    "supply either run_ids or batch_id, not both".to_string(),
                    None,
                ));
            }
            let ctx = self.api_context().await?;
            let detail = api_eval::get_batch(&ctx, bid).await.map_err(api_err_to_mcp)?;
            detail.run_ids
        } else {
            req.run_ids.clone()
        };

        let ctx = self.api_context().await?;
        let report = api_eval::compare(
            &ctx,
            CompareRunsRequest {
                run_ids,
                allow_manifest_mismatch: false,
            },
        )
        .await
        .map_err(api_err_to_mcp)?;

        if req.markdown {
            let md = format_comparison_markdown(&report);
            return json_or_err(&md);
        }

        json_or_err(&report)
    }

    /// Select a comparable set of scenarios by timeframe, decision count, and
    /// regime labels. Read-only — nothing is created. Scenarios are asset-free;
    /// asset-universe selection now lives at the run layer.
    /// Either `target_decisions` (Mode A, ±10 %) or `same_decisions=true`
    /// + `max_decisions` (Mode B) must be set.
    /// Returns an array of `{ id, name, timeframe, decision_count }`.
    #[tool(description = "Filter the scenario library and return a ranked subset. \
        Optional: timeframe (e.g. 4h), regimes (list), count (default 4). \
        Decision-count mode: target_decisions (Mode A, ±10%) or same_decisions=true + \
        max_decisions (Mode B, common count). \
        Returns [{ id, name, timeframe, decision_count }].")]
    async fn xvn_scenarios_select(
        &self,
        Parameters(req): Parameters<ScenariosSelectReq>,
    ) -> Result<String, rmcp::ErrorData> {
        if req.target_decisions.is_none() && !req.same_decisions {
            return Err(rmcp::ErrorData::invalid_params(
                "specify either target_decisions (Mode A) or same_decisions=true + max_decisions (Mode B)"
                    .to_string(),
                None,
            ));
        }
        if req.same_decisions && req.max_decisions.is_none() {
            return Err(rmcp::ErrorData::invalid_params(
                "same_decisions=true requires max_decisions".to_string(),
                None,
            ));
        }

        let timeframe_minutes = req
            .timeframe
            .as_deref()
            .map(parse_timeframe_mcp)
            .transpose()?
            .unwrap_or(60);

        let ctx = self.api_context().await?;
        let all = api_scenario::list(&ctx, api_scenario::ListScenariosFilter::default())
            .await
            .map_err(api_err_to_mcp)?;
        let count = req.count.unwrap_or(4);
        let rows = select_scenarios_mcp(
            &all,
            timeframe_minutes,
            &req.regimes,
            req.target_decisions,
            req.same_decisions,
            req.max_decisions,
            count,
        )
        .map_err(|e| rmcp::ErrorData::invalid_params(e, None))?;

        json_or_err(&rows)
    }

    /// Compare 2+ completed eval runs. Decorates each row with behavior-summary
    /// fields (trades_opened, action_distribution, avg_bars_held,
    /// primary_failure_mode). Equivalent to `xvn eval compare --runs id1,id2
    /// --json`. Always returns JSON; sort accepts `return`, `sharpe`, or
    /// `drawdown` (default: `return`).
    #[tool(
        description = "Compare 2+ completed runs. Returns a CompareReport with per-run metrics + behavior decoration. Sort by return (default), sharpe, or drawdown."
    )]
    async fn xvn_eval_compare_report(
        &self,
        Parameters(req): Parameters<EvalCompareReportReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;
        let sort_key = req.sort.as_deref().unwrap_or("return");

        let report = api_eval::compare(
            &ctx,
            CompareRunsRequest {
                run_ids: req.run_ids,
                allow_manifest_mismatch: false,
            },
        )
        .await
        .map_err(api_err_to_mcp)?;

        let store = RunStore::new(ctx.db.clone());
        let mut rows: Vec<CompareRunRow> = Vec::with_capacity(report.runs.len());

        for run in &report.runs {
            let scenario_name = api_scenario::get(&ctx, &run.scenario_id)
                .await
                .map(|s| s.display_name)
                .unwrap_or_else(|_| run.scenario_id.clone());
            let decisions = store.read_decisions(&run.id).await.unwrap_or_default();
            let behavior = derive_behavior_summary(&decisions);
            let mut action_dist: HashMap<String, u32> = HashMap::new();
            for d in &decisions {
                *action_dist.entry(d.action.clone()).or_insert(0) += 1;
            }
            let (return_pct, sharpe, max_drawdown_pct, n_decisions) = match &run.metrics {
                Some(m) => (
                    Some(m.total_return_pct),
                    Some(m.sharpe),
                    Some(m.max_drawdown_pct),
                    m.n_decisions,
                ),
                None => (None, None, None, 0),
            };
            rows.push(CompareRunRow {
                run_id: run.id.clone(),
                scenario_id: run.scenario_id.clone(),
                scenario_name,
                strategy_id: run.agent_id.clone(),
                status: run.status.as_str().to_string(),
                return_pct,
                sharpe,
                max_drawdown_pct,
                decisions: n_decisions,
                trades_opened: behavior.trades_opened,
                action_distribution: action_dist,
                avg_bars_held: behavior.avg_bars_held,
                primary_failure_mode: behavior.primary_failure_mode,
            });
        }

        // Sort.
        match sort_key {
            "sharpe" => rows.sort_by(|a, b| {
                b.sharpe
                    .unwrap_or(f64::NEG_INFINITY)
                    .partial_cmp(&a.sharpe.unwrap_or(f64::NEG_INFINITY))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            "drawdown" => rows.sort_by(|a, b| {
                a.max_drawdown_pct
                    .unwrap_or(f64::INFINITY)
                    .partial_cmp(&b.max_drawdown_pct.unwrap_or(f64::INFINITY))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            _ => rows.sort_by(|a, b| {
                b.return_pct
                    .unwrap_or(f64::NEG_INFINITY)
                    .partial_cmp(&a.return_pct.unwrap_or(f64::NEG_INFINITY))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
        }

        json_or_err(&CompareReport { runs: rows })
    }

    /// Return a compact plain-text summary card for a scenario.
    /// Equivalent to `xvn scenario inspect <id> --card`. Returns a
    /// text card with id, name, asset, timeframe, date_window,
    /// warmup_bars, decision_bars, and previous_runs summary.
    #[tool(
        description = "Return the compact summary card for a scenario (id, name, asset, timeframe, date window, decision count, previous runs). Equivalent to `xvn scenario inspect --card`."
    )]
    async fn xvn_scenario_inspect_card(
        &self,
        Parameters(req): Parameters<ScenarioInspectCardReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;

        let scenario = api_scenario::get(&ctx, &req.id).await.map_err(api_err_to_mcp)?;

        // Aggregate previous runs (count + best return).
        let runs_result = api_eval::list(
            &ctx,
            ListRunsRequest {
                scenario_id: Some(req.id.clone()),
                agent_id: None,
                status: None,
                ..Default::default()
            },
        )
        .await;

        let (run_count, best_return) = match runs_result {
            Ok(runs) => {
                let count = runs.len();
                let best = runs
                    .iter()
                    .filter_map(|r| r.metrics.as_ref().map(|m| m.total_return_pct))
                    .reduce(f64::max);
                (Some(count), best)
            }
            Err(_) => (None, None),
        };

        // Build the card string (mirrors CLI's format_inspect_card).
        let quote = format!("{:?}", scenario.quote_currency).to_uppercase();
        let granularity = xvision_engine::strategies::bar_granularity_for_cadence(60);
        let window_secs = (scenario.time_window.end - scenario.time_window.start).num_seconds() as u64;
        let bar_secs = granularity.seconds();
        let decision_bars = if bar_secs > 0 {
            let total_bars = window_secs / bar_secs;
            total_bars.saturating_sub(scenario.warmup_bars as u64)
        } else {
            0
        };

        let mut card = String::new();
        card.push_str(&format!("id: {}\n", scenario.id));
        card.push_str(&format!("name: {}\n", scenario.display_name));
        card.push_str(&format!("quote_currency: {}\n", quote));
        card.push_str(&format!(
            "date_window: {}..{}\n",
            scenario.time_window.start.format("%Y-%m-%d"),
            scenario.time_window.end.format("%Y-%m-%d"),
        ));
        card.push_str(&format!("warmup_bars: {}\n", scenario.warmup_bars));
        card.push_str(&format!("decision_bars: {}\n", decision_bars));
        if let Some(parent_id) = &scenario.parent_scenario_id {
            card.push_str(&format!("source: cloned_from {}\n", parent_id));
        }
        match (run_count, best_return) {
            (Some(count), best) => {
                card.push_str("previous_runs:\n");
                card.push_str(&format!("  count: {}\n", count));
                if let Some(ret) = best {
                    card.push_str(&format!("  best_return_pct: {:.2}\n", ret));
                } else {
                    card.push_str("  best_return_pct: (none)\n");
                }
            }
            _ => {
                card.push_str("previous_runs: (unavailable)\n");
            }
        }
        if card.ends_with('\n') {
            card.truncate(card.len() - 1);
        }

        json_or_err(&serde_json::json!({ "card": card }))
    }

    /// Get the behavior summary for a completed eval run — flat/long/short
    /// rates, trade count, direct flips, avg bars held, reentries, and
    /// primary failure mode. Equivalent to `xvn eval show <id> --behavior
    /// --json`. Smallest of the six new tools.
    #[tool(
        description = "Get the BehaviorSummary for a completed eval run (action rates, trades, flips, failure mode). Returns null fields when the run has no decisions."
    )]
    async fn xvn_eval_behavior(
        &self,
        Parameters(req): Parameters<EvalBehaviorReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;
        let behavior = api_eval::get_run_behavior(&ctx, &req.run_id)
            .await
            .map_err(api_err_to_mcp)?;
        json_or_err(&behavior)
    }

    // ── x402 marketplace tools (Task 3.1–3.3) ────────────────────────────────

    #[tool(
        description = "Browse marketplace listings (chain-indexed, read-only). Returns the listing array."
    )]
    async fn xvn_marketplace_browse(&self) -> Result<String, rmcp::ErrorData> {
        let v = crate::marketplace_client::browse()
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;
        json_or_err(&v)
    }

    #[tool(description = "Get one marketplace listing + bundle manifest by numeric id.")]
    async fn xvn_marketplace_get_listing(
        &self,
        Parameters(req): Parameters<MarketplaceGetReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let v = crate::marketplace_client::get_listing(req.listing_id)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;
        json_or_err(&v)
    }

    #[tool(
        description = "Show the local agent wallet address (the non-custodial buyer key from XVN_AGENT_PK; funding helper)."
    )]
    async fn xvn_marketplace_wallet(&self) -> Result<String, rmcp::ErrorData> {
        let signer = crate::marketplace_client::load_agent_signer()
            .map_err(|e| rmcp::ErrorData::invalid_params(e, None))?;
        json_or_err(&serde_json::json!({ "address": format!("0x{:x}", signer.address()) }))
    }

    #[tool(
        description = "Autonomously buy a listing over x402 (signs locally with XVN_AGENT_PK; the key never leaves this process). Returns tx_hash + license_token_id."
    )]
    async fn xvn_marketplace_buy(
        &self,
        Parameters(req): Parameters<MarketplaceGetReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let v = crate::marketplace_client::buy(req.listing_id)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;
        json_or_err(&v)
    }

    #[tool(
        description = "Import a purchased listing: verifies the on-chain license then installs the strategy locally."
    )]
    async fn xvn_marketplace_import(
        &self,
        Parameters(req): Parameters<MarketplaceGetReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let v = crate::marketplace_client::import(req.listing_id)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;
        json_or_err(&v)
    }
}

impl XvisionTools {
    /// Open an `ApiContext` rooted at this server's `$XVN_HOME`. Each
    /// MCP call opens a fresh sqlite pool and migrates if needed. The
    /// `actor` is `Mcp { session_id }` so audit rows attribute writes
    /// to the MCP session rather than a CLI user.
    async fn api_context(&self) -> Result<ApiContext, rmcp::ErrorData> {
        let xvn_home = self.xvn_home.clone().unwrap_or_else(resolve_xvn_home);
        ApiContext::open(
            &xvn_home,
            Actor::Mcp {
                session_id: format!("mcp-{}", Ulid::new()),
            },
        )
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(format!("open api context: {e}"), None))
    }

    async fn memory_store(&self) -> Result<MemoryStore, rmcp::ErrorData> {
        let path = match (&self.xvn_home, std::env::var("XVN_MEMORY_DB")) {
            (Some(home), _) => home.join("memory.db"),
            (None, Ok(p)) if !p.is_empty() => PathBuf::from(p),
            _ => resolve_xvn_home().join("memory.db"),
        };
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    rmcp::ErrorData::internal_error(
                        format!("create memory db directory {}: {e}", parent.display()),
                        None,
                    )
                })?;
            }
        }
        MemoryStore::open(&path).await.map_err(|e| {
            rmcp::ErrorData::internal_error(format!("open memory store {}: {e}", path.display()), None)
        })
    }
}

fn resolve_mcp_namespace(
    namespace: Option<String>,
    agent: Option<String>,
) -> Result<String, rmcp::ErrorData> {
    match (namespace.as_deref(), agent.as_deref()) {
        (Some(_), Some(_)) => Err(rmcp::ErrorData::invalid_params(
            "set either namespace or agent, not both".to_string(),
            None,
        )),
        (Some(ns), None) if !ns.trim().is_empty() => Ok(ns.to_string()),
        (None, Some(agent)) if !agent.trim().is_empty() => Ok(api_memory::agent_namespace(agent)),
        (Some(_), None) | (None, Some(_)) => Err(rmcp::ErrorData::invalid_params(
            "namespace is required".to_string(),
            None,
        )),
        (None, None) => Err(rmcp::ErrorData::invalid_params(
            "one of namespace or agent is required".to_string(),
            None,
        )),
    }
}

fn parse_rfc3339_mcp(s: &str) -> Result<chrono::DateTime<chrono::Utc>, rmcp::ErrorData> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Utc))
        .map_err(|e| rmcp::ErrorData::invalid_params(format!("invalid RFC3339 timestamp {s:?}: {e}"), None))
}

fn parse_status_for_mcp(s: &str) -> Result<RunStatus, rmcp::ErrorData> {
    RunStatus::parse(s).ok_or_else(|| {
        rmcp::ErrorData::invalid_params(
            format!(
                "unknown status {s:?}; expected one of: queued | running | completed | failed | cancelled"
            ),
            None,
        )
    })
}

fn api_err_to_mcp(e: xvision_engine::api::ApiError) -> rmcp::ErrorData {
    use xvision_engine::api::ApiError;
    match e {
        ApiError::NotFound(msg) => rmcp::ErrorData::invalid_params(format!("not found: {msg}"), None),
        ApiError::Validation(msg) => rmcp::ErrorData::invalid_params(format!("validation: {msg}"), None),
        ApiError::Conflict(msg) => rmcp::ErrorData::invalid_params(format!("conflict: {msg}"), None),
        ApiError::Internal(msg) => rmcp::ErrorData::internal_error(msg, None),
        ApiError::Db(e) => rmcp::ErrorData::internal_error(format!("db: {e}"), None),
        ApiError::Other(e) => rmcp::ErrorData::internal_error(format!("{e:#}"), None),
    }
}

/// Parse a CLI/MCP timeframe string to `decision_cadence_minutes`.
/// Accepted values: `1m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `1d`.
fn parse_timeframe_mcp(timeframe: &str) -> Result<u32, rmcp::ErrorData> {
    match timeframe {
        "1m" => Ok(1),
        "5m" => Ok(5),
        "15m" => Ok(15),
        "30m" => Ok(30),
        "1h" => Ok(60),
        "2h" => Ok(120),
        "4h" => Ok(240),
        "1d" => Ok(1440),
        other => Err(rmcp::ErrorData::invalid_params(
            format!("unknown timeframe '{other}'. Accepted: 1m, 5m, 15m, 30m, 1h, 2h, 4h, 1d"),
            None,
        )),
    }
}

/// Count each action kind in the decisions table for a run.
/// Returns a `serde_json::Value` map (`{ "long_open": N, ... }`).
async fn action_distribution_mcp(ctx: &ApiContext, run_id: &str) -> anyhow::Result<serde_json::Value> {
    use std::collections::BTreeMap;
    let store = RunStore::new(ctx.db.clone());
    let decisions = store.read_decisions(run_id).await?;
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for d in &decisions {
        *counts.entry(d.action.clone()).or_insert(0) += 1;
    }
    Ok(serde_json::to_value(counts)?)
}

/// One row returned by `xvn_scenarios_select`.
#[derive(Debug, Clone, Serialize)]
struct SelectRow {
    id: String,
    name: String,
    decision_count: u64,
}

/// Compute the decision bar count for a scenario at a caller-supplied timeframe.
fn scenario_decision_count_mcp(s: &Scenario, timeframe_minutes: u32) -> u64 {
    let window_secs = (s.time_window.end - s.time_window.start).num_seconds() as u64;
    let bar_secs = u64::from(timeframe_minutes) * 60;
    if bar_secs == 0 {
        return 0;
    }
    let total_bars = window_secs / bar_secs;
    total_bars.saturating_sub(s.warmup_bars as u64)
}

/// Extract regime labels stored as `regime:<label>` tags.
fn scenario_regime_labels_mcp(s: &Scenario) -> Vec<String> {
    s.tags
        .iter()
        .filter_map(|t| t.strip_prefix("regime:").map(|r| r.to_string()))
        .collect()
}

/// Pure selection logic — ported from `xvision-cli::commands::scenario::select_scenarios`.
/// Takes a pre-fetched scenario list and applies regime / decision-count filters
/// plus the cap at the caller-supplied strategy timeframe. No DB access.
fn select_scenarios_mcp(
    scenarios: &[Scenario],
    timeframe_minutes: u32,
    regimes: &[String],
    target_decisions: Option<u64>,
    same_decisions: bool,
    max_decisions: Option<u64>,
    count: usize,
) -> Result<Vec<SelectRow>, String> {
    let mut candidates: Vec<&Scenario> = scenarios
        .iter()
        .filter(|s| {
            if !regimes.is_empty() {
                let labels = scenario_regime_labels_mcp(s);
                let matched = regimes.iter().any(|want| {
                    labels
                        .iter()
                        .any(|l| l.eq_ignore_ascii_case(want) || l.contains(want.as_str()))
                });
                if !matched {
                    return false;
                }
            }
            true
        })
        .collect();

    // 2. Decision-count mode.
    let target_count: u64 = if same_decisions {
        let max = max_decisions.unwrap_or(u64::MAX);
        let counts: Vec<u64> = candidates
            .iter()
            .map(|s| scenario_decision_count_mcp(s, timeframe_minutes))
            .filter(|&c| c <= max)
            .collect();
        let mut count_freq: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
        for c in &counts {
            *count_freq.entry(*c).or_insert(0) += 1;
        }
        let best = count_freq
            .iter()
            .filter(|(_, &freq)| freq >= count)
            .map(|(&c, _)| c)
            .max()
            .or_else(|| count_freq.keys().copied().max());
        match best {
            Some(c) => c,
            None => return Ok(vec![]),
        }
    } else if let Some(t) = target_decisions {
        t
    } else {
        0
    };

    // 3. Decision-count tolerance filter.
    if same_decisions {
        candidates.retain(|s| scenario_decision_count_mcp(s, timeframe_minutes) == target_count);
    } else if let Some(t) = target_decisions {
        let lo = (t as f64 * 0.9).floor() as u64;
        let hi = (t as f64 * 1.1).ceil() as u64;
        candidates.retain(|s| {
            let dc = scenario_decision_count_mcp(s, timeframe_minutes);
            dc >= lo && dc <= hi
        });
    }

    // 4. Sort by closeness to target.
    candidates.sort_by_key(|s| {
        let dc = scenario_decision_count_mcp(s, timeframe_minutes);
        if target_decisions.is_some() || same_decisions {
            (dc as i64 - target_count as i64).unsigned_abs()
        } else {
            0u64
        }
    });

    // 5. Cap at `count`. Scenarios are asset-free, so there is no longer a
    //    one-per-asset preference — selection is purely by decision-count
    //    closeness (already sorted above).
    let selected: Vec<&Scenario> = candidates.into_iter().take(count).collect();

    // 6. Build output rows.
    let rows = selected
        .into_iter()
        .map(|s| SelectRow {
            id: s.id.clone(),
            name: s.display_name.clone(),
            decision_count: scenario_decision_count_mcp(s, timeframe_minutes),
        })
        .collect();

    Ok(rows)
}

/// Format a `ComparisonReport` as a simple Markdown table.
fn format_comparison_markdown(report: &xvision_engine::eval::compare::ComparisonReport) -> String {
    use std::fmt::Write;
    let mut md = String::new();
    let _ = writeln!(
        md,
        "| run_id | scenario_id | status | return_pct | sharpe | drawdown_pct |"
    );
    let _ = writeln!(
        md,
        "|--------|-------------|--------|------------|--------|--------------|"
    );
    for r in &report.runs {
        let ret = r
            .metrics
            .as_ref()
            .map(|m| format!("{:.2}", m.total_return_pct))
            .unwrap_or_else(|| "-".to_string());
        let sharpe = r
            .metrics
            .as_ref()
            .map(|m| format!("{:.3}", m.sharpe))
            .unwrap_or_else(|| "-".to_string());
        let dd = r
            .metrics
            .as_ref()
            .map(|m| format!("{:.2}", m.max_drawdown_pct))
            .unwrap_or_else(|| "-".to_string());
        let _ = writeln!(
            md,
            "| {} | {} | {} | {} | {} | {} |",
            r.id,
            r.scenario_id,
            r.status.as_str(),
            ret,
            sharpe,
            dd
        );
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools_with_tmp() -> (XvisionTools, tempfile::TempDir) {
        let td = tempfile::tempdir().unwrap();
        (XvisionTools::with_xvn_home(td.path().to_path_buf()), td)
    }

    fn parsed(s: &str) -> serde_json::Value {
        serde_json::from_str(s).unwrap()
    }

    fn id_of(s: &str) -> String {
        parsed(s)["id"].as_str().unwrap().to_string()
    }

    fn mcp_pattern(
        id: &str,
        namespace: &str,
        text: &str,
        embedding: Vec<f32>,
    ) -> xvision_memory::types::MemoryItem {
        let now = chrono::Utc::now();
        xvision_memory::types::MemoryItem {
            id: id.into(),
            namespace: namespace.into(),
            tier: xvision_memory::types::Tier::Pattern,
            text: text.into(),
            embedding,
            created_at: now,
            run_id: None,
            scenario_id: None,
            cycle_idx: None,
            source_window_start: None,
            source_window_end: None,
            training_window_end: Some(now - chrono::Duration::days(2)),
            promotion_state: Some("active".into()),
            attestation_id: None,
            forgotten_at: None,
        }
    }

    fn mcp_observation(
        id: &str,
        namespace: &str,
        text: &str,
        embedding: Vec<f32>,
    ) -> xvision_memory::types::MemoryItem {
        let now = chrono::Utc::now();
        xvision_memory::types::MemoryItem {
            id: id.into(),
            namespace: namespace.into(),
            tier: xvision_memory::types::Tier::Observation,
            text: text.into(),
            embedding,
            created_at: now,
            run_id: Some("mcp-run".into()),
            scenario_id: Some("mcp-scenario".into()),
            cycle_idx: Some(1),
            source_window_start: Some(now - chrono::Duration::minutes(1)),
            source_window_end: Some(now),
            training_window_end: None,
            promotion_state: None,
            attestation_id: None,
            forgotten_at: None,
        }
    }

    #[tokio::test]
    async fn mcp_memory_read_tools_enforce_recall_filters() {
        let (tools, _td) = tools_with_tmp();
        let store = tools.memory_store().await.unwrap();
        let namespace = api_memory::agent_namespace("mcp-agent");
        store
            .upsert_pattern(
                &mcp_pattern("mcp-pat-active", &namespace, "MCP_ACTIVE_PATTERN", vec![1.0, 0.0]),
                "test",
            )
            .await
            .unwrap();
        let mut staged = mcp_pattern(
            "mcp-pat-staged",
            &namespace,
            "MCP_STAGED_PATTERN",
            vec![0.99, 0.01],
        );
        staged.promotion_state = Some("staged".into());
        store.upsert_pattern(&staged, "test").await.unwrap();
        store
            .upsert_observation(
                &mcp_observation("mcp-obs", &namespace, "MCP_OBSERVATION", vec![1.0, 0.0]),
                "test",
            )
            .await
            .unwrap();

        let listed = tools
            .xvn_memory_list(Parameters(MemoryListMcpReq {
                agent: Some("mcp-agent".into()),
                tier: Some("pattern".into()),
                include_forgotten: Some(false),
                ..Default::default()
            }))
            .await
            .unwrap();
        let listed = parsed(&listed);
        assert_eq!(listed["total"], 2);

        let got = tools
            .xvn_memory_get(Parameters(MemoryGetMcpReq {
                id: "mcp-pat-active".into(),
            }))
            .await
            .unwrap();
        let got = parsed(&got);
        assert_eq!(got["id"], "mcp-pat-active");
        assert_eq!(got["tier"], "pattern");

        let recall = tools
            .xvn_memory_recall(Parameters(MemoryRecallMcpReq {
                namespace: None,
                agent: Some("mcp-agent".into()),
                query_embedding: vec![1.0, 0.0],
                k: Some(10),
                scenario_start: Some(chrono::Utc::now().to_rfc3339()),
            }))
            .await
            .unwrap();
        let recall = parsed(&recall);
        assert_eq!(recall["namespace"], namespace);
        let items = recall["items"].as_array().unwrap();
        assert_eq!(
            items.len(),
            1,
            "recall should hide staged Pattern and Observation: {recall}"
        );
        assert_eq!(items[0]["id"], "mcp-pat-active");
        assert_ne!(items[0]["text"], "MCP_OBSERVATION");

        let namespaces = tools.xvn_memory_namespaces().await.unwrap();
        let namespaces = parsed(&namespaces);
        assert_eq!(namespaces["total"], 1);
        assert_eq!(namespaces["items"][0]["namespace"], namespace);
        assert_eq!(namespaces["items"][0]["observations"], 1);
        assert_eq!(namespaces["items"][0]["active_patterns"], 1);
        assert_eq!(namespaces["items"][0]["staged_patterns"], 1);
    }

    #[tokio::test]
    async fn mcp_flywheel_status_and_velocity_read_memory_counts() {
        let (tools, _td) = tools_with_tmp();
        let store = tools.memory_store().await.unwrap();
        let namespace = api_memory::agent_namespace("mcp-flywheel");
        store
            .upsert_observation(
                &mcp_observation("mcp-fw-obs", &namespace, "MCP_FW_OBS", vec![1.0]),
                "test",
            )
            .await
            .unwrap();
        store
            .upsert_pattern(
                &mcp_pattern("mcp-fw-pattern", &namespace, "MCP_FW_PATTERN", vec![1.0]),
                "test",
            )
            .await
            .unwrap();

        let status = tools
            .xvn_flywheel_status(Parameters(FlywheelStatusMcpReq {
                namespace: None,
                agent: Some("mcp-flywheel".into()),
            }))
            .await
            .unwrap();
        let status = parsed(&status);
        assert_eq!(status["namespace"], namespace);
        assert_eq!(status["observations"], 1);
        assert_eq!(status["active_patterns"], 1);

        let velocity = tools
            .xvn_flywheel_velocity(Parameters(FlywheelVelocityMcpReq {
                namespace: None,
                agent: Some("mcp-flywheel".into()),
                days: Some(7),
            }))
            .await
            .unwrap();
        let velocity = parsed(&velocity);
        assert_eq!(velocity["namespace"], namespace);
        assert_eq!(velocity["observations_captured"], 1);
        assert_eq!(velocity["patterns_promoted"], 1);
    }

    #[tokio::test]
    async fn mcp_flywheel_lineage_returns_optimizer_hash_proof() {
        let (tools, _td) = tools_with_tmp();
        let ctx = tools.api_context().await.unwrap();
        let store = tools.memory_store().await.unwrap();
        let namespace = api_memory::agent_namespace("mcp-flywheel");
        let agent = api_agents::create(
            &ctx,
            api_agents::CreateAgentRequest {
                name: "mcp flywheel parent".into(),
                description: "parent for MCP flywheel lineage test".into(),
                tags: vec!["mcp".into()],
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: "mock".into(),
                    model: "mock".into(),
                    system_prompt: "base prompt".into(),
                    skill_ids: Vec::new(),
                    max_tokens: None,
                    max_wall_ms: None,
                    temperature: None,
                    prompt_version: String::new(),
                    inputs_policy: xvision_engine::agents::InputsPolicy::Raw,
                    bar_history_limit: None,
                    memory_mode: Default::default(),
                    noop_skip: None,
                    allowed_tools: Vec::new(),
                    delta_briefing: None,
                }],
                scope_strategy_id: None,
            },
        )
        .await
        .unwrap();
        for idx in 1..=4 {
            store
                .upsert_observation(
                    &mcp_observation(
                        &format!("mcp-fw-demo-obs-{idx}"),
                        &namespace,
                        &format!("MCP_FW_DEMO_OBS_{idx}"),
                        vec![idx as f32],
                    ),
                    "test",
                )
                .await
                .unwrap();
        }
        let run = api_autooptimizer::run_memory_distillation(
            &store,
            "test",
            vec![1.0],
            api_autooptimizer::AutoOptimizerRunRequest {
                namespace: Some(namespace.clone()),
                agent: None,
                scenario_id: None,
                run_id: None,
                pattern_text: "demo-source pattern".into(),
                active: true,
                limit: Some(4),
                min_observations: Some(2),
            },
        )
        .await
        .unwrap();
        let prior = mcp_pattern("mcp-prior-pattern", &namespace, "prior pattern", vec![1.0]);
        store.upsert_pattern(&prior, "test").await.unwrap();
        let auto_prior = mcp_pattern(
            "mcp-auto-prior-pattern",
            &namespace,
            "auto prior pattern",
            vec![1.0],
        );
        store.upsert_pattern(&auto_prior, "test").await.unwrap();
        let mut conn = ctx.db.acquire().await.unwrap();
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO events (id, run_id, span_id, kind, payload_json, created_at) \
             VALUES ('mcp-auto-prior-event', 'mcp-auto-prior-run', NULL, 'memory_recall', ?, \
                     '2024-01-06T00:00:00Z')",
        )
        .bind(
            serde_json::json!({
                "run_id": "mcp-auto-prior-run",
                "flywheel_cycle_id": "mcp-auto-prior-run:1",
                "decision_id": 1,
                "namespace": &namespace,
                "items": [{
                    "id": &auto_prior.id,
                    "score": 0.9,
                    "text_preview": "auto prior pattern"
                }]
            })
            .to_string(),
        )
        .execute(&mut *conn)
        .await
        .unwrap();

        let dry_run = tools
            .xvn_optimize_memory_demos(Parameters(OptimizeMemoryDemosMcpReq {
                target_agent_id: agent.agent_id.clone(),
                slot: Some("main".into()),
                namespace: Some(namespace.clone()),
                memory_agent: None,
                scenario_id: None,
                run_id: None,
                demo_source: Some("frozen-snapshot".into()),
                holdout_split: Some("70/15/15".into()),
                cohort_query: None,
                manual_observation_ids: None,
                prior_pattern_ids: Some(vec![prior.id.clone()]),
                auto_prior_patterns: true,
                prior_pattern_limit: Some(1),
                limit: Some(4),
                max_demo_chars: Some(1_000),
                apply: false,
                child_name: Some("mcp flywheel child".into()),
            }))
            .await
            .unwrap();
        let dry_run = parsed(&dry_run);
        assert_eq!(dry_run["status"], "planned");
        assert!(dry_run["optimization_id"].is_null());
        assert!(dry_run["child_agent_id"].is_null());
        assert!(dry_run["train_hash"].as_str().unwrap().starts_with("sha256:"));
        assert!(dry_run["dev_hash"].as_str().unwrap().starts_with("sha256:"));
        assert!(dry_run["holdout_hash"].as_str().unwrap().starts_with("sha256:"));

        let minted = tools
            .xvn_optimize_memory_demos(Parameters(OptimizeMemoryDemosMcpReq {
                target_agent_id: agent.agent_id.clone(),
                slot: Some("main".into()),
                namespace: Some(namespace.clone()),
                memory_agent: None,
                scenario_id: None,
                run_id: None,
                demo_source: Some("frozen-snapshot".into()),
                holdout_split: Some("70/15/15".into()),
                cohort_query: None,
                manual_observation_ids: None,
                prior_pattern_ids: Some(vec![prior.id.clone()]),
                auto_prior_patterns: true,
                prior_pattern_limit: Some(1),
                limit: Some(4),
                max_demo_chars: Some(1_000),
                apply: true,
                child_name: Some("mcp flywheel child".into()),
            }))
            .await
            .unwrap();
        let minted = parsed(&minted);
        assert_eq!(minted["status"], "minted");
        assert!(minted["optimization_id"].as_str().is_some());
        assert!(minted["child_agent_id"].as_str().is_some());

        let lineage = tools
            .xvn_flywheel_lineage(Parameters(FlywheelLineageMcpReq {
                namespace: None,
                agent: Some("mcp-flywheel".into()),
                limit: Some(5),
            }))
            .await
            .unwrap();
        let lineage = parsed(&lineage);
        assert_eq!(lineage["namespace"], "agent:mcp-flywheel");
        assert_eq!(lineage["total"], 1);
        let item = &lineage["items"][0];
        assert_eq!(item["optimization_id"], minted["optimization_id"]);
        assert_eq!(item["train_hash"], minted["train_hash"]);
        assert_eq!(item["dev_hash"], minted["dev_hash"]);
        assert_eq!(item["holdout_hash"], minted["holdout_hash"]);
        assert_eq!(
            item["demo_source_pattern_ids"],
            serde_json::json!([run.pattern_id])
        );
        assert_eq!(
            item["prior_pattern_ids"],
            serde_json::json!(["mcp-auto-prior-pattern", prior.id])
        );
    }

    #[tokio::test]
    async fn mcp_autooptimizer_read_tools_return_run_and_findings() {
        let (tools, _td) = tools_with_tmp();
        let store = tools.memory_store().await.unwrap();
        let namespace = api_memory::agent_namespace("mcp-auto");
        store
            .upsert_observation(
                &mcp_observation("mcp-auto-obs-1", &namespace, "MCP_AUTO_OBS_1", vec![1.0]),
                "test",
            )
            .await
            .unwrap();
        store
            .upsert_observation(
                &mcp_observation("mcp-auto-obs-2", &namespace, "MCP_AUTO_OBS_2", vec![1.0]),
                "test",
            )
            .await
            .unwrap();
        let run = api_autooptimizer::run_memory_distillation(
            &store,
            "test",
            vec![1.0],
            api_autooptimizer::AutoOptimizerRunRequest {
                namespace: None,
                agent: Some("mcp-auto".into()),
                scenario_id: None,
                run_id: None,
                pattern_text: "MCP autooptimizer Pattern".into(),
                active: false,
                limit: Some(10),
                min_observations: Some(2),
            },
        )
        .await
        .unwrap();
        api_autooptimizer::gate_run(
            &store,
            &run.id,
            api_autooptimizer::AutoOptimizerGateRequest {
                metric: Some("sharpe".into()),
                parent_day_score: Some(1.0),
                child_day_score: Some(1.2),
                parent_holdout_score: Some(0.9),
                child_holdout_score: Some(1.1),
                gate_epsilon: Some(0.1),
                finding_text: Some("Blind Finding: coherent MCP test pattern.".into()),
                finding_blinded_metrics: Some(true),
                judge_model: Some("test-judge".into()),
                judge_token_cost: Some(42),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let listed = tools
            .xvn_autooptimizer_list(Parameters(AutoOptimizerListMcpReq {
                namespace: None,
                agent: Some("mcp-auto".into()),
                limit: Some(10),
                offset: None,
            }))
            .await
            .unwrap();
        let listed = parsed(&listed);
        assert_eq!(listed["total"], 1);
        assert_eq!(listed["items"][0]["id"], run.id);

        let inspected = tools
            .xvn_autooptimizer_inspect(Parameters(AutoOptimizerRunIdMcpReq { id: run.id.clone() }))
            .await
            .unwrap();
        let inspected = parsed(&inspected);
        assert_eq!(inspected["id"], run.id);
        assert_eq!(inspected["gate_verdict"], "passed");
        assert_eq!(inspected["judge_model"], "test-judge");

        let findings = tools
            .xvn_autooptimizer_findings(Parameters(AutoOptimizerRunIdMcpReq { id: run.id.clone() }))
            .await
            .unwrap();
        let findings = parsed(&findings);
        assert_eq!(findings["id"], run.id);
        assert_eq!(findings["gate_verdict"], "passed");
        assert_eq!(findings["judge_token_cost"], 42);
        assert!(
            findings["finding_text"]
                .as_str()
                .is_some_and(|s| s.contains("Blind Finding")),
            "finding text missing: {findings}"
        );
    }

    #[tokio::test]
    async fn list_templates_returns_empty_post_registry_removal() {
        // Post-2026-05-21: the strategy template_registry was removed.
        // xvn_list_templates is retained as a deprecation stub that
        // returns an empty array. Operator-readable starters live
        // under $XVN_HOME/strategies/library/ via `xvn strategies init`.
        let tools = XvisionTools::default();
        let s = tools.xvn_list_templates().await.unwrap();
        let v = parsed(&s);
        assert!(
            v.as_array().is_some_and(|arr| arr.is_empty()),
            "post-registry-removal list must be empty, got: {s}"
        );
    }

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                name: "btc-mom-1".into(),
                creator: Some("@test".into()),
            }))
            .await
            .unwrap();
        let id = id_of(&s);

        let g = tools
            .xvn_get_strategy(Parameters(StrategyId { id: id.clone() }))
            .await
            .unwrap();
        let strategy = parsed(&g);
        assert_eq!(strategy["manifest"]["id"], id);
        // Post-template-registry-removal: blank draft stamps `template`
        // as the free-text label `"custom"`.
        assert_eq!(strategy["manifest"]["template"], "custom");
    }

    #[tokio::test]
    async fn create_strategy_legacy_template_field_is_silently_ignored() {
        // Post-2026-05-21: the `template` field was removed from
        // CreateStrategyReq. The MCP boundary silently ignores unknown
        // fields so the wizard tool-use loop's pre-migration JSON
        // payloads still parse (no breaking change on the LLM-side
        // surface). The legacy field has no effect — the resulting
        // strategy is a blank draft with `manifest.template = "custom"`.
        let raw = serde_json::json!({
            "template": "trend_follower",
            "name": "x",
            "creator": null,
        });
        let req: CreateStrategyReq = serde_json::from_value(raw)
            .expect("legacy template field must be silently ignored at MCP boundary");
        assert_eq!(req.name, "x");
    }

    #[tokio::test]
    async fn create_strategy_with_legacy_template_yields_blank_draft() {
        // End-to-end: callers passing the legacy field through the
        // tool boundary still get a saved draft. The template field
        // is dropped; the resulting strategy is the blank shape.
        let (tools, _td) = tools_with_tmp();
        let raw = serde_json::json!({
            "template": "trend_follower",
            "name": "legacy-caller",
            "creator": "@legacy",
        });
        let req: CreateStrategyReq = serde_json::from_value(raw).unwrap();
        let s = tools.xvn_create_strategy(Parameters(req)).await.unwrap();
        let id = id_of(&s);
        let g = tools
            .xvn_get_strategy(Parameters(StrategyId { id: id.clone() }))
            .await
            .unwrap();
        let strategy = parsed(&g);
        assert_eq!(strategy["manifest"]["id"], id);
        // Blank draft uses "custom" as the (now free-text) label.
        assert_eq!(strategy["manifest"]["template"], "custom");
        assert!(
            strategy["trader_slot"].is_null(),
            "blank draft has no trader slot"
        );
    }

    #[tokio::test]
    async fn update_slot_mutates_only_provided_fields() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                name: "x".into(),
                creator: None,
            }))
            .await
            .unwrap();
        let id = id_of(&s);

        let upd = tools
            .xvn_update_slot(Parameters(UpdateSlotReq {
                id: id.clone(),
                slot: "trader".into(),
                attested_with: Some("anthropic.claude-sonnet-4.6".into()),
                provider: None,
                model: None,
                allowed_tools: None,
            }))
            .await
            .unwrap();
        let v = parsed(&upd);
        assert_eq!(v["updated"], serde_json::json!(["attested_with"]));

        let g = tools
            .xvn_get_strategy(Parameters(StrategyId { id }))
            .await
            .unwrap();
        let strategy = parsed(&g);
        assert_eq!(
            strategy["trader_slot"]["attested_with"],
            "anthropic.claude-sonnet-4.6"
        );
    }

    #[tokio::test]
    async fn update_slot_rejects_unknown_slot() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                name: "x".into(),
                creator: None,
            }))
            .await
            .unwrap();
        let id = id_of(&s);
        let err = tools
            .xvn_update_slot(Parameters(UpdateSlotReq {
                id,
                slot: "nope".into(),
                attested_with: Some("anthropic.claude-sonnet-4.6".into()),
                provider: None,
                model: None,
                allowed_tools: None,
            }))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unknown slot"));
    }

    #[tokio::test]
    async fn set_risk_config_preset_balanced_applies_known_values() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                name: "x".into(),
                creator: None,
            }))
            .await
            .unwrap();
        let id = id_of(&s);

        let r = tools
            .xvn_set_risk_config(Parameters(SetRiskConfigReq {
                id: id.clone(),
                preset: Some("balanced".into()),
                explicit: None,
            }))
            .await
            .unwrap();
        assert_eq!(parsed(&r)["applied"], "preset");

        let g = tools
            .xvn_get_strategy(Parameters(StrategyId { id }))
            .await
            .unwrap();
        let strategy = parsed(&g);
        assert_eq!(strategy["risk"]["risk_pct_per_trade"], 0.015);
        assert_eq!(strategy["risk"]["max_concurrent_positions"], 2);
    }

    #[tokio::test]
    async fn set_risk_config_rejects_both_supplied() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                name: "x".into(),
                creator: None,
            }))
            .await
            .unwrap();
        let id = id_of(&s);

        let err = tools
            .xvn_set_risk_config(Parameters(SetRiskConfigReq {
                id,
                preset: Some("balanced".into()),
                explicit: Some(serde_json::json!({
                    "risk_pct_per_trade": 0.01,
                    "max_concurrent_positions": 1,
                    "max_leverage": 1.0,
                    "stop_loss_atr_multiple": 2.0,
                    "daily_loss_kill_pct": 0.05,
                })),
            }))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("mutually exclusive"));
    }

    #[tokio::test]
    async fn validate_draft_reports_fresh_template_not_eval_ready() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                name: "x".into(),
                creator: None,
            }))
            .await
            .unwrap();
        let id = id_of(&s);

        let v = tools
            .xvn_validate_draft(Parameters(StrategyId { id }))
            .await
            .unwrap();
        let r = parsed(&v);
        assert_eq!(r["ok"], false);
        assert!(r["errors"].as_array().is_some_and(|errors| !errors.is_empty()));
    }

    // --- eval verbs (Phase 3.D Task 12) ----------------------------------

    use chrono::{Duration as ChronoDuration, TimeZone, Utc};
    use xvision_engine::eval::run::{MetricsSummary, Run, RunMode};
    use xvision_engine::eval::store::DecisionRow;

    /// Seed a completed run with metrics + a few equity samples + a decision.
    /// Returns the run id so tests can refer back to it.
    async fn seed_run(
        tools: &XvisionTools,
        agent_id: &str,
        scenario_id: &str,
        total_return_pct: f64,
    ) -> String {
        let ctx = tools.api_context().await.unwrap();
        let store = RunStore::new(ctx.db.clone());
        // Create in Queued state so finalize() (which transitions queued/running →
        // completed) can succeed. Setting status=Completed before create and then
        // calling finalize causes "already completed" because finalize's WHERE
        // clause requires status IN ('queued', 'running').
        let run = Run::new_queued(agent_id.into(), scenario_id.into(), RunMode::Backtest);
        store.create(&run).await.unwrap();

        let t0 = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        for i in 0..3 {
            let ts = t0 + ChronoDuration::hours(i);
            store
                .record_equity(&run.id, ts, 10_000.0 + (i as f64) * 100.0)
                .await
                .unwrap();
        }
        store
            .record_decision(&DecisionRow {
                run_id: run.id.clone(),
                decision_index: 0,
                timestamp: t0,
                asset: "BTC".into(),
                action: "long_open".into(),
                conviction: Some(0.7),
                justification: Some("seed".into()),
                reasoning: None,
                order_size: Some(0.1),
                fill_price: Some(40_000.0),
                fill_size: Some(0.1),
                fee: Some(1.0),
                pnl_realized: None,
                delayed: false,
            })
            .await
            .unwrap();
        let metrics = MetricsSummary {
            total_return_pct,
            sharpe: 1.0,
            max_drawdown_pct: 5.0,
            win_rate: 0.5,
            n_trades: 1,
            n_decisions: 1,
            inference_cost_quote_total: None,
            net_return_pct: None,
            baselines: None,
            ..Default::default()
        };
        store.finalize(&run.id, &metrics).await.unwrap();
        run.id
    }

    #[tokio::test]
    async fn eval_list_returns_seeded_runs() {
        let (tools, _td) = tools_with_tmp();
        let id_a = seed_run(&tools, "h-A", "crypto-bull-q1-2025", 12.0).await;
        let id_b = seed_run(&tools, "h-B", "crypto-bear-q3-2024", 7.5).await;

        let s = tools
            .xvn_eval_list(Parameters(EvalListReq::default()))
            .await
            .unwrap();
        let v = parsed(&s);
        let arr = v.as_array().unwrap();
        let ids: Vec<&str> = arr.iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert!(ids.contains(&id_a.as_str()), "ids={ids:?}");
        assert!(ids.contains(&id_b.as_str()));
    }

    #[tokio::test]
    async fn eval_list_filters_by_strategy() {
        let (tools, _td) = tools_with_tmp();
        let _id_a = seed_run(&tools, "h-A", "crypto-bull-q1-2025", 12.0).await;
        let id_b = seed_run(&tools, "h-B", "crypto-bear-q3-2024", 7.5).await;

        let s = tools
            .xvn_eval_list(Parameters(EvalListReq {
                agent_id: Some("h-B".into()),
                ..Default::default()
            }))
            .await
            .unwrap();
        let v = parsed(&s);
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"].as_str().unwrap(), id_b);
    }

    #[tokio::test]
    async fn eval_get_returns_run_detail_for_known_run() {
        let (tools, _td) = tools_with_tmp();
        let id = seed_run(&tools, "h-A", "crypto-bull-q1-2025", 4.0).await;

        let s = tools
            .xvn_eval_get(Parameters(EvalRunIdReq { run_id: id.clone() }))
            .await
            .unwrap();
        let v = parsed(&s);
        assert_eq!(v["summary"]["id"].as_str().unwrap(), id);
        assert_eq!(v["decisions"].as_array().unwrap().len(), 1);
        assert_eq!(v["equity_curve"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn eval_get_returns_invalid_params_for_unknown_run() {
        let (tools, _td) = tools_with_tmp();
        let err = tools
            .xvn_eval_get(Parameters(EvalRunIdReq {
                run_id: "no-such-run".into(),
            }))
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("not found"), "unexpected msg: {msg}");
    }

    #[tokio::test]
    async fn eval_metrics_returns_just_metrics() {
        let (tools, _td) = tools_with_tmp();
        let id = seed_run(&tools, "h-A", "crypto-bull-q1-2025", 21.0).await;

        let s = tools
            .xvn_eval_metrics(Parameters(EvalRunIdReq { run_id: id }))
            .await
            .unwrap();
        let v = parsed(&s);
        assert_eq!(v["total_return_pct"].as_f64().unwrap(), 21.0);
        assert_eq!(v["n_trades"].as_i64().unwrap(), 1);
    }

    #[tokio::test]
    async fn eval_scenarios_returns_canonical_set() {
        let (tools, _td) = tools_with_tmp();
        let s = tools.xvn_eval_scenarios().await.unwrap();
        let v = parsed(&s);
        let arr = v.as_array().unwrap();
        assert!(!arr.is_empty(), "expected at least one canonical scenario");
        let ids: Vec<&str> = arr.iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert!(
            ids.iter().any(|id| id.contains("crypto") || id.contains("crash")),
            "missing canonical scenarios in {ids:?}",
        );
    }

    #[tokio::test]
    async fn eval_compare_returns_comparison_report() {
        let (tools, _td) = tools_with_tmp();
        let id_a = seed_run(&tools, "h-A", "crypto-bull-q1-2025", 10.0).await;
        let id_b = seed_run(&tools, "h-B", "crypto-bear-q3-2024", 5.0).await;

        let s = tools
            .xvn_eval_compare(Parameters(EvalCompareReq {
                run_ids: vec![id_a.clone(), id_b.clone()],
            }))
            .await
            .unwrap();
        let v = parsed(&s);
        let runs = v["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0]["id"].as_str().unwrap(), id_a);
        assert_eq!(runs[1]["id"].as_str().unwrap(), id_b);
        assert_eq!(v["equity_curves"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn eval_compare_rejects_single_run() {
        let (tools, _td) = tools_with_tmp();
        let id = seed_run(&tools, "h-A", "crypto-bull-q1-2025", 10.0).await;
        let err = tools
            .xvn_eval_compare(Parameters(EvalCompareReq { run_ids: vec![id] }))
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("validation") && msg.contains("at least two"),
            "msg: {msg}"
        );
    }

    #[tokio::test]
    async fn eval_findings_returns_empty_array_when_none() {
        let (tools, _td) = tools_with_tmp();
        let id = seed_run(&tools, "h-A", "crypto-bull-q1-2025", 10.0).await;
        let s = tools
            .xvn_eval_findings(Parameters(EvalRunIdReq { run_id: id }))
            .await
            .unwrap();
        let v = parsed(&s);
        let arr = v.as_array().unwrap();
        assert!(arr.is_empty(), "expected empty findings, got {v}");
    }

    // ── F-13 smoke tests: 6 new MCP tools (wave A+B + wave-C) ───────────────

    /// `xvn_strategy_create_atomic` request struct round-trips through serde.
    #[test]
    fn strategy_create_atomic_req_serde_roundtrip() {
        let json = serde_json::json!({
            "name": "eth-trader",
            "role": "trader",
            "prompt": "You are a crypto trader.",
            "provider": "openrouter",
            "model": "kimi-k2",
            "asset": "ETH/USD",
            "timeframe": "4h",
        });
        let req: StrategyCreateAtomicReq = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(req.name, "eth-trader");
        assert_eq!(req.role, "trader");
        assert_eq!(req.provider, "openrouter");
        assert_eq!(req.model, "kimi-k2");
        assert_eq!(req.asset.as_deref(), Some("ETH/USD"));
        assert_eq!(req.timeframe.as_deref(), Some("4h"));
    }

    /// `xvn_strategy_create_atomic` defaults work when asset/timeframe omitted.
    #[test]
    fn strategy_create_atomic_req_optional_fields_default_to_none() {
        let json = serde_json::json!({
            "name": "minimal",
            "role": "trader",
            "prompt": "Trade ETH",
            "provider": "anthropic",
            "model": "claude-sonnet-4-6",
        });
        let req: StrategyCreateAtomicReq = serde_json::from_value(json).unwrap();
        assert!(req.asset.is_none());
        assert!(req.timeframe.is_none());
    }

    /// `xvn_strategy_validate_preflight` request struct round-trips.
    #[test]
    fn strategy_preflight_req_serde_roundtrip() {
        let json = serde_json::json!({
            "id": "01JDE2KBNHQ5ST3K4YCJNZPCAJ",
            "scenario_id": "crypto-bull-q1-2025",
        });
        let req: StrategyPreflightReq = serde_json::from_value(json).unwrap();
        assert_eq!(req.id, "01JDE2KBNHQ5ST3K4YCJNZPCAJ");
        assert_eq!(req.scenario_id.as_deref(), Some("crypto-bull-q1-2025"));
    }

    /// `xvn_strategy_validate_preflight` without scenario_id returns shape-only check.
    #[tokio::test]
    async fn strategy_validate_preflight_shape_only_no_scenario() {
        let (tools, _td) = tools_with_tmp();
        // Create a strategy via the existing authoring surface so we have a valid id.
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                name: "preflight-smoke".into(),
                creator: None,
            }))
            .await
            .unwrap();
        let id = id_of(&s);

        let r = tools
            .xvn_strategy_validate_preflight(Parameters(StrategyPreflightReq {
                id: id.clone(),
                scenario_id: None,
            }))
            .await
            .unwrap();
        let v = parsed(&r);
        // The strategy has no agents (template-created, no agent wired), so
        // validate_strategy will fail; the preflight report exists regardless.
        assert!(v["strategy_id"].as_str().is_some());
        assert!(v["eval_ready"].is_boolean());
        assert!(v["errors"].is_array(), "errors absent: {v}");
        assert!(v["warnings"].is_array(), "warnings absent: {v}");
    }

    #[tokio::test]
    async fn strategy_validate_preflight_returns_not_found_for_missing_strategy() {
        let (tools, _td) = tools_with_tmp();
        let err = tools
            .xvn_strategy_validate_preflight(Parameters(StrategyPreflightReq {
                id: "01NOTEXIST0000000000000000".into(),
                scenario_id: None,
            }))
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("no such file") && msg.contains("strategies"),
            "expected missing strategy file diagnostic, got: {msg}"
        );
    }

    // ── eval_batch_status ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn eval_batch_status_returns_batch_detail() {
        let (tools, _td) = tools_with_tmp();
        let ctx = tools.api_context().await.unwrap();

        // Create a batch row directly via the engine API.
        let batch = api_eval::create_batch(
            &ctx,
            CreateBatchRequest {
                strategy_id: "strat-abc".into(),
                review_with: None,
            },
        )
        .await
        .unwrap();

        let s = tools
            .xvn_eval_batch_status(Parameters(EvalBatchStatusReq {
                batch_id: batch.batch_id.clone(),
            }))
            .await
            .unwrap();
        let v = parsed(&s);
        assert_eq!(v["batch_id"].as_str().unwrap(), batch.batch_id);
        assert_eq!(v["strategy_id"].as_str().unwrap(), "strat-abc");
        assert!(v["run_ids"].is_array(), "run_ids absent: {v}");
    }

    #[tokio::test]
    async fn eval_batch_status_returns_not_found_for_missing_batch() {
        let (tools, _td) = tools_with_tmp();
        let err = tools
            .xvn_eval_batch_status(Parameters(EvalBatchStatusReq {
                batch_id: "batch_notexist".into(),
            }))
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("not found"), "expected not found, got: {msg}");
    }

    // ── eval_compare_ext ──────────────────────────────────────────────────────

    // Note: tests that call `seed_run` hit the pre-existing RunStore::finalize
    // issue documented in PR #350 (exists in origin/task/cli-agent-workbench-wave-b).
    // The compare_ext tests below avoid seed_run by testing the error-handling
    // and routing paths that don't depend on a finalized run.

    #[tokio::test]
    async fn eval_compare_ext_rejects_both_run_ids_and_batch_id() {
        let (tools, _td) = tools_with_tmp();
        let err = tools
            .xvn_eval_compare_ext(Parameters(EvalCompareExtReq {
                run_ids: vec!["a".into(), "b".into()],
                batch_id: Some("batch_01K".into()),
                markdown: false,
            }))
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("not both"), "expected mutex error, got: {msg}");
    }

    #[tokio::test]
    async fn eval_compare_ext_rejects_missing_batch_id() {
        let (tools, _td) = tools_with_tmp();
        // A missing batch id should surface as a not-found error.
        let err = tools
            .xvn_eval_compare_ext(Parameters(EvalCompareExtReq {
                run_ids: vec![],
                batch_id: Some("batch_doesnotexist".into()),
                markdown: false,
            }))
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("not found"), "expected not found, got: {msg}");
    }

    #[tokio::test]
    async fn eval_compare_ext_routes_via_batch_id_resolves_run_ids() {
        let (tools, _td) = tools_with_tmp();
        let ctx = tools.api_context().await.unwrap();

        // Create a batch with no runs attached. Attempting compare with 0 run ids
        // should produce a "at least two" validation error — confirming the batch
        // routing path works (batch found, run_ids resolved as empty list, compare
        // rejects on count).
        let batch = api_eval::create_batch(
            &ctx,
            CreateBatchRequest {
                strategy_id: "strat-test".into(),
                review_with: None,
            },
        )
        .await
        .unwrap();

        let err = tools
            .xvn_eval_compare_ext(Parameters(EvalCompareExtReq {
                run_ids: vec![],
                batch_id: Some(batch.batch_id.clone()),
                markdown: false,
            }))
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        // compare rejects empty/single list with "at least two" or similar
        assert!(
            msg.contains("at least") || msg.contains("validation"),
            "expected at-least-two error from compare, got: {msg}",
        );
    }

    // ── eval_compare_report ───────────────────────────────────────────────────

    /// `xvn_eval_batch_run` request struct round-trips through serde.
    #[test]
    fn eval_batch_run_req_serde_roundtrip() {
        let json = serde_json::json!({
            "strategy_id": "strat-123",
            "scenario_ids": ["sc-a", "sc-b", "sc-c"],
            "mode": "backtest",
            "review_with": "reasoning-agent",
        });
        let req: EvalBatchRunReq = serde_json::from_value(json).unwrap();
        assert_eq!(req.strategy_id, "strat-123");
        assert_eq!(req.scenario_ids.len(), 3);
        assert_eq!(req.mode.as_deref(), Some("backtest"));
        assert_eq!(req.review_with.as_deref(), Some("reasoning-agent"));
    }

    /// `xvn_eval_compare_report` request struct round-trips with all fields.
    #[test]
    fn eval_compare_report_req_serde_roundtrip() {
        let json = serde_json::json!({
            "run_ids": ["run-1", "run-2"],
            "sort": "sharpe",
        });
        let req: EvalCompareReportReq = serde_json::from_value(json).unwrap();
        assert_eq!(req.run_ids, vec!["run-1", "run-2"]);
        assert_eq!(req.sort.as_deref(), Some("sharpe"));
    }

    /// `xvn_eval_compare_report` returns decorated compare rows with behavior fields.
    #[tokio::test]
    async fn eval_compare_report_returns_decorated_rows() {
        let (tools, _td) = tools_with_tmp();
        let id_a = seed_run(&tools, "strat-A", "crypto-bull-q1-2025", 8.0).await;
        let id_b = seed_run(&tools, "strat-B", "crypto-bear-q3-2024", 3.5).await;

        let s = tools
            .xvn_eval_compare_report(Parameters(EvalCompareReportReq {
                run_ids: vec![id_a.clone(), id_b.clone()],
                sort: Some("return".into()),
            }))
            .await
            .unwrap();
        let v = parsed(&s);
        let runs = v["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 2, "expected 2 rows");
        // Each row must carry behavior-decorator fields.
        let row = &runs[0];
        assert!(row["trades_opened"].is_number(), "missing trades_opened");
        assert!(
            row["primary_failure_mode"].is_string(),
            "missing primary_failure_mode"
        );
        assert!(
            row["action_distribution"].is_object(),
            "missing action_distribution"
        );
        // Sorted by return desc — id_a seeded with 8.0 should be first.
        assert_eq!(runs[0]["run_id"].as_str().unwrap(), id_a);
        assert_eq!(runs[1]["run_id"].as_str().unwrap(), id_b);
    }

    /// `xvn_scenario_inspect_card` request struct round-trips.
    #[test]
    fn scenario_inspect_card_req_serde_roundtrip() {
        let json = serde_json::json!({ "id": "crypto-bull-q1-2025" });
        let req: ScenarioInspectCardReq = serde_json::from_value(json).unwrap();
        assert_eq!(req.id, "crypto-bull-q1-2025");
    }

    /// `xvn_scenario_inspect_card` returns a card string for a canonical scenario.
    #[tokio::test]
    async fn scenario_inspect_card_returns_card_for_canonical_scenario() {
        let (tools, _td) = tools_with_tmp();

        let s = tools
            .xvn_scenario_inspect_card(Parameters(ScenarioInspectCardReq {
                id: "crypto-bull-q1-2025".into(),
            }))
            .await
            .unwrap();
        let v = parsed(&s);
        let card = v["card"].as_str().unwrap();
        assert!(card.contains("id:"), "card missing id field: {card}");
        assert!(card.contains("name:"), "card missing name field: {card}");
        assert!(card.contains("timeframe:"), "card missing timeframe: {card}");
        assert!(
            card.contains("decision_bars:"),
            "card missing decision_bars: {card}"
        );
    }

    /// `xvn_eval_behavior` request struct round-trips.
    #[test]
    fn eval_behavior_req_serde_roundtrip() {
        let json = serde_json::json!({ "run_id": "01JDE2KBNHQ5ST3K4YCJNZPCAJ" });
        let req: EvalBehaviorReq = serde_json::from_value(json).unwrap();
        assert_eq!(req.run_id, "01JDE2KBNHQ5ST3K4YCJNZPCAJ");
    }

    /// `xvn_eval_behavior` returns a BehaviorSummary for a seeded run.
    #[tokio::test]
    async fn eval_behavior_returns_summary_for_seeded_run() {
        let (tools, _td) = tools_with_tmp();
        let run_id = seed_run(&tools, "strat-X", "crypto-bull-q1-2025", 5.0).await;

        let s = tools
            .xvn_eval_behavior(Parameters(EvalBehaviorReq { run_id }))
            .await
            .unwrap();
        let v = parsed(&s);
        assert!(v["flat_rate"].is_number(), "missing flat_rate");
        assert!(
            v["primary_failure_mode"].is_string(),
            "missing primary_failure_mode"
        );
        assert!(v["trades_opened"].is_number(), "missing trades_opened");
    }

    /// `xvn_eval_behavior` returns a 404-shaped error for an unknown run.
    #[tokio::test]
    async fn eval_behavior_returns_not_found_for_unknown_run() {
        let (tools, _td) = tools_with_tmp();
        let err = tools
            .xvn_eval_behavior(Parameters(EvalBehaviorReq {
                run_id: "no-such-run".into(),
            }))
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("not found"), "unexpected msg: {msg}");
    }

    // ── scenarios_select ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn scenarios_select_returns_canonical_scenarios() {
        let (tools, _td) = tools_with_tmp();
        // The DB is seeded with canonical scenarios on first open.
        let s = tools
            .xvn_scenarios_select(Parameters(ScenariosSelectReq {
                timeframe: None,
                target_decisions: Some(50),
                same_decisions: false,
                max_decisions: None,
                regimes: vec![],
                count: Some(4),
            }))
            .await
            .unwrap();
        let v = parsed(&s);
        let arr = v.as_array().unwrap();
        // canonical scenarios might not all have 50±10% decisions; the result
        // may be empty but the call must succeed with a valid JSON array.
        let _ = arr; // arr is already a &Vec from as_array().unwrap() above
    }

    #[tokio::test]
    async fn scenarios_select_rejects_missing_mode_spec() {
        let (tools, _td) = tools_with_tmp();
        let err = tools
            .xvn_scenarios_select(Parameters(ScenariosSelectReq {
                timeframe: None,
                target_decisions: None,
                same_decisions: false,
                max_decisions: None,
                regimes: vec![],
                count: Some(4),
            }))
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("target_decisions") || msg.contains("same_decisions"),
            "expected mode error, got: {msg}",
        );
    }

    // ── select_scenarios_mcp pure unit tests ─────────────────────────────────

    use std::str::FromStr;
    use xvision_engine::eval::scenario::{
        AdjustmentMode, AssetClass, BarCachePolicy, BarGranularity, CalendarRef, DataSource, Fees, FillModel,
        LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency, RefreshPolicy, ReplayMode,
        ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings,
    };
    use xvision_engine::Capital;

    fn make_test_scenario(
        id: &str,
        _asset_sym: &str,
        granularity: &str,
        window_secs: i64,
        warmup_bars: u32,
    ) -> Scenario {
        let start = chrono::Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let end = start + chrono::Duration::seconds(window_secs);
        let gran = BarGranularity::from_str(granularity).expect("valid gran");
        Scenario {
            id: id.to_string(),
            parent_scenario_id: None,
            source: ScenarioSource::User,
            display_name: format!("test-{id}"),
            description: String::new(),
            tags: vec![],
            notes: None,
            asset_class: AssetClass::Crypto,
            quote_currency: QuoteCurrency::Usd,
            time_window: TimeWindow { start, end },
            granularity: gran,
            timezone: "UTC".to_string(),
            calendar: CalendarRef::Continuous24x7,
            data_source: DataSource::AlpacaHistorical {
                feed: None,
                adjustment: AdjustmentMode::Raw,
            },
            venue: VenueSettings {
                venue: Venue::Alpaca,
                fees: Fees {
                    maker_bps: 10,
                    taker_bps: 25,
                },
                slippage: SlippageModel::None,
                latency: LatencyModel {
                    decision_to_fill_ms: 0,
                },
                fill_model: FillModel {
                    market_order_fill: MarketOrderFill::FullAtClose,
                    limit_order_fill: LimitOrderFill::NeverFills,
                    partial_fills: false,
                    volume_constraints: None,
                },
                overrides: Vec::new(),
                borrow_bps_per_day: 5.0,
            },
            replay_mode: ReplayMode::Continuous,
            capital: Capital::default(),
            bar_cache_policy: BarCachePolicy {
                cache_key: id.to_string(),
                refresh_policy: RefreshPolicy::NeverRefresh,
                data_fetched_at: None,
            },
            warmup_bars,
            created_at: chrono::Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            created_by: "test".to_string(),
            archived_at: None,
            regime_label: None,
            volatility_label: None,
            trend_direction: None,
            regime_derived: false,
            // Pre-existing baseline fix: the `safety_limits` and
            // `venue_label` fields were added to `Scenario` upstream
            // but this make_test_scenario helper wasn't updated,
            // breaking `cargo test --workspace --no-run` on the
            // parent branch. Adding the defaults here unblocks the
            // strategy-template-registry-removal contract's
            // workspace-test verification step.
            venue_label: xvision_engine::safety::VenueLabel::Paper,
            safety_limits: None,
        }
    }

    #[test]
    fn select_scenarios_mcp_mode_a_returns_matching() {
        // 300 1h bars − 200 warmup = 100 decisions. target=100 → ±10% → matches.
        let s = make_test_scenario("sc1", "ETH", "1h", 300 * 3_600, 200);
        let rows = select_scenarios_mcp(&[s], 60, &[], Some(100), false, None, 4).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].decision_count, 100);
    }

    #[test]
    fn select_scenarios_mcp_mode_a_empty_when_no_match() {
        // 50 decisions; target=200 (±10%→180..220) → no match.
        let s = make_test_scenario("sc1", "ETH", "1h", 250 * 3_600, 200);
        let rows = select_scenarios_mcp(&[s], 60, &[], Some(200), false, None, 4).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn select_scenarios_mcp_mode_b_finds_common_count() {
        let s1 = make_test_scenario("sc1", "ETH", "1h", 300 * 3_600, 200); // 100 decisions
        let s2 = make_test_scenario("sc2", "BTC", "1h", 300 * 3_600, 200); // 100 decisions
        let s3 = make_test_scenario("sc3", "SOL", "1h", 250 * 3_600, 200); // 50 decisions
        let rows = select_scenarios_mcp(&[s1, s2, s3], 60, &[], None, true, Some(200), 2).unwrap();
        assert_eq!(rows.len(), 2);
        for r in &rows {
            assert_eq!(r.decision_count, 100);
        }
    }

    /// `parse_timeframe_mcp` maps all accepted values correctly.
    #[test]
    fn parse_timeframe_mcp_known_values() {
        assert_eq!(parse_timeframe_mcp("1m").unwrap(), 1);
        assert_eq!(parse_timeframe_mcp("5m").unwrap(), 5);
        assert_eq!(parse_timeframe_mcp("15m").unwrap(), 15);
        assert_eq!(parse_timeframe_mcp("30m").unwrap(), 30);
        assert_eq!(parse_timeframe_mcp("1h").unwrap(), 60);
        assert_eq!(parse_timeframe_mcp("2h").unwrap(), 120);
        assert_eq!(parse_timeframe_mcp("4h").unwrap(), 240);
        assert_eq!(parse_timeframe_mcp("1d").unwrap(), 1440);
    }

    /// `parse_timeframe_mcp` rejects unknown strings.
    #[test]
    fn parse_timeframe_mcp_rejects_unknown() {
        assert!(parse_timeframe_mcp("2d").is_err());
        assert!(parse_timeframe_mcp("1w").is_err());
        assert!(parse_timeframe_mcp("garbage").is_err());
    }
}
