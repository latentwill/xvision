//! HTTP handlers for the AI annotation chart (chart-rework Track B B3).
//!
//! Two endpoints, both delegating to
//! `xvision_engine::api::charts_annotated`:
//!   - `GET /api/v2/charts/annotated/:run_id`
//!   - `GET /api/v2/charts/annotated/live/:symbol`
//!
//! Demo routes stay fixture-backed for `/demo`; real run routes read
//! persisted review annotations from `eval_reviews.annotations_json`.

use axum::extract::{Path, State};
use axum::Json;
use xvision_engine::api::charts_annotated::{self as engine, AnnotatedChartPayload};
use xvision_engine::eval::store::RunStore;

use crate::error::DashboardError;
use crate::state::AppState;

/// `GET /api/v2/charts/annotated/:run_id`
pub async fn run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<AnnotatedChartPayload>, DashboardError> {
    let ctx = state.api_context();
    let store = RunStore::new(ctx.db.clone());
    let p = engine::build_annotated_run(&store, &run_id).await?;
    Ok(Json(p))
}

/// `GET /api/v2/charts/annotated/live/:symbol`
pub async fn live(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Result<Json<AnnotatedChartPayload>, DashboardError> {
    let ctx = state.api_context();
    let store = RunStore::new(ctx.db.clone());
    let p = engine::build_annotated_live(&store, &symbol).await?;
    Ok(Json(p))
}
