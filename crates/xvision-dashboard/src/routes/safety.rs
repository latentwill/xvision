//! Safety API routes — pause/resume gate + audit log.
//!
//! `GET  /api/safety/state`   — current pause state (no auth required for reads)
//! `POST /api/safety/pause`   — pause all broker/wallet/contract writes
//! `POST /api/safety/resume`  — resume after pause
//! `GET  /api/safety/audit`   — recent audit log rows
//!
//! Auth note: in v2b this uses the local `AuthContext` stub.
//! When `v2b-dashboard-auth-boundary` merges, the import swaps to
//! `xvision_dashboard::auth::AuthContext` per the contract Notes.

use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

use xvision_engine::api::safety::routes::{
    get_audit, get_state, pause, resume, PauseRequest, SafetyStateResponse,
};
use xvision_engine::safety::audit::SafetyAuditRow;
use xvision_engine::safety::auth_stub::AuthContext;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct AuditQueryParams {
    pub limit: Option<i64>,
}

/// `GET /api/safety/state`
pub async fn get_state_handler(
    State(state): State<AppState>,
) -> Result<Json<SafetyStateResponse>, DashboardError> {
    let manager = state.safety_manager();
    let resp = get_state(manager).await;
    Ok(Json(resp))
}

/// `POST /api/safety/pause`
pub async fn pause_handler(
    State(state): State<AppState>,
    body: Option<Json<PauseRequest>>,
) -> Result<Json<SafetyStateResponse>, DashboardError> {
    let manager = state.safety_manager();
    let req = body.map(|b| b.0).unwrap_or_default();
    // Use the anonymous stub auth context — swapped for real AuthContext
    // when v2b-dashboard-auth-boundary merges.
    let auth = AuthContext::api_anonymous();
    let resp = pause(manager, req, &auth)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("safety pause: {e:#}")))?;
    Ok(Json(resp))
}

/// `POST /api/safety/resume`
pub async fn resume_handler(
    State(state): State<AppState>,
    body: Option<Json<PauseRequest>>,
) -> Result<Json<SafetyStateResponse>, DashboardError> {
    let manager = state.safety_manager();
    let req = body.map(|b| b.0).unwrap_or_default();
    let auth = AuthContext::api_anonymous();
    let resp = resume(manager, req, &auth)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("safety resume: {e:#}")))?;
    Ok(Json(resp))
}

/// `GET /api/safety/audit?limit=N`
pub async fn audit_handler(
    State(state): State<AppState>,
    Query(params): Query<AuditQueryParams>,
) -> Result<Json<Vec<SafetyAuditRow>>, DashboardError> {
    let manager = state.safety_manager();
    let limit = params.limit.unwrap_or(100).min(500);
    let rows = get_audit(manager, limit)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("safety audit: {e:#}")))?;
    Ok(Json(rows))
}
