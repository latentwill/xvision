//! `/api/settings/providers` — registered LLM provider CRUD.
//!
//! Reads from / writes to `config/default.toml` via `toml_edit` so comments
//! and formatting survive round-trips. Single source of truth for the
//! provider list — the `xvn provider` CLI and the dashboard's Settings
//! route both dispatch through here.
//!
//! Mutations re-validate the resulting config via
//! `xvision_core::config::load_runtime` before returning, so a route can
//! only ever leave the file in a valid state.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::task;

use xvision_core::config::{ProviderEntry, ProviderKind, RuntimeConfig};

use crate::api::{
    audit::{self, Outcome},
    ApiContext, ApiError, ApiResult,
};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersReport {
    pub providers: Vec<ProviderRow>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRow {
    pub name: String,
    /// Stable string form — `"anthropic" | "openai-compat" | "local-candle"`.
    pub kind: String,
    pub base_url: String,
    /// Env var holding the API key. Empty string for no-auth endpoints.
    pub api_key_env: String,
    /// True if `api_key_env` is non-empty and the env var is set.
    pub api_key_set: bool,
    /// True for synthetic rows (name starts with `_`) — read-only.
    pub synthetic: bool,
    /// True if this provider is the workspace default (referenced by the
    /// `[default_llm]` block). UI should disable the delete button when
    /// `is_default` is set — removing it would orphan the workspace default.
    pub is_default: bool,
    /// Subset of the provider's catalog the operator has enabled for the
    /// chat-rail / wizard dropdown. Empty until the operator picks
    /// models via Settings → Providers → Manage models.
    pub enabled_models: Vec<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelEntry {
    /// Canonical model id used in `/chat/completions` calls.
    pub id: String,
    /// Human-readable label when the provider exposes one (Anthropic does;
    /// most OpenAI-compat providers don't). Falls back to `id` on the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Free-form provider tag — `openai`, `anthropic`, `meta`, etc.
    /// Surfaced as a sub-label so OpenRouter's "anthropic/claude-…" rows
    /// can be filtered alongside DeepSeek's "deepseek-…".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_by: Option<String>,
    /// Context window if the provider returns one. Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u32>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelsReport {
    pub models: Vec<ProviderModelEntry>,
}

/// Result of a `POST /providers/:name/test-connection` call. Reports
/// whether the provider's catalog endpoint responded, how long it took,
/// and how many models were returned (a secondary success signal — a
/// "200 with 0 models" usually means a misconfigured base URL).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConnectionReport {
    pub ok: bool,
    pub latency_ms: u32,
    /// Number of models the catalog returned. 0 on error or when the
    /// provider's catalog endpoint genuinely returned nothing.
    pub model_count: u32,
    /// Failure message when `ok` is false. None on success.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddProviderRequest {
    pub name: String,
    pub kind: String,
    /// Optional — when blank, a kind-aware default is used
    /// ("https://api.anthropic.com" for anthropic, "https://api.openai.com/v1"
    /// for the canonical "openai" openai-compat, "" for local-candle).
    #[serde(default)]
    pub base_url: String,
    /// Optional override for the env var that holds the API key. Defaults to
    /// a kind-aware convention so users don't have to pick one.
    #[serde(default)]
    pub api_key_env: String,
    /// The actual API key (cleartext over the API). When set, persisted to
    /// `$XVN_HOME/secrets/providers.toml` (mode 0600) and exported into the
    /// daemon process env under `api_key_env` so the provider works right away.
    /// Never logged or returned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// Persisted provider secrets. Lives in `$XVN_HOME/secrets/providers.toml`,
/// keyed by provider name → `[provider]` table. Never returned through the
/// read API — only `ProviderRow::api_key_set` (a presence flag) surfaces.
#[derive(Debug, Default, Serialize, Deserialize)]
struct ProvidersSecretsFile {
    #[serde(default)]
    provider: std::collections::BTreeMap<String, ProviderSecret>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderSecret {
    /// Env var name the daemon exports this secret under.
    env_var: String,
    /// Plaintext API key. Treat the file like an SSH private key.
    api_key: String,
}

pub async fn list(ctx: &ApiContext, config_path: &Path) -> ApiResult<ProvidersReport> {
    let started = Instant::now();
    let result = list_inner(config_path, &ctx.xvn_home).await;

    let outcome = audit_outcome(&result);
    let _ = audit::record(
        ctx,
        "settings",
        "providers.list",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

pub async fn show(
    ctx: &ApiContext,
    config_path: &Path,
    name: &str,
) -> ApiResult<ProviderRow> {
    let started = Instant::now();
    let result = show_inner(config_path, &ctx.xvn_home, name).await;

    let outcome = audit_outcome(&result);
    let _ = audit::record(
        ctx,
        "settings",
        "providers.show",
        Some(name),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

pub async fn add(
    ctx: &ApiContext,
    config_path: &Path,
    req: AddProviderRequest,
) -> ApiResult<ProviderRow> {
    let started = Instant::now();
    // Strip the api_key from the audited args — the secret never lands in
    // api_audit. Everything else is fair game.
    let args = serde_json::to_string(&serde_json::json!({
        "name": req.name,
        "kind": req.kind,
        "base_url": req.base_url,
        "api_key_env": req.api_key_env,
        "api_key_provided": req.api_key.as_ref().is_some_and(|k| !k.is_empty()),
    }))
    .ok();
    let target = req.name.clone();
    let result = add_inner(config_path, &ctx.xvn_home, req).await;

    let outcome = audit_outcome(&result);
    let _ = audit::record(
        ctx,
        "settings",
        "providers.add",
        Some(&target),
        args.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

pub async fn remove(
    ctx: &ApiContext,
    config_path: &Path,
    name: &str,
) -> ApiResult<()> {
    let started = Instant::now();
    let result = remove_inner(config_path, &ctx.xvn_home, name).await;

    let outcome = audit_outcome(&result);
    let _ = audit::record(
        ctx,
        "settings",
        "providers.remove",
        Some(name),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

/// Hit the provider's catalog endpoint and return a normalized list.
/// The HTTP call lives behind the engine API so the dashboard never has
/// to learn about provider-specific endpoint paths or auth headers.
pub async fn fetch_models(
    ctx: &ApiContext,
    config_path: &Path,
    name: &str,
) -> ApiResult<ProviderModelsReport> {
    let started = Instant::now();
    let result = fetch_models_inner(config_path, name).await;
    let outcome = audit_outcome(&result);
    let _ = audit::record(
        ctx,
        "settings",
        "providers.fetch_models",
        Some(name),
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

/// Connectivity probe: call the provider's catalog endpoint and report
/// success, latency, and model count. Wraps the same dispatch as
/// `fetch_models` but always returns `Ok(report)` — a network/auth
/// failure becomes `report.ok == false` so the UI can render an error
/// pill instead of a top-level HTTP error.
pub async fn test_connection(
    ctx: &ApiContext,
    config_path: &Path,
    name: &str,
) -> ApiResult<TestConnectionReport> {
    let started = Instant::now();
    let inner_result = fetch_models_inner(config_path, name).await;
    let elapsed_ms = started.elapsed().as_millis() as u32;

    let report = match &inner_result {
        Ok(catalog) => TestConnectionReport {
            ok: true,
            latency_ms: elapsed_ms,
            model_count: catalog.models.len() as u32,
            error: None,
        },
        Err(e) => TestConnectionReport {
            ok: false,
            latency_ms: elapsed_ms,
            model_count: 0,
            error: Some(e.to_string()),
        },
    };

    let outcome = audit_outcome(&inner_result);
    let _ = audit::record(
        ctx,
        "settings",
        "providers.test_connection",
        Some(name),
        None,
        outcome,
        elapsed_ms as i64,
    )
    .await;

    Ok(report)
}

async fn fetch_models_inner(
    config_path: &Path,
    name: &str,
) -> ApiResult<ProviderModelsReport> {
    let cfg = load_cfg(config_path).await?;
    let entry = cfg
        .providers
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| ApiError::NotFound(format!("provider `{name}` not found")))?;
    let api_key = if entry.api_key_env.is_empty() {
        String::new()
    } else {
        std::env::var(&entry.api_key_env).map_err(|_| {
            ApiError::Validation(format!(
                "no API key set for `{}` (env var {} unset); paste a key first",
                entry.name, entry.api_key_env
            ))
        })?
    };
    if api_key.is_empty() && entry.kind != ProviderKind::LocalCandle {
        return Err(ApiError::Validation(format!(
            "provider `{}` has no API key set",
            entry.name
        )));
    }

    let base_url = entry.base_url.clone();
    let kind = entry.kind;
    tracing::info!(
        target: "xvision::providers::fetch_models",
        provider = %name,
        kind = ?kind,
        base_url = %base_url,
        "fetching model catalog"
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| ApiError::Internal(format!("build http client: {e}")))?;

    let models = match kind {
        ProviderKind::Anthropic => fetch_anthropic_models(&client, &api_key).await?,
        ProviderKind::OpenaiCompat => {
            fetch_openai_compat_models(&client, &base_url, &api_key).await?
        }
        ProviderKind::LocalCandle => {
            return Err(ApiError::Validation(
                "local-candle providers don't expose a catalog endpoint".into(),
            ));
        }
    };

    Ok(ProviderModelsReport { models })
}

async fn fetch_anthropic_models(
    client: &reqwest::Client,
    api_key: &str,
) -> ApiResult<Vec<ProviderModelEntry>> {
    let resp = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("anthropic /v1/models: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ApiError::Validation(format!(
            "anthropic /v1/models {status}: {body}"
        )));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("parse anthropic models: {e}")))?;
    let arr = v["data"].as_array().cloned().unwrap_or_default();
    let mut out = Vec::with_capacity(arr.len());
    for m in arr {
        let id = m["id"].as_str().unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        out.push(ProviderModelEntry {
            id: id.to_string(),
            display_name: m["display_name"].as_str().map(str::to_string),
            owned_by: Some("anthropic".to_string()),
            context_length: None,
        });
    }
    Ok(out)
}

async fn fetch_openai_compat_models(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> ApiResult<Vec<ProviderModelEntry>> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let mut req = client.get(&url);
    if !api_key.is_empty() {
        req = req.header("authorization", format!("Bearer {api_key}"));
    }
    let resp = req
        .send()
        .await
        .map_err(|e| ApiError::Internal(format!("GET {url}: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ApiError::Validation(format!(
            "GET {url} {status}: {body}"
        )));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("parse {url}: {e}")))?;
    let arr = v["data"].as_array().cloned().unwrap_or_default();
    let mut out = Vec::with_capacity(arr.len());
    for m in arr {
        let id = m["id"].as_str().unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        // OpenRouter exposes context_length under the same key; OpenAI/Groq
        // don't. Be permissive — extra fields are fine, missing ones are
        // None.
        let context_length = m["context_length"]
            .as_u64()
            .or_else(|| m["max_context_length"].as_u64())
            .map(|n| n as u32);
        out.push(ProviderModelEntry {
            id: id.to_string(),
            display_name: m["name"].as_str().map(str::to_string),
            owned_by: m["owned_by"]
                .as_str()
                .map(str::to_string)
                .or_else(|| m["provider"]["name"].as_str().map(str::to_string)),
            context_length,
        });
    }
    Ok(out)
}

/// Persist the operator's curated subset of models for a provider —
/// the chat-rail picker only surfaces ids in this list. Empty `models`
/// clears the selection (UI then prompts the operator to pick again).
pub async fn set_enabled_models(
    ctx: &ApiContext,
    config_path: &Path,
    name: &str,
    models: Vec<String>,
) -> ApiResult<ProviderRow> {
    let started = Instant::now();
    let result = set_enabled_models_inner(config_path, &ctx.xvn_home, name, models.clone()).await;
    let outcome = audit_outcome(&result);
    let args = serde_json::to_string(&serde_json::json!({ "count": models.len() })).ok();
    let _ = audit::record(
        ctx,
        "settings",
        "providers.set_enabled_models",
        Some(name),
        args.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

/// Point `[default_llm]` at a different provider so the previous default
/// becomes deletable. Optional `model` overrides `default_llm.model`; when
/// omitted, the existing model is kept (operator's choice if it's
/// incompatible with the new provider).
pub async fn set_default(
    ctx: &ApiContext,
    config_path: &Path,
    name: &str,
    model: Option<&str>,
) -> ApiResult<()> {
    let started = Instant::now();
    let result = set_default_inner(config_path, name, model).await;
    let outcome = audit_outcome(&result);
    let _ = audit::record(
        ctx,
        "settings",
        "providers.set_default",
        Some(name),
        model.map(|m| format!(r#"{{"model":"{m}"}}"#)).as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

// --- inner impls (no auditing) ---------------------------------------------

async fn list_inner(config_path: &Path, xvn_home: &Path) -> ApiResult<ProvidersReport> {
    let cfg = load_cfg(config_path).await?;
    let intern_kind: ProviderKind = cfg.default_llm.provider.into();
    let secrets = load_providers_secrets(xvn_home).await?;
    let providers = cfg
        .providers
        .iter()
        // Synthetic rows (auto-derived from [intern] / names with `_` prefix)
        // are plumbing — hide them from the UI so the empty-state is honest.
        .filter(|p| !p.name.starts_with('_'))
        .map(|p| row_from_entry(p, &cfg, intern_kind, &secrets))
        .collect();
    Ok(ProvidersReport { providers })
}

async fn show_inner(
    config_path: &Path,
    xvn_home: &Path,
    name: &str,
) -> ApiResult<ProviderRow> {
    let cfg = load_cfg(config_path).await?;
    let intern_kind: ProviderKind = cfg.default_llm.provider.into();
    let secrets = load_providers_secrets(xvn_home).await?;
    let entry = cfg
        .providers
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| ApiError::NotFound(format!("provider `{name}` not found")))?;
    Ok(row_from_entry(entry, &cfg, intern_kind, &secrets))
}

async fn add_inner(
    config_path: &Path,
    xvn_home: &Path,
    req: AddProviderRequest,
) -> ApiResult<ProviderRow> {
    let AddProviderRequest {
        name,
        kind,
        base_url,
        api_key_env,
        api_key,
    } = req;

    let parsed_kind = parse_kind(&kind)?;
    if name.trim().is_empty() {
        return Err(ApiError::Validation("name is empty".into()));
    }
    if name.starts_with('_') {
        return Err(ApiError::Validation(
            "provider names starting with '_' are reserved".into(),
        ));
    }
    // Require an API key for auth-bearing kinds, but only when the
    // operator hasn't already exported one via the env var (the CLI
    // `xvn provider add` flow assumes the env was set before the
    // command ran). Without this guard the route silently persisted
    // a row that surfaced in Settings → Providers as "missing key".
    let trimmed_key = api_key.as_deref().map(str::trim).unwrap_or("");
    if trimmed_key.is_empty() && parsed_kind != ProviderKind::LocalCandle {
        // Compute the env var we'd use for this provider so the env
        // pre-set check matches what the daemon will read.
        let env_var = if api_key_env.trim().is_empty() {
            default_api_key_env_for(parsed_kind, &name)
        } else {
            api_key_env.clone()
        };
        let env_set = std::env::var(&env_var)
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        if !env_set {
            return Err(ApiError::Validation(format!(
                "api_key is required for `{name}` — paste a key or export {env_var} before adding"
            )));
        }
    }
    // Cheap base-URL sanity check so a typo like "api.deepseek.com" (no
    // scheme) doesn't silently land in the config and 500 every subsequent
    // chat request.
    if !base_url.trim().is_empty() {
        let bu = base_url.trim();
        if !(bu.starts_with("http://") || bu.starts_with("https://")) {
            return Err(ApiError::Validation(format!(
                "base_url must start with http:// or https:// (got `{bu}`)"
            )));
        }
    }
    // Sensible defaults when the request omits these — the UI sends just
    // (name, kind, api_key) for the common case.
    let base_url = if base_url.trim().is_empty() {
        default_base_url_for(parsed_kind, &name).to_string()
    } else {
        base_url
    };
    let api_key_env = if api_key_env.trim().is_empty() {
        default_api_key_env_for(parsed_kind, &name)
    } else {
        api_key_env
    };

    let path = config_path.to_path_buf();
    let n = name.clone();
    let k = kind.clone();
    let b = base_url.clone();
    let e = api_key_env.clone();
    task::spawn_blocking(move || -> ApiResult<()> {
        use toml_edit::{value, ArrayOfTables, DocumentMut, Table};

        let raw = std::fs::read_to_string(&path).map_err(|e| {
            ApiError::Internal(format!("read {}: {e}", path.display()))
        })?;
        let mut doc: DocumentMut = raw.parse().map_err(|e| {
            ApiError::Internal(format!("parse {}: {e}", path.display()))
        })?;
        let providers = match doc
            .entry("providers")
            .or_insert_with(|| toml_edit::Item::ArrayOfTables(ArrayOfTables::new()))
        {
            toml_edit::Item::ArrayOfTables(arr) => arr,
            _ => {
                return Err(ApiError::Validation(
                    "[[providers]] is not an array of tables".into(),
                ))
            }
        };
        if providers
            .iter()
            .any(|t| t.get("name").and_then(|v| v.as_str()) == Some(&n))
        {
            return Err(ApiError::Conflict(format!(
                "provider `{n}` already exists"
            )));
        }
        let mut row = Table::new();
        row.insert("name", value(n));
        row.insert("kind", value(k));
        row.insert("base_url", value(b));
        row.insert("api_key_env", value(e));
        providers.push(row);

        std::fs::write(&path, doc.to_string()).map_err(|e| {
            ApiError::Internal(format!("write {}: {e}", path.display()))
        })?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))??;

    // Persist the secret if the caller supplied one. Empty keys are
    // treated as "no key" — local-candle / no-auth endpoints take this path.
    if let Some(key) = api_key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if api_key_env.is_empty() {
            return Err(ApiError::Validation(
                "cannot store an api_key for a provider with no api_key_env".into(),
            ));
        }
        upsert_provider_secret(xvn_home, &name, &api_key_env, key).await?;
        // Inject into the live process env so the new provider is immediately
        // usable without restarting the daemon.
        // Safe in 2021 edition; we serialize all secret writes through this
        // function so there's no concurrent set_var contention.
        std::env::set_var(&api_key_env, key);
    }

    // Re-validate the resulting config; bubble up a validation error if the
    // file is no longer well-formed (eg. a hand-edit clashed with our row).
    let cfg = load_cfg(config_path).await?;

    // First-time setup ergonomics: if the current intern default has no
    // key set but we just added one *with* a key, auto-promote the new
    // row. Without this the user would add DeepSeek with a key, then see
    // a "broken default" warning until they explicitly hit "Set as
    // default" — a step that's invisible until they go looking for it.
    let intern_kind: ProviderKind = cfg.default_llm.provider.into();
    let intern_entry = cfg.providers.iter().find(|p| {
        p.matches_triple(intern_kind, &cfg.default_llm.base_url, &cfg.default_llm.api_key_env)
    });
    let intern_has_key = intern_entry
        .map(|e| {
            !e.api_key_env.is_empty()
                && std::env::var(&e.api_key_env)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false)
        })
        .unwrap_or(false);
    let new_has_key = !api_key_env.is_empty()
        && std::env::var(&api_key_env)
            .map(|v| !v.is_empty())
            .unwrap_or(false);
    if !intern_has_key && new_has_key {
        // Best-effort — failure here doesn't undo the add. Pick a sane
        // default model for the wire kind so the wizard (which has no
        // model picker yet) doesn't hit a 404 for the old default's
        // model id on the new provider.
        let default_model = sensible_default_model(parsed_kind, &name);
        let _ = set_default_inner(config_path, &name, default_model).await;
    }

    show_inner(config_path, xvn_home, &name).await
}

async fn remove_inner(config_path: &Path, xvn_home: &Path, name: &str) -> ApiResult<()> {
    let cfg = load_cfg(config_path).await?;
    let intern_kind: ProviderKind = cfg.default_llm.provider.into();
    let entry = cfg
        .providers
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| ApiError::NotFound(format!("provider `{name}` not found")))?;
    if entry.name.starts_with('_') {
        return Err(ApiError::Validation(format!(
            "cannot remove synthetic provider `{name}`"
        )));
    }
    if entry.matches_triple(intern_kind, &cfg.default_llm.base_url, &cfg.default_llm.api_key_env) {
        return Err(ApiError::Conflict(format!(
            "cannot remove `{name}`: it's the workspace default LLM ([default_llm]). \
             Set another provider as default first, then come back to remove this one."
        )));
    }

    let path: PathBuf = config_path.to_path_buf();
    let n = name.to_string();
    task::spawn_blocking(move || -> ApiResult<()> {
        use toml_edit::DocumentMut;
        let raw = std::fs::read_to_string(&path).map_err(|e| {
            ApiError::Internal(format!("read {}: {e}", path.display()))
        })?;
        let mut doc: DocumentMut = raw.parse().map_err(|e| {
            ApiError::Internal(format!("parse {}: {e}", path.display()))
        })?;
        if let Some(toml_edit::Item::ArrayOfTables(arr)) = doc.get_mut("providers") {
            let before = arr.len();
            arr.retain(|t| t.get("name").and_then(|v| v.as_str()) != Some(&n));
            if arr.len() == before {
                return Err(ApiError::NotFound(format!(
                    "provider `{n}` not found in TOML (race / synthetic row)"
                )));
            }
        } else {
            return Err(ApiError::Validation(format!(
                "no [[providers]] block in {}",
                path.display()
            )));
        }
        std::fs::write(&path, doc.to_string()).map_err(|e| {
            ApiError::Internal(format!("write {}: {e}", path.display()))
        })?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))??;

    // Drop the stored secret too. Forgive a missing file — the user may
    // have added the provider without a key.
    forget_provider_secret(xvn_home, name).await?;

    // Re-validate.
    let _ = load_cfg(config_path).await?;
    Ok(())
}

async fn set_enabled_models_inner(
    config_path: &Path,
    xvn_home: &Path,
    name: &str,
    models: Vec<String>,
) -> ApiResult<ProviderRow> {
    // Refuse silently-bad inputs before opening the TOML — gives the UI a
    // typed validation error instead of a confusing parse failure later.
    for m in &models {
        let trimmed = m.trim();
        if trimmed.is_empty() {
            return Err(ApiError::Validation("empty model id in list".into()));
        }
        if trimmed.len() > 256 {
            return Err(ApiError::Validation(format!(
                "model id too long ({} chars): `{}`",
                trimmed.len(),
                &trimmed[..40]
            )));
        }
    }
    // Deduplicate while preserving order — operators copy/paste pages
    // and we'd rather not have the same id twice in the TOML array.
    let mut seen = std::collections::HashSet::new();
    let deduped: Vec<String> = models
        .into_iter()
        .filter(|m| seen.insert(m.clone()))
        .collect();

    {
        let cfg = load_cfg(config_path).await?;
        let exists = cfg.providers.iter().any(|p| p.name == name);
        if !exists {
            return Err(ApiError::NotFound(format!("provider `{name}` not found")));
        }
    }

    let path: PathBuf = config_path.to_path_buf();
    let target = name.to_string();
    let to_write = deduped.clone();
    task::spawn_blocking(move || -> ApiResult<()> {
        use toml_edit::{value, Array, ArrayOfTables, DocumentMut};
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| ApiError::Internal(format!("read {}: {e}", path.display())))?;
        let mut doc: DocumentMut = raw
            .parse()
            .map_err(|e| ApiError::Internal(format!("parse {}: {e}", path.display())))?;
        let providers = match doc
            .entry("providers")
            .or_insert_with(|| toml_edit::Item::ArrayOfTables(ArrayOfTables::new()))
        {
            toml_edit::Item::ArrayOfTables(arr) => arr,
            _ => {
                return Err(ApiError::Validation(
                    "[[providers]] is not an array of tables".into(),
                ))
            }
        };
        let mut matched = false;
        for tbl in providers.iter_mut() {
            if tbl.get("name").and_then(|v| v.as_str()) == Some(&target) {
                let mut arr = Array::new();
                for m in &to_write {
                    arr.push(m.as_str());
                }
                tbl.insert("enabled_models", value(arr));
                matched = true;
                break;
            }
        }
        if !matched {
            return Err(ApiError::NotFound(format!(
                "provider `{target}` not found in TOML (race / synthetic row)"
            )));
        }
        std::fs::write(&path, doc.to_string())
            .map_err(|e| ApiError::Internal(format!("write {}: {e}", path.display())))?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))??;

    // Re-validate and re-emit the canonical row so the caller can render
    // the new state without an extra GET.
    let _ = load_cfg(config_path).await?;
    show_inner(config_path, xvn_home, name).await
}

async fn set_default_inner(
    config_path: &Path,
    name: &str,
    model: Option<&str>,
) -> ApiResult<()> {
    let cfg = load_cfg(config_path).await?;
    let entry = cfg
        .providers
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| ApiError::NotFound(format!("provider `{name}` not found")))?;
    if entry.name.starts_with('_') {
        return Err(ApiError::Validation(format!(
            "cannot set default to synthetic provider `{name}`"
        )));
    }
    let new_kind = entry.kind;
    let new_base = entry.base_url.clone();
    let new_env = entry.api_key_env.clone();

    let kind_str = kind_to_str(new_kind).to_string();
    let model_owned = model.map(str::to_string);
    let path: PathBuf = config_path.to_path_buf();
    task::spawn_blocking(move || -> ApiResult<()> {
        use toml_edit::{value, DocumentMut, Item};
        let raw = std::fs::read_to_string(&path).map_err(|e| {
            ApiError::Internal(format!("read {}: {e}", path.display()))
        })?;
        let mut doc: DocumentMut = raw.parse().map_err(|e| {
            ApiError::Internal(format!("parse {}: {e}", path.display()))
        })?;
        let intern = doc
            .entry("intern")
            .or_insert(Item::Table(Default::default()))
            .as_table_mut()
            .ok_or_else(|| {
                ApiError::Validation("[intern] is not a table".into())
            })?;
        intern.insert("provider", value(kind_str));
        intern.insert("base_url", value(new_base));
        intern.insert("api_key_env", value(new_env));
        if let Some(m) = model_owned {
            intern.insert("model", value(m));
        }
        std::fs::write(&path, doc.to_string()).map_err(|e| {
            ApiError::Internal(format!("write {}: {e}", path.display()))
        })?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))??;

    // Re-validate the resulting config.
    let _ = load_cfg(config_path).await?;
    Ok(())
}

// --- helpers ---------------------------------------------------------------

async fn load_cfg(config_path: &Path) -> ApiResult<RuntimeConfig> {
    let path = config_path.to_path_buf();
    task::spawn_blocking(move || xvision_core::config::load_runtime(&path))
        .await
        .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))?
        .map_err(|e| ApiError::Validation(format!("load config: {e}")))
}

fn row_from_entry(
    entry: &ProviderEntry,
    cfg: &RuntimeConfig,
    intern_kind: ProviderKind,
    secrets: &ProvidersSecretsFile,
) -> ProviderRow {
    let api_key_set = if entry.api_key_env.is_empty() {
        false
    } else {
        // Counts as set if EITHER a stored secret exists OR the env var is
        // already populated (CI / one-shot scripts).
        secrets.provider.contains_key(&entry.name)
            || std::env::var(&entry.api_key_env)
                .map(|v| !v.is_empty())
                .unwrap_or(false)
    };
    let is_default =
        entry.matches_triple(intern_kind, &cfg.default_llm.base_url, &cfg.default_llm.api_key_env);
    ProviderRow {
        name: entry.name.clone(),
        kind: kind_to_str(entry.kind).into(),
        base_url: entry.base_url.clone(),
        api_key_env: entry.api_key_env.clone(),
        api_key_set,
        synthetic: entry.name.starts_with('_'),
        is_default,
        enabled_models: entry.enabled_models.clone(),
    }
}

/// Conventional env var name for a (kind, name) tuple. Matches the names
/// most SDKs / docs use so existing shell setups still work.
fn default_api_key_env_for(kind: ProviderKind, name: &str) -> String {
    match kind {
        ProviderKind::Anthropic => "ANTHROPIC_API_KEY".to_string(),
        ProviderKind::OpenaiCompat if name == "openai" => "OPENAI_API_KEY".to_string(),
        ProviderKind::OpenaiCompat => format!(
            "XVN_PROVIDER_{}_KEY",
            name.to_ascii_uppercase().replace('-', "_")
        ),
        ProviderKind::LocalCandle => String::new(),
    }
}

/// Best-effort model id for a `(kind, name)` provider — mirrors the
/// fallbacks the dashboard chat-rail dropdown uses so the wizard
/// (which lacks a model picker) gets a working model out of the box.
fn sensible_default_model(kind: ProviderKind, name: &str) -> Option<&'static str> {
    match kind {
        ProviderKind::Anthropic => Some("claude-sonnet-4-6"),
        ProviderKind::OpenaiCompat => match name {
            // V4 names per https://api-docs.deepseek.com — `deepseek-chat`
            // retires 2026-07-24, so auto-promote points at the new id.
            "deepseek" => Some("deepseek-v4-flash"),
            "groq" => Some("llama-3.3-70b-versatile"),
            "openrouter" => Some("anthropic/claude-3.5-sonnet"),
            "openai" => Some("gpt-4o-mini"),
            _ => None,
        },
        ProviderKind::LocalCandle => None,
    }
}

fn default_base_url_for(kind: ProviderKind, name: &str) -> &'static str {
    match kind {
        ProviderKind::Anthropic => "https://api.anthropic.com",
        ProviderKind::OpenaiCompat if name == "openai" => "https://api.openai.com/v1",
        ProviderKind::OpenaiCompat => "http://localhost:11434/v1",
        ProviderKind::LocalCandle => "",
    }
}

// --- secrets persistence ---------------------------------------------------

fn providers_secrets_path(xvn_home: &Path) -> PathBuf {
    xvn_home.join("secrets").join("providers.toml")
}

async fn load_providers_secrets(xvn_home: &Path) -> ApiResult<ProvidersSecretsFile> {
    let path = providers_secrets_path(xvn_home);
    match tokio::fs::read_to_string(&path).await {
        Ok(s) => toml::from_str::<ProvidersSecretsFile>(&s)
            .map_err(|e| ApiError::Internal(format!("parse {}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(ProvidersSecretsFile::default())
        }
        Err(e) => Err(ApiError::Internal(format!(
            "read {}: {e}",
            path.display()
        ))),
    }
}

async fn save_providers_secrets(
    xvn_home: &Path,
    file: &ProvidersSecretsFile,
) -> ApiResult<()> {
    let path = providers_secrets_path(xvn_home);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            ApiError::Internal(format!("mkdir {}: {e}", parent.display()))
        })?;
    }
    let serialized = toml::to_string_pretty(file)
        .map_err(|e| ApiError::Internal(format!("serialize providers secrets: {e}")))?;
    tokio::fs::write(&path, serialized)
        .await
        .map_err(|e| ApiError::Internal(format!("write {}: {e}", path.display())))?;
    set_owner_only(&path)?;
    Ok(())
}

async fn upsert_provider_secret(
    xvn_home: &Path,
    name: &str,
    env_var: &str,
    api_key: &str,
) -> ApiResult<()> {
    let mut file = load_providers_secrets(xvn_home).await?;
    file.provider.insert(
        name.to_string(),
        ProviderSecret {
            env_var: env_var.to_string(),
            api_key: api_key.to_string(),
        },
    );
    save_providers_secrets(xvn_home, &file).await
}

async fn forget_provider_secret(xvn_home: &Path, name: &str) -> ApiResult<()> {
    let mut file = load_providers_secrets(xvn_home).await?;
    if file.provider.remove(name).is_some() {
        save_providers_secrets(xvn_home, &file).await?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_owner_only(path: &Path) -> ApiResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms).map_err(|e| {
        ApiError::Internal(format!("chmod 600 {}: {e}", path.display()))
    })
}

#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> ApiResult<()> {
    Ok(())
}

