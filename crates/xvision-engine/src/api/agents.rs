//! `/api/agents` — agent records CRUD. Wraps `engine::agents::store` with
//! audit-emitting handlers.
//!
//! v1 surface; see `docs/superpowers/plans/2026-05-11-agents-page-v1.md`.
//! Cross-references (`deployed_in`, `recent_runs`) return empty until the
//! strategies + eval refactors land — keeping the hooks here lets the
//! dashboard call them without knowing they're empty by design.

use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::agents::{
    builtin_templates, validate_agent, Agent, AgentSlot, AgentStore, AgentTemplate, ListFilter,
    NewAgent, UpdateAgent, ValidationDiagnostic,
};
use crate::api::audit::{self, Outcome};
use crate::api::{ApiContext, ApiError, ApiResult};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ListAgentsRequest {
    pub include_archived: bool,
    pub q: Option<String>,
    pub limit: Option<i64>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub slots: Vec<AgentSlot>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub slots: Option<Vec<AgentSlot>>,
}

/// A strategy that references an agent. Empty in v1 — see plan §Downstream impact.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRef {
    pub strategy_id: String,
    pub name: String,
}

/// An eval-run that included a given agent. Empty in v1 — strategies don't
/// reference agents yet, so we can't attribute runs to agents.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRef {
    pub run_id: String,
    pub scenario_id: String,
    pub status: String,
}

pub async fn list(ctx: &ApiContext, req: ListAgentsRequest) -> ApiResult<Vec<Agent>> {
    let started = Instant::now();
    let result = list_inner(ctx, req).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "list",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn list_inner(ctx: &ApiContext, req: ListAgentsRequest) -> ApiResult<Vec<Agent>> {
    let store = AgentStore::new(ctx.db.clone());
    store
        .list(ListFilter {
            include_archived: req.include_archived,
            name_contains: req.q,
            limit: req.limit,
        })
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))
}

pub async fn create(ctx: &ApiContext, req: CreateAgentRequest) -> ApiResult<Agent> {
    let started = Instant::now();
    let target_name = req.name.clone();
    let result = create_inner(ctx, req).await;
    let target = result.as_ref().ok().map(|a| a.agent_id.clone());
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "create",
        target.as_deref(),
        Some(&target_name),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn create_inner(ctx: &ApiContext, req: CreateAgentRequest) -> ApiResult<Agent> {
    let store = AgentStore::new(ctx.db.clone());

    if req.name.trim().is_empty() {
        return Err(ApiError::Validation("name is required".into()));
    }
    if req.slots.is_empty() {
        return Err(ApiError::Validation(
            "agent needs at least one slot".into(),
        ));
    }
    if store
        .name_exists(&req.name, None)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        return Err(ApiError::Conflict(format!(
            "an agent named '{}' already exists",
            req.name
        )));
    }

    let id = store
        .create(NewAgent {
            name: req.name,
            description: req.description,
            tags: req.tags,
            slots: req.slots,
        })
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    store
        .get(&id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::Internal("agent vanished after create".into()))
}

pub async fn get(ctx: &ApiContext, agent_id: &str) -> ApiResult<Agent> {
    let started = Instant::now();
    let result = get_inner(ctx, agent_id).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "get",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn get_inner(ctx: &ApiContext, agent_id: &str) -> ApiResult<Agent> {
    let store = AgentStore::new(ctx.db.clone());
    store
        .get(agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("agent {}", agent_id)))
}

pub async fn update(
    ctx: &ApiContext,
    agent_id: &str,
    req: UpdateAgentRequest,
) -> ApiResult<Agent> {
    let started = Instant::now();
    let result = update_inner(ctx, agent_id, req).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "update",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn update_inner(
    ctx: &ApiContext,
    agent_id: &str,
    req: UpdateAgentRequest,
) -> ApiResult<Agent> {
    let store = AgentStore::new(ctx.db.clone());

    if let Some(ref name) = req.name {
        if name.trim().is_empty() {
            return Err(ApiError::Validation("name must be non-empty".into()));
        }
        if store
            .name_exists(name, Some(agent_id))
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
        {
            return Err(ApiError::Conflict(format!(
                "an agent named '{}' already exists",
                name
            )));
        }
    }

    let updated = store
        .update(
            agent_id,
            UpdateAgent {
                name: req.name,
                description: req.description,
                tags: req.tags,
                slots: req.slots,
            },
        )
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    updated.ok_or_else(|| ApiError::NotFound(format!("agent {}", agent_id)))
}

pub async fn archive(ctx: &ApiContext, agent_id: &str) -> ApiResult<()> {
    let started = Instant::now();
    let result = archive_inner(ctx, agent_id).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "archive",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn archive_inner(ctx: &ApiContext, agent_id: &str) -> ApiResult<()> {
    let store = AgentStore::new(ctx.db.clone());
    let archived = store
        .archive(agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if !archived {
        return Err(ApiError::NotFound(format!(
            "agent {} (or already archived)",
            agent_id
        )));
    }
    Ok(())
}

pub async fn validate(
    ctx: &ApiContext,
    agent_id: &str,
) -> ApiResult<Vec<ValidationDiagnostic>> {
    let started = Instant::now();
    let result = validate_inner(ctx, agent_id).await;
    let outcome = outcome_of(&result);
    let _ = audit::record(
        ctx,
        "agents",
        "validate",
        Some(agent_id),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn validate_inner(
    ctx: &ApiContext,
    agent_id: &str,
) -> ApiResult<Vec<ValidationDiagnostic>> {
    let store = AgentStore::new(ctx.db.clone());
    let agent = store
        .get(agent_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("agent {}", agent_id)))?;
    Ok(validate_agent(&agent))
}

/// V1 stub — returns empty until the strategies refactor lands and
/// strategies start referencing agents.
pub async fn deployed_in(_ctx: &ApiContext, _agent_id: &str) -> ApiResult<Vec<StrategyRef>> {
    Ok(Vec::new())
}

/// Starter templates for the `/agents/new` picker. Hardcoded for v1;
/// user-authored templates land later when the strategies refactor
/// makes promote-from-strategy a real flow.
pub async fn templates(_ctx: &ApiContext) -> ApiResult<Vec<AgentTemplate>> {
    Ok(builtin_templates())
}

/// V1 stub — returns empty until eval-runs are attributed to agents.
pub async fn recent_runs(
    _ctx: &ApiContext,
    _agent_id: &str,
    _limit: u32,
) -> ApiResult<Vec<RunRef>> {
    Ok(Vec::new())
}

fn outcome_of<T>(result: &ApiResult<T>) -> Outcome {
    match result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    }
}
