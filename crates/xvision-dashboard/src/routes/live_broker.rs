//! `GET /api/live/venue-account` — live execution-venue account snapshot.
//!
//! Thin wrapper over `xvision_engine::api::live_broker::venue_account`.
//! Always 200; a missing/unreachable venue renders as
//! `{ connected: false, reason: "…" }` so the live page can show a
//! "not configured" state instead of an HTTP error.
//!
//! Optional query param: `?venue=<name>` — when omitted, defaults to Orderly.

use axum::{extract::Query, extract::State, Json};
use serde::Deserialize;

use xvision_engine::api::live_broker::{self, VenueAccountDto};

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct VenueQuery {
    pub venue: Option<String>,
}

pub async fn venue_account(
    State(state): State<AppState>,
    Query(q): Query<VenueQuery>,
) -> Result<Json<VenueAccountDto>, DashboardError> {
    let dto = live_broker::venue_account(q.venue.as_deref(), &state.xvn_home).await?;
    Ok(Json(dto))
}