/// Inject every stored provider secret into the process env. Called once at
/// daemon startup so backend constructors that read `std::env::var(env_var)`
/// pick up persisted keys without the user having to re-export them.
pub async fn load_providers_secrets_into_env(xvn_home: &Path) -> ApiResult<usize> {
    let file = load_providers_secrets(xvn_home).await?;
    let mut applied = 0usize;
    for (_name, secret) in file.provider.iter() {
        if secret.env_var.is_empty() || secret.api_key.is_empty() {
            continue;
        }
        // Don't clobber a key the operator already exported — env wins so
        // CI / one-shot scripts stay deterministic.
        if std::env::var_os(&secret.env_var).is_some() {
            continue;
        }
        std::env::set_var(&secret.env_var, &secret.api_key);
        applied += 1;
    }
    Ok(applied)
}

fn kind_to_str(k: ProviderKind) -> &'static str {
    match k {
        ProviderKind::Anthropic => "anthropic",
        ProviderKind::OpenaiCompat => "openai-compat",
        ProviderKind::LocalCandle => "local-candle",
    }
}

fn parse_kind(s: &str) -> ApiResult<ProviderKind> {
    match s {
        "anthropic" => Ok(ProviderKind::Anthropic),
        "openai-compat" => Ok(ProviderKind::OpenaiCompat),
        "local-candle" => Ok(ProviderKind::LocalCandle),
        other => Err(ApiError::Validation(format!(
            "invalid kind `{other}`; must be one of: anthropic | openai-compat | local-candle"
        ))),
    }
}

