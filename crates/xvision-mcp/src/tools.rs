//! Tool surface advertised over MCP.
//!
//! Two surfaces:
//! - **Indicator tools** (`xvn_health`, `xvn_sma`, ...) — stateless: the
//!   caller supplies the price / HLC series as parameters and we dispatch
//!   into `xvision-data`. NaN positions in the output mark indicator warmup
//!   and travel through the wire as JSON `null` (we round-trip through
//!   `Option<f64>` for that).
//! - **Authoring tools** (`xvn_list_templates`, `xvn_create_strategy`, ...)
//!   — stateful: persist `StrategyBundle`s to `$XVN_HOME/strategies/`
//!   via `xvision_engine::bundle::store::FilesystemStore`.

use std::path::PathBuf;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use ulid::Ulid;

use xvision_data as xvn;
use xvision_engine::api::eval::{self as api_eval, CompareRunsRequest, ListRunsRequest};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::authoring;
use xvision_engine::bundle::{
    risk::RiskConfig,
    store::{BundleStore, FilesystemStore},
};
use xvision_engine::eval::run::RunStatus;
use xvision_engine::eval::store::RunStore;
use xvision_skills::attach::attach_skill_to_agent;
use xvision_skills::store::{FilesystemSkillStore, SkillStore};

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
    /// Tools the slot is allowed to call.
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetMechanicalParamReq {
    pub id: String,
    /// Key inside `bundle.mechanical_params` (template-specific).
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
    /// Optional filter: only return runs for this strategy bundle id.
    #[serde(default)]
    pub strategy_bundle_hash: Option<String>,
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

// --- skill-domain request shapes (Plan 2b Phase B) -------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateSkillReq {
    /// Full skill markdown including YAML frontmatter. The skill `name` is
    /// taken from the frontmatter and used as the persisted filename.
    pub markdown: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AttachSkillReq {
    /// ULID of the strategy bundle to mutate.
    pub agent_id: String,
    /// Slot to overwrite: `regime` | `intern` | `trader`.
    pub slot: String,
    /// Name of a skill saved under `$XVN_HOME/skills/<skill_name>.md`.
    pub skill_name: String,
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
        return PathBuf::from(s);
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

    fn skill_store(&self) -> FilesystemSkillStore {
        let root = self
            .xvn_home
            .clone()
            .unwrap_or_else(resolve_xvn_home)
            .join("skills");
        FilesystemSkillStore::new(root)
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
    // xvision_engine's bundle store + template registry + validator.
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
        description = "Create a new strategy draft from a template. Persists the bundle and returns { id } (ULID)."
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

    /// Get a strategy bundle by id. Returns the full `StrategyBundle` JSON.
    #[tool(description = "Get a strategy bundle by id. Returns the full StrategyBundle JSON.")]
    async fn xvn_get_strategy(
        &self,
        Parameters(req): Parameters<StrategyId>,
    ) -> Result<String, rmcp::ErrorData> {
        let bundle = authoring::get_strategy(&self.store(), &req.id)
            .await
            .map_err(authoring_err)?;
        json_or_err(&bundle)
    }

    /// Update a slot on a strategy bundle. Only fields with non-null values
    /// are mutated. Returns `{ id, updated: [...] }` listing which fields
    /// changed.
    #[tool(
        description = "Update a slot on a strategy bundle. Only fields with non-null values are mutated. Returns { id, updated }."
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
                allowed_tools: req.allowed_tools,
            },
        )
        .await
        .map_err(authoring_err)?;
        json_or_err(&out)
    }

    /// Set a key inside `bundle.mechanical_params`. Templates document
    /// which keys they accept. Returns `{ id, key }`.
    #[tool(
        description = "Set a key inside bundle.mechanical_params. Templates document which keys they accept. Returns { id, key }."
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

    /// Set the risk config on a strategy bundle. Provide either `preset`
    /// (one of `conservative` / `balanced` / `aggressive`) or `explicit`
    /// (a full `RiskConfig`). Mutually exclusive. Returns `{ id, applied }`.
    #[tool(
        description = "Set the risk config on a strategy bundle. Supply either preset (conservative/balanced/aggressive) or explicit (full RiskConfig). Mutually exclusive. Returns { id, applied }."
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

    /// Validate a strategy draft against bundle invariants (trader slot
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

    // -----------------------------------------------------------------------
    // skill verbs (Plan 2b Phase B). All three operate on
    // `$XVN_HOME/skills/<name>.md` (markdown bodies) and, for attach, the
    // existing `$XVN_HOME/strategies/<id>.json` bundle store.
    // -----------------------------------------------------------------------

    /// Persist a new skill from raw markdown. The `name` field of the
    /// frontmatter becomes the on-disk filename (`<name>.md`). Returns the
    /// parsed `name` and the SHA-256 of the supplied markdown for
    /// content-addressable bookkeeping.
    #[tool(description = "Create / overwrite a skill from raw markdown. Returns { name, content_hash }.")]
    async fn xvn_create_skill(
        &self,
        Parameters(req): Parameters<CreateSkillReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let parsed = xvision_skills::parse(&req.markdown)
            .map_err(|e| rmcp::ErrorData::invalid_params(format!("{e}"), None))?;
        self.skill_store()
            .save(&parsed.name, &req.markdown)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("{e:#}"), None))?;
        json_or_err(&serde_json::json!({
            "name": parsed.name,
            "content_hash": parsed.content_hash,
        }))
    }

    /// List skills saved under `$XVN_HOME/skills/`. Returns an array of
    /// `{ name, display_name, description, version }` (sorted by filename).
    /// Empty when no skills are registered yet.
    #[tool(
        description = "List saved skills. Returns array of {name, display_name, description, version}, sorted by name."
    )]
    async fn xvn_list_skills(&self) -> Result<String, rmcp::ErrorData> {
        let store = self.skill_store();
        let names = store
            .list()
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("{e:#}"), None))?;
        let mut out = Vec::with_capacity(names.len());
        for name in names {
            let skill = store
                .load(&name)
                .await
                .map_err(|e| rmcp::ErrorData::internal_error(format!("{e:#}"), None))?;
            out.push(serde_json::json!({
                "name": skill.name,
                "display_name": skill.display_name,
                "description": skill.description,
                "version": skill.version,
            }));
        }
        json_or_err(&out)
    }

    /// Attach a saved skill to a slot of a saved strategy bundle. Replaces
    /// the slot prompt with the skill body, sets `model_requirement`, and
    /// unions skill `allowed_tools` into the slot's tool set. Persists the
    /// mutated bundle back to `$XVN_HOME/strategies/<agent_id>.json`.
    /// Errors if the strategy or skill is missing, the slot role is unknown,
    /// or the targeted slot is empty.
    #[tool(
        description = "Attach a skill to a slot (regime|intern|trader) of a saved strategy. Returns { agent_id, slot, skill_name }."
    )]
    async fn xvn_attach_skill_to_agent(
        &self,
        Parameters(req): Parameters<AttachSkillReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let strategies = self.store();
        let mut bundle = strategies
            .load(&req.agent_id)
            .await
            .map_err(|e| rmcp::ErrorData::invalid_params(format!("load strategy: {e:#}"), None))?;
        let skill = self
            .skill_store()
            .load(&req.skill_name)
            .await
            .map_err(|e| rmcp::ErrorData::invalid_params(format!("load skill: {e:#}"), None))?;
        attach_skill_to_agent(&mut bundle, &req.slot, &skill)
            .map_err(|e| rmcp::ErrorData::invalid_params(format!("{e:#}"), None))?;
        strategies
            .save(&bundle)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("save strategy: {e:#}"), None))?;
        json_or_err(&serde_json::json!({
            "agent_id": req.agent_id,
            "slot": req.slot,
            "skill_name": req.skill_name,
        }))
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
    /// strategy_bundle_hash + scenario_id + mode + status + started_at +
    /// completed_at + headline metrics). Optional filters narrow the
    /// result to a strategy / scenario / status.
    #[tool(
        description = "List eval runs (slim shape). Optional filters: strategy_bundle_hash, scenario_id, status."
    )]
    async fn xvn_eval_list(
        &self,
        Parameters(req): Parameters<EvalListReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let ctx = self.api_context().await?;
        let status = req.status.as_deref().map(parse_status_for_mcp).transpose()?;
        let summaries = api_eval::list_summaries(
            &ctx,
            ListRunsRequest {
                strategy_bundle_hash: req.strategy_bundle_hash,
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

    /// List the canonical scenarios bundled with this binary. These are
    /// the same scenarios the CLI's `xvn eval scenarios` shows.
    #[tool(
        description = "List canonical scenarios bundled with this binary. Returns id, display_name, asset_universe, regime_tags, time_window_days."
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
        let bundle = parsed(&g);
        assert_eq!(bundle["manifest"]["id"], id);
        assert_eq!(bundle["manifest"]["template"], "trend_follower");
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
        let bundle = parsed(&g);
        assert_eq!(bundle["trader_slot"]["prompt"], "New prompt");
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
        let bundle = parsed(&g);
        assert_eq!(bundle["mechanical_params"]["ema_fast"], 8);
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
        let bundle = parsed(&g);
        assert_eq!(bundle["risk"]["risk_pct_per_trade"], 0.015);
        assert_eq!(bundle["risk"]["max_concurrent_positions"], 2);
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
        bundle_hash: &str,
        scenario_id: &str,
        total_return_pct: f64,
    ) -> String {
        let ctx = tools.api_context().await.unwrap();
        let store = RunStore::new(ctx.db.clone());
        let mut run = Run::new_queued(bundle_hash.into(), scenario_id.into(), RunMode::Backtest);
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
                strategy_bundle_hash: Some("h-B".into()),
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

    // --- skill verbs (Plan 2b Phase B) -----------------------------------

    const SKILL_FIXTURE: &str = include_str!("../../xvision-skills/tests/fixtures/crypto-trader-base.md");

    #[tokio::test]
    async fn create_skill_persists_and_returns_name_and_hash() {
        let (tools, _td) = tools_with_tmp();
        let s = tools
            .xvn_create_skill(Parameters(CreateSkillReq {
                markdown: SKILL_FIXTURE.into(),
            }))
            .await
            .unwrap();
        let v = parsed(&s);
        assert_eq!(v["name"].as_str().unwrap(), "crypto-trader-base");
        assert_eq!(v["content_hash"].as_str().unwrap().len(), 64);

        // round-trip: list_skills surfaces it
        let s = tools.xvn_list_skills().await.unwrap();
        let v = parsed(&s);
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"].as_str().unwrap(), "crypto-trader-base");
        assert_eq!(
            arr[0]["display_name"].as_str().unwrap(),
            "Generalist crypto trader"
        );
    }

    #[tokio::test]
    async fn create_skill_rejects_malformed_markdown() {
        let (tools, _td) = tools_with_tmp();
        let err = tools
            .xvn_create_skill(Parameters(CreateSkillReq {
                markdown: "no frontmatter here".into(),
            }))
            .await
            .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("frontmatter"),
            "msg: {err}"
        );
    }

    #[tokio::test]
    async fn list_skills_empty_when_dir_absent() {
        let (tools, _td) = tools_with_tmp();
        let s = tools.xvn_list_skills().await.unwrap();
        let v = parsed(&s);
        assert!(v.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn attach_skill_mutates_strategy_trader_slot() {
        let (tools, _td) = tools_with_tmp();
        // 1) create a strategy from a template (trader slot is filled)
        let s = tools
            .xvn_create_strategy(Parameters(CreateStrategyReq {
                template: "trend_follower".into(),
                name: "skill-attach-test".into(),
                creator: None,
            }))
            .await
            .unwrap();
        let id = id_of(&s);

        // 2) register the skill
        tools
            .xvn_create_skill(Parameters(CreateSkillReq {
                markdown: SKILL_FIXTURE.into(),
            }))
            .await
            .unwrap();

        // 3) attach
        let s = tools
            .xvn_attach_skill_to_agent(Parameters(AttachSkillReq {
                agent_id: id.clone(),
                slot: "trader".into(),
                skill_name: "crypto-trader-base".into(),
            }))
            .await
            .unwrap();
        let v = parsed(&s);
        assert_eq!(v["agent_id"].as_str().unwrap(), id);
        assert_eq!(v["slot"].as_str().unwrap(), "trader");
        assert_eq!(v["skill_name"].as_str().unwrap(), "crypto-trader-base");

        // 4) verify the bundle's trader slot now carries the skill body
        let g = tools
            .xvn_get_strategy(Parameters(StrategyId { id }))
            .await
            .unwrap();
        let bundle = parsed(&g);
        let prompt = bundle["trader_slot"]["prompt"].as_str().unwrap();
        assert!(prompt.contains("crypto trader"), "prompt was: {prompt}");
    }

    #[tokio::test]
    async fn attach_skill_unknown_strategy_404() {
        let (tools, _td) = tools_with_tmp();
        tools
            .xvn_create_skill(Parameters(CreateSkillReq {
                markdown: SKILL_FIXTURE.into(),
            }))
            .await
            .unwrap();
        let err = tools
            .xvn_attach_skill_to_agent(Parameters(AttachSkillReq {
                agent_id: "no-such-strategy".into(),
                slot: "trader".into(),
                skill_name: "crypto-trader-base".into(),
            }))
            .await
            .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("load strategy"),
            "msg: {err}"
        );
    }

    #[tokio::test]
    async fn attach_skill_unknown_skill_404() {
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
            .xvn_attach_skill_to_agent(Parameters(AttachSkillReq {
                agent_id: id,
                slot: "trader".into(),
                skill_name: "no-such-skill".into(),
            }))
            .await
            .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("load skill"),
            "msg: {err}"
        );
    }
}
