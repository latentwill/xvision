//! Strategy authoring dispatcher — pure Rust functions over `&dyn StrategyStore`
//! that mutate `Strategy`s. Both surfaces call into here:
//!
//! - `xvision-mcp` exposes these to external AI agents via MCP tool calls
//!   (`xvn_create_strategy`, `xvn_update_slot`, ...).
//! - `xvision-dashboard::wizard_loop` drives the same verbs from the
//!   server-side wizard agent over the tool-use loop.
//!
//! Errors are flat `anyhow::Result`; surface-specific error mapping
//! (rmcp::ErrorData, axum::Json, etc.) happens at the call site.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use ulid::Ulid;

use crate::strategies::{
    agent_ref::canonical_role,
    manifest::PublicManifest,
    mechanistic::{DecisionMode, MechanisticConfig},
    risk::{RiskConfig, RiskPreset},
    slot::LLMSlot,
    store::{apply_metadata_patch, StrategyMetadataPatch, StrategyStore},
    validate::{every_bar_warning, high_position_size_warning, no_filter_warnings, validate_strategy},
    AgentRef, PipelineDef, PipelineKind, Strategy,
};
use xvision_filters::{parse_json, validate as validate_filter_dsl, ActivationMode, Filter};

// ---------------------------------------------------------------------------
// types — request / response shapes shared by both surfaces.
// MCP wraps these with JsonSchema derives in its own request structs; the
// dashboard speaks them directly.
// ---------------------------------------------------------------------------

/// Stale shape kept only so external callers that imported the type
/// continue to compile. The MCP `xvn_list_templates` tool that
/// surfaced these was removed alongside the strategy template
/// registry on 2026-05-21; operator-readable strategy starters now
/// live as prepop seeds under `docs/strategies/templates/` and are
/// surfaced through the strategies folder (`xvn strategies init`),
/// not through this struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateInfo {
    pub name: String,
    pub display_name: String,
    pub plain_summary: String,
}

