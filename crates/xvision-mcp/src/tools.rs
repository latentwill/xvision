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

use xvision_data as xvn;
use xvision_engine::api::eval::{
    self as api_eval, BatchDetail, CompareRunsRequest, CreateBatchRequest, ListRunsRequest,
};
use xvision_engine::api::scenario as api_scenario;
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::authoring;
use xvision_engine::eval::run::RunStatus;
use xvision_engine::eval::scenario::Scenario;
use xvision_engine::eval::store::RunStore;
use xvision_engine::strategies::{
    risk::RiskConfig, store::FilesystemStore, store::StrategyStore, store::strategy_store_dir,
};
use xvision_engine::strategies::validate::preflight_validate;
use xvision_engine::api::agents as api_agents;
use xvision_engine::agents::AgentSlot;

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
// strategy `id` (ULID); `xvn_list_templates` and `xvn_create_strategy`
// don't need an existing one.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateStrategyReq {
    /// Template name. Use `xvn_list_templates` to enumerate options.
    pub template: String,
    /// Human-readable name (e.g., `btc-momentum-v1`).
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
    /// Slot to update: `regime` | `intern` | `trader`.
    pub slot: String,
    /// New system prompt for the slot.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Model requirement (e.g., `anthropic.claude-sonnet-4.6+`).
    #[serde(default)]
    pub model_requirement: Option<String>,
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
pub struct SetMechanicalParamReq {
    pub id: String,
    /// Key inside `Strategy.mechanical_params` (template-specific).
    pub key: String,
    /// New value (any JSON type the template accepts).
    pub value: serde_json::Value,
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

// ---------------------------------------------------------------------------
// New wave-C request shapes for the 6 parity tools.
// ---------------------------------------------------------------------------

/// `strategy_create_atomic` — create a strategy + agent + provider/model in
/// one atomic operation (wraps `xvn strategy create --prompt ... --json`).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StrategyCreateAtomicReq {
    /// Prompt text for the agent (inline). Required.
    pub prompt: String,
    /// Human-readable strategy name. Required.
    pub name: String,
    /// Provider name (e.g. `openrouter`, `anthropic`). Required.
    pub provider: String,
    /// Model id (e.g. `kimi-k2`). Required.
    pub model: String,
    /// Primary asset the strategy trades (e.g. `ETH/USD`). Required.
    pub asset: String,
    /// Decision timeframe / bar granularity.
    /// Accepted: `1m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `1d`. Required.
    pub timeframe: String,
    /// Role the created agent plays (default: `trader`).
    #[serde(default)]
    pub role: Option<String>,
    /// Optional creator handle. Defaults to `@anonymous`.
    #[serde(default)]
    pub creator: Option<String>,
}

/// `strategy_validate_preflight` — validate a strategy against eval-readiness
/// criteria, optionally cross-checking a scenario.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StrategyValidatePreflightReq {
    /// Strategy agent id. Required.
    pub strategy_id: String,
    /// Optional scenario id to cross-check asset/timeframe alignment.
    #[serde(default)]
    pub scenario_id: Option<String>,
}

/// `eval_batch_run` — launch one eval run per scenario and collect results.
/// Equivalent to `xvn eval batch run --strategy --scenarios --wait --json`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EvalBatchRunReq {
    /// Strategy agent id. Required.
    pub strategy_id: String,
    /// List of scenario ids to run against. Required; at least one.
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

