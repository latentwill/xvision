//! `/api/settings/brokers` routes.
//!
//! - `GET  /api/settings/brokers`               — env + stored snapshot
//! - `POST /api/settings/brokers/alpaca`        — persist Alpaca creds
//! - `DELETE /api/settings/brokers/alpaca`      — drop stored Alpaca creds
//!
//! Mutation routes write to `$XVN_HOME/secrets/brokers.toml` (mode 0600).
//! Secrets never come back through `GET` — only a redacted key-id suffix.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};

use xvision_engine::api::settings::brokers::{
    self, AlpacaStored, AlpacaTestReport, BrokersReport, ByrealStored, DegenArenaStored, SetAlpacaReq,
    SetByrealReq, SetDegenArenaReq,
};

use crate::error::DashboardError;
use crate::state::AppState;

pub async fn get(State(state): State<AppState>) -> Result<Json<BrokersReport>, DashboardError> {
    let report = brokers::get(&state.api_context()).await?;
    Ok(Json(report))
}

pub async fn set_alpaca(
    State(state): State<AppState>,
    Json(req): Json<SetAlpacaReq>,
) -> Result<(StatusCode, Json<AlpacaStored>), DashboardError> {
    let stored = brokers::set_alpaca(&state.api_context(), req).await?;
    Ok((StatusCode::CREATED, Json(stored)))
}

pub async fn delete_alpaca(State(state): State<AppState>) -> Result<impl IntoResponse, DashboardError> {
    brokers::clear_alpaca(&state.api_context()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST `/api/settings/brokers/byreal` — persist a byreal trading-only agent
/// key (+ optional network/account). The key never comes back through `GET`.
pub async fn set_byreal(
    State(state): State<AppState>,
    Json(req): Json<SetByrealReq>,
) -> Result<(StatusCode, Json<ByrealStored>), DashboardError> {
    let stored = brokers::set_byreal(&state.api_context(), req).await?;
    Ok((StatusCode::CREATED, Json(stored)))
}

/// DELETE `/api/settings/brokers/byreal` — drop stored byreal creds.
pub async fn delete_byreal(State(state): State<AppState>) -> Result<impl IntoResponse, DashboardError> {
    brokers::clear_byreal(&state.api_context()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST `/api/settings/brokers/alpaca/test-connection` — connectivity
/// probe against `/v2/account`. Always 200 with the test report body;
/// auth/network failures surface in `error` so the UI renders a pill.
pub async fn test_alpaca(State(state): State<AppState>) -> Result<Json<AlpacaTestReport>, DashboardError> {
    let report = brokers::test_alpaca(&state.api_context()).await?;
    Ok(Json(report))
}

/// POST `/api/live/deploy/degen-arena` — persist Degen Arena trade-only
/// HL agent-wallet credentials so a subsequent live run can use them.
///
/// Body: `{ apiKey: string, accountAddress: string, network: "testnet"|"mainnet" }`
///
/// Validates format (regex) before writing. The key is NEVER echoed back.
/// Returns `200 { ok: true }` on success; `400` on invalid input.
pub async fn set_degen_arena(
    State(state): State<AppState>,
    Json(req): Json<SetDegenArenaReq>,
) -> Result<Json<DegenArenaStored>, DashboardError> {
    let stored = brokers::set_degen_arena(&state.api_context(), req).await?;
    Ok(Json(stored))
}

/// DELETE `/api/live/deploy/degen-arena` — drop stored Degen Arena creds.
pub async fn delete_degen_arena(
    State(state): State<AppState>,
) -> Result<axum::http::StatusCode, DashboardError> {
    brokers::clear_degen_arena(&state.api_context()).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
