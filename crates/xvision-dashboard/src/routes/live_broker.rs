//! `GET /api/live/venue-account` — live execution-venue account snapshot.
//!
//! Thin wrapper over `xvision_engine::api::live_broker::venue_account`.
//! Always 200; a missing/unreachable venue renders as
//! `{ connected: false, reason: "…" }` so the live page can show a
//! "not configured" state instead of an HTTP error.

use axum::Json;

use xvision_engine::api::live_broker::{self, VenueAccountDto};

use crate::error::DashboardError;

pub async fn venue_account() -> Result<Json<VenueAccountDto>, DashboardError> {
    let dto = live_broker::venue_account().await?;
    Ok(Json(dto))
}
