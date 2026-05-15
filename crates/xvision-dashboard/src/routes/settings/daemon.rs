use axum::{extract::State, Json};

use xvision_engine::api::settings::daemon::{self, DaemonReport};

use crate::error::DashboardError;
use crate::state::AppState;

pub async fn get(State(state): State<AppState>) -> Result<Json<DaemonReport>, DashboardError> {
    let report = daemon::get(&state.api_context()).await?;
    Ok(Json(report))
}
