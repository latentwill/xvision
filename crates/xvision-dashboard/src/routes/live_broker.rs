//! `GET /api/live/venue-account` — live execution-venue account snapshot.
//!
//! Thin wrapper over `xvision_engine::api::live_broker::venue_account`.
//! Always 200; a missing/unreachable venue renders as
//! `{ connected: false, reason: "…" }` so the live page can show a
//! "not configured" state instead of an HTTP error.
//!
//! Optional query param: `?venue=<name>` — when omitted, defaults to Orderly.

use axum::{extract::Query, extract::State, http::HeaderMap, Json};
use serde::Deserialize;

use crate::auth::require_auth::verify_configured_dashboard_password;
use crate::auth::{AUTH_TOKEN_ENV, AUTH_TOKEN_HEADER};
use xvision_engine::api::live_broker::{self, VenueAccountDto};

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct VenueQuery {
    pub venue: Option<String>,
}

fn extract_cookie_token(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let kv = part.trim();
        if let Some(value) = kv.strip_prefix(&format!("{name}=")) {
            return Some(value.to_string());
        }
    }
    None
}

fn has_outer_dashboard_token(headers: &HeaderMap) -> bool {
    let Ok(expected) = std::env::var(AUTH_TOKEN_ENV) else {
        return false;
    };
    if expected.is_empty() {
        return false;
    }
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let header = headers.get(AUTH_TOKEN_HEADER).and_then(|v| v.to_str().ok());
    let cookie = extract_cookie_token(headers, "xvn_dashboard_token");
    bearer == Some(expected.as_str())
        || header == Some(expected.as_str())
        || cookie.as_deref() == Some(expected.as_str())
}

pub async fn venue_account(
    State(state): State<AppState>,
    Query(q): Query<VenueQuery>,
    headers: HeaderMap,
) -> Result<Json<VenueAccountDto>, DashboardError> {
    if !has_outer_dashboard_token(&headers)
        && !verify_configured_dashboard_password(&state.pool, &headers, None).await
    {
        return Err(DashboardError::Unauthorized(
            "live venue account requires dashboard authentication".into(),
        ));
    }
    let dto = live_broker::venue_account(q.venue.as_deref(), &state.xvn_home).await?;
    Ok(Json(dto))
}
