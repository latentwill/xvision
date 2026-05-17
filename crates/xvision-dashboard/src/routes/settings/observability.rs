use axum::{extract::State, Json};

use xvision_engine::api::settings::observability::{
    self, ObservabilityReport, UpdateObservabilityRequest,
};

use crate::error::DashboardError;
use crate::state::AppState;

pub async fn get(
    State(state): State<AppState>,
) -> Result<Json<ObservabilityReport>, DashboardError> {
    let report = observability::get(&state.api_context()).await?;
    Ok(Json(report))
}

pub async fn put(
    State(state): State<AppState>,
    Json(req): Json<UpdateObservabilityRequest>,
) -> Result<Json<ObservabilityReport>, DashboardError> {
    let report = observability::set_mode(&state.api_context(), req).await?;
    Ok(Json(report))
}
