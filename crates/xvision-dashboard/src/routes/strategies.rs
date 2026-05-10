//! `/api/strategies` + `/api/strategy/:id*` — thin wrappers around
//! `engine::api::strategy::*`. The Inspector page (separate frontend
//! follow-up) consumes these.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use xvision_engine::api::strategy::{
    self, set_risk_config, update_slot, validate_draft, StrategySummary,
};
use xvision_engine::authoring::{
    SetRiskConfigOut, SetRiskConfigReq, UpdateSlotOut, UpdateSlotReq, ValidateDraftOut,
};
use xvision_engine::bundle::risk::RiskConfig;
use xvision_engine::bundle::StrategyBundle;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct StrategiesListResponse {
    pub items: Vec<StrategySummary>,
}

pub async fn list(
    State(state): State<AppState>,
) -> Result<Json<StrategiesListResponse>, DashboardError> {
    let items = strategy::list(&state.api_context()).await?;
    Ok(Json(StrategiesListResponse { items }))
}

/// Inspector render path — full bundle for `/authoring/<id>`.
pub async fn get(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<StrategyBundle>, DashboardError> {
    let bundle = strategy::get(&state.api_context(), &id).await?;
    Ok(Json(bundle))
}

#[derive(Deserialize)]
pub struct UpdateSlotBody {
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub model_requirement: Option<String>,
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

/// `PUT /api/strategy/:id/risk` — set the bundle's risk config via preset
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
