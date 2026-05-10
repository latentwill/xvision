//! `GET /api/eval/runs` — list eval runs.
//!
//! Thin wrapper over `engine::api::eval::list_summaries`. Query params
//! (`strategy_bundle_hash`, `scenario_id`, `status`) are honored. Body shape
//! is `{ "items": RunSummary[] }` to match `/api/strategies`.

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use xvision_engine::api::eval::{self, ListRunsRequest, RunSummary};
use xvision_engine::eval::run::RunStatus;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    pub strategy_bundle_hash: Option<String>,
    pub scenario_id: Option<String>,
    /// Free-form status string from the URL ("queued", "running", …). Parsed
    /// into the typed `RunStatus` enum below; unknown values surface as a
    /// validation error.
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