/// `scenarios_select` — filter the scenario library by asset / timeframe /
/// decision count / regime labels and return a ranked subset.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScenariosSelectReq {
    /// Comma-separated asset symbols (e.g. `["ETH/USD","BTC/USD"]`).
    /// Empty → all assets.
    #[serde(default)]
    pub assets: Vec<String>,

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
    // xvision_engine's strategy store + template registry + validator.
    // -----------------------------------------------------------------------

    /// List strategy templates available to `xvn_create_strategy`. Returns an
    /// array of `{ name, display_name, plain_summary }`.
    #[tool(
        description = "List the strategy templates available for xvn_create_strategy. Returns array of {name, display_name, plain_summary}."
    )]
    async fn xvn_list_templates(&self) -> Result<String, rmcp::ErrorData> {
        json_or_err(&authoring::list_templates())
    }

    /// Create a new strategy draft from a template. Persists to
    /// `$XVN_HOME/strategies/<id>.json`. Returns `{ id }`.
    #[tool(
        description = "Create a new strategy draft from a template. Persists the strategy and returns { id } (ULID)."
    )]
    async fn xvn_create_strategy(
        &self,
        Parameters(req): Parameters<CreateStrategyReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let out = authoring::create_strategy(
            &self.store(),
            authoring::CreateStrategyReq {
                template: req.template,
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
                prompt: req.prompt,
                model_requirement: req.model_requirement,
                provider: req.provider,
                model: req.model,
                allowed_tools: req.allowed_tools,
            },
        )
        .await
        .map_err(authoring_err)?;
        json_or_err(&out)
    }

    /// Set a key inside `Strategy.mechanical_params`. Templates document
    /// which keys they accept. Returns `{ id, key }`.
    #[tool(
        description = "Set a key inside Strategy.mechanical_params. Templates document which keys they accept. Returns { id, key }."
    )]
    async fn xvn_set_mechanical_param(
        &self,
        Parameters(req): Parameters<SetMechanicalParamReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let id = req.id.clone();
        let key = req.key.clone();
        authoring::set_mechanical_param(
            &self.store(),
            authoring::SetMechanicalParamReq {
                id: req.id,
                key: req.key,
                value: req.value,
            },
        )
        .await
        .map_err(authoring_err)?;
        json_or_err(&serde_json::json!({ "id": id, "key": key }))
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
    // (the dashboard's chat rail, the autoresearcher) can browse runs +
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
        let status = req.status.as_deref().map(parse_status_for_mcp).transpose()?;
        let summaries = api_eval::list_summaries(
            &ctx,
            ListRunsRequest {
                agent_id: req.agent_id,
                scenario_id: req.scenario_id,
                status,
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
    /// dashboard's run cards, the autoresearcher's lineage gate).
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
        let report = api_eval::compare(&ctx, CompareRunsRequest { run_ids: req.run_ids })
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

    // ── Wave-C parity tools ──────────────────────────────────────────────────

    /// Atomically create a strategy + agent + provider/model binding in one
    /// call. Requires: `prompt`, `name`, `provider`, `model`, `asset`,
    /// `timeframe`. Optional: `role` (default `trader`), `creator`.
    /// Returns `{ strategy_id, agent_id, eval_ready, provider, model, warnings }`.
    #[tool(
        description = "Atomically create a strategy + agent + provider/model in one call. \
        Required inputs: prompt (inline text), name, provider, model, asset, timeframe \
        (1m/5m/15m/30m/1h/2h/4h/1d). Optional: role (default trader), creator. \
        Returns { strategy_id, agent_id, eval_ready, provider, model, warnings }."
    )]
    async fn xvn_strategy_create_atomic(
        &self,
        Parameters(req): Parameters<StrategyCreateAtomicReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;

        let cadence_minutes = parse_timeframe_minutes_mcp(&req.timeframe)?;
        let role = req.role.unwrap_or_else(|| "trader".to_string());
        let creator = req
            .creator
            .unwrap_or_else(|| "@anonymous".to_string());

        // 1. Create the agent library entry.
        let agent = api_agents::create(
            &ctx,
            api_agents::CreateAgentRequest {
                name: format!("{} {role}", req.name),
                description: format!(
                    "Created atomically with strategy '{}' role '{role}'",
                    req.name
                ),
                tags: vec!["atomic-create".to_string()],
                slots: vec![AgentSlot {
                    name: "main".to_string(),
                    provider: req.provider.clone(),
                    model: req.model.clone(),
                    system_prompt: req.prompt,
                    skill_ids: Vec::new(),
                    max_tokens: None,
                    prompt_version: String::new(),
                    inputs_policy: Default::default(),
                }],
            },
        )
        .await
        .map_err(api_err_to_mcp)?;

        let agent_id = agent.agent_id.clone();

        // 2. Build the strategy with the agent wired in.
        let strategy_id = Ulid::new().to_string();
        let strategy = xvision_engine::strategies::Strategy {
            manifest: xvision_engine::strategies::manifest::PublicManifest {
                id: strategy_id.clone(),
                display_name: req.name.clone(),
                plain_summary: String::new(),
                creator,
                template: "custom".to_string(),
                regime_fit: Vec::new(),
                asset_universe: vec![req.asset.clone()],
                decision_cadence_minutes: cadence_minutes,
                required_models: Vec::new(),
                required_tools: Vec::new(),
                risk_preset_or_config: "balanced".to_string(),
                published_at: None,
                min_warmup_bars: None,
            },
            agents: vec![xvision_engine::strategies::AgentRef {
                agent_id: agent_id.clone(),
                role: role.clone(),
            }],
            pipeline: xvision_engine::strategies::PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: None,
            risk: xvision_engine::strategies::risk::RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({}),
            hypothesis: None,
        };

        // 3. Validate shape.
        let preflight = preflight_validate(&strategy, None);
        if !preflight.errors.is_empty() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("strategy validation failed: {}", preflight.errors.join("; ")),
                None,
            ));
        }

        // 4. Persist the strategy.
        let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
        store
            .save(&strategy)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("save strategy: {e}"), None))?;

        // 5. Build output.
        let warnings = preflight.warnings;
        let eval_ready = warnings.is_empty();
        json_or_err(&serde_json::json!({
            "strategy_id": strategy_id,
            "agent_id": agent_id,
            "eval_ready": eval_ready,
            "provider": req.provider,
            "model": req.model,
            "warnings": warnings,
        }))
    }

    /// Validate a strategy against eval-readiness criteria. Without
    /// `scenario_id`: shape-only check. With `scenario_id`: additionally
    /// checks asset/timeframe alignment. Required input: `strategy_id`.
    /// Optional: `scenario_id`. Returns
    /// `{ eval_ready, errors, warnings, asset?, timeframe? }`.
    #[tool(
        description = "Preflight-validate a strategy for eval readiness. Required: strategy_id. \
        Optional: scenario_id (cross-checks asset/timeframe alignment). \
        Returns { eval_ready, errors, warnings } — errors is empty when eval_ready=true."
    )]
    async fn xvn_strategy_validate_preflight(
        &self,
        Parameters(req): Parameters<StrategyValidatePreflightReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;

        // Load strategy from filesystem store.
        let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
        let strategy = store
            .load(&req.strategy_id)
            .await
            .map_err(|e| {
                // Walk the error chain to detect file-not-found vs other errors.
                let is_not_found = e.chain().any(|cause| {
                    if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
                        io_err.kind() == std::io::ErrorKind::NotFound
                    } else {
                        false
                    }
                }) || e.to_string().contains("not found");
                if is_not_found {
                    rmcp::ErrorData::invalid_params(
                        format!("not found: strategy '{}'", req.strategy_id),
                        None,
                    )
                } else {
                    rmcp::ErrorData::internal_error(e.to_string(), None)
                }
            })?;

        // Optionally load the scenario for cross-check.
        let scenario = if let Some(sid) = &req.scenario_id {
            let sc = api_scenario::get(&ctx, sid)
                .await
                .map_err(api_err_to_mcp)?;
            Some(sc)
        } else {
            None
        };

        let preflight = preflight_validate(&strategy, scenario.as_ref());

        let mut out = serde_json::json!({
            "eval_ready": preflight.eval_ready,
            "errors": preflight.errors,
            "warnings": preflight.warnings,
        });

        if let Some(sc) = &scenario {
            let asset = sc.asset.first().map(|a| a.venue_symbol.as_str()).unwrap_or("");
            let tf_minutes = (sc.granularity.seconds() / 60) as u32;
            let timeframe = if tf_minutes % 60 == 0 {
                format!("{}h", tf_minutes / 60)
            } else {
                format!("{tf_minutes}m")
            };
            out["asset"] = serde_json::Value::String(asset.to_string());
            out["timeframe"] = serde_json::Value::String(timeframe);
        }

        json_or_err(&out)
    }

    /// Launch one eval run per scenario against a strategy, wait for all to
    /// complete, and return a unified batch result. Required inputs:
    /// `strategy_id`, `scenario_ids` (at least one). Optional: `mode`
    /// (default `backtest`), `review_with` (agent profile id).
    /// Returns `{ batch_id, strategy_id, runs: [...] }`.
    #[tool(
        description = "Run a strategy against multiple scenarios in one batch, wait for all to complete. \
        Required: strategy_id, scenario_ids (non-empty list). Optional: mode (default backtest), \
        review_with (agent profile id for post-run review). \
        Returns { batch_id, strategy_id, runs: [{ scenario_id, scenario_name, run_id, status, \
        return_pct, sharpe, drawdown_pct, decisions, actions, error?, review? }] }."
    )]
    async fn xvn_eval_batch_run(
        &self,
        Parameters(req): Parameters<EvalBatchRunReq>,
    ) -> Result<String, rmcp::ErrorData> {
        if req.scenario_ids.is_empty() {
            return Err(rmcp::ErrorData::invalid_params(
                "scenario_ids must be non-empty".to_string(),
                None,
            ));
        }

        let mode_str = req.mode.as_deref().unwrap_or("backtest");
        let _mode = xvision_engine::eval::run::RunMode::parse(mode_str).ok_or_else(|| {
            rmcp::ErrorData::invalid_params(
                format!("unknown mode {mode_str:?}; expected one of: paper | backtest"),
                None,
            )
        })?;

        let ctx = self.api_context().await?;

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

        // Launch one run per scenario via the env-bound `api_eval::run`
        // (same path the CLI uses for `xvn eval batch run`).
        let mut entries: Vec<serde_json::Value> = Vec::with_capacity(req.scenario_ids.len());

        for (scenario_id, scenario_name) in req.scenario_ids.iter().zip(scenario_names.iter()) {
            let run_req = api_eval::EvalRunRequest {
                agent_id: req.strategy_id.clone(),
                scenario_id: scenario_id.clone(),
                mode: _mode,
                params_override: None,
            };

            let entry = match api_eval::run(&ctx, run_req).await {
                Ok(run) => {
                    let _ = api_eval::attach_run_to_batch(&ctx, &run.id, &batch_id).await;
                    let actions = action_distribution_mcp(&ctx, &run.id)
                        .await
                        .unwrap_or_default();
                    let (return_pct, sharpe, drawdown_pct, decisions) =
                        if let Some(m) = &run.metrics {
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
                Err(e) => serde_json::json!({
                    "scenario_id": scenario_id,
                    "scenario_name": scenario_name,
                    "run_id": "",
                    "status": "failed",
                    "return_pct": null,
                    "sharpe": null,
                    "drawdown_pct": null,
                    "decisions": 0,
                    "actions": {},
                    "error": e.to_string(),
                }),
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
    #[tool(
        description = "Show the persisted status of an eval batch by id. \
        Required: batch_id. \
        Returns { batch_id, strategy_id, status, created_at, completed_at, review_with, run_ids }."
    )]
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
            let detail = api_eval::get_batch(&ctx, bid)
                .await
                .map_err(api_err_to_mcp)?;
            detail.run_ids
        } else {
            req.run_ids.clone()
        };

        let ctx = self.api_context().await?;
        let report = api_eval::compare(&ctx, CompareRunsRequest { run_ids })
            .await
            .map_err(api_err_to_mcp)?;

        if req.markdown {
            let md = format_comparison_markdown(&report);
            return json_or_err(&md);
        }

        json_or_err(&report)
    }

    /// Select a comparable set of scenarios by asset, timeframe, decision
    /// count, and regime labels. Read-only — nothing is created.
    /// Either `target_decisions` (Mode A, ±10 %) or `same_decisions=true`
    /// + `max_decisions` (Mode B) must be set.
    /// Returns an array of `{ id, name, asset, timeframe, decision_count }`.
    #[tool(
        description = "Filter the scenario library and return a ranked subset. \
        Optional: assets (list), timeframe (e.g. 4h), regimes (list), count (default 4). \
        Decision-count mode: target_decisions (Mode A, ±10%) or same_decisions=true + \
        max_decisions (Mode B, common count). \
        Returns [{ id, name, asset, timeframe, decision_count }]."
    )]
    async fn xvn_scenarios_select(
        &self,
        Parameters(req): Parameters<ScenariosSelectReq>,
    ) -> Result<String, rmcp::ErrorData> {
        if req.target_decisions.is_none() && !req.same_decisions {
            return Err(rmcp::ErrorData::invalid_params(
                "specify either target_decisions (Mode A) or same_decisions=true + max_decisions (Mode B)".to_string(),
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
            .map(parse_timeframe_minutes_mcp)
            .transpose()?;

        let ctx = self.api_context().await?;
        let all = api_scenario::list(
            &ctx,
            api_scenario::ListScenariosFilter {
                source: None,
                tags: vec![],
                include_archived: false,
                parent_scenario_id: None,
            },
        )
        .await
        .map_err(api_err_to_mcp)?;

        let count = req.count.unwrap_or(4);
        let rows = select_scenarios_mcp(
            &all,
            &req.assets,
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

/// Parse a timeframe string (e.g. `4h`) to `decision_cadence_minutes`.
/// Accepted: `1m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `1d`.
fn parse_timeframe_minutes_mcp(timeframe: &str) -> Result<u32, rmcp::ErrorData> {
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
            format!(
                "unknown timeframe '{other}'. Accepted: 1m, 5m, 15m, 30m, 1h, 2h, 4h, 1d"
            ),
            None,
        )),
    }
}

/// Count each action kind in the decisions table for a run.
/// Returns a `serde_json::Value` map (`{ "long_open": N, ... }`).
async fn action_distribution_mcp(
    ctx: &ApiContext,
    run_id: &str,
) -> anyhow::Result<serde_json::Value> {
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
    asset: String,
    timeframe: String,
    decision_count: u64,
}

/// Compute the decision bar count for a scenario (window bars minus warmup).
fn scenario_decision_count_mcp(s: &Scenario) -> u64 {
    let window_secs = (s.time_window.end - s.time_window.start).num_seconds() as u64;
    let bar_secs = s.granularity.seconds();
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
/// Takes a pre-fetched scenario list and applies asset / timeframe / regime /
/// decision-count filters plus the cap. No DB access.
fn select_scenarios_mcp(
    scenarios: &[Scenario],
    assets: &[String],
    timeframe_minutes: Option<u32>,
    regimes: &[String],
    target_decisions: Option<u64>,
    same_decisions: bool,
    max_decisions: Option<u64>,
    count: usize,
) -> Result<Vec<SelectRow>, String> {
    use std::collections::HashSet;

    // 1. Pre-filter by asset / timeframe / regime.
    let mut candidates: Vec<&Scenario> = scenarios
        .iter()
        .filter(|s| {
            if !assets.is_empty() {
                let sym = s.asset.first().map(|a| a.symbol.as_str()).unwrap_or("");
                let matched = assets.iter().any(|want| {
                    let norm = want.split('/').next().unwrap_or(want);
                    sym.eq_ignore_ascii_case(norm) || want.eq_ignore_ascii_case(sym)
                });
                if !matched {
                    return false;
                }
            }
            if let Some(tf_min) = timeframe_minutes {
                let bar_min = (s.granularity.seconds() / 60) as u32;
                if bar_min != tf_min {
                    return false;
                }
            }
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
            .map(|s| scenario_decision_count_mcp(s))
            .filter(|&c| c <= max)
            .collect();
        let mut count_freq: std::collections::HashMap<u64, usize> =
            std::collections::HashMap::new();
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
        candidates.retain(|s| scenario_decision_count_mcp(s) == target_count);
    } else if let Some(t) = target_decisions {
        let lo = (t as f64 * 0.9).floor() as u64;
        let hi = (t as f64 * 1.1).ceil() as u64;
        candidates.retain(|s| {
            let dc = scenario_decision_count_mcp(s);
            dc >= lo && dc <= hi
        });
    }

    // 4. Sort by closeness to target.
    candidates.sort_by_key(|s| {
        let dc = scenario_decision_count_mcp(s);
        if target_decisions.is_some() || same_decisions {
            (dc as i64 - target_count as i64).unsigned_abs()
        } else {
            0u64
        }
    });

    // 5. Cap at `count`, one-per-asset preference.
    let mut seen_assets: HashSet<String> = HashSet::new();
    let mut selected: Vec<&Scenario> = Vec::with_capacity(count);
    for s in &candidates {
        if selected.len() >= count {
            break;
        }
        let sym = s
            .asset
            .first()
            .map(|a| a.symbol.as_str())
            .unwrap_or("-")
            .to_string();
        if seen_assets.insert(sym) {
            selected.push(s);
        }
    }
    for s in &candidates {
        if selected.len() >= count {
            break;
        }
        if !selected.iter().any(|r| r.id == s.id) {
            selected.push(s);
        }
    }

    // 6. Build output rows.
    let rows = selected
        .into_iter()
        .map(|s| {
            let asset = s
                .asset
                .first()
                .map(|a| a.symbol.as_str())
                .unwrap_or("-")
                .to_string();
            SelectRow {
                id: s.id.clone(),
                name: s.display_name.clone(),
                asset,
                timeframe: s.granularity.to_string(),
                decision_count: scenario_decision_count_mcp(s),
            }
        })
        .collect();

    Ok(rows)
}

/// Format a `ComparisonReport` as a simple Markdown table.
fn format_comparison_markdown(report: &xvision_engine::eval::compare::ComparisonReport) -> String {
    use std::fmt::Write;
    let mut md = String::new();
    let _ = writeln!(md, "| run_id | scenario_id | status | return_pct | sharpe | drawdown_pct |");
    let _ = writeln!(md, "|--------|-------------|--------|------------|--------|--------------|");
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
            r.id, r.scenario_id, r.status.as_str(), ret, sharpe, dd
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

    #[tokio::test]
    async fn list_templates_returns_known_set() {
        let tools = XvisionTools::default();
        let s = tools.xvn_list_templates().await.unwrap();
        let v = parsed(&s);
        let names: Vec<_> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"trend_follower"), "names: {names:?}");
        assert!(names.contains(&"breakout"));
        assert!(names.contains(&"mean_reversion"));
    }

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                template: "trend_follower".into(),
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
        assert_eq!(strategy["manifest"]["template"], "trend_follower");
    }

    #[tokio::test]
    async fn create_rejects_unknown_template() {
        let (tools, _td) = tools_with_tmp();
        let err = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                template: "nope".into(),
                name: "x".into(),
                creator: None,
            }))
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("unknown template"), "msg: {msg}");
    }

    #[tokio::test]
    async fn update_slot_mutates_only_provided_fields() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                template: "trend_follower".into(),
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
                prompt: Some("New prompt".into()),
                model_requirement: None,
                provider: None,
                model: None,
                allowed_tools: None,
            }))
            .await
            .unwrap();
        let v = parsed(&upd);
        assert_eq!(v["updated"], serde_json::json!(["prompt"]));

        let g = tools
            .xvn_get_strategy(Parameters(StrategyId { id }))
            .await
            .unwrap();
        let strategy = parsed(&g);
        assert_eq!(strategy["trader_slot"]["prompt"], "New prompt");
    }

    #[tokio::test]
    async fn update_slot_rejects_unknown_slot() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                template: "trend_follower".into(),
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
                prompt: Some("p".into()),
                model_requirement: None,
                provider: None,
                model: None,
                allowed_tools: None,
            }))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unknown slot"));
    }

    #[tokio::test]
    async fn set_mechanical_param_round_trips() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            }))
            .await
            .unwrap();
        let id = id_of(&s);

        tools
            .xvn_set_mechanical_param(Parameters(SetMechanicalParamReq {
                id: id.clone(),
                key: "ema_fast".into(),
                value: serde_json::json!(8),
            }))
            .await
            .unwrap();

        let g = tools
            .xvn_get_strategy(Parameters(StrategyId { id }))
            .await
            .unwrap();
        let strategy = parsed(&g);
        assert_eq!(strategy["mechanical_params"]["ema_fast"], 8);
    }

    #[tokio::test]
    async fn set_risk_config_preset_balanced_applies_known_values() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                template: "trend_follower".into(),
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
                template: "trend_follower".into(),
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
    async fn validate_draft_succeeds_for_fresh_template() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                template: "trend_follower".into(),
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
        assert_eq!(r["ok"], true);
        assert_eq!(r["errors"], serde_json::json!([]));
    }

    // --- eval verbs (Phase 3.D Task 12) ----------------------------------

    use chrono::{Duration as ChronoDuration, TimeZone, Utc};
    use xvision_engine::eval::run::{MetricsSummary, Run, RunMode, RunStatus};
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
        let mut run = Run::new_queued(agent_id.into(), scenario_id.into(), RunMode::Backtest);
        run.status = RunStatus::Completed;
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
            baselines: None,
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

    // ── Wave-C parity tool tests ──────────────────────────────────────────────

    // ── parse_timeframe_minutes_mcp ───────────────────────────────────────────

    #[test]
    fn parse_timeframe_1h_returns_60() {
        assert_eq!(parse_timeframe_minutes_mcp("1h").unwrap(), 60);
    }

    #[test]
    fn parse_timeframe_4h_returns_240() {
        assert_eq!(parse_timeframe_minutes_mcp("4h").unwrap(), 240);
    }

    #[test]
    fn parse_timeframe_1d_returns_1440() {
        assert_eq!(parse_timeframe_minutes_mcp("1d").unwrap(), 1440);
    }

    #[test]
    fn parse_timeframe_unknown_returns_error() {
        let e = parse_timeframe_minutes_mcp("3h").unwrap_err();
        assert!(e.to_string().contains("unknown timeframe"), "msg: {e}");
    }

    // ── strategy_create_atomic ────────────────────────────────────────────────

    #[tokio::test]
    async fn strategy_create_atomic_returns_strategy_id_and_agent_id() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_strategy_create_atomic(Parameters(StrategyCreateAtomicReq {
                prompt: "You are a trader.".into(),
                name: "mcp-atomic-test".into(),
                provider: "openrouter".into(),
                model: "kimi-k2".into(),
                asset: "ETH/USD".into(),
                timeframe: "4h".into(),
                role: None,
                creator: Some("@test".into()),
            }))
            .await
            .unwrap();
        let v = parsed(&s);
        assert!(v["strategy_id"].as_str().is_some(), "strategy_id absent: {v}");
        assert!(v["agent_id"].as_str().is_some(), "agent_id absent: {v}");
        // eval_ready may be true or false depending on preflight warnings
        assert!(v["eval_ready"].is_boolean(), "eval_ready absent: {v}");
        assert_eq!(v["provider"], "openrouter");
        assert_eq!(v["model"], "kimi-k2");
    }

    #[tokio::test]
    async fn strategy_create_atomic_rejects_unknown_timeframe() {
        let (tools, _td) = tools_with_tmp();
        let err = tools
            .xvn_strategy_create_atomic(Parameters(StrategyCreateAtomicReq {
                prompt: "Trade.".into(),
                name: "x".into(),
                provider: "openrouter".into(),
                model: "kimi-k2".into(),
                asset: "ETH/USD".into(),
                timeframe: "3h".into(), // invalid
                role: None,
                creator: None,
            }))
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("unknown timeframe"), "msg: {msg}");
    }

    // ── strategy_validate_preflight ───────────────────────────────────────────

    #[tokio::test]
    async fn strategy_validate_preflight_returns_preflight_result() {
        let (tools, _td) = tools_with_tmp();

        // Create a strategy first via the authoring path.
        let cs = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                template: "trend_follower".into(),
                name: "preflight-test".into(),
                creator: None,
            }))
            .await
            .unwrap();
        let id = id_of(&cs);

        let s = tools
            .xvn_strategy_validate_preflight(Parameters(StrategyValidatePreflightReq {
                strategy_id: id.clone(),
                scenario_id: None,
            }))
            .await
            .unwrap();
        let v = parsed(&s);
        assert!(v["eval_ready"].is_boolean(), "eval_ready absent: {v}");
        assert!(v["errors"].is_array(), "errors absent: {v}");
        assert!(v["warnings"].is_array(), "warnings absent: {v}");
    }

    #[tokio::test]
    async fn strategy_validate_preflight_returns_not_found_for_missing_strategy() {
        let (tools, _td) = tools_with_tmp();
        let err = tools
            .xvn_strategy_validate_preflight(Parameters(StrategyValidatePreflightReq {
                strategy_id: "01NOTEXIST0000000000000000".into(),
                scenario_id: None,
            }))
            .await
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("not found"), "expected not found, got: {msg}");
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

    // ── scenarios_select ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn scenarios_select_returns_canonical_scenarios() {
        let (tools, _td) = tools_with_tmp();
        // The DB is seeded with canonical scenarios on first open.
        let s = tools
            .xvn_scenarios_select(Parameters(ScenariosSelectReq {
                assets: vec![],
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
                assets: vec![],
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

    use xvision_engine::eval::scenario::{
        AdjustmentMode, AssetClass, AssetRef, BarCachePolicy, BarGranularity, CalendarRef,
        DataSource, Fees, FillModel, LatencyModel, LimitOrderFill, MarketOrderFill, QuoteCurrency,
        RefreshPolicy, ReplayMode, ScenarioSource, SlippageModel, TimeWindow, Venue, VenueSettings,
    };
    use xvision_engine::Capital;
    use std::str::FromStr;

    fn make_test_scenario(
        id: &str,
        asset_sym: &str,
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
            asset: vec![AssetRef {
                class: AssetClass::Crypto,
                symbol: asset_sym.to_string(),
                venue_symbol: format!("{asset_sym}/USD"),
            }],
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
                fees: Fees { maker_bps: 10, taker_bps: 25 },
                slippage: SlippageModel::None,
                latency: LatencyModel { decision_to_fill_ms: 0 },
                fill_model: FillModel {
                    market_order_fill: MarketOrderFill::FullAtClose,
                    limit_order_fill: LimitOrderFill::NeverFills,
                    partial_fills: false,
                    volume_constraints: None,
                },
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
        }
    }

    #[test]
    fn select_scenarios_mcp_mode_a_returns_matching() {
        // 300 1h bars − 200 warmup = 100 decisions. target=100 → ±10% → matches.
        let s = make_test_scenario("sc1", "ETH", "1h", 300 * 3_600, 200);
        let rows = select_scenarios_mcp(
            &[s],
            &[],
            None,
            &[],
            Some(100),
            false,
            None,
            4,
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].decision_count, 100);
    }

    #[test]
    fn select_scenarios_mcp_mode_a_empty_when_no_match() {
        // 50 decisions; target=200 (±10%→180..220) → no match.
        let s = make_test_scenario("sc1", "ETH", "1h", 250 * 3_600, 200);
        let rows = select_scenarios_mcp(
            &[s],
            &[],
            None,
            &[],
            Some(200),
            false,
            None,
            4,
        )
        .unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn select_scenarios_mcp_mode_b_finds_common_count() {
        let s1 = make_test_scenario("sc1", "ETH", "1h", 300 * 3_600, 200); // 100 decisions
        let s2 = make_test_scenario("sc2", "BTC", "1h", 300 * 3_600, 200); // 100 decisions
        let s3 = make_test_scenario("sc3", "SOL", "1h", 250 * 3_600, 200); // 50 decisions
        let rows = select_scenarios_mcp(&[s1, s2, s3], &[], None, &[], None, true, Some(200), 2)
            .unwrap();
        assert_eq!(rows.len(), 2);
        for r in &rows {
            assert_eq!(r.decision_count, 100);
        }
    }
}
