//! HTTP handlers for the AI annotation chart (chart-rework Track B B3).
//!
//! Two endpoints, both delegating to
//! `xvision_engine::api::charts_annotated`:
//!   - `GET /api/v2/charts/annotated/:run_id`
//!   - `GET /api/v2/charts/annotated/live/:symbol`
//!
//! B3 ships fixture-backed stubs; the live annotation producer is
//! explicitly out of scope per spec §9 (the live handler returns an
//! empty `annotations` array so the UI renders an EmptyState).

use axum::extract::{Path, State};
use axum::Json;
use xvision_engine::api::charts_annotated::{self as engine, AnnotatedChartPayload};

use crate::error::DashboardError;
use crate::state::AppState;

/// `GET /api/v2/charts/annotated/:run_id`
pub async fn run(
    State(_state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<AnnotatedChartPayload>, DashboardError> {
    let p = engine::build_annotated_run_stub(&run_id)?;
    Ok(Json(p))
}

/// `GET /api/v2/charts/annotated/live/:symbol`
pub async fn live(
    State(_state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<AnnotatedChartPayload>, DashboardError> {
    let p = engine::build_annotated_live_stub(&symbol)?;
    Ok(Json(p))
}
