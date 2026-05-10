use axum::{extract::State, Json};

use xvision_engine::api::settings::brokers::{self, BrokersReport};

use crate::error::DashboardError;
use crate::state::AppState;

pub async fn get(
    State(state): State<AppState>,
) -> Result<Json<BrokersReport>, DashboardError> {
    let report = brokers::get(&state.api_context()).await?;
    Ok(Json(report))
}
