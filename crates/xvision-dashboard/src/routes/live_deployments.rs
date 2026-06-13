//! `GET /api/live/deployments[/:id]` — read-only live-deployment endpoints.
//!
//! Thin wrappers over `xvision_engine::api::eval::{list_live_deployments,
//! get_live_deployment}`. Part of CT5 live-deployments contract.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use xvision_engine::api::eval::{get_live_deployment, list_live_deployments, LiveDeploymentSummary};

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<LiveDeploymentSummary>>, DashboardError> {
    // Default to "running"; an explicit empty ?status= means "all"
    // (handled in the engine fn which treats empty as no filter).
    let status = q.status.as_deref().or(Some("running"));
    Ok(Json(list_live_deployments(&state.api_context(), status).await?))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<LiveDeploymentSummary>, DashboardError> {
    match get_live_deployment(&state.api_context(), &id).await? {
        Some(d) => Ok(Json(d)),
        None => Err(DashboardError::NotFound(format!(
            "deployment '{id}' not found"
        ))),
    }
}
