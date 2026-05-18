//! `/api/settings/danger/*` — destructive workspace ops. Thin shim over
//! `engine::api::settings::danger::*`. Every endpoint takes a JSON body
//! `{ "confirm": "yes-i-am-sure" }`; the engine returns `Validation` if
//! the literal doesn't match and the dashboard maps that to a 400.

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;

use xvision_engine::api::settings::danger::{
    self, FactoryResetReport, RegenIdentityReport, ResetWorkspaceReport,
};

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct DangerConfirm {
    /// Per-route confirm phrase — the engine validates it against
    /// `RESET_WORKSPACE_CONFIRM` / `REGEN_IDENTITY_CONFIRM` /
    /// `FACTORY_RESET_CONFIRM` depending on which handler this body
    /// is routed to. Anything else → 400 validation.
    #[serde(default)]
    pub confirm: String,
}

pub async fn reset_workspace(
    State(state): State<AppState>,
    Json(req): Json<DangerConfirm>,
) -> Result<(StatusCode, Json<ResetWorkspaceReport>), DashboardError> {
    let report = danger::reset_workspace(&state.api_context(), &req.confirm).await?;
    Ok((StatusCode::OK, Json(report)))
}

pub async fn regen_identity(
    State(state): State<AppState>,
    Json(req): Json<DangerConfirm>,
) -> Result<(StatusCode, Json<RegenIdentityReport>), DashboardError> {
    let report = danger::regen_identity(&state.api_context(), &req.confirm).await?;
    Ok((StatusCode::OK, Json(report)))
}

pub async fn factory_reset(
    State(state): State<AppState>,
    Json(req): Json<DangerConfirm>,
) -> Result<(StatusCode, Json<FactoryResetReport>), DashboardError> {
    let report = danger::factory_reset(&state.api_context(), &req.confirm).await?;
    Ok((StatusCode::OK, Json(report)))
}
