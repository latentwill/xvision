//! `/api/agents` — thin wrappers around `engine::api::agents::*`.
//!
//! The Agents page (frontend) consumes these. See
//! `docs/superpowers/plans/2026-05-11-agents-page-v1.md`.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use xvision_engine::agents::{Agent, AgentTemplate, ValidationDiagnostic};
use xvision_engine::api::agents::{
    self, CreateAgentRequest, ListAgentsRequest, RunRef, StrategyRef, UpdateAgentRequest,
};

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct AgentsListResponse {
    pub items: Vec<Agent>,
}

#[derive(Serialize)]
pub struct ValidateResponse {
    pub diagnostics: Vec<ValidationDiagnostic>,
}

#[derive(Serialize)]
pub struct StrategyRefsResponse {
    pub items: Vec<StrategyRef>,
}

#[derive(Serialize)]
pub struct RunRefsResponse {
    pub items: Vec<RunRef>,
}

#[derive(Serialize)]
pub struct TemplatesResponse {
    pub items: Vec<AgentTemplate>,
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    #[serde(default)]
    pub include_archived: bool,
    pub q: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Deserialize, Default)]
pub struct RunsQuery {
    pub limit: Option<u32>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<AgentsListResponse>, DashboardError> {
    let items = agents::list(
        &state.api_context(),
        ListAgentsRequest {
            include_archived: q.include_archived,
            q: q.q,
            limit: q.limit,
        },
    )
    .await?;
    Ok(Json(AgentsListResponse { items }))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateAgentRequest>,
) -> Result<Json<Agent>, DashboardError> {
    let agent = agents::create(&state.api_context(), body).await?;
    Ok(Json(agent))
}

pub async fn get(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Agent>, DashboardError> {
    let agent = agents::get(&state.api_context(), &id).await?;
    Ok(Json(agent))
}

pub async fn update(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<UpdateAgentRequest>,
) -> Result<Json<Agent>, DashboardError> {
    let agent = agents::update(&state.api_context(), &id, body).await?;
    Ok(Json(agent))
}

pub async fn archive(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    agents::archive(&state.api_context(), &id).await?;
    Ok(Json(serde_json::json!({ "archived": true })))
}

pub async fn validate(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ValidateResponse>, DashboardError> {
    let diagnostics = agents::validate(&state.api_context(), &id).await?;
    Ok(Json(ValidateResponse { diagnostics }))
}

pub async fn deployed_in(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<StrategyRefsResponse>, DashboardError> {
    let items = agents::deployed_in(&state.api_context(), &id).await?;
    Ok(Json(StrategyRefsResponse { items }))
}

pub async fn recent_runs(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(q): Query<RunsQuery>,
) -> Result<Json<RunRefsResponse>, DashboardError> {
    let limit = q.limit.unwrap_or(5);
    let items = agents::recent_runs(&state.api_context(), &id, limit).await?;
    Ok(Json(RunRefsResponse { items }))
}

pub async fn templates(State(state): State<AppState>) -> Result<Json<TemplatesResponse>, DashboardError> {
    let items = agents::templates(&state.api_context()).await?;
    Ok(Json(TemplatesResponse { items }))
}
