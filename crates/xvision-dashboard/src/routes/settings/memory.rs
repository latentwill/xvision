use axum::{extract::State, Json};

use xvision_engine::api::memory::MemoryStatus;
use xvision_engine::api::settings::memory::{self, MemoryReport, UpdateMemoryRequest};

use crate::error::DashboardError;
use crate::state::AppState;

pub async fn get(State(state): State<AppState>) -> Result<Json<MemoryReport>, DashboardError> {
    let report = memory::get(&state.api_context()).await?;
    Ok(Json(report))
}

/// `GET /api/settings/memory/status` — read-only operator health snapshot
/// (resolved embedder source, store path/writability, grace window,
/// per-namespace live-observation counts). Thin wrapper around
/// `engine::api::memory::status`; the store is resolved via the same
/// process-wide handle the `/api/memory` routes use.
pub async fn status(State(state): State<AppState>) -> Result<Json<MemoryStatus>, DashboardError> {
    let store = crate::routes::memory::resolve_store().await?;
    let s = xvision_engine::api::memory::status(&store, &state.xvn_home).await?;
    Ok(Json(s))
}

pub async fn put(
    State(state): State<AppState>,
    Json(req): Json<UpdateMemoryRequest>,
) -> Result<Json<MemoryReport>, DashboardError> {
    let report = memory::set(&state.api_context(), req).await?;
    Ok(Json(report))
}
