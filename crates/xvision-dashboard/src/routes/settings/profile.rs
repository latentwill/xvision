use axum::{extract::State, Json};

use xvision_engine::api::settings::profile::{self, ProfileReport, UpdateProfileRequest};

use crate::error::DashboardError;
use crate::state::AppState;

/// `GET /api/settings/profile` — read the operator profile (display name / handle).
pub async fn get(State(state): State<AppState>) -> Result<Json<ProfileReport>, DashboardError> {
    let report = profile::get(&state.api_context()).await?;
    Ok(Json(report))
}

/// `PUT /api/settings/profile` — partial update of the operator profile.
pub async fn put(
    State(state): State<AppState>,
    Json(req): Json<UpdateProfileRequest>,
) -> Result<Json<ProfileReport>, DashboardError> {
    let report = profile::set(&state.api_context(), req).await?;
    Ok(Json(report))
}
