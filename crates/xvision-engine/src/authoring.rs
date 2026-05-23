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
    risk::{RiskConfig, RiskPreset},
    slot::LLMSlot,
    store::StrategyStore,
    validate::{no_filter_warnings, validate_strategy},
    AgentRef, PipelineDef, PipelineKind, Strategy,
};

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
    pub asset_universe: Option<Vec<String>>,
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
    /// `Some(Capability::Filter)` is the value the strategy editor's
    /// inline Filter composer sets when attaching a Filter agent so
    /// the Phase B dispatcher picks the Filter handler at this
    /// position even if the referenced agent also advertises Trader.
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
pub struct SetMechanicalParamReq {
    pub id: String,
    pub key: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetRiskConfigReq {
    pub id: String,
    pub preset: Option<String>,
    pub explicit: Option<RiskConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetRiskConfigOut {
    pub id: String,
    /// `preset` or `explicit`.
    pub applied: String,
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
/// agents / mechanical_params / risk on the blank draft before save.
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
/// Manifest carries `template: "custom"` so the typed-`MechanicalParams`
/// dispatch falls through to `MechanicalParams::Custom` and
/// `mechanical_params: {}` validates. The strategies module is not edited
/// — the public `Strategy` / `PublicManifest` types are constructed
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
        },
        hypothesis: None,
        agents: Vec::new(),
        pipeline: PipelineDef::default(),
        regime_slot: None,
        intern_slot: None,
        trader_slot: None,
        risk: RiskPreset::Conservative.expand(),
        mechanical_params: serde_json::json!({}),
        activation_mode: xvision_filters::ActivationMode::EveryBar,
        filter: None,
    acknowledge_no_filter: false,
    };
    store.save(&draft).await?;
    Ok(CreateStrategyOut { id })
}

pub async fn get_strategy(store: &dyn StrategyStore, id: &str) -> anyhow::Result<Strategy> {
    store.load(id).await
}

pub async fn update_slot(store: &dyn StrategyStore, req: UpdateSlotReq) -> anyhow::Result<UpdateSlotOut> {
    let mut strategy = store.load(&req.id).await?;
    let slot_field = match req.slot.as_str() {
        "regime" => &mut strategy.regime_slot,
        "intern" => &mut strategy.intern_slot,
        "trader" => &mut strategy.trader_slot,
        other => anyhow::bail!("unknown slot `{other}` — must be one of: regime, intern, trader"),
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
    let mut updated: Vec<String> = Vec::new();

    if let Some(asset_universe) = req.asset_universe {
        let mut normalized = Vec::with_capacity(asset_universe.len());
        for asset in asset_universe {
            let asset = asset.trim();
            if asset.is_empty() {
                anyhow::bail!("asset_universe cannot include blank assets");
            }
            if !normalized.iter().any(|item| item == asset) {
                normalized.push(asset.to_string());
            }
        }
        if normalized.is_empty() {
            anyhow::bail!("asset_universe must include at least one asset");
        }
        strategy.manifest.asset_universe = normalized;
        updated.push("asset_universe".into());
    }

    if let Some(decision_cadence_minutes) = req.decision_cadence_minutes {
        if decision_cadence_minutes == 0 {
            anyhow::bail!("decision_cadence_minutes must be greater than 0");
        }
        strategy.manifest.decision_cadence_minutes = decision_cadence_minutes;
        updated.push("decision_cadence_minutes".into());
    }

    if updated.is_empty() {
        anyhow::bail!("no manifest fields to update — supply asset_universe and/or decision_cadence_minutes");
    }

    store.save(&strategy).await?;
    Ok(UpdateManifestOut { id: req.id, updated })
}

pub async fn add_agent_ref(store: &dyn StrategyStore, req: AddAgentRefRequest) -> anyhow::Result<Strategy> {
    let mut strategy = store.load(&req.strategy_id).await?;
    let role = canonical_role(&req.role);
    if role.is_empty() {
        anyhow::bail!("role is required");
    }
    if strategy.agents.iter().any(|a| canonical_role(&a.role) == role) {
        anyhow::bail!("role '{role}' already exists on strategy");
    }
    strategy.agents.push(AgentRef {
        agent_id: req.agent_id,
        role,
        activates: req.activates,
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

pub async fn set_mechanical_param(
    store: &dyn StrategyStore,
    req: SetMechanicalParamReq,
) -> anyhow::Result<()> {
    let mut strategy = store.load(&req.id).await?;
    let map = strategy
        .mechanical_params
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("mechanical_params is not a JSON object"))?;
    map.insert(req.key, req.value);
    // Post-2026-05-21 template-registry removal: no per-template typed
    // dispatch exists, so the param is persisted verbatim. Per-strategy
    // schema validation lands in a future change keyed on the
    // strategies-folder seed library, not on a binary registry.
    store.save(&strategy).await
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
    let warnings = no_filter_warnings(&strategy);
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
        assert!(
            draft.mechanical_params.as_object().is_some_and(|m| m.is_empty()),
            "blank draft must have empty mechanical_params, got: {:?}",
            draft.mechanical_params
        );
        assert!(draft.agents.is_empty(), "blank draft must have no AgentRefs");
        assert!(
            draft.trader_slot.is_none(),
            "blank draft must not carry a placeholder trader slot"
        );
        assert!(
            draft.regime_slot.is_none(),
            "blank draft should not carry a regime slot"
        );
        assert!(
            draft.intern_slot.is_none(),
            "blank draft should not carry an intern slot"
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
    async fn add_agent_ref_threads_activates_capability_to_pipeline_position() {
        // Phase 3 of agent-firing-filter: the strategy editor's inline
        // composer attaches a Filter agent by sending
        // `activates: Some(Capability::Filter)` on AddAgentRefRequest.
        // The new AgentRef must carry that value so the Phase B
        // dispatcher picks the Filter handler at this position even
        // when the referenced agent advertises more than one
        // capability. None on the request preserves today's behavior.
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

        // Some(Filter) → the new AgentRef carries it verbatim.
        let s = add_agent_ref(
            &store,
            AddAgentRefRequest {
                strategy_id: out.id,
                agent_id: "01HZAGENTFILTER0000000000000".into(),
                role: "regime_filter".into(),
                activates: Some(crate::agents::Capability::Filter),
            },
        )
        .await
        .unwrap();
        let added = s
            .agents
            .iter()
            .find(|r| r.role == "regime_filter")
            .expect("added");
        assert_eq!(added.activates, Some(crate::agents::Capability::Filter));
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
            },
            AgentRef {
                agent_id: "01HZTRADER".into(),
                role: "Trader".into(),
                activates: None,
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
        // L2 of the firing-filter operator-surface spec — the SPA
        // validate endpoint must surface the no-Filter soft-warning so
        // the strategy editor can render it alongside errors. Without
        // this wiring the CLI sees the warning but the SPA does not.
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
        // fire the warning — only explicit Trader/Critic does.
        let mut strategy = store.load(&out.id).await.unwrap();
        strategy.agents.push(AgentRef {
            agent_id: "01HZAGENT_TRADER".into(),
            role: "trader".into(),
            activates: Some(Capability::Trader),
        });
        store.save(&strategy).await.unwrap();

        let v = validate_draft(&store, &out.id).await.unwrap();
        assert!(
            v.warnings.iter().any(|w| w.contains("no upstream Filter")),
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