/// Request shape for [`create_strategy`]. After the 2026-05-21
/// template-registry removal, no `template` discriminator is taken —
/// `create_strategy` always produces a blank draft and operators
/// fill it in via the wizard / folder / subsequent slot writes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateStrategyReq {
    pub name: String,
    #[serde(default)]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStrategyOut {
    pub id: String,
    /// Informational warnings about the created strategy (e.g. no filter / every-bar
    /// token cost). Non-empty does not block creation; suppress with
    /// `acknowledge_no_filter`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateSlotReq {
    pub id: String,
    pub slot: String,
    pub attested_with: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSlotOut {
    pub id: String,
    pub updated: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifestReq {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain_summary: Option<String>,
    /// Optional display color. Use `Some("#RRGGBB")` to set, `Some("")` to clear,
    /// `None` to leave unchanged. Must be a 7-character CSS hex string when non-empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_universe: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_cadence_minutes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifestOut {
    pub id: String,
    pub updated: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddAgentRefRequest {
    pub strategy_id: String,
    pub agent_id: String,
    pub role: String,
    /// Phase A `AgentRef.activates`. `None` (default, the back-compat
    /// path) lets the dispatcher pick the slot's first capability.
    /// `Some(Capability::Filter)` is rejected; filters are saved JSON
    /// artifacts on the strategy, not agent refs.
    #[serde(default)]
    pub activates: Option<crate::agents::Capability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveAgentRefRequest {
    pub strategy_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameAgentRoleRequest {
    pub strategy_id: String,
    pub role: String,
    pub new_role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetPipelineRequest {
    pub strategy_id: String,
    pub pipeline: PipelineDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetRiskConfigReq {
    pub id: String,
    pub preset: Option<String>,
    pub explicit: Option<RiskConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetFilterReq {
    pub strategy_id: String,
    #[serde(default)]
    pub filter: Option<serde_json::Value>,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetRiskConfigOut {
    pub id: String,
    /// `preset` or `explicit`.
    pub applied: String,
}

/// Request to set or replace the strategy's deterministic DSL Filter.
///
/// The caller supplies the filter as DSL JSON source text and
/// the server parses + validates it. This keeps the operator's text
/// editor as the source of truth and avoids round-tripping a deeply
/// nested JSON tree through every client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetStrategyFilterReq {
    pub id: String,
    pub source: String,
    /// Must be `"json"`. Kept on the wire so older clients fail with a
    /// validation error instead of silently sending the wrong source form.
    #[serde(default = "default_filter_format")]
    pub format: String,
}

fn default_filter_format() -> String {
    "json".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetStrategyFilterOut {
    pub id: String,
    pub filter: Filter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateDraftOut {
    pub id: String,
    pub ok: bool,
    pub errors: Vec<String>,
    /// Soft validation signals — the strategy is still saveable but the
    /// operator may want to address them. The dashboard's strategy
    /// editor surfaces these alongside errors (without blocking save).
    ///
    /// Populated as of the agent-firing-filter wave with the no-Filter
    /// soft-warning from `validate::no_filter_warnings`. The field is
    /// additive — clients that omit it on the wire still parse cleanly
    /// via `#[serde(default)]`, and the
    /// `skip_serializing_if = "Vec::is_empty"` collapses the field out
    /// of the JSON when no warnings fire (e.g. the strategy carries
    /// `acknowledge_no_filter = true`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// dispatcher functions
// ---------------------------------------------------------------------------

/// Returns an empty template list.
///
/// The strategy `template_registry` was removed on 2026-05-21
/// (see `team/contracts/strategy-template-registry-removal.md`).
/// Operator-readable strategy starters now live as prepop seeds under
/// `docs/strategies/templates/` and surface through the strategies
/// folder (`xvn strategies init`). This function is kept as a stub so
/// existing non-wizard callers that haven't migrated yet don't fail
/// to compile; new callers should read seeds from
/// `xvision_engine::strategies_folder` instead.
pub fn list_templates() -> Vec<TemplateInfo> {
    Vec::new()
}

/// Create a new draft strategy. After the 2026-05-21
/// template-registry removal this always produces a blank `Strategy`
/// — there is no `template` discriminator to scaffold from. Operators
/// (via the wizard, CLI, or MCP follow-up calls) populate slots /
/// agents / risk on the blank draft before save.
pub async fn create_strategy(
    store: &dyn StrategyStore,
    req: CreateStrategyReq,
) -> anyhow::Result<CreateStrategyOut> {
    create_blank_strategy(store, req.name, req.creator).await
}

/// Build a minimal draft Strategy with no agents and no placeholder
/// trader prompt. The wizard uses this path: subsequent
/// `create_strategy_agent` / `update_slot` calls fill in real agent
/// content before the operator hits save.
///
/// Manifest carries `template: "custom"`. The strategies module is not
/// edited — the public `Strategy` / `PublicManifest` types are constructed
/// directly. No call into `template_registry`.
pub async fn create_blank_strategy(
    store: &dyn StrategyStore,
    name: String,
    creator: Option<String>,
) -> anyhow::Result<CreateStrategyOut> {
    let id = Ulid::new().to_string();
    let creator = creator.unwrap_or_else(|| "@anonymous".to_string());
    let draft = Strategy {
        manifest: PublicManifest {
            id: id.clone(),
            display_name: name,
            plain_summary: String::new(),
            creator,
            template: "custom".into(),
            regime_fit: Vec::new(),
            asset_universe: vec!["BTC/USD".into()],
            decision_cadence_minutes: 60,
            attested_with: Vec::new(),
            required_tools: Vec::new(),
            risk_preset_or_config: "conservative".into(),
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
        trader_slot: None,
        risk: RiskPreset::Conservative.expand(),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
        acknowledge_no_filter: false,
        decision_mode: Default::default(),
        mechanistic_config: None,
        briefing_indicators: Vec::new(),
        tunable_bounds: Vec::new(),
    };
    let mut warnings = every_bar_warning(&draft).map(|w| vec![w]).unwrap_or_default();
    if let Some(w) = high_position_size_warning(&draft) {
        warnings.push(w);
    }
    store.save(&draft).await?;
    Ok(CreateStrategyOut { id, warnings })
}

pub async fn get_strategy(store: &dyn StrategyStore, id: &str) -> anyhow::Result<Strategy> {
    store.load(id).await
}

pub async fn update_slot(store: &dyn StrategyStore, req: UpdateSlotReq) -> anyhow::Result<UpdateSlotOut> {
    let mut strategy = store.load(&req.id).await?;
    let slot_field = match req.slot.as_str() {
        "regime" => &mut strategy.regime_slot,
        "trader" => &mut strategy.trader_slot,
        other => anyhow::bail!("unknown slot `{other}` — must be one of: regime, trader"),
    };
    let slot = slot_field.get_or_insert_with(|| LLMSlot {
        role: req.slot.clone(),
        attested_with: String::new(),
        allowed_tools: vec![],
        provider: None,
        model: None,
    });
    let mut updated: Vec<String> = Vec::new();
    if let Some(m) = req.attested_with {
        slot.attested_with = m;
        updated.push("attested_with".into());
    }
    if let Some(p) = req.provider {
        slot.provider = if p.trim().is_empty() { None } else { Some(p) };
        updated.push("provider".into());
    }
    if let Some(m) = req.model {
        slot.model = if m.trim().is_empty() { None } else { Some(m) };
        updated.push("model".into());
    }
    if let Some(t) = req.allowed_tools {
        slot.allowed_tools = t;
        updated.push("allowed_tools".into());
    }
    if updated.is_empty() {
        anyhow::bail!(
            "no fields to update — supply at least one of attested_with / provider / model / allowed_tools"
        );
    }
    store.save(&strategy).await?;
    Ok(UpdateSlotOut { id: req.id, updated })
}

pub async fn update_manifest(
    store: &dyn StrategyStore,
    req: UpdateManifestReq,
) -> anyhow::Result<UpdateManifestOut> {
    let mut strategy = store.load(&req.id).await?;

    // Determine which fields the caller intends to set (before applying)
    // so we can record the `updated` list in a deterministic order.
    // Color is included when Some("") (clear) or Some(non-empty) (set).
    let has_display_name = req.display_name.is_some();
    let has_plain_summary = req.plain_summary.is_some();
    let has_color = req.color.is_some();
    let has_asset_universe = req.asset_universe.is_some();
    let has_decision_cadence = req.decision_cadence_minutes.is_some();

    if !has_display_name && !has_plain_summary && !has_color && !has_asset_universe && !has_decision_cadence {
        anyhow::bail!(
            "no manifest fields to update — supply at least one of: \
             display_name, plain_summary, color, asset_universe, decision_cadence_minutes"
        );
    }

    // Delegate all validation + mutation to the shared metadata-patch helper.
    // This keeps semantics consistent with the REST inspector path
    // (update_inspector → update_metadata → StrategyMetadataPatch).
    let patch = StrategyMetadataPatch {
        display_name: req.display_name,
        plain_summary: req.plain_summary,
        color: req.color,
        asset_universe: req.asset_universe,
        decision_cadence_minutes: req.decision_cadence_minutes,
        creator: None,
    };
    apply_metadata_patch(&mut strategy, patch).map_err(|e| anyhow::anyhow!("{e}"))?;

    store.save(&strategy).await?;

    // Build the `updated` list in a stable field order.
    let mut updated: Vec<String> = Vec::new();
    if has_display_name {
        updated.push("display_name".into());
    }
    if has_plain_summary {
        updated.push("plain_summary".into());
    }
    if has_color {
        updated.push("color".into());
    }
    if has_asset_universe {
        updated.push("asset_universe".into());
    }
    if has_decision_cadence {
        updated.push("decision_cadence_minutes".into());
    }

    Ok(UpdateManifestOut { id: req.id, updated })
}

fn reject_reserved_agent_role(role: &str) -> anyhow::Result<()> {
    if role == "filter" {
        anyhow::bail!(
            "agent role 'filter' is reserved for strategy JSON filters; choose a trader, analyst, risk, or reviewer role"
        );
    }
    Ok(())
}

pub async fn add_agent_ref(store: &dyn StrategyStore, req: AddAgentRefRequest) -> anyhow::Result<Strategy> {
    let mut strategy = store.load(&req.strategy_id).await?;
    let role = canonical_role(&req.role);
    if role.is_empty() {
        anyhow::bail!("role is required");
    }
    reject_reserved_agent_role(&role)?;
    if matches!(req.activates, Some(crate::agents::Capability::Filter)) {
        anyhow::bail!("agent type 'filter' is removed; attach a strategy JSON filter instead");
    }
    if strategy.agents.iter().any(|a| canonical_role(&a.role) == role) {
        anyhow::bail!("role '{role}' already exists on strategy");
    }
    strategy.agents.push(AgentRef {
        agent_id: req.agent_id,
        role,
        activates: req.activates,
        prompt_override: None,
        model_override: None,
        checkpoint: None,
        veto: None,
    });
    if strategy.pipeline.kind == PipelineKind::Single && strategy.agents.len() > 1 {
        strategy.pipeline.kind = PipelineKind::Sequential;
    }
    store.save(&strategy).await?;
    Ok(strategy)
}

pub async fn remove_agent_ref(
    store: &dyn StrategyStore,
    req: RemoveAgentRefRequest,
) -> anyhow::Result<Strategy> {
    let mut strategy = store.load(&req.strategy_id).await?;
    let role = canonical_role(&req.role);
    let before = strategy.agents.len();
    strategy.agents.retain(|a| canonical_role(&a.role) != role);
    if strategy.agents.len() == before {
        anyhow::bail!("role '{}' not found on strategy", req.role);
    }
    if strategy.pipeline.kind == PipelineKind::Graph {
        strategy
            .pipeline
            .edges
            .retain(|edge| canonical_role(&edge.from_role) != role && canonical_role(&edge.to_role) != role);
    }
    if strategy.agents.len() <= 1 {
        strategy.pipeline = PipelineDef::default();
    } else if strategy.pipeline.kind == PipelineKind::Graph {
        validate_graph_pipeline(&strategy)?;
    }
    store.save(&strategy).await?;
    Ok(strategy)
}

pub async fn rename_agent_role(
    store: &dyn StrategyStore,
    req: RenameAgentRoleRequest,
) -> anyhow::Result<Strategy> {
    let mut strategy = store.load(&req.strategy_id).await?;
    let role = canonical_role(&req.role);
    let new_role = canonical_role(&req.new_role);
    if new_role.is_empty() {
        anyhow::bail!("new role is required");
    }
    reject_reserved_agent_role(&new_role)?;
    if strategy
        .agents
        .iter()
        .any(|a| canonical_role(&a.role) == new_role && canonical_role(&a.role) != role)
    {
        anyhow::bail!("role '{new_role}' already exists on strategy");
    }
    let mut found = false;
    for agent in &mut strategy.agents {
        if canonical_role(&agent.role) == role {
            agent.role = new_role.clone();
            found = true;
            break;
        }
    }
    if !found {
        anyhow::bail!("role '{}' not found on strategy", req.role);
    }
    if strategy.pipeline.kind == PipelineKind::Graph {
        for edge in &mut strategy.pipeline.edges {
            if canonical_role(&edge.from_role) == role {
                edge.from_role = new_role.clone();
            }
            if canonical_role(&edge.to_role) == role {
                edge.to_role = new_role.clone();
            }
        }
        validate_graph_pipeline(&strategy)?;
    }
    store.save(&strategy).await?;
    Ok(strategy)
}

pub async fn set_pipeline(store: &dyn StrategyStore, req: SetPipelineRequest) -> anyhow::Result<Strategy> {
    if req.pipeline.kind != PipelineKind::Graph && !req.pipeline.edges.is_empty() {
        anyhow::bail!("pipeline edges are only valid for graph pipelines");
    }
    let mut strategy = store.load(&req.strategy_id).await?;
    strategy.pipeline = req.pipeline;
    validate_pipeline_shape(&strategy)?;
    store.save(&strategy).await?;
    Ok(strategy)
}

fn validate_pipeline_shape(strategy: &Strategy) -> anyhow::Result<()> {
    if strategy.pipeline.kind == PipelineKind::Single && strategy.agents.len() > 1 {
        anyhow::bail!("single pipelines cannot include more than one agent");
    }
    if strategy.pipeline.kind == PipelineKind::Graph {
        validate_graph_pipeline(strategy)?;
    }
    Ok(())
}

fn validate_graph_pipeline(strategy: &Strategy) -> anyhow::Result<()> {
    let roles: HashSet<String> = strategy
        .agents
        .iter()
        .map(|agent| canonical_role(&agent.role))
        .collect();
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    let mut seen_edges: HashSet<(String, String)> = HashSet::new();

    for edge in &strategy.pipeline.edges {
        let from = canonical_role(&edge.from_role);
        let to = canonical_role(&edge.to_role);

        if !roles.contains(&from) || !roles.contains(&to) {
            anyhow::bail!("graph edges must reference existing strategy roles");
        }
        if from == to {
            anyhow::bail!("graph pipelines cannot contain self-loops");
        }
        if !seen_edges.insert((from.clone(), to.clone())) {
            anyhow::bail!("graph pipelines cannot contain duplicate edges");
        }
        adjacency.entry(from).or_default().push(to);
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for role in &roles {
        if graph_cycle_from(role, &adjacency, &mut visiting, &mut visited) {
            anyhow::bail!("graph pipelines must be acyclic");
        }
    }

    Ok(())
}

fn graph_cycle_from(
    role: &str,
    adjacency: &HashMap<String, Vec<String>>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) -> bool {
    if visited.contains(role) {
        return false;
    }
    if !visiting.insert(role.to_string()) {
        return true;
    }
    if let Some(neighbors) = adjacency.get(role) {
        for next in neighbors {
            if graph_cycle_from(next, adjacency, visiting, visited) {
                return true;
            }
        }
    }
    visiting.remove(role);
    visited.insert(role.to_string());
    false
}

pub async fn set_filter(store: &dyn StrategyStore, req: SetFilterReq) -> anyhow::Result<Strategy> {
    let mut strategy = store.load(&req.strategy_id).await?;
    let filter = parse_filter_payload(req.filter, req.source.as_deref(), &req.strategy_id)?;
    strategy.filter = filter;
    strategy.activation_mode = if strategy.filter.is_some() {
        xvision_filters::ActivationMode::FilterGated
    } else {
        xvision_filters::ActivationMode::EveryBar
    };
    store.save(&strategy).await?;
    Ok(strategy)
}

fn parse_filter_payload(
    raw_filter: Option<serde_json::Value>,
    source: Option<&str>,
    strategy_id: &str,
) -> anyhow::Result<Option<xvision_filters::Filter>> {
    let Some(raw_filter) = raw_filter else {
        return Ok(None);
    };
    if raw_filter.is_null() {
        return Ok(None);
    }
    let raw_filter = extract_filter_payload(raw_filter);
    let maybe_filter = match raw_filter {
        serde_json::Value::String(src) => parse_filter_text(&src, source, strategy_id),
        other_value => match source {
            Some("json") => parse_filter_value(other_value, strategy_id),
            Some(source) if source.trim().is_empty() => parse_filter_value(other_value, strategy_id),
            None => parse_filter_value(other_value, strategy_id),
            Some(_) => parse_filter_text(
                &serde_json::to_string(&other_value)
                    .map_err(|e| anyhow::anyhow!("filter parse error: {e}"))?,
                source,
                strategy_id,
            ),
        },
    };
    maybe_filter.map(Some)
}

/// Parse + validate a raw filter JSON value into a `Filter`, stamping a fresh
/// `id` (when absent) and the owning `strategy_id`. Runs `xvision_filters::validate`,
/// so the returned filter is guaranteed well-formed. Shared with the optimizer's
/// structural filter-creation path (xvision-vxn) so an LLM-authored filter is
/// validated exactly like an operator-authored one.
pub(crate) fn parse_filter_value(
    raw_filter: serde_json::Value,
    strategy_id: &str,
) -> anyhow::Result<xvision_filters::Filter> {
    let serde_json::Value::Object(mut obj) = raw_filter else {
        anyhow::bail!("filter parse error: filter payload must be an object");
    };
    if obj
        .get("id")
        .and_then(serde_json::Value::as_str)
        .is_none_or(|id| id.trim().is_empty())
    {
        obj.insert("id".into(), serde_json::Value::String(Ulid::new().to_string()));
    }
    obj.insert(
        "strategy_id".into(),
        serde_json::Value::String(strategy_id.to_string()),
    );
    let json = serde_json::to_string(&serde_json::Value::Object(obj))
        .map_err(|e| anyhow::anyhow!("filter parse error: {e}"))?;
    parse_filter_text(&json, Some("json"), strategy_id)
}

fn parse_filter_text(
    source_text: &str,
    source: Option<&str>,
    strategy_id: &str,
) -> anyhow::Result<xvision_filters::Filter> {
    if let Some(source) = source {
        if source != "json" {
            anyhow::bail!("unknown filter source format `{source}` — must be `json`");
        }
    }
    let mut filter = parse_filter_text_preferring_json(source_text, strategy_id)?;
    if filter.id.as_str().is_empty() {
        filter.id = xvision_filters::FilterId::new(Ulid::new().to_string());
    }
    filter.strategy_id = strategy_id.to_string().into();
    xvision_filters::validate(&filter).map_err(|e| anyhow::anyhow!("filter validation error: {e}"))?;
    Ok(filter)
}

fn parse_filter_text_preferring_json(
    source_text: &str,
    strategy_id: &str,
) -> anyhow::Result<xvision_filters::Filter> {
    let mut filter =
        xvision_filters::parse_json(source_text).map_err(|e| anyhow::anyhow!("filter parse error: {e}"))?;
    if filter.id.as_str().is_empty() {
        filter.id = xvision_filters::FilterId::new(Ulid::new().to_string());
    }
    filter.strategy_id = strategy_id.to_string().into();
    xvision_filters::validate(&filter).map_err(|e| anyhow::anyhow!("filter validation error: {e}"))?;
    Ok(filter)
}

fn extract_filter_payload(raw_filter: serde_json::Value) -> serde_json::Value {
    match raw_filter {
        serde_json::Value::Object(mut obj) => {
            if let Some(filter) = obj.remove("filter") {
                filter
            } else {
                serde_json::Value::Object(obj)
            }
        }
        other => other,
    }
}

pub async fn set_risk_config(
    store: &dyn StrategyStore,
    req: SetRiskConfigReq,
) -> anyhow::Result<SetRiskConfigOut> {
    let (config, applied, manifest_risk) = match (req.preset, req.explicit) {
        (Some(p), None) => {
            let preset = match p.as_str() {
                "conservative" => RiskPreset::Conservative,
                "balanced" => RiskPreset::Balanced,
                "aggressive" => RiskPreset::Aggressive,
                other => anyhow::bail!(
                    "unknown preset `{other}` — must be one of: conservative, balanced, aggressive"
                ),
            };
            (preset.expand(), "preset", p)
        }
        (None, Some(cfg)) => (cfg, "explicit", "custom".to_string()),
        (Some(_), Some(_)) => anyhow::bail!("preset and explicit are mutually exclusive"),
        (None, None) => anyhow::bail!("supply either preset or explicit"),
    };
    let mut strategy = store.load(&req.id).await?;
    strategy.risk = config;
    strategy.manifest.risk_preset_or_config = manifest_risk;
    store.save(&strategy).await?;
    Ok(SetRiskConfigOut {
        id: req.id,
        applied: applied.into(),
    })
}

/// Parse the supplied DSL source, validate it, and write it to
/// `Strategy.filter`. Promotes `activation_mode` from `EveryBar` to
/// `FilterGated` so the runtime actually consults the filter on every
/// bar — the inverse of [`clear_strategy_filter`].
pub async fn set_strategy_filter(
    store: &dyn StrategyStore,
    req: SetStrategyFilterReq,
) -> anyhow::Result<SetStrategyFilterOut> {
    let filter = match req.format.as_str() {
        "json" => parse_json(&req.source).map_err(|e| anyhow::anyhow!("filter parse error: {e}"))?,
        other => anyhow::bail!("unknown filter source format `{other}` — must be `json`"),
    };
    validate_filter_dsl(&filter).map_err(|e| anyhow::anyhow!("filter validation error: {e}"))?;

    let mut strategy = store.load(&req.id).await?;
    strategy.filter = Some(filter.clone());
    strategy.activation_mode = ActivationMode::FilterGated;
    store.save(&strategy).await?;
    Ok(SetStrategyFilterOut { id: req.id, filter })
}

/// Clear the strategy's filter, reverting `activation_mode` to
/// `EveryBar`. No-op when the strategy already has no filter.
pub async fn clear_strategy_filter(store: &dyn StrategyStore, id: &str) -> anyhow::Result<()> {
    let mut strategy = store.load(id).await?;
    strategy.filter = None;
    strategy.activation_mode = ActivationMode::EveryBar;
    store.save(&strategy).await
}

/// Set the strategy's decision mode and optional mechanistic config.
/// When `decision_mode == Mechanistic`, the caller must supply a
/// `mechanistic_config`; when `decision_mode == Agentic`, the config is
/// cleared to `None`. Returns the updated `Strategy`.
pub async fn set_mechanistic_config(
    store: &dyn StrategyStore,
    id: &str,
    decision_mode: DecisionMode,
    mechanistic_config: Option<MechanisticConfig>,
) -> anyhow::Result<Strategy> {
    let mut strategy = store.load(id).await?;
    strategy.decision_mode = decision_mode;
    strategy.mechanistic_config = mechanistic_config;
    store.save(&strategy).await?;
    Ok(strategy)
}

pub async fn validate_draft(store: &dyn StrategyStore, id: &str) -> anyhow::Result<ValidateDraftOut> {
    let strategy = store.load(id).await?;
    let mut errors = match validate_strategy(&strategy) {
        Ok(()) => vec![],
        Err(e) => vec![e.to_string()],
    };
    if strategy.agents.is_empty() {
        errors.push(
            "strategy is not eval-ready: attach at least one complete agent with provider/model before validation"
                .to_string(),
        );
    }
    let ok = errors.is_empty();
    // Soft signals — surfaced alongside errors but do not block save.
    // L2 of the firing-filter operator-surface spec (2026-05-22) calls
    // for the SPA validate panel to render the no-Filter warning so the
    // operator sees it whether they're using the CLI or the SPA.
    let mut warnings = no_filter_warnings(&strategy);
    if let Some(w) = high_position_size_warning(&strategy) {
        warnings.push(w);
    }
    Ok(ValidateDraftOut {
        id: id.to_string(),
        ok,
        errors,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategies::store::FilesystemStore;
    use crate::strategies::PipelineEdge;

    fn store_in_tmp() -> (FilesystemStore, tempfile::TempDir) {
        let td = tempfile::tempdir().unwrap();
        let store = FilesystemStore::new(td.path().to_path_buf());
        (store, td)
    }

    #[tokio::test]
    async fn create_blank_strategy_produces_empty_agents_and_no_placeholder_slot() {
        // Acceptance: the wizard's blank-draft path produces a draft
        // with no AgentRefs and no trader_slot (no placeholder prompt),
        // so the downstream `create_strategy_agent` / `update_slot` flow
        // fills in real agent content before the save-gate sees it.
        let (store, _td) = store_in_tmp();
        let out = create_blank_strategy(&store, "Blank Run".into(), Some("@test".into()))
            .await
            .expect("blank strategy create must succeed");

        let draft = get_strategy(&store, &out.id).await.expect("draft must load");
        assert!(draft.agents.is_empty(), "blank draft must have no AgentRefs");
        assert!(
            draft.trader_slot.is_none(),
            "blank draft must not carry a placeholder trader slot"
        );
        assert!(
            draft.regime_slot.is_none(),
            "blank draft should not carry a regime slot"
        );
        assert_eq!(draft.manifest.template, "custom");
        assert_eq!(draft.manifest.display_name, "Blank Run");
        assert_eq!(draft.manifest.creator, "@test");
    }

    #[tokio::test]
    async fn create_blank_strategy_defaults_creator_to_anonymous() {
        let (store, _td) = store_in_tmp();
        let out = create_blank_strategy(&store, "x".into(), None)
            .await
            .expect("blank strategy create must succeed");
        let draft = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(draft.manifest.creator, "@anonymous");
    }

    #[test]
    fn create_strategy_request_rejects_unknown_fields() {
        let err = serde_json::from_str::<CreateStrategyReq>(r#"{"name":"x","creator":null,"surprise":true}"#)
            .expect_err("unknown create-strategy fields must be rejected");

        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn create_strategy_request_rejects_legacy_template_field() {
        // Post-template-registry-removal: the `template` field is no
        // longer accepted on the create request shape. Existing
        // callers must drop it; serde catches stragglers.
        let err = serde_json::from_str::<CreateStrategyReq>(
            r#"{"template":"trend_follower","name":"x","creator":null}"#,
        )
        .expect_err("legacy template field must be rejected");
        assert!(err.to_string().contains("unknown field"));
        assert!(err.to_string().contains("template"));
    }

    #[tokio::test]
    async fn create_strategy_warns_for_default_every_bar_draft() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "warn-test".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        assert!(
            !out.warnings.is_empty(),
            "default blank draft (EveryBar) must produce a creation warning"
        );
        assert!(
            out.warnings[0].contains("burns tokens"),
            "warning must mention token cost, got: {}",
            out.warnings[0]
        );
    }

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "btc-mom-1".into(),
                creator: Some("@test".into()),
            },
        )
        .await
        .unwrap();
        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(strategy.manifest.id, out.id);
        // create_strategy now produces a blank draft. The `template`
        // field stays on `PublicManifest` as a free-text label but
        // create_blank_strategy stamps it `"custom"` for back-compat.
        assert_eq!(strategy.manifest.template, "custom");
    }

    #[tokio::test]
    async fn update_slot_reports_updated_fields() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let upd = update_slot(
            &store,
            UpdateSlotReq {
                id: out.id.clone(),
                slot: "trader".into(),
                attested_with: Some("anthropic.claude-sonnet-4.6".into()),
                provider: None,
                model: None,
                allowed_tools: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(upd.updated, vec!["attested_with".to_string()]);

        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(
            strategy.trader_slot.unwrap().attested_with,
            "anthropic.claude-sonnet-4.6"
        );
    }

    #[tokio::test]
    async fn update_manifest_round_trips_inspector_fields() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let upd = update_manifest(
            &store,
            UpdateManifestReq {
                id: out.id.clone(),
                display_name: None,
                plain_summary: None,
                color: None,
                asset_universe: Some(vec!["BTC/USD".into()]),
                decision_cadence_minutes: Some(360),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            upd.updated,
            vec![
                "asset_universe".to_string(),
                "decision_cadence_minutes".to_string()
            ]
        );

        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(strategy.manifest.asset_universe, vec!["BTC/USD"]);
        assert_eq!(strategy.manifest.decision_cadence_minutes, 360);
    }

    // W6: new failing tests for display_name / plain_summary / color fields.
    // These must fail before the implementation is in place.

    #[tokio::test]
    async fn update_manifest_sets_display_name_and_plain_summary() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "Initial Name".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let upd = update_manifest(
            &store,
            UpdateManifestReq {
                id: out.id.clone(),
                display_name: Some("Renamed Strategy".into()),
                plain_summary: Some("Buys dips on momentum signals".into()),
                color: None,
                asset_universe: None,
                decision_cadence_minutes: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(upd.updated, vec!["display_name", "plain_summary"]);

        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(strategy.manifest.display_name, "Renamed Strategy");
        assert_eq!(strategy.manifest.plain_summary, "Buys dips on momentum signals");
    }

    #[tokio::test]
    async fn update_manifest_only_display_name_no_other_fields_succeeds() {
        // Guard test: a call supplying ONLY display_name must succeed,
        // not bail with the old "no manifest fields to update" guard.
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "Orig".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let upd = update_manifest(
            &store,
            UpdateManifestReq {
                id: out.id.clone(),
                display_name: Some("New Name".into()),
                plain_summary: None,
                color: None,
                asset_universe: None,
                decision_cadence_minutes: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(upd.updated, vec!["display_name"]);
        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(strategy.manifest.display_name, "New Name");
        // plain_summary and asset_universe must be untouched
        assert_eq!(strategy.manifest.plain_summary, "");
    }

    #[tokio::test]
    async fn update_manifest_partial_no_clobber() {
        // Setting display_name must not touch asset_universe, cadence, or plain_summary.
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "Stable".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        // First, set asset_universe and cadence.
        update_manifest(
            &store,
            UpdateManifestReq {
                id: out.id.clone(),
                display_name: None,
                plain_summary: None,
                color: None,
                asset_universe: Some(vec!["ETH/USD".into()]),
                decision_cadence_minutes: Some(120),
            },
        )
        .await
        .unwrap();

        // Now update only display_name — other fields must remain unchanged.
        update_manifest(
            &store,
            UpdateManifestReq {
                id: out.id.clone(),
                display_name: Some("New Display".into()),
                plain_summary: None,
                color: None,
                asset_universe: None,
                decision_cadence_minutes: None,
            },
        )
        .await
        .unwrap();

        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(strategy.manifest.display_name, "New Display");
        assert_eq!(strategy.manifest.asset_universe, vec!["ETH/USD"]);
        assert_eq!(strategy.manifest.decision_cadence_minutes, 120);
        assert_eq!(strategy.manifest.plain_summary, "");
    }

    #[tokio::test]
    async fn update_manifest_sets_color() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "Colored".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let upd = update_manifest(
            &store,
            UpdateManifestReq {
                id: out.id.clone(),
                display_name: None,
                plain_summary: None,
                color: Some("#D4A547".into()),
                asset_universe: None,
                decision_cadence_minutes: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(upd.updated, vec!["color"]);
        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(strategy.manifest.color, Some("#D4A547".into()));
    }

    #[tokio::test]
    async fn update_manifest_clears_color_with_empty_string() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "Clearable".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        // Set a color first.
        update_manifest(
            &store,
            UpdateManifestReq {
                id: out.id.clone(),
                display_name: None,
                plain_summary: None,
                color: Some("#AABBCC".into()),
                asset_universe: None,
                decision_cadence_minutes: None,
            },
        )
        .await
        .unwrap();

        // Clear with empty string.
        let upd = update_manifest(
            &store,
            UpdateManifestReq {
                id: out.id.clone(),
                display_name: None,
                plain_summary: None,
                color: Some("".into()),
                asset_universe: None,
                decision_cadence_minutes: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(upd.updated, vec!["color"]);
        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert!(strategy.manifest.color.is_none());
    }

    #[tokio::test]
    async fn add_agent_ref_canonicalizes_role_and_rejects_variant_duplicates() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let strategy = add_agent_ref(
            &store,
            AddAgentRefRequest {
                strategy_id: out.id.clone(),
                agent_id: "01HZAGENT1".into(),
                role: " Trader ".into(),
                activates: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(strategy.agents[0].role, "trader");

        let err = add_agent_ref(
            &store,
            AddAgentRefRequest {
                strategy_id: out.id,
                agent_id: "01HZAGENT2".into(),
                role: "TRADER".into(),
                activates: None,
            },
        )
        .await
        .expect_err("canonical duplicate should be rejected");
        assert!(err.to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn add_agent_ref_rejects_filter_agent_type() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "z".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        // None → activates stays None on the resulting AgentRef.
        let s = add_agent_ref(
            &store,
            AddAgentRefRequest {
                strategy_id: out.id.clone(),
                agent_id: "01HZAGENTPLAIN0000000000000".into(),
                role: "trader".into(),
                activates: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(s.agents[0].activates, None);

        // Some(Filter) is no longer a valid agent ref. Filters are saved
        // JSON artifacts on the strategy, not agents.
        let err = add_agent_ref(
            &store,
            AddAgentRefRequest {
                strategy_id: out.id,
                agent_id: "01HZAGENTFILTER0000000000000".into(),
                role: "regime_filter".into(),
                activates: Some(crate::agents::Capability::Filter),
            },
        )
        .await
        .expect_err("filter agent type should be rejected");
        assert!(err.to_string().contains("agent type 'filter' is removed"));
    }

    #[tokio::test]
    async fn rename_and_remove_agent_role_match_canonical_role_keys() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let mut strategy = get_strategy(&store, &out.id).await.unwrap();
        strategy.agents = vec![
            AgentRef {
                agent_id: "01HZSCOUT".into(),
                role: "Scout".into(),
                activates: None,
                prompt_override: None,
                model_override: None,
                checkpoint: None,
                veto: None,
            },
            AgentRef {
                agent_id: "01HZTRADER".into(),
                role: "Trader".into(),
                activates: None,
                prompt_override: None,
                model_override: None,
                checkpoint: None,
                veto: None,
            },
        ];
        strategy.pipeline = PipelineDef {
            kind: PipelineKind::Graph,
            edges: vec![PipelineEdge {
                from_role: "SCOUT".into(),
                to_role: "TRADER".into(),
                condition: None,
            }],
        };
        store.save(&strategy).await.unwrap();

        let strategy = rename_agent_role(
            &store,
            RenameAgentRoleRequest {
                strategy_id: out.id.clone(),
                role: " scout ".into(),
                new_role: " Analyst ".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(strategy.agents[0].role, "analyst");
        assert_eq!(strategy.pipeline.edges[0].from_role, "analyst");
        assert_eq!(strategy.pipeline.edges[0].to_role, "trader");

        let strategy = remove_agent_ref(
            &store,
            RemoveAgentRefRequest {
                strategy_id: out.id,
                role: " TRADER ".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(strategy.agents.len(), 1);
        assert_eq!(strategy.agents[0].role, "analyst");
        assert_eq!(strategy.pipeline, PipelineDef::default());
    }

    #[tokio::test]
    async fn set_strategy_filter_parses_json_and_flips_activation_mode() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "filter-x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        // Baseline: every-bar, no filter.
        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert!(strategy.filter.is_none());
        assert!(matches!(strategy.activation_mode, ActivationMode::EveryBar));

        // Set: minimal valid Filter JSON — the only accepted authoring form.
        const FILTER_JSON: &str = r#"{
  "id": "f_01JX0000000000000000000000",
  "strategy_id": "s_01JX0000000000000000000000",
  "display_name": "EMA Cross",
  "asset_scope": ["BTC/USD"],
  "timeframe": "1h",
  "scan_cadence": "bar_close",
  "cooldown_bars": 3,
  "conditions": {
    "all": [
      { "lhs": "ema_20", "op": ">", "rhs": "ema_50" }
    ]
  }
}"#;
        let r = set_strategy_filter(
            &store,
            SetStrategyFilterReq {
                id: out.id.clone(),
                source: FILTER_JSON.to_string(),
                format: "json".to_string(),
            },
        )
        .await
        .unwrap();
        assert_eq!(r.filter.display_name, "EMA Cross");

        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert!(strategy.filter.is_some());
        assert!(matches!(strategy.activation_mode, ActivationMode::FilterGated));

        // Clear: filter goes away, activation mode reverts.
        clear_strategy_filter(&store, &out.id).await.unwrap();
        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert!(strategy.filter.is_none());
        assert!(matches!(strategy.activation_mode, ActivationMode::EveryBar));
    }

    #[tokio::test]
    async fn set_strategy_filter_rejects_malformed_source() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "filter-bad".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let r = set_strategy_filter(
            &store,
            SetStrategyFilterReq {
                id: out.id.clone(),
                source: "this is not valid json".to_string(),
                format: "json".to_string(),
            },
        )
        .await;
        assert!(r.is_err(), "malformed source must error");

        // Strategy unchanged on error.
        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert!(strategy.filter.is_none());
        assert!(matches!(strategy.activation_mode, ActivationMode::EveryBar));
    }

    #[tokio::test]
    async fn set_risk_config_preset_balanced_updates_manifest_label() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let r = set_risk_config(
            &store,
            SetRiskConfigReq {
                id: out.id.clone(),
                preset: Some("balanced".into()),
                explicit: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(r.applied, "preset");

        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(strategy.risk.risk_pct_per_trade, 0.015);
        assert_eq!(strategy.manifest.risk_preset_or_config, "balanced");
    }

    #[tokio::test]
    async fn validate_draft_surfaces_no_filter_warning_for_explicit_trader() {
        // The validate endpoint must surface the no-filter soft-warning
        // so the strategy editor can render it alongside errors.
        use crate::agents::Capability;

        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "explicit-trader-no-filter".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        // Hand-author a strategy with one explicit-Trader AgentRef and
        // no Filter. Going through `add_agent_ref` would leave
        // `activates: None`, which (per the warning's design) does NOT
        // fire the warning — only explicit Trader does.
        let mut strategy = store.load(&out.id).await.unwrap();
        strategy.agents.push(AgentRef {
            agent_id: "01HZAGENT_TRADER".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        });
        store.save(&strategy).await.unwrap();

        let v = validate_draft(&store, &out.id).await.unwrap();
        assert!(
            v.warnings.iter().any(|w| w.contains("no saved JSON filter")),
            "expected no-Filter warning in ValidateDraftOut.warnings, got: {:?}",
            v.warnings,
        );
        // Errors stay clean — the warning is soft.
        assert!(
            v.errors.is_empty(),
            "warning must not push the draft into errors, got: {:?}",
            v.errors,
        );
        // And the round-trip JSON includes the field so SPA consumers
        // see it (skip_serializing_if pulls it out only when empty).
        let json = serde_json::to_value(&v).unwrap();
        assert!(
            json.get("warnings").is_some(),
            "warnings field must be serialized when populated; got {json}",
        );
    }

    #[tokio::test]
    async fn validate_draft_omits_warnings_field_when_strategy_acknowledges() {
        // `acknowledge_no_filter = true` silences the warning. The
        // `skip_serializing_if = "Vec::is_empty"` then drops the field
        // from the wire shape so the SPA panel renders nothing.
        use crate::agents::Capability;

        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "ack-no-filter".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let mut strategy = store.load(&out.id).await.unwrap();
        strategy.agents.push(AgentRef {
            agent_id: "01HZAGENT_TRADER".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
            prompt_override: None,
            model_override: None,
            checkpoint: None,
            veto: None,
        });
        strategy.acknowledge_no_filter = true;
        store.save(&strategy).await.unwrap();

        let v = validate_draft(&store, &out.id).await.unwrap();
        assert!(
            v.warnings.is_empty(),
            "acknowledge_no_filter must suppress all warnings, got: {:?}",
            v.warnings,
        );
        let json = serde_json::to_value(&v).unwrap();
        assert!(
            json.get("warnings").is_none(),
            "empty warnings must be omitted from the wire shape; got {json}",
        );
    }

    #[tokio::test]
    async fn validate_draft_reports_missing_agent_for_fresh_template() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let v = validate_draft(&store, &out.id).await.unwrap();
        assert!(!v.ok);
        // Match the discriminating phrase from the actual error emitted at line 501:
        // "attach at least one complete agent with provider/model before validation".
        // We assert on "attach at least one complete agent" — specific enough to
        // distinguish this error from other validation errors, but not so brittle
        // that rephrasing the trailing "before validation" clause breaks the test.
        assert!(
            v.errors
                .iter()
                .any(|e| e.contains("attach at least one complete agent")),
            "expected missing attached agent error, got {:?}",
            v.errors,
        );
    }
}
