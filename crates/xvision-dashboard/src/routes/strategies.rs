//! `/api/strategies` + `/api/strategy/:id*` — thin wrappers around
//! `engine::api::strategy::*`. The Inspector page (separate frontend
//! follow-up) consumes these.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use xvision_engine::api::chart::{self as chart_api, StrategyChartPayload};
use xvision_engine::api::strategy::{
    self, add_agent, remove_agent, rename_agent_role, set_pipeline, set_risk_config, update_slot,
    validate_draft, AddAgentReq, RemoveAgentReq, RenameAgentRoleReq, SetPipelineReq, StrategyAgentsOut,
    StrategySummary,
};
use xvision_engine::authoring::{
    self, CreateStrategyOut, CreateStrategyReq, SetRiskConfigOut, SetRiskConfigReq, TemplateInfo,
    UpdateSlotOut, UpdateSlotReq, ValidateDraftOut,
};
use xvision_engine::strategies::risk::RiskConfig;
use xvision_engine::strategies::Strategy;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct StrategiesListResponse {
    pub items: Vec<StrategySummary>,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<StrategiesListResponse>, DashboardError> {
    let items = strategy::list(&state.api_context()).await?;
    Ok(Json(StrategiesListResponse { items }))
}

#[derive(Serialize)]
pub struct TemplatesListResponse {
    pub items: Vec<TemplateInfo>,
}

/// `GET /api/templates` — list the built-in strategy templates the
/// template picker shows. The list is a static registry (no DB or env
/// dependency) so no audit log is needed.
pub async fn list_templates() -> Json<TemplatesListResponse> {
    Json(TemplatesListResponse {
        items: authoring::list_templates(),
    })
}

/// `POST /api/strategies` — create a new draft strategy from a template.
/// Body: `{ template, name, creator? }`. Returns `{ id }` (the new
/// agent_id); the frontend redirects to `/authoring/:id`.
pub async fn post_create(
    State(state): State<AppState>,
    Json(body): Json<CreateStrategyReq>,
) -> Result<(StatusCode, Json<CreateStrategyOut>), DashboardError> {
    let out = strategy::create_strategy(&state.api_context(), body).await?;
    Ok((StatusCode::CREATED, Json(out)))
}

/// Inspector render path — full strategy for `/authoring/<id>`.
pub async fn get(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Strategy>, DashboardError> {
    let strategy = strategy::get(&state.api_context(), &id).await?;
    Ok(Json(strategy))
}

/// `DELETE /api/strategy/:id` — delete a draft strategy entity.
pub async fn delete(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, DashboardError> {
    strategy::delete(&state.api_context(), &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct UpdateSlotBody {
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub model_requirement: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
}

/// `PUT /api/strategy/:id/slot/:role` — update one or more fields on an
/// LLM slot. Body carries the partial fields the Inspector edited.
pub async fn put_slot(
    Path((id, role)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(body): Json<UpdateSlotBody>,
) -> Result<Json<UpdateSlotOut>, DashboardError> {
    let req = UpdateSlotReq {
        id,
        slot: role,
        prompt: body.prompt,
        model_requirement: body.model_requirement,
        provider: body.provider,
        model: body.model,
        allowed_tools: body.allowed_tools,
    };
    let out = update_slot(&state.api_context(), req).await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct PutRiskBody {
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub explicit: Option<RiskConfig>,
}

/// `PUT /api/strategy/:id/risk` — set the strategy's risk config via preset
/// (Conservative / Balanced / Aggressive) or explicit blob, but not both.
pub async fn put_risk(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<PutRiskBody>,
) -> Result<Json<SetRiskConfigOut>, DashboardError> {
    let req = SetRiskConfigReq {
        id,
        preset: body.preset,
        explicit: body.explicit,
    };
    let out = set_risk_config(&state.api_context(), req).await?;
    Ok(Json(out))
}

/// `POST /api/strategy/:id/validate` — re-validate the draft. The
/// validation result type carries `ok` + `errors`; this returns it
/// verbatim (validation failures round-trip as 200 OK with `ok: false`,
/// not as a 4xx).
pub async fn post_validate(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ValidateDraftOut>, DashboardError> {
    let out = validate_draft(&state.api_context(), &id).await?;
    Ok(Json(out))
}

/// `GET /api/strategies/:id/chart` — strategy chart payload.
///
/// Lists all runs for the strategy, computes per-run normalised
/// equity curves and headline metrics (final PnL, max drawdown, Sharpe),
/// and returns the grouped result. An unknown or unused strategy id
/// returns 200 with an empty `run_series` (not 404 — the strategy may exist
/// even if no runs reference it yet).
pub async fn chart(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<StrategyChartPayload>, DashboardError> {
    let payload = chart_api::build_strategy_payload(&state.api_context(), &id).await?;
    Ok(Json(payload))
}

#[derive(Deserialize)]
pub struct AddAgentBody {
    pub agent_id: String,
    pub role: String,
}

pub async fn post_add_agent(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<AddAgentBody>,
) -> Result<Json<StrategyAgentsOut>, DashboardError> {
    let out = add_agent(
        &state.api_context(),
        AddAgentReq {
            strategy_id: id,
            agent_id: body.agent_id,
            role: body.role,
        },
    )
    .await?;
    Ok(Json(out))
}

pub async fn delete_agent(
    Path((id, role)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<StrategyAgentsOut>, DashboardError> {
    let out = remove_agent(
        &state.api_context(),
        RemoveAgentReq {
            strategy_id: id,
            role,
        },
    )
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct RenameAgentRoleBody {
    pub new_role: String,
}

pub async fn patch_agent_role(
    Path((id, role)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(body): Json<RenameAgentRoleBody>,
) -> Result<Json<StrategyAgentsOut>, DashboardError> {
    let out = rename_agent_role(
        &state.api_context(),
        RenameAgentRoleReq {
            strategy_id: id,
            role,
            new_role: body.new_role,
        },
    )
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct SetPipelineBody {
    pub kind: xvision_engine::strategies::PipelineKind,
    #[serde(default)]
    pub edges: Vec<xvision_engine::strategies::PipelineEdge>,
}

pub async fn put_pipeline(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<SetPipelineBody>,
) -> Result<Json<StrategyAgentsOut>, DashboardError> {
    let out = set_pipeline(
        &state.api_context(),
        SetPipelineReq {
            strategy_id: id,
            kind: body.kind,
            edges: body.edges,
        },
    )
    .await?;
    Ok(Json(out))
}
