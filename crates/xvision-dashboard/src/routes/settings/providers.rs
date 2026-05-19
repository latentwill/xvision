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

use xvision_core::providers::Catalog;
use xvision_engine::api::settings::providers::{
    self, AddProviderRequest, ProviderModelsReport, ProviderRow, ProvidersReport, TestConnectionReport,
    UpdateProviderRequest,
};
use xvision_engine::api::settings::providers_catalog::{self, RefreshOutcome};

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

pub async fn list(State(state): State<AppState>) -> Result<Json<ProvidersReport>, DashboardError> {
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

pub async fn update(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<UpdateProviderRequest>,
) -> Result<Json<ProviderRow>, DashboardError> {
    let row = providers::update(&state.api_context(), &config_path(), &name, req).await?;
    state.models_cache_invalidate(&name);
    Ok(Json(row))
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

/// POST `/api/settings/providers/:name/set-default` — point `[default_llm]`
/// at the named provider so the previous default becomes deletable.
pub async fn set_default(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<SetDefaultBody>,
) -> Result<StatusCode, DashboardError> {
    providers::set_default(&state.api_context(), &config_path(), &name, body.model.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET `/api/settings/providers/:name/models` — fetch the provider's
/// model catalog from upstream. Result is cached in-process for a few
/// minutes to keep the chat-rail dropdown snappy.
pub async fn list_models(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ProviderModelsReport>, DashboardError> {
    if let Some(cached) = state.models_cache_get(&name) {
        return Ok(Json(cached));
    }
    let report = providers::fetch_models(&state.api_context(), &config_path(), &name).await?;
    state.models_cache_put(name, report.clone());
    Ok(Json(report))
}

#[derive(Debug, Deserialize)]
pub struct EnabledModelsBody {
    pub models: Vec<String>,
}

/// PUT `/api/settings/providers/:name/enabled-models` — persist the
/// operator's curated subset of models for this provider.
pub async fn put_enabled_models(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<EnabledModelsBody>,
) -> Result<Json<ProviderRow>, DashboardError> {
    let row = providers::set_enabled_models(&state.api_context(), &config_path(), &name, body.models).await?;
    Ok(Json(row))
}

/// POST `/api/settings/providers/:name/test-connection` — connectivity
/// probe. Always 200 with `{ ok, latency_ms, model_count, error? }`;
/// network/auth failures surface in the body so the UI renders a pill.
pub async fn test_connection(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<TestConnectionReport>, DashboardError> {
    let report = providers::test_connection(&state.api_context(), &config_path(), &name).await?;
    Ok(Json(report))
}

/// GET `/api/settings/providers/:name/catalog` — read the persisted
/// catalog (provider-supplied model metadata: context window, max
/// output tokens, pricing) without hitting the network. Returns 404
/// when the catalog hasn't been fetched yet; the UI then prompts the
/// operator to click "Refresh".
///
/// Distinct from `list_models` above, which goes through the older
/// `fetch_models` path and only carries `id`/`display_name`/`context_length`.
/// Once the catalog covers everything `fetch_models` does, the older
/// route collapses into this one (PR #2).
pub async fn get_catalog(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Catalog>, DashboardError> {
    // `get` returns NotFound for unconfigured providers — that maps to
    // a 404 via DashboardError::from(ApiError), which the UI surfaces
    // as "provider was removed; refresh the page" rather than feeding
    // a phantom catalog from a stale disk file.
    let cat = providers_catalog::get(&state.api_context(), &config_path(), &name).await?;
    match cat {
        Some(arc) => Ok(Json((*arc).clone())),
        None => Err(DashboardError::NotFound(format!(
            "no cached catalog for `{name}`; POST /catalog/refresh first"
        ))),
    }
}

/// POST `/api/settings/providers/:name/catalog/refresh` — force a
/// fetch + cache write for one provider. Returns the fresh catalog.
pub async fn refresh_catalog(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Catalog>, DashboardError> {
    let cat = providers_catalog::refresh(&state.api_context(), &config_path(), &name).await?;
    // Keep the in-process `models_cache` (the legacy per-name lru) in
    // sync so the chat-rail dropdown reflects the refresh immediately.
    state.models_cache_invalidate(&name);
    Ok(Json((*cat).clone()))
}

/// POST `/api/settings/providers/catalog/refresh-all` — refresh every
/// non-local-candle provider in parallel. Always 200 with a per-row
/// status array; transient per-provider failures (auth, network) ride
/// through as `ok=false` rows so the UI can render a partial result.
pub async fn refresh_all_catalogs(
    State(state): State<AppState>,
) -> Result<Json<Vec<RefreshOutcome>>, DashboardError> {
    let rows = providers_catalog::refresh_all(&state.api_context(), &config_path()).await?;
    for row in &rows {
        if row.ok {
            state.models_cache_invalidate(&row.provider);
        }
    }
    Ok(Json(rows))
}
