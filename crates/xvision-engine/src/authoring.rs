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
    AgentRef,
    PipelineDef,
    PipelineKind,
    risk::{RiskConfig, RiskPreset},
    slot::LLMSlot,
    store::StrategyStore,
    validate::validate_strategy,
    Strategy,
};
use crate::templates::registry as template_registry;

// ---------------------------------------------------------------------------
// types — request / response shapes shared by both surfaces.
// MCP wraps these with JsonSchema derives in its own request structs; the
// dashboard speaks them directly.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateInfo {
    pub name: String,
    pub display_name: String,
    pub plain_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct AddAgentRefRequest {
    pub strategy_id: String,
    pub agent_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveAgentRefRequest {
    pub strategy_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameAgentRoleRequest {
    pub strategy_id: String,
    pub role: String,
    pub new_role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetPipelineRequest {
    pub strategy_id: String,
    pub pipeline: PipelineDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetMechanicalParamReq {
    pub id: String,
    pub key: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    let tpl = template_registry::get(&req.template).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown template '{}' — try list_templates",
            req.template
        )
    })?;
    let id = Ulid::new().to_string();
    let creator = req.creator.unwrap_or_else(|| "@anonymous".to_string());
    let draft = tpl.new_draft(id.clone(), req.name, creator);
    store.save(&draft).await?;
    Ok(CreateStrategyOut { id })
}

pub async fn get_strategy(
    store: &dyn StrategyStore,
    id: &str,
) -> anyhow::Result<Strategy> {
    store.load(id).await
}

pub async fn update_slot(
    store: &dyn StrategyStore,
    req: UpdateSlotReq,
) -> anyhow::Result<UpdateSlotOut> {
    let mut strategy = store.load(&req.id).await?;
    let slot_field = match req.slot.as_str() {
        "regime" => &mut strategy.regime_slot,
        "intern" => &mut strategy.intern_slot,
        "trader" => &mut strategy.trader_slot,
        other => anyhow::bail!(
            "unknown slot `{other}` — must be one of: regime, intern, trader"
        ),
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
    Ok(UpdateSlotOut {
        id: req.id,
        updated,
    })
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
        anyhow::bail!(
            "no manifest fields to update — supply asset_universe and/or decision_cadence_minutes"
        );
    }

    store.save(&strategy).await?;
    Ok(UpdateManifestOut {
        id: req.id,
        updated,
    })
}

pub async fn add_agent_ref(
    store: &dyn StrategyStore,
    req: AddAgentRefRequest,
) -> anyhow::Result<Strategy> {
    let mut strategy = store.load(&req.strategy_id).await?;
    let role = req.role.trim();
    if role.is_empty() {
        anyhow::bail!("role is required");
    }
    if strategy.agents.iter().any(|a| a.role == role) {
        anyhow::bail!("role '{role}' already exists on strategy");
    }
    strategy.agents.push(AgentRef {
        agent_id: req.agent_id,
        role: role.to_string(),
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
    let before = strategy.agents.len();
    strategy.agents.retain(|a| a.role != req.role);
    if strategy.agents.len() == before {
        anyhow::bail!("role '{}' not found on strategy", req.role);
    }
    if strategy.pipeline.kind == PipelineKind::Graph {
        strategy
            .pipeline
            .edges
            .retain(|edge| edge.from_role != req.role && edge.to_role != req.role);
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
    let new_role = req.new_role.trim();
    if new_role.is_empty() {
        anyhow::bail!("new role is required");
    }
    if strategy
        .agents
        .iter()
        .any(|a| a.role == new_role && a.role != req.role)
    {
        anyhow::bail!("role '{new_role}' already exists on strategy");
    }
    let mut found = false;
    for agent in &mut strategy.agents {
        if agent.role == req.role {
            agent.role = new_role.to_string();
            found = true;
            break;
        }
    }
    if !found {
        anyhow::bail!("role '{}' not found on strategy", req.role);
    }
    if strategy.pipeline.kind == PipelineKind::Graph {
        for edge in &mut strategy.pipeline.edges {
            if edge.from_role == req.role {
                edge.from_role = new_role.to_string();
            }
            if edge.to_role == req.role {
                edge.to_role = new_role.to_string();
            }
        }
        validate_graph_pipeline(&strategy)?;
    }
    store.save(&strategy).await?;
    Ok(strategy)
}

pub async fn set_pipeline(
    store: &dyn StrategyStore,
    req: SetPipelineRequest,
) -> anyhow::Result<Strategy> {
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
    let roles: HashSet<&str> = strategy.agents.iter().map(|agent| agent.role.as_str()).collect();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut seen_edges: HashSet<(&str, &str)> = HashSet::new();

    for edge in &strategy.pipeline.edges {
        let from = edge.from_role.as_str();
        let to = edge.to_role.as_str();

        if !roles.contains(from) || !roles.contains(to) {
            anyhow::bail!("graph edges must reference existing strategy roles");
        }
        if from == to {
            anyhow::bail!("graph pipelines cannot contain self-loops");
        }
        if !seen_edges.insert((from, to)) {
            anyhow::bail!("graph pipelines cannot contain duplicate edges");
        }
        adjacency.entry(from).or_default().push(to);
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for role in &roles {
        if graph_cycle_from(*role, &adjacency, &mut visiting, &mut visited) {
            anyhow::bail!("graph pipelines must be acyclic");
        }
    }

    Ok(())
}

fn graph_cycle_from<'a>(
    role: &'a str,
    adjacency: &HashMap<&'a str, Vec<&'a str>>,
    visiting: &mut HashSet<&'a str>,
    visited: &mut HashSet<&'a str>,
) -> bool {
    if visited.contains(role) {
        return false;
    }
    if !visiting.insert(role) {
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
    visited.insert(role);
    false
}

pub async fn set_mechanical_param(
    store: &dyn StrategyStore,
    req: SetMechanicalParamReq,
) -> anyhow::Result<()> {
    let mut strategy = store.load(&req.id).await?;
    let map = strategy.mechanical_params.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!(
            "mechanical_params is not a JSON object — template invariant violation"
        )
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

pub async fn validate_draft(
    store: &dyn StrategyStore,
    id: &str,
) -> anyhow::Result<ValidateDraftOut> {
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
