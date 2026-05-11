//! `/api/settings/providers` — list / show / add / remove registered
//! LLM providers. Thin shim over `engine::api::settings::providers::*`.
//!
//! Config path resolution: `$XVN_CONFIG_PATH` if set, else
//! `<cwd>/config/default.toml`. Matches what the `xvn provider` CLI uses.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::path::PathBuf;

use xvision_engine::api::settings::providers::{
    self, AddProviderRequest, ProviderRow, ProvidersReport,
};

use crate::error::DashboardError;
use crate::state::AppState;

fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("XVN_CONFIG_PATH") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("config/default.toml")
}

pub async fn list(
    State(state): State<AppState>,
) -> Result<Json<ProvidersReport>, DashboardError> {
    let report = providers::list(&state.api_context(), &config_path()).await?;
    Ok(Json(report))
}

pub async fn show(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ProviderRow>, DashboardError> {
    let row = providers::show(&state.api_context(), &config_path(), &name).await?;
    Ok(Json(row))
}

pub async fn add(
    State(state): State<AppState>,
    Json(req): Json<AddProviderRequest>,
) -> Result<(StatusCode, Json<ProviderRow>), DashboardError> {
    let row = providers::add(&state.api_context(), &config_path(), req).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

pub async fn remove(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, DashboardError> {
    providers::remove(&state.api_context(), &config_path(), &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct SetDefaultBody {
    #[serde(default)]
    pub model: Option<String>,
}

/// POST `/api/settings/providers/:name/set-default` — point `[intern]` at
/// the named provider so the previous default becomes deletable.
pub async fn set_default(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<SetDefaultBody>,
) -> Result<StatusCode, DashboardError> {
    providers::set_default(
        &state.api_context(),
        &config_path(),
        &name,
        body.model.as_deref(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
