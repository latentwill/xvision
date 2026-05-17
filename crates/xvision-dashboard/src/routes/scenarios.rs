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
    Query(q): Query<chart_api::ScenarioChartQuery>,
) -> Result<Json<ScenarioChartPayload>, DashboardError> {
    let payload = chart_api::build_scenario_payload_with_granularity(
        &state.api_context(),
        &id,
        q.granularity.as_deref(),
    )
    .await?;
    Ok(Json(payload))
}

/// `GET /api/scenarios/preview` — transient chart payload for the
/// new-scenario wizard. No DB row is created. Query params:
/// asset, from (YYYY-MM-DD), to, granularity, baseline (bool, optional).
pub async fn preview(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<chart_api::PreviewQuery>,
) -> Result<Json<chart_api::ScenarioPreviewPayload>, DashboardError> {
    let payload = chart_api::build_scenario_preview(&state.api_context(), q).await?;
    Ok(Json(payload))
}

#[cfg(test)]
pub mod get {
    //! Shape: `cargo test -p xvision-dashboard scenarios::get` (q15-
    //! object-json-output contract verification).

    use xvision_engine::api::scenario as api_scenario;
    use xvision_engine::api::{Actor, ApiContext};
    use xvision_engine::eval::export as eval_export;
    use xvision_engine::eval::run::{Run, RunMode, RunStatus};
    use xvision_engine::eval::store::RunStore;

    #[tokio::test]
    async fn route_shape_matches_eval_export_scenario_slot() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ApiContext::open(
            dir.path(),
            Actor::Cli {
                user: "object-json-test".into(),
            },
        )
        .await
        .expect("open ApiContext");

        let scenario_id = "crypto-bull-q1-2025";

        let store = RunStore::new(ctx.db.clone());
        let mut run = Run::new_queued(
            "agent-fixture".into(),
            scenario_id.into(),
            RunMode::Backtest,
        );
        run.status = RunStatus::Completed;
        store.create(&run).await.expect("seed run");
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .expect("transition");

        let direct = api_scenario::get(&ctx, scenario_id)
            .await
            .expect("scenario get");
        let export = eval_export::build_export(&ctx, &run.id)
            .await
            .expect("build_export");

        let route_json = serde_json::to_value(&direct).expect("scenario->json");
        let from_export = export
            .scenario
            .as_ref()
            .map(serde_json::to_value)
            .expect("export.scenario present")
            .expect("export.scenario->json");
        assert_eq!(
            route_json, from_export,
            "GET /api/scenarios/:id shape must equal `EvalRunExport.scenario`",
        );
    }
}
