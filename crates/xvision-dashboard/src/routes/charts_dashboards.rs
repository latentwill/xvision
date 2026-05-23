//! HTTP handlers for the Charts dashboard section (chart-rework Track B).
//!
//! Delegates to `xvision_engine::api::charts_dashboards` for payload
//! construction. B0 ships the `overview` endpoint as a fixture-backed
//! stub; B1 swaps the engine-side builder for live data without
//! touching this route module.

use axum::extract::State;
use axum::Json;
use xvision_engine::api::charts_dashboards::{
    self as engine, MultiStrategyEquityBundle,
};

use crate::error::DashboardError;
use crate::state::AppState;

/// `GET /api/v2/charts/dashboards/overview`
///
/// Returns a [`MultiStrategyEquityBundle`] used by the B1 Dark Minimal
/// Strategy Dashboard surface and reused by B2/B4. B0 returns the
/// deterministic frontend fixture verbatim.
pub async fn overview(
    State(_state): State<AppState>,
) -> Result<Json<MultiStrategyEquityBundle>, DashboardError> {
    let bundle = engine::build_dashboard_overview_stub()?;
    Ok(Json(bundle))
}
