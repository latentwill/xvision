use axum::{extract::State, Json};

use xvision_engine::api::settings::identity::{self, IdentityReport};

use crate::error::DashboardError;
use crate::state::AppState;

pub async fn get(
    State(state): State<AppState>,
) -> Result<Json<IdentityReport>, DashboardError> {
    let report = identity::get(&state.api_context()).await?;
    Ok(Json(report))
}
