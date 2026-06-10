//! `GET /api/assets` — tradeable asset registry.

use axum::extract::State;
use axum::Json;

use xvision_engine::api::assets::{list_assets, AssetInfo};

use crate::error::DashboardError;
use crate::state::AppState;

/// Returns all assets from the process-global registry, sorted by symbol.
///
/// Reads from the in-memory `asset_registry::REGISTRY` loaded at startup —
/// no DB query required. Returns `[]` before the whitelist is loaded.
pub async fn list(
    State(_state): State<AppState>,
) -> Result<Json<Vec<AssetInfo>>, DashboardError> {
    Ok(Json(list_assets()))
}