fn audit_outcome<T>(result: &ApiResult<T>) -> Outcome {
    match result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Actor;
    use sqlx::SqlitePool;
    use tempfile::TempDir;

    const MIN_CONFIG: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "anthropic"
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"

[intern]
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

    async fn ctx_in(dir: &TempDir) -> ApiContext {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        // Mirror engine api migrations so audit.record works in tests.
        sqlx::query(include_str!("../../../migrations/001_api_audit.sql"))
            .execute(&pool)
            .await
            .unwrap();
        ApiContext::new(
            pool,
            Actor::Cli {
                user: "test".into(),
            },
            dir.path().to_path_buf(),
        )
    }

    fn write_min_config(dir: &TempDir) -> std::path::PathBuf {
        let p = dir.path().join("default.toml");
        std::fs::write(&p, MIN_CONFIG).unwrap();
        p
    }

    #[tokio::test]
    async fn list_returns_seeded_anthropic_row() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let report = list(&ctx, &path).await.unwrap();
        assert_eq!(report.providers.len(), 1);
        let p = &report.providers[0];
        assert_eq!(p.name, "anthropic");
        assert_eq!(p.kind, "anthropic");
        assert!(p.is_default);
        assert!(!p.synthetic);
    }

    #[tokio::test]
    async fn show_returns_404_for_unknown_name() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let err = show(&ctx, &path, "nope").await.unwrap_err();
        assert!(matches!(err, ApiError::NotFound(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn add_appends_provider_and_returns_row() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let row = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "openai".into(),
                kind: "openai-compat".into(),
                base_url: "https://api.openai.com/v1".into(),
                api_key_env: "OPENAI_API_KEY".into(),
                api_key: Some("sk-test".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(row.name, "openai");
        assert_eq!(row.kind, "openai-compat");
        let report = list(&ctx, &path).await.unwrap();
        assert_eq!(report.providers.len(), 2);
    }

    #[tokio::test]
    async fn add_rejects_invalid_kind() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let err = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "x".into(),
                kind: "BOGUS".into(),
                base_url: "https://x".into(),
                api_key_env: "K".into(),
                api_key: Some("k".into()),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn add_rejects_duplicate_name() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let err = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "anthropic".into(),
                kind: "anthropic".into(),
                base_url: "https://x".into(),
                api_key_env: "K".into(),
                api_key: Some("k".into()),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn add_rejects_underscore_prefix() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let err = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "_synth".into(),
                kind: "openai-compat".into(),
                base_url: "https://x".into(),
                api_key_env: "K".into(),
                api_key: Some("k".into()),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn add_rejects_empty_api_key_for_auth_kind() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let err = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "groq".into(),
                kind: "openai-compat".into(),
                base_url: "https://api.groq.com/openai/v1".into(),
                api_key_env: "GROQ_API_KEY".into(),
                api_key: None,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn add_rejects_base_url_without_scheme() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let err = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "deepseek".into(),
                kind: "openai-compat".into(),
                base_url: "api.deepseek.com/v1".into(),
                api_key_env: "DEEPSEEK_API_KEY".into(),
                api_key: Some("sk-test".into()),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn remove_refuses_when_intern_references_provider() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let err = remove(&ctx, &path, "anthropic").await.unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn remove_drops_provider_row() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let mut src = MIN_CONFIG.to_string();
        src.push_str(
            r#"
[[providers]]
name = "ephemeral"
kind = "openai-compat"
base_url = "https://x"
api_key_env = "K"
"#,
        );
        std::fs::write(&path, src).unwrap();
        let ctx = ctx_in(&dir).await;
        remove(&ctx, &path, "ephemeral").await.unwrap();
        let report = list(&ctx, &path).await.unwrap();
        assert!(report.providers.iter().all(|p| p.name != "ephemeral"));
    }

    #[tokio::test]
    async fn remove_returns_404_for_unknown_name() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let err = remove(&ctx, &path, "nope").await.unwrap_err();
        assert!(matches!(err, ApiError::NotFound(_)), "got {err:?}");
    }
}
