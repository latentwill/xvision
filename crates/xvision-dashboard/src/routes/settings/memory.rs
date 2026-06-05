use axum::{extract::State, Json};

use xvision_engine::api::settings::memory::{self, MemoryReport, UpdateMemoryRequest};

use crate::error::DashboardError;
use crate::state::AppState;

pub async fn get(State(state): State<AppState>) -> Result<Json<MemoryReport>, DashboardError> {
    let report = memory::get(&state.api_context()).await?;
    Ok(Json(report))
}

pub async fn put(
    State(state): State<AppState>,
    Json(req): Json<UpdateMemoryRequest>,
) -> Result<Json<MemoryReport>, DashboardError> {
    let report = memory::set(&state.api_context(), req).await?;
    Ok(Json(report))
}
