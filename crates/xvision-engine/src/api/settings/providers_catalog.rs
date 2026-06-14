//! Engine-side surface for the model catalog. Used by the `xvn provider`
//! CLI and the dashboard's `/api/providers/:name/models{,/refresh}` routes.
//!
//! This module is a thin orchestration layer over
//! `xvision_engine::providers::CatalogService` plus the existing
//! `Config::load`/`ProviderEntry` lookup helpers in
//! `xvision_engine::api::settings::providers`. The service owns the
//! HTTP client, the in-memory map, and the on-disk cache; this layer
//! just resolves "which provider" and writes the audit log.
//!
//! Why this lives alongside the existing `providers` module rather than
//! inside it: keeping the two surfaces side-by-side makes the upgrade
//! path obvious — once the catalog covers what `fetch_models` does
//! (PR #2), we can collapse `fetch_models` into `get_catalog` without
//! disturbing CLI callers that go through `xvn provider models` /
//! `xvn provider refresh-models`.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use xvision_core::config::{load_runtime_lenient, ProviderEntry, ProviderKind};
use xvision_core::providers::Catalog;

use crate::api::{
    audit::{self, Outcome},
    ApiContext, ApiError, ApiResult,
};
use crate::providers::CatalogService;

/// Refresh one provider's catalog by name. Errors if the provider is
/// `local-candle` (no remote catalog) or unknown.
pub async fn refresh(ctx: &ApiContext, config_path: &Path, name: &str) -> ApiResult<Arc<Catalog>> {
    let started = Instant::now();
    let result = refresh_inner(ctx, config_path, name).await;
    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "settings",
        "providers.catalog.refresh",
        Some(name),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn refresh_inner(ctx: &ApiContext, config_path: &Path, name: &str) -> ApiResult<Arc<Catalog>> {
    let provider = load_provider(config_path, name)?;
    let svc = CatalogService::new(ctx.xvn_home.clone())
        .map_err(|e| ApiError::Internal(format!("init catalog service: {e}")))?;
    svc.refresh(&provider)
        .await
        .map_err(|e| ApiError::Validation(format!("refresh catalog for `{name}`: {e}")))
}

/// Refresh every non-local-candle provider in parallel. Returns one
/// row per attempted provider so the caller can render a per-row
/// success/failure indicator. Local-candle providers are skipped
/// silently — they appear in the config but have nothing to fetch.
pub async fn refresh_all(ctx: &ApiContext, config_path: &Path) -> ApiResult<Vec<RefreshOutcome>> {
    let started = Instant::now();
    let result = refresh_all_inner(ctx, config_path).await;
    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "settings",
        "providers.catalog.refresh_all",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

async fn refresh_all_inner(ctx: &ApiContext, config_path: &Path) -> ApiResult<Vec<RefreshOutcome>> {
    // Lenient load: a single hand-edited bad row shouldn't fail the whole
    // catalog refresh — refresh the valid providers and ignore the dropped ones
    // (the list endpoint already surfaces them for removal).
    let (cfg, _invalid) =
        load_runtime_lenient(config_path).map_err(|e| ApiError::Validation(format!("load config: {e}")))?;
    let refreshable: Vec<ProviderEntry> = cfg
        .providers
        .into_iter()
        .filter(|p| !matches!(p.kind, ProviderKind::LocalCandle))
        .collect();
    let svc = CatalogService::new(ctx.xvn_home.clone())
        .map_err(|e| ApiError::Internal(format!("init catalog service: {e}")))?;
    let results = svc.refresh_all(&refreshable).await;
    Ok(results
        .into_iter()
        .map(|(name, result)| match result {
            Ok(cat) => RefreshOutcome {
                provider: name,
                ok: true,
                model_count: Some(cat.models.len() as u32),
                error: None,
                source_url: Some(cat.source_url.clone()),
            },
            Err(e) => RefreshOutcome {
                provider: name,
                ok: false,
                model_count: None,
                error: Some(e.to_string()),
                source_url: None,
            },
        })
        .collect())
}

/// Read the cached catalog for a provider.
///
/// Returns:
/// - `Err(NotFound)` when `name` isn't in the current config. This
///   prevents stale catalog files from removed providers being served
///   indefinitely, and it slams the door on `--name ../etc/passwd`
///   style inputs before they reach the cache layer's filename check.
/// - `Ok(None)` when the provider is configured but its catalog has
///   never been fetched. The caller surfaces "click refresh" UX.
/// - `Ok(Some(arc))` on a cache hit.
pub async fn get(ctx: &ApiContext, config_path: &Path, name: &str) -> ApiResult<Option<Arc<Catalog>>> {
    // Validate before touching disk — `load_provider` returns
    // `ApiError::NotFound` when the name isn't registered, which the
    // dashboard maps to 404 and the CLI surfaces as a clear error.
    let _ = load_provider(config_path, name)?;
    let svc = CatalogService::new(ctx.xvn_home.clone())
        .map_err(|e| ApiError::Internal(format!("init catalog service: {e}")))?;
    svc.get_or_load(name)
        .await
        .map_err(|e| ApiError::Internal(format!("load cached catalog `{name}`: {e}")))
}

/// One result row in `refresh_all`. Successful rows carry the model
/// count and source url; failed rows carry an error string the UI
/// surfaces under that provider's row.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RefreshOutcome {
    pub provider: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub model_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source_url: Option<String>,
}

fn load_provider(config_path: &Path, name: &str) -> ApiResult<ProviderEntry> {
    // Lenient load so a single invalid row elsewhere in the config doesn't block
    // fetching this (valid) provider's catalog. An invalid row won't appear in
    // `cfg.providers`, so it correctly resolves to NotFound below.
    let (cfg, _invalid) =
        load_runtime_lenient(config_path).map_err(|e| ApiError::Validation(format!("load config: {e}")))?;
    cfg.providers
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| ApiError::NotFound(format!("provider `{name}` not in config")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Actor;
    use tempfile::TempDir;

    // A full runtime config with one extra provider appended. Mirrors
    // the `MIN_CONFIG` fixture used by `api::settings::providers` tests —
    // keep them in sync if the required field list changes.
    const BASE_CONFIG: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "anthropic"
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"

[default_llm]
provider = "anthropic"
base_url = "https://api.anthropic.com"
model = "x"
api_key_env = "ANTHROPIC_API_KEY"
temperature = 0.0
max_tokens = 1024

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = false
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#;

    async fn fresh_ctx() -> (TempDir, ApiContext) {
        let tmp = TempDir::new().unwrap();
        let ctx = ApiContext::open(tmp.path(), Actor::Cli { user: "test".into() })
            .await
            .unwrap();
        (tmp, ctx)
    }

    fn write_config(dir: &Path, body: &str) -> std::path::PathBuf {
        let p = dir.join("config.toml");
        std::fs::write(&p, body).unwrap();
        p
    }

    #[tokio::test]
    async fn get_returns_none_when_provider_configured_but_no_cache() {
        let (tmp, ctx) = fresh_ctx().await;
        let cfg = write_config(tmp.path(), BASE_CONFIG);
        let result = get(&ctx, &cfg, "anthropic").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_returns_not_found_for_unconfigured_provider() {
        // Regression for the PR #198 review: `get` used to read disk
        // by raw name without checking config, so stale catalog files
        // from removed providers stayed readable forever and there
        // was no defense against `--name ../something` reaching the
        // cache layer.
        let (tmp, ctx) = fresh_ctx().await;
        let cfg = write_config(tmp.path(), BASE_CONFIG);
        let err = get(&ctx, &cfg, "openrouter-not-configured").await.unwrap_err();
        assert!(
            matches!(err, ApiError::NotFound(_)),
            "expected NotFound, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn refresh_errors_for_unknown_provider() {
        let (tmp, ctx) = fresh_ctx().await;
        let cfg = write_config(tmp.path(), BASE_CONFIG);
        let err = refresh(&ctx, &cfg, "nonexistent").await.unwrap_err();
        assert!(
            matches!(err, ApiError::NotFound(_)),
            "expected NotFound, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn refresh_errors_for_local_candle() {
        let (tmp, ctx) = fresh_ctx().await;
        let extended = format!(
            "{BASE_CONFIG}\n[[providers]]\nname = \"candle\"\nkind = \"local-candle\"\nbase_url = \"\"\napi_key_env = \"\"\n"
        );
        let cfg = write_config(tmp.path(), &extended);
        let err = refresh(&ctx, &cfg, "candle").await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("local-candle"), "got: {msg}");
    }
}
