//! Strategy operations. Backed by the existing filesystem strategy
//! store from Plan #1. Every function
//! records to `api_audit` via `audit::record` on completion.
//!
//! Read-only ops (`list`, `get`) ship today; the mutation surface
//! (`create_strategy`, `update_slot`, `set_risk_config`, `validate_draft`)
//! lands here as audit-emitting wrappers around the `engine::authoring::*`
//! dispatcher. The dashboard's Inspector route calls these; the MCP tool
//! layer (PR #31) goes through `engine::authoring::*` directly.

use crate::api::{
    audit::{self, Outcome},
    search as api_search, ApiContext, ApiError, ApiResult,
};
use crate::agents::AgentStore;
use crate::authoring::{
    self, AddAgentRefRequest, CreateStrategyOut, CreateStrategyReq, RemoveAgentRefRequest,
    RenameAgentRoleRequest, SetPipelineRequest, SetRiskConfigOut, SetRiskConfigReq, UpdateSlotOut,
    UpdateManifestOut, UpdateManifestReq, UpdateSlotReq, ValidateDraftOut,
};
use crate::strategies::{
    AgentRef, PipelineDef, PipelineEdge, PipelineKind,
    store::{strategy_store_dir, StrategyStore, FilesystemStore},
    Strategy,
};
use std::time::Instant;

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategySummary {
    pub agent_id: String,
    pub display_name: String,
    pub template: String,
    pub decision_cadence_minutes: u32,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Model summary for attached AgentRefs. Falls back to legacy slot config
    /// and shows the first unique model plus a count when multiple are present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Unique provider names required by this strategy's executable slots.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<String>,
    /// Unique model ids required by this strategy's executable slots.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddAgentReq {
    pub strategy_id: String,
    pub agent_id: String,
    pub role: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoveAgentReq {
    pub strategy_id: String,
    pub role: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetPipelineReq {
    pub strategy_id: String,
    pub kind: PipelineKind,
    #[serde(default)]
    pub edges: Vec<PipelineEdge>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenameAgentRoleReq {
    pub strategy_id: String,
    pub role: String,
    pub new_role: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategyAgentsOut {
    pub strategy_id: String,
    pub agents: Vec<AgentRef>,
    pub pipeline: PipelineDef,
}

pub async fn list(ctx: &ApiContext) -> ApiResult<Vec<StrategySummary>> {
    let started = Instant::now();
    let result = list_inner(ctx).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "list",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn list_inner(ctx: &ApiContext) -> ApiResult<Vec<StrategySummary>> {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let agent_store = AgentStore::new(ctx.db.clone());
    let ids = store
        .list()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let strategy = store
            .load(&id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let inventory = provider_model_inventory(ctx, &agent_store, &strategy).await?;
        let model = model_summary(&inventory.models);
        let tags = strategy_tags(&strategy);
        out.push(StrategySummary {
            agent_id: strategy.manifest.id.clone(),
            display_name: strategy.manifest.display_name.clone(),
            template: strategy.manifest.template.clone(),
            decision_cadence_minutes: strategy.manifest.decision_cadence_minutes,
            tags,
            model,
            providers: inventory.providers,
            models: inventory.models,
        });
    }
    Ok(out)
}

#[derive(Default)]
struct ProviderModelInventory {
    providers: Vec<String>,
    models: Vec<String>,
}

async fn provider_model_inventory(
    ctx: &ApiContext,
    agent_store: &AgentStore,
    strategy: &Strategy,
) -> ApiResult<ProviderModelInventory> {
    let mut inventory = ProviderModelInventory::default();

    for agent_ref in &strategy.agents {
        let agent = agent_store
            .get(&agent_ref.agent_id)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        let Some(agent) = agent else {
            tracing::warn!(
                agent_id = %agent_ref.agent_id,
                strategy_id = %strategy.manifest.id,
                actor = ?&ctx.actor,
                "strategy summary skipped missing AgentRef"
            );
            continue;
        };
        for slot in agent.slots {
            push_unique_trimmed(&mut inventory.providers, slot.provider);
            push_unique_trimmed(&mut inventory.models, slot.model);
        }
    }

    if inventory.models.is_empty() && inventory.providers.is_empty() {
        // Legacy slot fallback for older strategy JSON. Trader is the
        // decision-maker, so it wins over advisory/scoring slots.
        if let Some(slot) = strategy
            .trader_slot
            .as_ref()
            .or(strategy.intern_slot.as_ref())
            .or(strategy.regime_slot.as_ref())
        {
            if let Some(provider) = slot.provider.as_ref() {
                push_unique_trimmed(&mut inventory.providers, provider.clone());
            }
            push_unique_trimmed(&mut inventory.models, slot.effective_model());
        }
    }

    Ok(inventory)
}

fn model_summary(models: &[String]) -> Option<String> {
    match models {
        [] => None,
        [one] => Some(one.clone()),
        [first, rest @ ..] => Some(format!("{first} +{}", rest.len())),
    }
}

fn push_unique_trimmed(items: &mut Vec<String>, value: String) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    if !items.iter().any(|m| m == value) {
        items.push(value.to_string());
    }
}

fn strategy_tags(strategy: &Strategy) -> Vec<String> {
    let mut tags = Vec::new();
    push_unique_tag(&mut tags, strategy.manifest.template.clone());
    for asset in &strategy.manifest.asset_universe {
        push_unique_tag(&mut tags, asset.clone());
    }
    for regime in &strategy.manifest.regime_fit {
        push_unique_tag(&mut tags, regime_tag(*regime));
    }
    for tool in &strategy.manifest.required_tools {
        push_unique_tag(&mut tags, tool.clone());
    }
    tags
}

fn push_unique_tag(tags: &mut Vec<String>, tag: String) {
    let tag = tag.trim();
    if tag.is_empty() {
        return;
    }
    if !tags.iter().any(|t| t == tag) {
        tags.push(tag.to_string());
    }
}

fn regime_tag(regime: crate::strategies::manifest::RegimeFit) -> String {
    match regime {
        crate::strategies::manifest::RegimeFit::TrendingBull => "trending_bull",
        crate::strategies::manifest::RegimeFit::TrendingBear => "trending_bear",
        crate::strategies::manifest::RegimeFit::RangeBound => "range_bound",
        crate::strategies::manifest::RegimeFit::Chop => "chop",
        crate::strategies::manifest::RegimeFit::HighVol => "high_vol",
        crate::strategies::manifest::RegimeFit::LowVol => "low_vol",
        crate::strategies::manifest::RegimeFit::EventDriven => "event_driven",
    }
    .to_string()
}

pub async fn get(ctx: &ApiContext, agent_id: &str) -> ApiResult<Strategy> {
    let started = Instant::now();
    let result = get_inner(ctx, agent_id).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "get",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

pub async fn delete(ctx: &ApiContext, agent_id: &str) -> ApiResult<()> {
    let started = Instant::now();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = delete_inner(&store, agent_id).await;

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "delete",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        api_search::delete_strategy(ctx, agent_id).await;
    }
    result
}

async fn get_inner(ctx: &ApiContext, agent_id: &str) -> ApiResult<Strategy> {
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    store.load(agent_id).await.map_err(|e| {
        if is_not_found(&e) {
            ApiError::NotFound(format!("strategy '{agent_id}'"))
        } else {
            ApiError::Internal(e.to_string())
        }
    })
}

async fn delete_inner(store: &FilesystemStore, agent_id: &str) -> ApiResult<()> {
    store.delete(agent_id).await.map_err(|e| {
        if is_not_found(&e) {
            ApiError::NotFound(format!("strategy '{agent_id}'"))
        } else {
            ApiError::Internal(e.to_string())
        }
    })
}

fn is_not_found(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
            if io_err.kind() == std::io::ErrorKind::NotFound {
                return true;
            }
        }
    }
    false
}

/// Map an `anyhow::Error` from `engine::authoring::*` dispatcher fns to a
/// typed `ApiError`. The dispatcher emits validation failures as
/// `anyhow!("...")` strings (no typed enum), so we string-match the prefix
/// for the cases we want to surface as `Validation`. Anything else falls
/// through to `Internal`.
fn map_authoring_error(err: anyhow::Error, agent_id: Option<&str>) -> ApiError {
    if is_not_found(&err) {
        return match agent_id {
            Some(id) => ApiError::NotFound(format!("strategy '{id}'")),
            None => ApiError::NotFound(err.to_string()),
        };
    }
    let msg = err.to_string();
    let validation_markers = [
        "unknown slot",
        "no fields to update",
        "no manifest fields to update",
        "asset_universe",
        "decision_cadence_minutes",
        "unknown preset",
        "preset and explicit are mutually exclusive",
        "supply either preset or explicit",
        "unknown template",
        "mechanical_params is not a JSON object",
        "role is required",
        "already exists on strategy",
        "not found on strategy",
        "pipeline edges are only valid for graph pipelines",
        "single pipelines cannot include more than one agent",
        "graph edges must reference existing strategy roles",
        "graph pipelines cannot contain self-loops",
        "graph pipelines cannot contain duplicate edges",
        "graph pipelines must be acyclic",
    ];
    if validation_markers.iter().any(|m| msg.contains(m)) {
        return ApiError::Validation(msg);
    }
    ApiError::Internal(msg)
}

fn strategy_agents_out(strategy: Strategy) -> StrategyAgentsOut {
    StrategyAgentsOut {
        strategy_id: strategy.manifest.id.clone(),
        agents: strategy.agents,
        pipeline: strategy.pipeline,
    }
}

/// Create a new draft strategy from a template. Wraps
/// `authoring::create_strategy` with an audit row keyed on the resulting
/// `agent_id` (or no target on failure, since the id only exists on
/// success).
pub async fn create_strategy(
    ctx: &ApiContext,
    req: CreateStrategyReq,
) -> ApiResult<CreateStrategyOut> {
    let started = Instant::now();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::create_strategy(&store, req)
        .await
        .map_err(|e| map_authoring_error(e, None));

    let (outcome, target) = match &result {
        Ok(o) => (Outcome::Ok, Some(o.id.clone())),
        Err(e) => (Outcome::Error(e.to_string()), None),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "create",
        target.as_deref(),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if let Some(id) = target.as_deref() {
        index_strategy_after_mutation(ctx, &store, id).await;
    }
    result
}

/// Update one or more fields on an LLM slot. Wraps `authoring::update_slot`.
pub async fn update_slot(ctx: &ApiContext, req: UpdateSlotReq) -> ApiResult<UpdateSlotOut> {
    let started = Instant::now();
    let agent_id = req.id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::update_slot(&store, req)
        .await
        .map_err(|e| map_authoring_error(e, Some(&agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "update_slot",
        Some(&agent_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &agent_id).await;
    }
    result
}

/// Update manifest fields shown by the Strategy Inspector.
pub async fn update_manifest(
    ctx: &ApiContext,
    req: UpdateManifestReq,
) -> ApiResult<UpdateManifestOut> {
    let started = Instant::now();
    let agent_id = req.id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::update_manifest(&store, req)
        .await
        .map_err(|e| map_authoring_error(e, Some(&agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "update_manifest",
        Some(&agent_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &agent_id).await;
    }
    result
}

pub async fn add_agent(ctx: &ApiContext, req: AddAgentReq) -> ApiResult<StrategyAgentsOut> {
    let started = Instant::now();
    let strategy_id = req.strategy_id.clone();
    let agent_id = req.agent_id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let agent_store = AgentStore::new(ctx.db.clone());
    let result = match agent_store
        .get(&agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        Some(_) => authoring::add_agent_ref(
            &store,
            AddAgentRefRequest {
                strategy_id: req.strategy_id,
                agent_id: req.agent_id,
                role: req.role,
            },
        )
        .await
        .map(strategy_agents_out)
        .map_err(|e| map_authoring_error(e, Some(&strategy_id))),
        None => Err(ApiError::NotFound(format!("agent {agent_id}"))),
    };

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "strategy_add_agent",
        Some(&strategy_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &strategy_id).await;
    }
    result
}

pub async fn remove_agent(
    ctx: &ApiContext,
    req: RemoveAgentReq,
) -> ApiResult<StrategyAgentsOut> {
    let started = Instant::now();
    let strategy_id = req.strategy_id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::remove_agent_ref(
        &store,
        RemoveAgentRefRequest {
            strategy_id: req.strategy_id,
            role: req.role,
        },
    )
    .await
    .map(strategy_agents_out)
    .map_err(|e| map_authoring_error(e, Some(&strategy_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "strategy_remove_agent",
        Some(&strategy_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &strategy_id).await;
    }
    result
}

pub async fn rename_agent_role(
    ctx: &ApiContext,
    req: RenameAgentRoleReq,
) -> ApiResult<StrategyAgentsOut> {
    let started = Instant::now();
    let strategy_id = req.strategy_id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::rename_agent_role(
        &store,
        RenameAgentRoleRequest {
            strategy_id: req.strategy_id,
            role: req.role,
            new_role: req.new_role,
        },
    )
    .await
    .map(strategy_agents_out)
    .map_err(|e| map_authoring_error(e, Some(&strategy_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "strategy_rename_agent_role",
        Some(&strategy_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &strategy_id).await;
    }
    result
}

pub async fn set_pipeline(ctx: &ApiContext, req: SetPipelineReq) -> ApiResult<StrategyAgentsOut> {
    let started = Instant::now();
    let strategy_id = req.strategy_id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::set_pipeline(
        &store,
        SetPipelineRequest {
            strategy_id: req.strategy_id,
            pipeline: PipelineDef {
                kind: req.kind,
                edges: req.edges,
            },
        },
    )
    .await
    .map(strategy_agents_out)
    .map_err(|e| map_authoring_error(e, Some(&strategy_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "strategy_set_pipeline",
        Some(&strategy_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &strategy_id).await;
    }
    result
}

/// Set one mechanical parameter on the strategy and refresh the search index.
pub async fn set_mechanical_param(
    ctx: &ApiContext,
    req: authoring::SetMechanicalParamReq,
) -> ApiResult<serde_json::Value> {
    let started = Instant::now();
    let agent_id = req.id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::set_mechanical_param(&store, req)
        .await
        .map(|_| serde_json::json!({ "ok": true }))
        .map_err(|e| map_authoring_error(e, Some(&agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "set_mechanical_param",
        Some(&agent_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &agent_id).await;
    }
    result
}

/// Update the strategy's risk config — preset (Conservative / Balanced /
/// Aggressive) or an explicit `RiskConfig` blob, but not both.
pub async fn set_risk_config(
    ctx: &ApiContext,
    req: SetRiskConfigReq,
) -> ApiResult<SetRiskConfigOut> {
    let started = Instant::now();
    let agent_id = req.id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::set_risk_config(&store, req)
        .await
        .map_err(|e| map_authoring_error(e, Some(&agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "set_risk_config",
        Some(&agent_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &agent_id).await;
    }
    result
}

/// Re-load the strategy after a successful mutation and refresh its row in
/// the search index. Best-effort: a failure here is logged inside
/// `api::search::upsert_strategy` and never bubbled up — the mutation has
/// already succeeded and the audit row is already written.
async fn index_strategy_after_mutation(
    ctx: &ApiContext,
    store: &FilesystemStore,
    agent_id: &str,
) {
    match store.load(agent_id).await {
        Ok(strategy) => api_search::upsert_strategy(ctx, &strategy).await,
        Err(e) => tracing::warn!(error = %e, agent_id, "post-mutation reload for indexer failed"),
    }
}

/// Run the strategy through the validator. The result type carries the
/// success/failure verdict + reasons; this wrapper only surfaces an
/// `ApiError` for hard load failures (NotFound / Internal). A validation
/// failure round-trips as `Ok(ValidateDraftOut { ok: false, errors })`.
pub async fn validate_draft(ctx: &ApiContext, agent_id: &str) -> ApiResult<ValidateDraftOut> {
    let started = Instant::now();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::validate_draft(&store, agent_id)
        .await
        .map_err(|e| map_authoring_error(e, Some(agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "validate",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Actor;
    use sqlx::SqlitePool;

    async fn ctx_with_audit() -> (ApiContext, tempfile::TempDir) {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(include_str!("../../migrations/001_api_audit.sql"))
            .execute(&pool)
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(strategy_store_dir(dir.path())).unwrap();
        let ctx = ApiContext::new(
            pool,
            Actor::Cli {
                user: "tester".into(),
            },
            dir.path().to_path_buf(),
        );
        (ctx, dir)
    }

    async fn audit_row_exists(ctx: &ApiContext, op: &str, target: &str) -> bool {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM api_audit WHERE operation = ?1 AND target = ?2",
        )
        .bind(op)
        .bind(target)
        .fetch_one(&ctx.db)
        .await
        .unwrap();
        n > 0
    }

    #[tokio::test]
    async fn create_strategy_round_trips_and_audits() {
        let (ctx, _d) = ctx_with_audit().await;
        let out = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "btc-mom".into(),
                creator: Some("@tester".into()),
            },
        )
        .await
        .unwrap();
        assert!(audit_row_exists(&ctx, "create", &out.id).await);
        let strategy = get(&ctx, &out.id).await.unwrap();
        assert_eq!(strategy.manifest.id, out.id);
    }

    #[tokio::test]
    async fn create_strategy_unknown_template_is_validation_error() {
        let (ctx, _d) = ctx_with_audit().await;
        let r = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "no-such-template".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await;
        assert!(
            matches!(r, Err(ApiError::Validation(_))),
            "expected Validation, got {r:?}",
        );
    }

    #[tokio::test]
    async fn update_slot_audits_and_returns_updated_fields() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let out = update_slot(
            &ctx,
            UpdateSlotReq {
                id: created.id.clone(),
                slot: "trader".into(),
                prompt: Some("New prompt.".into()),
                model_requirement: None,
                provider: None,
                model: None,
                allowed_tools: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(out.updated, vec!["prompt".to_string()]);
        assert!(audit_row_exists(&ctx, "update_slot", &created.id).await);
    }

    #[tokio::test]
    async fn update_slot_unknown_role_is_validation_error() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let r = update_slot(
            &ctx,
            UpdateSlotReq {
                id: created.id,
                slot: "no-such-role".into(),
                prompt: Some("p".into()),
                model_requirement: None,
                provider: None,
                model: None,
                allowed_tools: None,
            },
        )
        .await;
        assert!(matches!(r, Err(ApiError::Validation(_))));
    }

    #[tokio::test]
    async fn update_slot_missing_draft_is_not_found() {
        let (ctx, _d) = ctx_with_audit().await;
        let r = update_slot(
            &ctx,
            UpdateSlotReq {
                id: "01TOTALLYMISSINGAGENTID000".into(),
                slot: "trader".into(),
                prompt: Some("p".into()),
                model_requirement: None,
                provider: None,
                model: None,
                allowed_tools: None,
            },
        )
        .await;
        assert!(
            matches!(r, Err(ApiError::NotFound(_))),
            "expected NotFound, got {r:?}",
        );
    }

    #[tokio::test]
    async fn set_risk_config_preset_round_trips() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let out = set_risk_config(
            &ctx,
            SetRiskConfigReq {
                id: created.id.clone(),
                preset: Some("conservative".into()),
                explicit: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(out.applied, "preset");
        assert!(audit_row_exists(&ctx, "set_risk_config", &created.id).await);
    }

    #[tokio::test]
    async fn update_manifest_round_trips_and_audits() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "mean_reversion".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();

        let out = update_manifest(
            &ctx,
            UpdateManifestReq {
                id: created.id.clone(),
                asset_universe: Some(vec!["BTC/USD".into()]),
                decision_cadence_minutes: Some(360),
            },
        )
        .await
        .unwrap();

        assert_eq!(out.updated, vec!["asset_universe", "decision_cadence_minutes"]);
        assert!(audit_row_exists(&ctx, "update_manifest", &created.id).await);
        let strategy = get(&ctx, &created.id).await.unwrap();
        assert_eq!(strategy.manifest.asset_universe, vec!["BTC/USD"]);
        assert_eq!(strategy.manifest.decision_cadence_minutes, 360);
    }

    #[tokio::test]
    async fn set_risk_config_unknown_preset_is_validation_error() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let r = set_risk_config(
            &ctx,
            SetRiskConfigReq {
                id: created.id,
                preset: Some("totally-not-a-preset".into()),
                explicit: None,
            },
        )
        .await;
        assert!(matches!(r, Err(ApiError::Validation(_))));
    }

    #[tokio::test]
    async fn validate_draft_reports_missing_agent_for_template_default() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "trend_follower".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        let out = validate_draft(&ctx, &created.id).await.unwrap();
        assert_eq!(out.id, created.id);
        assert!(!out.ok);
        assert!(
            out.errors.iter().any(|e| e.contains("attached agent")),
            "expected missing attached agent error, got {:?}",
            out.errors,
        );
        assert!(audit_row_exists(&ctx, "validate", &created.id).await);
    }

    #[tokio::test]
    async fn validate_draft_reports_manifest_slot_prompt_drift() {
        let (ctx, _d) = ctx_with_audit().await;
        let created = create_strategy(
            &ctx,
            CreateStrategyReq {
                template: "mean_reversion".into(),
                name: "x".into(),
                creator: None,
            },
        )
        .await
        .unwrap();
        update_slot(
            &ctx,
            UpdateSlotReq {
                id: created.id.clone(),
                slot: "trader".into(),
                prompt: Some("Trade BTC/USD on a 6h candle schedule.".into()),
                model_requirement: None,
                provider: None,
                model: None,
                allowed_tools: None,
            },
        )
        .await
        .unwrap();

        let out = validate_draft(&ctx, &created.id).await.unwrap();

        assert!(!out.ok);
        assert!(
            out.errors.iter().any(|e| e.contains("BTC/USD")),
            "expected asset drift error, got {:?}",
            out.errors,
        );
        assert!(
            out.errors.iter().any(|e| e.contains("6h")),
            "expected cadence drift error, got {:?}",
            out.errors,
        );
    }

    #[tokio::test]
    async fn validate_draft_missing_is_not_found() {
        let (ctx, _d) = ctx_with_audit().await;
        let r = validate_draft(&ctx, "01TOTALLYMISSINGAGENTID000").await;
        assert!(matches!(r, Err(ApiError::NotFound(_))));
    }
}
