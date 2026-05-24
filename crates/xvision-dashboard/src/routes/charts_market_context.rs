//! HTTP handler for the market context endpoint (chart-rework Track B B4
//! follow-up). Delegates to
//! `xvision_engine::api::charts_market_context::build_market_context_stub`.
//!
//! Route: `GET /api/v2/charts/market-context`
//!
//! Returns a deterministic stub (`MarketContextPayload`) containing BTC spot
//! price, funding rate, open interest, 24 h liquidation volume, and a 4-entry
//! regime distribution. Real exchange-data integration is a separate follow-up;
//! this handler is production-wired so the frontend can fetch live instead of
//! using inlined literals.

use axum::extract::State;
use axum::Json;
use xvision_engine::api::charts_market_context::{self as engine, MarketContextPayload};

use crate::error::DashboardError;
use crate::state::AppState;

/// `GET /api/v2/charts/market-context`
pub async fn get(State(_state): State<AppState>) -> Result<Json<MarketContextPayload>, DashboardError> {
    let p = engine::build_market_context_stub()?;
    Ok(Json(p))
}
