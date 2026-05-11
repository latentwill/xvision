//! `GET /api/scenarios` — list scenarios (filterable).
//! `GET /api/scenarios/:id` — fetch one scenario.
//! `GET /api/scenarios/:id/chart` — scenario chart payload (bars + cache status).
//! `POST /api/scenarios` — create a new scenario.
//! `POST /api/scenarios/:id/clone` — derive a new scenario from an existing one.
//! `POST /api/scenarios/:id/archive` — soft-delete (sets archived_at).
//! `DELETE /api/scenarios/:id` — hard-delete (rejected if eval_runs reference it).
//!
//! All handlers are thin wrappers over `engine::api::scenario::*`. Errors
//! surface via `DashboardError: From<ApiError>` with the correct HTTP status
//! (404, 400, 409, 500).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use xvision_engine::api::chart::{self as chart_api, ScenarioChartPayload};
use xvision_engine::api::scenario::{
    self as api_scenario, CreateScenarioRequest, ListScenariosFilter, ScenarioMutations,
};
use xvision_engine::eval::scenario::{Scenario, ScenarioSource};

use crate::error::DashboardError;
use crate::state::AppState;

/// Query params for `GET /api/scenarios`. Mirrors `ListScenariosFilter` but
/// uses a flat, query-string-friendly shape. `tags` is repeated: `?tags=a&tags=b`.
/// `#[serde(default)]` ensures missing fields use their defaults rather than 400ing.
#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub source: Option<ScenarioSource>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub include_archived: bool,
    pub parent_scenario_id: Option<String>,
}

#[derive(Serialize)]
pub struct ScenariosListResponse {
    pub items: Vec<Scenario>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<ScenariosListResponse>, DashboardError> {
    let filter = ListScenariosFilter {
        source: params.source,
        tags: params.tags,
        include_archived: params.include_archived,
        parent_scenario_id: params.parent_scenario_id,
    };
    let items = api_scenario::list(&state.api_context(), filter).await?;
    Ok(Json(ScenariosListResponse { items }))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Scenario>, DashboardError> {
    let scenario = api_scenario::get(&state.api_context(), &id).await?;
    Ok(Json(scenario))
}

pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateScenarioRequest>,
) -> Result<(StatusCode, Json<Scenario>), DashboardError> {
    let scenario = api_scenario::create(&state.api_context(), req).await?;
    Ok((StatusCode::CREATED, Json(scenario)))
}

/// Clone an existing scenario, optionally applying mutations. An empty body
/// (or no body) means "inherit all fields from parent".
pub async fn clone(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<ScenarioMutations>>,
) -> Result<(StatusCode, Json<Scenario>), DashboardError> {
    let mutations = body.map(|Json(m)| m).unwrap_or_default();
    let scenario = api_scenario::clone(&state.api_context(), &id, mutations).await?;
    Ok((StatusCode::CREATED, Json(scenario)))
}

pub async fn archive(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, DashboardError> {
    api_scenario::archive(&state.api_context(), &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, DashboardError> {
    api_scenario::delete(&state.api_context(), &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/scenarios/:id/chart` — scenario chart payload.
///
/// Returns the OHLCV bars for the scenario (if cached) together with
/// the `CacheStatus` (FullyCached / PartiallyCached / NotCached).
/// Returns 404 when the scenario id is not found.
pub async fn chart(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ScenarioChartPayload>, DashboardError> {
    let payload = chart_api::build_scenario_payload(&state.api_context(), &id).await?;
    Ok(Json(payload))
}
