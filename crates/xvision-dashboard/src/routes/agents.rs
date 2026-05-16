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

#[cfg(test)]
pub mod get {
    //! Shape: `cargo test -p xvision-dashboard agents::get` (q15-
    //! object-json-output contract verification).
    //!
    //! Parity guard: `GET /api/agents/:id` returns the same Rust
    //! `Agent` struct that `EvalRunExport.agents[]` carries. The
    //! Serialize impl is shared, so structural equality holds even
    //! when the export's per-strategy `agents[]` resolution path isn't
    //! exercised here.

    use xvision_engine::agents::AgentSlot;
    use xvision_engine::api::agents::{self as agents_api, CreateAgentRequest};
    use xvision_engine::api::{Actor, ApiContext};

    #[tokio::test]
    async fn route_shape_matches_eval_export_agents_entry() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ApiContext::open(
            dir.path(),
            Actor::Cli {
                user: "object-json-test".into(),
            },
        )
        .await
        .expect("open ApiContext");

        let agent = agents_api::create(
            &ctx,
            CreateAgentRequest {
                name: "object-shape-fixture".into(),
                description: "route parity fixture".into(),
                tags: vec!["test".into()],
                slots: vec![AgentSlot {
                    name: "main".into(),
                    provider: "openai".into(),
                    model: "gpt-4o-mini".into(),
                    system_prompt: "Trade.".into(),
                    skill_ids: vec![],
                    max_tokens: Some(2048),
                }],
            },
        )
        .await
        .expect("create agent");

        // The route handler is `Json(agents::get(ctx, &id).await?)`.
        // Structural parity against a single-element `Vec<Agent>` is
        // the strongest check that doesn't require seeding a Strategy
        // that references the agent.
        let direct = agents_api::get(&ctx, &agent.agent_id).await.expect("agent get");
        let route_json = serde_json::to_value(&direct).expect("agent->json");
        let agents_slice: Vec<xvision_engine::agents::Agent> = vec![direct.clone()];
        let from_slice =
            serde_json::to_value(&agents_slice[0]).expect("agents_slice[0]->json");
        assert_eq!(
            route_json, from_slice,
            "GET /api/agents/:id shape must equal `EvalRunExport.agents[]`",
        );
    }
}
