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

use xvision_engine::api::eval::{
    self, CompareRunsRequest, EvalRunRequest, ListRunsRequest, RunDetail, RunSummary,
    ScenarioSummary,
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

#[derive(Serialize)]
pub struct ScenariosListResponse {
    pub items: Vec<ScenarioSummary>,
}

/// `GET /api/eval/scenarios` — list the canonical scenarios the start
/// modal renders in its dropdown. Wraps `eval::scenarios` (which audits
/// the call). The list is small and static; clients are free to cache.
pub async fn list_scenarios(
    State(state): State<AppState>,
) -> Result<Json<ScenariosListResponse>, DashboardError> {
    let items = eval::scenarios(&state.api_context()).await?;
    Ok(Json(ScenariosListResponse { items }))
}

/// `POST /api/eval/runs` — kick off a new eval run. Body
/// `EvalRunRequest { agent_id, scenario_id, mode, params_override? }`.
///
/// Returns 202 Accepted with the freshly-persisted `RunDetail` (status
/// = `Queued`). The actual run drives in a background tokio task; the
/// caller is expected to poll `GET /api/eval/runs/:id` until status
/// reaches a terminal state.
pub async fn post_start(
    State(state): State<AppState>,
    Json(body): Json<EvalRunRequest>,
) -> Result<(StatusCode, Json<RunDetail>), DashboardError> {
    let detail = eval::start_run(&state.api_context(), body).await?;
    Ok((StatusCode::ACCEPTED, Json(detail)))
}
