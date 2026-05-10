//! `GET /api/strategies` — thin wrap around `engine::api::strategy::list`.

use axum::{extract::State, Json};
use serde::Serialize;

use xvision_engine::api::strategy::{self, StrategySummary};

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct StrategiesListResponse {
    pub items: Vec<StrategySummary>,
}

pub async fn list(
    State(state): State<AppState>,
) -> Result<Json<StrategiesListResponse>, DashboardError> {
    let items = strategy::list(&state.api_context()).await?;
    Ok(Json(StrategiesListResponse { items }))
}
