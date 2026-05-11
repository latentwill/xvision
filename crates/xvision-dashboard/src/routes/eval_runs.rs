//! `GET /api/eval/runs[/:id]` — list + detail.
//! `GET /api/eval/compare?ids=a,b,c` — N-run comparison report.
//!
//! All handlers are thin wrappers over `engine::api::eval::*`. The detail
//! route maps `ApiError::NotFound` to `404 + JSON {code:"not_found"}` via
//! the existing `DashboardError: From<ApiError>` impl. The list handler
//! parses the query-string `?status=` into the typed `RunStatus` enum and
//! returns the slim `RunSummary` shape via `list_summaries`. Compare
//! parses `?ids=` (comma-separated) and surfaces api validation /
//! not-found errors transparently.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use xvision_engine::api::chart::{self as chart_api, CompareChartPayload, RunChartPayload};
use xvision_engine::api::eval::{
    self, CompareRunsRequest, EvalRunRequest, ListRunsRequest, RunDetail, RunSummary,
};
use xvision_engine::eval::compare::ComparisonReport;
use xvision_engine::eval::run::RunStatus;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub strategy_bundle_hash: Option<String>,
    pub scenario_id: Option<String>,
    /// Free-form status string ("queued", "running", …). Parsed into the
    /// typed `RunStatus` enum below; unknown values surface as a validation
    /// error.
    pub status: Option<String>,
}

#[derive(Serialize)]
pub struct RunsListResponse {
    pub items: Vec<RunSummary>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<RunsListResponse>, DashboardError> {
    let status = params
        .status
        .as_deref()
        .map(|s| {
            RunStatus::parse(s).ok_or_else(|| DashboardError::Validation {
                field: "status".into(),
                msg: format!("unknown run status '{s}'"),
            })
        })
        .transpose()?;

    let req = ListRunsRequest {
        strategy_bundle_hash: params.strategy_bundle_hash,
        scenario_id: params.scenario_id,
        status,
    };
    let items = eval::list_summaries(&state.api_context(), req).await?;
    Ok(Json(RunsListResponse { items }))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunDetail>, DashboardError> {
    let detail = eval::get_run(&state.api_context(), &id).await?;
    Ok(Json(detail))
}

#[derive(Debug, Deserialize)]
pub struct CompareParams {
    /// Comma-separated run ids: `?ids=01J…,01K…`. Whitespace around commas
    /// is tolerated; empty / single-element values surface as
    /// `ApiError::Validation` from `eval::compare`.
    pub ids: Option<String>,
}

pub async fn compare(
    State(state): State<AppState>,
    Query(params): Query<CompareParams>,
) -> Result<Json<ComparisonReport>, DashboardError> {
    let raw = params.ids.unwrap_or_default();
    let run_ids: Vec<String> = raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();

    let report = eval::compare(
        &state.api_context(),
        CompareRunsRequest { run_ids },
    )
    .await?;
    Ok(Json(report))
}

/// `GET /api/eval/runs/:id/chart` — build the chart payload for a single run.
///
/// Delegates to `chart_api::build_run_payload`. Returns `200 JSON RunChartPayload`
/// or `404 { code: "not_found" }` when the run id is unknown.
pub async fn chart(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunChartPayload>, DashboardError> {
    let payload = chart_api::build_run_payload(&state.api_context(), &id).await?;
    Ok(Json(payload))
}

/// `GET /api/eval/runs/compare/chart?ids=a,b,c` — build the compare chart payload.
///
/// Parses `?ids=` (comma-separated run ids) and delegates to
/// `chart_api::build_compare_payload`. Returns `200 JSON CompareChartPayload`,
/// `400` when more than 10 ids are given or the list is empty.
pub async fn compare_chart(
    State(state): State<AppState>,
    Query(params): Query<CompareParams>,
) -> Result<Json<CompareChartPayload>, DashboardError> {
    let raw = params.ids.unwrap_or_default();
    let run_ids: Vec<String> = raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();

    if run_ids.is_empty() {
        return Err(DashboardError::Validation {
            field: "ids".into(),
            msg: "must provide at least one run id".into(),
        });
    }

    let payload = chart_api::build_compare_payload(&state.api_context(), &run_ids).await?;
    Ok(Json(payload))
}

/// `POST /api/eval/runs` — launch a new eval run.
///
/// Constructs broker / dispatch / tools from environment variables (via
/// `eval::run`). Returns `201 Created` with the slim `RunSummary` on
/// success. Returns `400` for validation errors (unknown strategy /
/// scenario, missing env vars) and `500` for executor failures. The
/// Launch button in the dashboard wires these into an inline error banner.
pub async fn launch(
    State(state): State<AppState>,
    Json(req): Json<EvalRunRequest>,
) -> Result<(StatusCode, Json<RunSummary>), DashboardError> {
    let run = eval::run(&state.api_context(), req).await?;
    let summary = eval::summarise_run(run);
    Ok((StatusCode::CREATED, Json(summary)))
}
