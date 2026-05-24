//! HTTP handlers for the Charts dashboard section (chart-rework Track B).
//!
//! Delegates to `xvision_engine::api::charts_dashboards` for payload
//! construction. B0 ships the `overview` endpoint as a fixture-backed
//! stub; B1 swaps the engine-side builder for live data without
//! touching this route module — only the delegated function changes.

use axum::extract::State;
use axum::Json;
use xvision_engine::api::charts_dashboards::{self as engine, MultiStrategyEquityBundle};

use crate::error::DashboardError;
use crate::state::AppState;

/// `GET /api/v2/charts/dashboards/overview`
///
/// Returns a [`MultiStrategyEquityBundle`] used by the B1 Dark Minimal
/// Strategy Dashboard surface and reused by B2/B4. B1 calls the real
/// builder which pairs each Strategy with its latest backtest run equity
/// series. Falls back to the deterministic fixture stub when no completed
/// runs exist on disk (cold start / empty workspace).
pub async fn overview(
    State(state): State<AppState>,
) -> Result<Json<MultiStrategyEquityBundle>, DashboardError> {
    let ctx = state.api_context();
    let bundle = engine::build_dashboard_overview(&ctx).await?;
    Ok(Json(bundle))
}
