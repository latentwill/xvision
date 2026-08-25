//! `/api/settings/data-tools` — GET / PUT for `[[data_tools]]` in the
//! workspace config. Thin shim over `engine::api::settings::data_tools::*`.
//!
//! Config path resolution mirrors the providers route: `$XVN_CONFIG_PATH`,
//! then `$XVN_CONFIG`, else `<cwd>/config/default.toml`.

use axum::{extract::State, Json};
use std::path::PathBuf;

use xvision_engine::api::settings::data_tools::{self, DataToolsReport, SetDataToolsRequest};

use crate::error::DashboardError;
use crate::state::AppState;

fn config_path() -> PathBuf {
    for env_name in [
        xvision_core::config::XVN_CONFIG_PATH_ENV,
        xvision_core::config::XVN_CONFIG_ENV,
    ] {
        if let Ok(path) = std::env::var(env_name) {
            if !path.is_empty() {
                return PathBuf::from(path);
            }
        }
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("config/default.toml")
}

/// GET `/api/settings/data-tools` — return the current `[[data_tools]]` list.
/// Returns `{ "data_tools": [] }` when none are configured.
pub async fn get(State(state): State<AppState>) -> Result<Json<DataToolsReport>, DashboardError> {
    let report = data_tools::get(&state.api_context(), &config_path()).await?;
    Ok(Json(report))
}

/// PUT `/api/settings/data-tools` — atomically replace the `[[data_tools]]`
/// list. Sends `{ "data_tools": [{ ...entry, "api_key"?: "..." }] }`. An
/// optional plaintext `api_key` is persisted to
/// `$XVN_HOME/secrets/data_tools.toml` (mode 0600) and exported into the
/// daemon env under `api_key_env`; GET responses surface only an
/// `api_key_set` presence flag, never the key.
pub async fn put(
    State(state): State<AppState>,
    Json(req): Json<SetDataToolsRequest>,
) -> Result<Json<DataToolsReport>, DashboardError> {
    let report = data_tools::set(&state.api_context(), &config_path(), req).await?;
    Ok(Json(report))
}
