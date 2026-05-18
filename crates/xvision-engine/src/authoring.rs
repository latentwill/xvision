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
    risk::{RiskConfig, RiskPreset},
    slot::LLMSlot,
    store::StrategyStore,
    validate::validate_strategy,
    AgentRef, PipelineDef, PipelineKind, Strategy,
};
use crate::templates::registry as template_registry;

// ---------------------------------------------------------------------------
// types — request / response shapes shared by both surfaces.
// MCP wraps these with JsonSchema derives in its own request structs; the
// dashboard speaks them directly.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateInfo {
    pub name: String,
    pub display_name: String,
    pub plain_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateStrategyReq {
    pub template: String,
    pub name: String,
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
    pub prompt: Option<String>,
    pub model_requirement: Option<String>,
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
}

// ---------------------------------------------------------------------------
// dispatcher functions
// ---------------------------------------------------------------------------

pub fn list_templates() -> Vec<TemplateInfo> {
    template_registry::list_template_names()
        .iter()
        .filter_map(|name| {
            template_registry::get(name).map(|t| TemplateInfo {
                name: t.name().to_string(),
                display_name: t.display_name().to_string(),
                plain_summary: t.plain_summary().to_string(),
            })
        })
        .collect()
}

pub async fn create_strategy(
    store: &dyn StrategyStore,
    req: CreateStrategyReq,
) -> anyhow::Result<CreateStrategyOut> {
    let tpl = template_registry::get(&req.template)
        .ok_or_else(|| anyhow::anyhow!("unknown template '{}' — try list_templates", req.template))?;
    let id = Ulid::new().to_string();
    let creator = req.creator.unwrap_or_else(|| "@anonymous".to_string());
    let draft = tpl.new_draft(id.clone(), req.name, creator);
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
        prompt: String::new(),
        model_requirement: String::new(),
        allowed_tools: vec![],
        provider: None,
        model: None,
    });
    let mut updated: Vec<String> = Vec::new();
    if let Some(p) = req.prompt {
        slot.prompt = p;
        updated.push("prompt".into());
    }
    if let Some(m) = req.model_requirement {
        slot.model_requirement = m;
        updated.push("model_requirement".into());
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
            "no fields to update — supply at least one of prompt / model_requirement / allowed_tools"
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
    let map = strategy.mechanical_params.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!("mechanical_params is not a JSON object — template invariant violation")
    })?;
    map.insert(req.key, req.value);
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
    Ok(ValidateDraftOut {
        id: id.to_string(),
        ok,
        errors,
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

    #[test]
    fn list_templates_returns_known_set() {
        let names: Vec<_> = list_templates().into_iter().map(|t| t.name).collect();
        assert!(names.contains(&"trend_follower".to_string()));
        assert!(names.contains(&"breakout".to_string()));
        assert!(names.contains(&"mean_reversion".to_string()));
    }

    #[test]
    fn custom_template_is_registered_for_blank_draft_fallback() {
        // The wizard's `create_strategy` defaults to the `custom`
        // template when the agent omits `template`. Pin the
        // dependency: if `custom` ever gets renamed, the wizard
        // default needs to follow.
        let names: Vec<_> = list_templates().into_iter().map(|t| t.name).collect();
        assert!(
            names.contains(&"custom".to_string()),
            "wizard create_strategy fallback assumes the `custom` template \
             is registered; available: {names:?}"
        );
    }

    #[tokio::test]
    async fn create_strategy_from_custom_template_produces_blank_mechanical_params() {
        // Acceptance: the wizard's no-template path produces a draft
        // with empty `mechanical_params` and no legacy regime/intern
        // slots — a clean starting point the `set_*` tools can fill.
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                template: "custom".into(),
                name: "Blank Run".into(),
                creator: Some("@test".into()),
            },
        )
        .await
        .expect("custom template create must succeed");

        let draft = get_strategy(&store, &out.id).await.expect("draft must load");
        assert!(
            draft.mechanical_params.as_object().is_some_and(|m| m.is_empty()),
            "blank draft must have empty mechanical_params, got: {:?}",
            draft.mechanical_params
        );
        assert!(
            draft.regime_slot.is_none(),
            "blank draft should not carry a regime slot"
        );
        assert!(
            draft.intern_slot.is_none(),
            "blank draft should not carry an intern slot"
        );
    }

    #[test]
    fn create_strategy_request_rejects_unknown_fields() {
        let err = serde_json::from_str::<CreateStrategyReq>(
            r#"{"template":"trend_follower","name":"x","creator":null,"surprise":true}"#,
        )
        .expect_err("unknown create-strategy fields must be rejected");

        assert!(err.to_string().contains("unknown field"));
    }

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "btc-mom-1".into(),
                creator: Some("@test".into()),
            },
        )
        .await
        .unwrap();
        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(strategy.manifest.id, out.id);
        assert_eq!(strategy.manifest.template, "trend_follower");
    }

    #[tokio::test]
    async fn update_slot_reports_updated_fields() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                template: "trend_follower".into(),
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
                prompt: Some("New prompt".into()),
                model_requirement: None,
                provider: None,
                model: None,
                allowed_tools: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(upd.updated, vec!["prompt".to_string()]);

        let strategy = get_strategy(&store, &out.id).await.unwrap();
        assert_eq!(strategy.trader_slot.unwrap().prompt, "New prompt");
    }

    #[tokio::test]
    async fn update_manifest_round_trips_inspector_fields() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                template: "mean_reversion".into(),
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
                template: "trend_follower".into(),
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
            },
        )
        .await
        .expect_err("canonical duplicate should be rejected");
        assert!(err.to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn rename_and_remove_agent_role_match_canonical_role_keys() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                template: "trend_follower".into(),
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
            },
            AgentRef {
                agent_id: "01HZTRADER".into(),
                role: "Trader".into(),
            },
        ];
        strategy.pipeline = PipelineDef {
            kind: PipelineKind::Graph,
            edges: vec![PipelineEdge {
                from_role: "SCOUT".into(),
                to_role: "TRADER".into(),
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
                template: "trend_follower".into(),
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
    async fn validate_draft_reports_missing_agent_for_fresh_template() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let v = validate_draft(&store, &out.id).await.unwrap();
        assert!(!v.ok);
        assert!(
            v.errors.iter().any(|e| e.contains("attached agent")),
            "expected missing attached agent error, got {:?}",
            v.errors,
        );
    }

    #[tokio::test]
    async fn validate_draft_reports_prompt_manifest_asset_and_cadence_drift() {
        let (store, _td) = store_in_tmp();
        let out = create_strategy(
            &store,
            CreateStrategyReq {
                template: "mean_reversion".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let mut strategy = get_strategy(&store, &out.id).await.unwrap();
        strategy.trader_slot.as_mut().unwrap().prompt =
            "Trade BTC/USD on 6-hour candles. Return JSON.".into();
        store.save(&strategy).await.unwrap();

        let v = validate_draft(&store, &out.id).await.unwrap();

        assert!(!v.ok);
        assert!(
            v.errors.iter().any(|e| e.contains("BTC/USD")),
            "expected asset drift error, got {:?}",
            v.errors,
        );
        assert!(
            v.errors.iter().any(|e| e.contains("6h")),
            "expected cadence drift error, got {:?}",
            v.errors,
        );
    }
}
