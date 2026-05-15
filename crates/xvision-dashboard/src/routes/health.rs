//! `GET /api/health` — local probes (data dir, db, strategy store).
//!
//! Always 200; the body's `status` field is the canonical
//! ok/degraded/down indicator. Probe failures show up in `probes[*]`
//! rather than as HTTP errors so dashboards can render mixed state.

use axum::{extract::State, Json};

use xvision_engine::api::health::{self, HealthReport};

use crate::error::DashboardError;
use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Result<Json<HealthReport>, DashboardError> {
    let report = health::check(&state.api_context()).await?;
    Ok(Json(report))
}
