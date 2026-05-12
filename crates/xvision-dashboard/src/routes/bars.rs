//! `GET /api/bars/:cache_key` — fetch a single bars_cache row by cache key.
//!
//! Returns the metadata stored in the `bars_cache` table (bar count,
//! granularity, asset, window) but NOT the raw bar blob. Returns 404 when
//! the cache key is not present.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct BarsCacheRowResponse {
    pub cache_key: String,
    pub asset: String,
    pub granularity: String,
    pub window_start: String,
    pub window_end: String,
    pub bar_count: i64,
    pub fetched_at: String,
}

/// `GET /api/bars/:cache_key` — metadata for a bars_cache row.
pub async fn cache_row(
    State(state): State<AppState>,
    Path(cache_key): Path<String>,
) -> Result<Json<BarsCacheRowResponse>, DashboardError> {
    let ctx = state.api_context();
    let row: Option<(String, String, String, String, i64, String)> = sqlx::query_as(
        "SELECT asset, granularity, window_start, window_end, bar_count, fetched_at \
         FROM bars_cache WHERE cache_key = ?",
    )
    .bind(&cache_key)
    .fetch_optional(&ctx.db)
    .await
    .map_err(|e| {
        DashboardError::Internal(anyhow::anyhow!("bars cache_row query: {e}"))
    })?;

    match row {
        None => Err(DashboardError::NotFound(format!(
            "bars cache key '{cache_key}'"
        ))),
        Some((asset, granularity, window_start, window_end, bar_count, fetched_at)) => {
            Ok(Json(BarsCacheRowResponse {
                cache_key,
                asset,
                granularity,
                window_start,
                window_end,
                bar_count,
                fetched_at,
            }))
        }
    }
}

/// `DELETE /api/bars/:cache_key` — evict one bars_cache row.
/// Returns 204 on success, 404 if the key does not exist.
pub async fn evict(
    State(state): State<AppState>,
    Path(cache_key): Path<String>,
) -> Result<StatusCode, DashboardError> {
    let ctx = state.api_context();
    let result = sqlx::query("DELETE FROM bars_cache WHERE cache_key = ?")
        .bind(&cache_key)
        .execute(&ctx.db)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("bars evict: {e}")))?;

    if result.rows_affected() == 0 {
        Err(DashboardError::NotFound(format!(
            "bars cache key '{cache_key}'"
        )))
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}
