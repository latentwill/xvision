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
use crate::providers::fetcher::openai_compat_models_url;

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersReport {
    pub providers: Vec<ProviderRow>,
    /// The currently-configured model on `[default_llm]`. Surfaced
    /// alongside the per-row `is_default` flag so the Default-LLM UI
    /// can pre-fill its model dropdown without a second fetch. None
    /// when the operator hasn't set a default/model yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// `[[providers]]` rows that failed validation and were skipped during the
    /// lenient load (e.g. an uppercase name from a hand-edited config). Surfaced
    /// so the UI can warn the operator and offer to remove them, instead of the
    /// list silently going blank. Empty in the normal case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub invalid: Option<Vec<InvalidProviderRow>>,
}

/// Wire view of a dropped/invalid provider row (see [`ProvidersReport::invalid`]).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvalidProviderRow {
    /// The (invalid) name as written in the config — also the key used to
    /// remove the row via `DELETE /providers/:name`.
    pub name: String,
    /// Human-readable reason the row was rejected.
    pub reason: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRow {
    pub name: String,
    /// Stable string form — `"anthropic" | "openai-compat" | "local-candle" | "ollama" | "llama-cpp" | "vllm"`.
    pub kind: String,
    pub base_url: String,
    /// Env var holding the API key. Empty string for no-auth endpoints.
    pub api_key_env: String,
    /// True if `api_key_env` is non-empty and the env var is set.
    pub api_key_set: bool,
    /// True for synthetic rows (kept for wire compatibility; new configs do
    /// not auto-create provider rows from `[default_llm]`).
    pub synthetic: bool,
    /// True if this provider is the workspace default (referenced by the
    /// `[default_llm]` block). Removing it clears the default.
    pub is_default: bool,
    /// Subset of the provider's catalog the operator has enabled for the
    /// chat-rail / wizard dropdown. Empty until the operator picks
    /// models via Settings → Providers → Manage models.
    pub enabled_models: Vec<String>,
}

/// Canonical view of one provider — combines the persisted `ProviderEntry`,
/// secret/env presence, and the operator's `enabled_models` curation into
/// the answer to "is this `(provider, model)` launchable right now?".
///
/// Returned by [`effective_providers`]; the same shape backs:
/// - `xvn provider list --effective` (CLI)
/// - the dashboard's `/api/settings/providers` handler (via `list`)
/// - the `xvn doctor` `providers` block
/// - eval-launch refusal in `crate::api::eval::resolve_provider`
///
/// No other code path may independently compute `launchable`. If a new
/// surface needs the answer, route it through `effective_providers` so
/// the CLI / dashboard / eval-launch verdicts stay byte-identical.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveProvider {
    pub provider: String,
    /// Stable string form — `"anthropic" | "openai-compat" | "local-candle" | "ollama" | "llama-cpp" | "vllm"`.
    pub kind: String,
    pub base_url: String,
    /// Env var holding the API key. Empty string for no-auth endpoints.
    pub api_key_env: String,
    /// The env var the daemon reads this provider's key from BY CONVENTION
    /// (`default_api_key_env_for`), independent of whether the operator has
    /// overridden `api_key_env`. Surfaced so `provider list` / error
    /// messages can NAME the variable an operator must export when a key is
    /// missing (QA U8). Empty for no-auth local kinds.
    #[serde(default)]
    pub expected_api_key_env: String,
    /// Whether the provider is enabled in the workspace config. Today this
    /// is always `true` for every non-synthetic row — there is no separate
    /// `enabled` toggle — but the field is surfaced so future toggles plug
    /// in without breaking JSON consumers.
    pub enabled: bool,
    /// True iff an API key for this provider is materialized (stored
    /// secret, env-exported, or unnecessary because the kind is no-auth).
    pub has_key: bool,
    /// Per-model launch verdict, one entry per id in `enabled_models`.
    /// Empty when the operator hasn't curated a model list yet.
    pub models: Vec<EffectiveProviderModel>,
    /// Roll-up verdict — `enabled && has_key && (kind == LocalCandle || at least one model enabled)`.
    /// The single source of truth for "can the operator launch eval against
    /// this provider right now". Eval-launch refusal turns on the same
    /// predicate (see `ProviderUnavailableReason`).
    pub launchable: bool,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveProviderModel {
    pub id: String,
    /// True iff this model id appears in the provider's `enabled_models`
    /// list. All entries in `EffectiveProvider::models` are derived from
    /// that list, so today every entry is `enabled = true` — the field is
    /// kept on the wire so a future "disable individual models" toggle
    /// can ship without an API shape change.
    pub enabled: bool,
}

/// Discriminant for why eval-launch refused a `(provider, model)`. Replaces
/// the historic flat `"provider '{name}' is not configured"` string so
/// operators reading CLI output know whether to add a key or flip a toggle.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderUnavailableReason {
    /// Provider name is not in `[[providers]]`.
    ProviderUnknown,
    /// Provider row exists but `enabled` is false. Today no toggle exists
    /// — the variant is on the wire so the discriminant survives if one
    /// is added.
    ProviderDisabled,
    /// Provider is configured but no API key is materialized (env var
    /// unset, no stored secret).
    KeyMissing,
    /// Provider has the key but the requested model is not enabled (or no
    /// model is requested and `enabled_models` is empty).
    ModelDisabled,
}

impl ProviderUnavailableReason {
    /// Stable wire identifier — matches the `#[serde]` form. Surfaced on
    /// `ProviderUnavailable::reason_str` for code that walks the error
    /// without re-parsing it.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ProviderUnknown => "provider_unknown",
            Self::ProviderDisabled => "provider_disabled",
            Self::KeyMissing => "key_missing",
            Self::ModelDisabled => "model_disabled",
        }
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUnavailable {
    pub provider: String,
    pub reason: ProviderUnavailableReason,
    /// Set when the caller named a specific model. None means the caller
    /// only asked about provider-level launchability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Human-readable hint — names the env var to set or the toggle to
    /// flip. Eval refusal renders this verbatim into the CLI error.
    pub hint: String,
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

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProviderRequest {
    pub kind: String,
    pub base_url: String,
    pub api_key_env: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_models: Option<Vec<String>>,
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

/// Canonical "which providers are launchable right now" lookup. Returns
/// one row per non-synthetic provider in the workspace config; each row
/// carries the verdict the rest of the surface uses (CLI `provider list
/// --effective`, dashboard `/api/settings/providers`, `xvn doctor`,
/// eval-launch refusal).
///
/// Reads from existing config + secrets + provider catalog. No
/// persistence side-effects.
pub async fn effective_providers(ctx: &ApiContext, config_path: &Path) -> ApiResult<Vec<EffectiveProvider>> {
    effective_providers_with_paths(&ctx.xvn_home, config_path).await
}

/// Same canonical rollup as [`effective_providers`], but only requires
/// the on-disk paths. Used by `xvn doctor`, which is a diagnostic verb
/// and should not open an `ApiContext` (that side-effects tracing /
/// audit-table migration and pollutes a `--json` report). Keep the two
/// in lock-step: any change to launch verdicts goes through this
/// function.
pub async fn effective_providers_with_paths(
    xvn_home: &Path,
    config_path: &Path,
) -> ApiResult<Vec<EffectiveProvider>> {
    let cfg = load_cfg(config_path).await?;
    let secrets = load_providers_secrets(xvn_home).await?;
    let rows = cfg
        .providers
        .iter()
        .filter(|p| !p.name.starts_with('_'))
        .map(|entry| effective_from_entry(entry, &secrets))
        .collect();
    Ok(rows)
}

/// Resolve every configured provider's actual API key, keyed by provider
/// name. Uses the same env-first-then-secrets-file priority as
/// [`resolve_provider_key_value`], so a provider with a key only in
/// `secrets/providers.toml` (never exported to env) still resolves.
///
/// Providers with no usable key are simply omitted from the map. Used by
/// the Cortex memory embedder provisioning (`build_default_embedder`) to
/// build the pure resolver's `EmbedderEnv::resolved_provider_keys` without
/// duplicating the env/secrets precedence logic.
pub async fn resolved_provider_keys(
    xvn_home: &Path,
    config_path: &Path,
) -> ApiResult<std::collections::HashMap<String, String>> {
    let cfg = load_cfg(config_path).await?;
    let mut out = std::collections::HashMap::new();
    for entry in cfg.providers.iter().filter(|p| !p.name.starts_with('_')) {
        if let Some(key) = resolve_provider_key_value(xvn_home, entry).await? {
            if !key.is_empty() {
                out.insert(entry.name.clone(), key);
            }
        }
    }
    Ok(out)
}

/// Look up the launch verdict for a specific `(provider, model)` pair.
///
/// Returns `Ok(entry)` only when the helper's `launchable` predicate is
/// satisfied AND the requested `model` (if any) is in `enabled_models`
/// (or the provider is `local-candle`, which has no remote catalog).
///
/// On refusal, returns the typed `ProviderUnavailable` so callers can
/// surface a `reason` discriminant instead of pattern-matching a string.
pub async fn resolve_provider(
    ctx: &ApiContext,
    config_path: &Path,
    name: &str,
    model: Option<&str>,
) -> Result<ProviderEntry, ProviderUnavailable> {
    let cfg = match load_cfg(config_path).await {
        Ok(c) => c,
        Err(e) => {
            return Err(ProviderUnavailable {
                provider: name.to_string(),
                reason: ProviderUnavailableReason::ProviderUnknown,
                model: model.map(str::to_string),
                hint: format!("load runtime config: {e}"),
            });
        }
    };
    let entry = match cfg.providers.iter().find(|p| p.name == name) {
        Some(e) => e.clone(),
        None => {
            return Err(ProviderUnavailable {
                provider: name.to_string(),
                reason: ProviderUnavailableReason::ProviderUnknown,
                model: model.map(str::to_string),
                hint: format!(
                    "provider `{name}` is not in `[[providers]]`. Add it with `xvn provider add --name {name} …` or pick a configured provider/model for the strategy agent."
                ),
            });
        }
    };
    // Reserved/synthetic rows are not launchable from any surface.
    if entry.name.starts_with('_') {
        return Err(ProviderUnavailable {
            provider: name.to_string(),
            reason: ProviderUnavailableReason::ProviderDisabled,
            model: model.map(str::to_string),
            hint: format!("provider `{name}` is reserved/internal and cannot be used to launch eval"),
        });
    }
    // Key presence — env var OR stored secret OR no-auth kind.
    let secrets = match load_providers_secrets(&ctx.xvn_home).await {
        Ok(s) => s,
        Err(_) => ProvidersSecretsFile::default(),
    };
    let has_key = entry_has_key(&entry, &secrets);
    if !has_key && entry.kind != ProviderKind::LocalCandle {
        let env_hint = if entry.api_key_env.is_empty() {
            "no api_key_env configured for this provider; set one in Settings → Providers".to_string()
        } else {
            format!(
                "export {} or paste a key in Settings → Providers",
                entry.api_key_env
            )
        };
        return Err(ProviderUnavailable {
            provider: name.to_string(),
            reason: ProviderUnavailableReason::KeyMissing,
            model: model.map(str::to_string),
            hint: env_hint,
        });
    }
    // Model enablement — only applies when caller named a model. local-candle
    // bypasses the per-model gate (it has no remote catalog).
    if let Some(m) = model.map(str::trim).filter(|m| !m.is_empty()) {
        if entry.kind != ProviderKind::LocalCandle {
            let enabled = entry.enabled_models.iter().any(|enabled_id| enabled_id == m);
            if !enabled {
                let listing = if entry.enabled_models.is_empty() {
                    format!("no models are enabled for `{name}`; enable one in Settings → Providers → Manage models")
                } else {
                    format!("enabled models for `{name}`: {}", entry.enabled_models.join(", "))
                };
                return Err(ProviderUnavailable {
                    provider: name.to_string(),
                    reason: ProviderUnavailableReason::ModelDisabled,
                    model: Some(m.to_string()),
                    hint: listing,
                });
            }
        }
    }
    Ok(entry)
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

pub async fn show(ctx: &ApiContext, config_path: &Path, name: &str) -> ApiResult<ProviderRow> {
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

pub async fn add(ctx: &ApiContext, config_path: &Path, req: AddProviderRequest) -> ApiResult<ProviderRow> {
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

pub async fn update(
    ctx: &ApiContext,
    config_path: &Path,
    name: &str,
    req: UpdateProviderRequest,
) -> ApiResult<ProviderRow> {
    let started = Instant::now();
    let args = serde_json::to_string(&serde_json::json!({
        "kind": req.kind,
        "base_url": req.base_url,
        "api_key_env": req.api_key_env,
        "api_key_provided": req.api_key.as_ref().is_some_and(|k| !k.is_empty()),
        "enabled_models_count": req.enabled_models.as_ref().map(Vec::len),
    }))
    .ok();
    let result = update_inner(config_path, &ctx.xvn_home, name, req).await;

    let outcome = audit_outcome(&result);
    let _ = audit::record(
        ctx,
        "settings",
        "providers.update",
        Some(name),
        args.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

pub async fn remove(ctx: &ApiContext, config_path: &Path, name: &str) -> ApiResult<()> {
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

async fn fetch_models_inner(config_path: &Path, name: &str) -> ApiResult<ProviderModelsReport> {
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
    let no_auth_kind = matches!(
        entry.kind,
        ProviderKind::LocalCandle | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm
    );
    if api_key.is_empty() && !no_auth_kind {
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
        ProviderKind::OpenaiCompat | ProviderKind::Vllm => {
            fetch_openai_compat_models(&client, &base_url, &api_key).await?
        }
        ProviderKind::Ollama => fetch_ollama_provider_models(&client, &base_url, &api_key).await?,
        ProviderKind::LlamaCpp => fetch_openai_compat_models(&client, &base_url, &api_key).await?,
        ProviderKind::LocalCandle => {
            return Err(ApiError::Validation(
                "local-candle providers don't expose a catalog endpoint".into(),
            ));
        }
    };

    Ok(ProviderModelsReport { models })
}

async fn fetch_ollama_provider_models(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> ApiResult<Vec<ProviderModelEntry>> {
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
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
        return Err(ApiError::Validation(format!("GET {url} {status}: {body}")));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Internal(format!("parse {url}: {e}")))?;
    let arr = v["models"].as_array().cloned().unwrap_or_default();
    let mut out = Vec::with_capacity(arr.len());
    for m in arr {
        let id = m["name"].as_str().unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let family = m["details"]["family"].as_str().map(str::to_string);
        let param_size = m["details"]["parameter_size"].as_str().map(str::to_string);
        let display_name = match (family.as_deref(), param_size.as_deref()) {
            (Some(f), Some(p)) => Some(format!("{f} {p}")),
            _ => None,
        };
        out.push(ProviderModelEntry {
            id: id.to_string(),
            display_name,
            owned_by: None,
            context_length: None,
        });
    }
    Ok(out)
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
    let url = openai_compat_models_url(base_url);
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
        return Err(ApiError::Validation(format!("GET {url} {status}: {body}")));
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
    let (cfg, invalid_rows) = load_cfg_with_invalid(config_path).await?;
    let secrets = load_providers_secrets(xvn_home).await?;
    let providers = cfg
        .providers
        .iter()
        // Hide any historical internal rows so the empty-state is honest.
        .filter(|p| !p.name.starts_with('_'))
        .map(|p| row_from_entry(p, &cfg, &secrets))
        .collect();
    let default_model = {
        cfg.default_llm.as_ref().and_then(|default_llm| {
            let m = default_llm.model.trim();
            (!m.is_empty()).then(|| m.to_string())
        })
    };
    // Surface dropped rows so the UI can warn + offer removal. Internal `_`
    // rows are never operator-facing, so they don't appear here either.
    let invalid: Vec<InvalidProviderRow> = invalid_rows
        .into_iter()
        .filter(|p| !p.name.starts_with('_'))
        .map(|p| InvalidProviderRow {
            name: p.name,
            reason: p.reason,
        })
        .collect();
    Ok(ProvidersReport {
        providers,
        default_model,
        // None (omitted on the wire) in the normal case so consumers that
        // don't care never see the field.
        invalid: (!invalid.is_empty()).then_some(invalid),
    })
}

async fn show_inner(config_path: &Path, xvn_home: &Path, name: &str) -> ApiResult<ProviderRow> {
    let cfg = load_cfg(config_path).await?;
    let secrets = load_providers_secrets(xvn_home).await?;
    let entry = cfg
        .providers
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| ApiError::NotFound(format!("provider `{name}` not found")))?;
    Ok(row_from_entry(entry, &cfg, &secrets))
}

/// Write a settings file (provider config or secrets), mapping the
/// operator-fixable IO failures to a client-visible, actionable error instead of
/// an opaque 500 "internal error".
///
/// QA 2026-06-05: a config volume seeded by an older root-running image left
/// `default.toml` owned by root, so the non-root `xvision` runtime could read it
/// (Settings → Providers listed fine) but every provider add/edit failed with
/// `EACCES` on write — surfaced to the operator as a generic "internal error"
/// they couldn't diagnose. A permission/read-only failure is operator-fixable
/// (chown the volume / mount rw), so return a `Validation` error carrying the
/// actual cause and the fix, rather than masking it.
fn map_settings_write_err(path: &Path, e: &std::io::Error) -> ApiError {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => ApiError::Validation(format!(
            "could not save: `{}` is not writable ({e}). Its volume is likely owned by a \
             different user than the running process — `chown` it to the runtime user on the \
             host and retry.",
            path.display()
        )),
        _ => ApiError::Internal(format!("write {}: {e}", path.display())),
    }
}

fn write_settings_file(path: &Path, content: &str) -> ApiResult<()> {
    std::fs::write(path, content).map_err(|e| map_settings_write_err(path, &e))
}

async fn add_inner(config_path: &Path, xvn_home: &Path, req: AddProviderRequest) -> ApiResult<ProviderRow> {
    let AddProviderRequest {
        name,
        kind,
        base_url,
        api_key_env,
        api_key,
    } = req;

    let parsed_kind = parse_kind(&kind)?;
    // Normalize before any further checks or the disk write — a name with
    // surrounding whitespace should be stored trimmed, and validated trimmed.
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::Validation("name is empty".into()));
    }
    // Validate the name against the SAME `[a-z0-9-]+`, 1..=32, no-`_` rule the
    // config loader enforces, BEFORE writing to disk. Previously only empty /
    // `_`-prefix were checked here, so an invalid name like "Gemini" was
    // persisted to default.toml and only rejected by the post-write
    // load_runtime re-validation — corrupting the file so every subsequent
    // load failed. Reject up front instead. (Maps to HTTP 400.)
    if let Err(msg) = xvision_core::config::validate_provider_name_str(&name) {
        return Err(ApiError::Validation(msg));
    }
    // Require an API key for auth-bearing kinds, but only when the
    // operator hasn't already exported one via the env var (the CLI
    // `xvn provider add` flow assumes the env was set before the
    // command ran). Without this guard the route silently persisted
    // a row that surfaced in Settings → Providers as "missing key".
    let trimmed_key = api_key.as_deref().map(str::trim).unwrap_or("");
    let needs_api_key = !matches!(
        parsed_kind,
        ProviderKind::LocalCandle | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm
    );
    if trimmed_key.is_empty() && needs_api_key {
        // Compute the env var we'd use for this provider so the env
        // pre-set check matches what the daemon will read.
        let env_var = if api_key_env.trim().is_empty() {
            default_api_key_env_for(parsed_kind, &name)
        } else {
            api_key_env.clone()
        };
        let env_set = std::env::var(&env_var).map(|v| !v.is_empty()).unwrap_or(false);
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
        if providers
            .iter()
            .any(|t| t.get("name").and_then(|v| v.as_str()) == Some(&n))
        {
            return Err(ApiError::Conflict(format!("provider `{n}` already exists")));
        }
        let mut row = Table::new();
        row.insert("name", value(n));
        row.insert("kind", value(k));
        row.insert("base_url", value(b));
        row.insert("api_key_env", value(e));
        providers.push(row);

        write_settings_file(&path, &doc.to_string())?;
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

    // Re-validate the resulting config. Adding a provider does not promote it
    // to `[default_llm]`; defaults are explicit so zero-default workspaces stay
    // zero-default until the operator opts in.
    let _ = load_cfg(config_path).await?;

    show_inner(config_path, xvn_home, &name).await
}

async fn update_inner(
    config_path: &Path,
    xvn_home: &Path,
    name: &str,
    req: UpdateProviderRequest,
) -> ApiResult<ProviderRow> {
    let cfg = load_cfg(config_path).await?;
    let entry = cfg
        .providers
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| ApiError::NotFound(format!("provider `{name}` not found")))?;
    if entry.name.starts_with('_') {
        return Err(ApiError::Validation(format!(
            "cannot update internal provider `{name}`"
        )));
    }
    let parsed_kind = parse_kind(&req.kind)?;
    let trimmed_base_url = req.base_url.trim();
    if trimmed_base_url.is_empty() && parsed_kind != ProviderKind::LocalCandle {
        return Err(ApiError::Validation("base_url is empty".into()));
    }
    if !(trimmed_base_url.is_empty()
        || trimmed_base_url.starts_with("http://")
        || trimmed_base_url.starts_with("https://"))
    {
        return Err(ApiError::Validation(format!(
            "base_url must start with http:// or https:// (got `{trimmed_base_url}`)"
        )));
    }
    let trimmed_env = req.api_key_env.trim().to_string();
    let requires_env = !matches!(
        parsed_kind,
        ProviderKind::LocalCandle | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm
    );
    if trimmed_env.is_empty() && requires_env {
        return Err(ApiError::Validation(
            "api_key_env is required for auth-bearing providers".into(),
        ));
    }
    if let Some(models) = req.enabled_models.as_ref() {
        validate_model_ids(models)?;
    }
    let was_default = provider_matches_default(entry, &cfg);

    let path: PathBuf = config_path.to_path_buf();
    let n = name.to_string();
    let kind_str = req.kind.clone();
    let base_url = trimmed_base_url.to_string();
    let api_key_env = trimmed_env.clone();
    let enabled_models = req.enabled_models.clone().map(dedup_model_ids);
    let default_model = cfg.default_llm.as_ref().map(|d| d.model.clone());
    task::spawn_blocking(move || -> ApiResult<()> {
        use toml_edit::{value, Array, DocumentMut};
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| ApiError::Internal(format!("read {}: {e}", path.display())))?;
        let mut doc: DocumentMut = raw
            .parse()
            .map_err(|e| ApiError::Internal(format!("parse {}: {e}", path.display())))?;
        if let Some(toml_edit::Item::ArrayOfTables(arr)) = doc.get_mut("providers") {
            let mut matched = false;
            for tbl in arr.iter_mut() {
                if tbl.get("name").and_then(|v| v.as_str()) == Some(&n) {
                    tbl.insert("kind", value(kind_str.clone()));
                    tbl.insert("base_url", value(base_url.clone()));
                    tbl.insert("api_key_env", value(api_key_env.clone()));
                    if let Some(models) = &enabled_models {
                        let mut arr = Array::new();
                        for model in models {
                            arr.push(model.as_str());
                        }
                        tbl.insert("enabled_models", value(arr));
                    }
                    matched = true;
                    break;
                }
            }
            if !matched {
                return Err(ApiError::NotFound(format!(
                    "provider `{n}` not found in TOML (race / internal row)"
                )));
            }
        } else {
            return Err(ApiError::Validation(format!(
                "no [[providers]] block in {}",
                path.display()
            )));
        }
        if was_default {
            write_default_llm(
                &mut doc,
                parsed_kind,
                &base_url,
                &api_key_env,
                default_model.as_deref(),
            )?;
        }
        write_settings_file(&path, &doc.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))??;

    if let Some(key) = req.api_key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        upsert_provider_secret(xvn_home, name, &trimmed_env, key).await?;
        std::env::set_var(&trimmed_env, key);
    }

    let _ = load_cfg(config_path).await?;
    show_inner(config_path, xvn_home, name).await
}

async fn remove_inner(config_path: &Path, xvn_home: &Path, name: &str) -> ApiResult<()> {
    // Lenient load so an INVALID row (e.g. a hand-edited uppercase name) is
    // still removable — that's the whole point of the self-heal path. The row
    // won't appear in `cfg.providers` (it was dropped), so we also check the
    // reported `invalid` list to confirm it exists before touching the TOML.
    let (cfg, invalid) = load_cfg_with_invalid(config_path).await?;
    let valid_entry = cfg.providers.iter().find(|p| p.name == name);
    let is_invalid_row = invalid.iter().any(|p| p.name == name);
    if valid_entry.is_none() && !is_invalid_row {
        return Err(ApiError::NotFound(format!("provider `{name}` not found")));
    }
    if name.starts_with('_') {
        return Err(ApiError::Validation(format!(
            "cannot remove internal provider `{name}`"
        )));
    }
    let was_default = valid_entry
        .map(|entry| provider_matches_default(entry, &cfg))
        .unwrap_or(false);

    let path: PathBuf = config_path.to_path_buf();
    let n = name.to_string();
    task::spawn_blocking(move || -> ApiResult<()> {
        use toml_edit::DocumentMut;
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| ApiError::Internal(format!("read {}: {e}", path.display())))?;
        let mut doc: DocumentMut = raw
            .parse()
            .map_err(|e| ApiError::Internal(format!("parse {}: {e}", path.display())))?;
        if let Some(toml_edit::Item::ArrayOfTables(arr)) = doc.get_mut("providers") {
            let before = arr.len();
            arr.retain(|t| t.get("name").and_then(|v| v.as_str()) != Some(&n));
            if arr.len() == before {
                return Err(ApiError::NotFound(format!(
                    "provider `{n}` not found in TOML (race / internal row)"
                )));
            }
        } else {
            return Err(ApiError::Validation(format!(
                "no [[providers]] block in {}",
                path.display()
            )));
        }
        if was_default {
            clear_default_llm(&mut doc);
        }
        write_settings_file(&path, &doc.to_string())?;
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
    validate_model_ids(&models)?;
    let deduped = dedup_model_ids(models);

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
        write_settings_file(&path, &doc.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))??;

    // Re-validate and re-emit the canonical row so the caller can render
    // the new state without an extra GET.
    let _ = load_cfg(config_path).await?;
    show_inner(config_path, xvn_home, name).await
}

async fn set_default_inner(config_path: &Path, name: &str, model: Option<&str>) -> ApiResult<()> {
    let cfg = load_cfg(config_path).await?;
    let entry = cfg
        .providers
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| ApiError::NotFound(format!("provider `{name}` not found")))?;
    if entry.name.starts_with('_') {
        return Err(ApiError::Validation(format!(
            "cannot set default to internal provider `{name}`"
        )));
    }
    let new_kind = entry.kind;
    let new_base = entry.base_url.clone();
    let new_env = entry.api_key_env.clone();

    let model_owned = model
        .map(str::to_string)
        .or_else(|| cfg.default_llm.as_ref().map(|d| d.model.clone()))
        .or_else(|| sensible_default_model(new_kind, name).map(str::to_string));
    let path: PathBuf = config_path.to_path_buf();
    task::spawn_blocking(move || -> ApiResult<()> {
        use toml_edit::DocumentMut;
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| ApiError::Internal(format!("read {}: {e}", path.display())))?;
        let mut doc: DocumentMut = raw
            .parse()
            .map_err(|e| ApiError::Internal(format!("parse {}: {e}", path.display())))?;
        write_default_llm(&mut doc, new_kind, &new_base, &new_env, model_owned.as_deref())?;
        write_settings_file(&path, &doc.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))??;

    // Re-validate the resulting config.
    let _ = load_cfg(config_path).await?;
    Ok(())
}

// --- helpers ---------------------------------------------------------------

/// Load the runtime config for the provider surface, DROPPING any
/// individually-invalid `[[providers]]` rows (see
/// [`xvision_core::config::load_runtime_lenient`]). Every read/mutate path in
/// this module goes through here so a single malformed provider row can never
/// blank the whole list or wedge the config — the offending row is excluded
/// (and stays removable; see `remove_inner`). Non-provider config errors still
/// fail loudly.
async fn load_cfg(config_path: &Path) -> ApiResult<RuntimeConfig> {
    Ok(load_cfg_with_invalid(config_path).await?.0)
}

/// Same lenient load as [`load_cfg`], but also returns the dropped rows so the
/// list endpoint can surface them to the operator for repair/removal.
async fn load_cfg_with_invalid(
    config_path: &Path,
) -> ApiResult<(RuntimeConfig, Vec<xvision_core::config::InvalidProvider>)> {
    let path = config_path.to_path_buf();
    task::spawn_blocking(move || xvision_core::config::load_runtime_lenient(&path))
        .await
        .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))?
        .map_err(|e| ApiError::Validation(format!("load config: {e}")))
}

/// True iff the provider has a usable API key — stored secret OR env-exported
/// OR a no-auth kind that needs none. Shared by `row_from_entry`,
/// `effective_from_entry`, and `resolve_provider` so all three surfaces
/// agree on what "key set" means.
fn entry_has_key(entry: &ProviderEntry, secrets: &ProvidersSecretsFile) -> bool {
    if entry.kind == ProviderKind::LocalCandle {
        // No-auth kind — always "has key" for launchability purposes.
        return true;
    }
    // Ollama and LlamaCpp treat an empty api_key_env as no-auth (optional key).
    let optional_auth = matches!(
        entry.kind,
        ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm
    );
    if entry.api_key_env.is_empty() {
        return optional_auth;
    }
    secrets.provider.contains_key(&entry.name)
        || std::env::var(&entry.api_key_env)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
}

/// Build the canonical `EffectiveProvider` view for one row. Shared by
/// `effective_providers` (engine) and any future caller that needs the
/// same rollup off a single `ProviderEntry`.
fn effective_from_entry(entry: &ProviderEntry, secrets: &ProvidersSecretsFile) -> EffectiveProvider {
    let has_key = entry_has_key(entry, secrets);
    let models: Vec<EffectiveProviderModel> = entry
        .enabled_models
        .iter()
        .map(|id| EffectiveProviderModel {
            id: id.clone(),
            // Today every entry in `enabled_models` is enabled. The wire
            // shape carries the per-model bool so a future "disable
            // individual models" toggle doesn't break consumers.
            enabled: true,
        })
        .collect();
    // For now `enabled` mirrors "row exists and is non-synthetic". Filtered
    // upstream in `effective_providers`; setting `true` here keeps the
    // field meaningful if a caller hands a synthetic row directly.
    let enabled = !entry.name.starts_with('_');
    let has_enabled_model = !models.is_empty();
    // Local-candle has no remote catalog — launchability for it skips
    // the per-model gate. For network kinds we require at least one model.
    let launchable = enabled && has_key && (entry.kind == ProviderKind::LocalCandle || has_enabled_model);
    EffectiveProvider {
        provider: entry.name.clone(),
        kind: kind_to_str(entry.kind).into(),
        base_url: entry.base_url.clone(),
        api_key_env: entry.api_key_env.clone(),
        expected_api_key_env: default_api_key_env_for(entry.kind, &entry.name),
        enabled,
        has_key,
        models,
        launchable,
    }
}

fn row_from_entry(entry: &ProviderEntry, cfg: &RuntimeConfig, secrets: &ProvidersSecretsFile) -> ProviderRow {
    // `api_key_set` answers "is there an API key actually configured for this
    // provider?" — distinct from launchability. A no-auth kind (vLLM, Ollama,
    // llama-cpp) with an empty api_key_env and no stored secret has no key set
    // (api_key_set=false), even though it is still launchable without one.
    // entry_has_key covers launchability (used by effective_from_entry and
    // resolve_provider); here we compute the narrower "key is present" flag.
    let api_key_set = if entry.api_key_env.is_empty() {
        // No env var configured → the only possible key source is a stored secret.
        secrets.provider.contains_key(&entry.name)
    } else {
        // Env var configured → check env first, then stored secret.
        secrets.provider.contains_key(&entry.name)
            || std::env::var(&entry.api_key_env)
                .map(|v| !v.is_empty())
                .unwrap_or(false)
    };
    let is_default = provider_matches_default(entry, cfg);
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

fn provider_matches_default(entry: &ProviderEntry, cfg: &RuntimeConfig) -> bool {
    cfg.default_llm
        .as_ref()
        .map(|default_llm| {
            let kind: ProviderKind = default_llm.provider.into();
            entry.matches_triple(kind, &default_llm.base_url, &default_llm.api_key_env)
        })
        .unwrap_or(false)
}

fn clear_default_llm(doc: &mut toml_edit::DocumentMut) {
    doc.remove("default_llm");
}

fn validate_model_ids(models: &[String]) -> ApiResult<()> {
    for model in models {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return Err(ApiError::Validation("empty model id in list".into()));
        }
        if trimmed.len() > 256 {
            let prefix: String = trimmed.chars().take(40).collect();
            return Err(ApiError::Validation(format!(
                "model id too long ({} chars): `{prefix}`",
                trimmed.len()
            )));
        }
    }
    Ok(())
}

fn dedup_model_ids(models: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    models
        .into_iter()
        .filter(|model| seen.insert(model.clone()))
        .collect()
}

fn write_default_llm(
    doc: &mut toml_edit::DocumentMut,
    kind: ProviderKind,
    base_url: &str,
    api_key_env: &str,
    model: Option<&str>,
) -> ApiResult<()> {
    use toml_edit::{value, Item};

    let default_llm = doc
        .entry("default_llm")
        .or_insert(Item::Table(Default::default()))
        .as_table_mut()
        .ok_or_else(|| ApiError::Validation("[default_llm] is not a table".into()))?;
    default_llm.insert("provider", value(kind_to_str(kind)));
    default_llm.insert("base_url", value(base_url));
    default_llm.insert("api_key_env", value(api_key_env));
    if let Some(m) = model {
        default_llm.insert("model", value(m));
    }
    if default_llm.get("model").is_none() {
        default_llm.insert("model", value(""));
    }
    if default_llm.get("temperature").is_none() {
        default_llm.insert("temperature", value(0.0));
    }
    if default_llm.get("max_tokens").is_none() {
        default_llm.insert("max_tokens", value(1024));
    }
    Ok(())
}

/// Conventional env var name for a (kind, name) tuple. Matches the names
/// most SDKs / docs use so existing shell setups still work.
/// The env var the daemon reads this provider's API key from, by
/// convention, when the operator hasn't overridden `api_key_env`.
///
/// Made `pub` so operator-facing surfaces can NAME the expected variable
/// in error messages and listings (QA U8): the `xvn optimize`/`provider`
/// CLI and `provider list` render this so a missing-key failure tells the
/// operator exactly which `XVN_PROVIDER_…_KEY` to export. No-auth local
/// kinds (local-candle / ollama / llama-cpp / vllm) return `""` because
/// they need no key by default.
/// Build the operator-facing "provider key not found" message for a
/// resolved provider, NAMING the env var the operator must export (QA U8).
///
/// Prefers the provider's configured `api_key_env`; falls back to the
/// kind/name convention via [`default_api_key_env_for`] when the row has
/// no explicit `api_key_env` (e.g. an Ollama row that nonetheless needs a
/// key for its custom endpoint). Renders, for an Ollama provider whose
/// env is `XVN_PROVIDER_OLLAMA_LOCAL_KEY`:
///
/// ```text
/// Ollama provider key not found in environment. Set
/// XVN_PROVIDER_OLLAMA_LOCAL_KEY=<key> or add it to providers.toml
/// ```
///
/// Exposed for the `xvn optimize` / dispatch path (consolidation agent),
/// which previously emitted an error that didn't tell the operator which
/// variable to set.
pub fn missing_provider_key_message(kind: ProviderKind, name: &str, api_key_env: &str) -> String {
    let env_var = if api_key_env.trim().is_empty() {
        default_api_key_env_for(kind, name)
    } else {
        api_key_env.trim().to_string()
    };
    // Human-readable kind label for the leading clause.
    let label = match kind {
        ProviderKind::Anthropic => "Anthropic",
        ProviderKind::OpenaiCompat => "OpenAI-compatible",
        ProviderKind::LocalCandle => "local-candle",
        ProviderKind::Ollama => "Ollama",
        ProviderKind::LlamaCpp => "llama.cpp",
        ProviderKind::Vllm => "vLLM",
    };
    if env_var.is_empty() {
        format!(
            "{label} provider `{name}` key not found in environment, and this provider has no \
             api_key_env configured. Set one in Settings → Providers or add the key to providers.toml"
        )
    } else {
        format!(
            "{label} provider key not found in environment. Set {env_var}=<key> or add it to providers.toml"
        )
    }
}

pub fn default_api_key_env_for(kind: ProviderKind, name: &str) -> String {
    match kind {
        ProviderKind::Anthropic => "ANTHROPIC_API_KEY".to_string(),
        ProviderKind::OpenaiCompat if name == "openai" => "OPENAI_API_KEY".to_string(),
        // Conventional env vars for named OpenAI-compatible presets so the
        // add-flow env matches the UI presets without seeding provider rows.
        ProviderKind::OpenaiCompat if name == "gemini" => "GEMINI_API_KEY".to_string(),
        ProviderKind::OpenaiCompat if name == "nous-research" => "NOUS_API_KEY".to_string(),
        ProviderKind::OpenaiCompat => {
            format!("XVN_PROVIDER_{}_KEY", name.to_ascii_uppercase().replace('-', "_"))
        }
        ProviderKind::LocalCandle | ProviderKind::Ollama | ProviderKind::LlamaCpp | ProviderKind::Vllm => {
            String::new()
        }
    }
}

/// Best-effort model id for a `(kind, name)` provider — mirrors the
/// fallbacks the dashboard chat-rail dropdown uses so the wizard
/// (which lacks a model picker) gets a working model out of the box.
fn sensible_default_model(kind: ProviderKind, name: &str) -> Option<&'static str> {
    match kind {
        ProviderKind::Anthropic => Some("claude-sonnet-4-6"),
        ProviderKind::OpenaiCompat => match name {
            // Prefer the newer DeepSeek id when an operator explicitly sets
            // this provider as the workspace default.
            "deepseek" => Some("deepseek-v4-flash"),
            "groq" => Some("llama-3.3-70b-versatile"),
            "openrouter" => Some("anthropic/claude-3.5-sonnet"),
            "openai" => Some("gpt-4o-mini"),
            // Named presets — pick a current default so "Set as default"
            // yields a working model without a manual catalog pick. Operators
            // can override via Settings → Providers → Manage models.
            "gemini" => Some("gemini-2.5-flash"),
            "nous-research" => Some("Hermes-4-405B"),
            _ => None,
        },
        ProviderKind::Vllm => None,
        ProviderKind::LocalCandle | ProviderKind::Ollama | ProviderKind::LlamaCpp => None,
    }
}

fn default_base_url_for(kind: ProviderKind, _name: &str) -> &'static str {
    match kind {
        ProviderKind::Anthropic => "https://api.anthropic.com",
        ProviderKind::OpenaiCompat => "https://api.openai.com/v1",
        ProviderKind::Ollama => "http://localhost:11434",
        ProviderKind::LlamaCpp => "http://localhost:8080",
        ProviderKind::Vllm => "http://localhost:8000/v1",
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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ProvidersSecretsFile::default()),
        Err(e) => Err(ApiError::Internal(format!("read {}: {e}", path.display()))),
    }
}

async fn save_providers_secrets(xvn_home: &Path, file: &ProvidersSecretsFile) -> ApiResult<()> {
    let path = providers_secrets_path(xvn_home);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ApiError::Internal(format!("mkdir {}: {e}", parent.display())))?;
    }
    let serialized = toml::to_string_pretty(file)
        .map_err(|e| ApiError::Internal(format!("serialize providers secrets: {e}")))?;
    tokio::fs::write(&path, serialized)
        .await
        .map_err(|e| map_settings_write_err(&path, &e))?;
    set_owner_only(&path)?;
    Ok(())
}

async fn upsert_provider_secret(xvn_home: &Path, name: &str, env_var: &str, api_key: &str) -> ApiResult<()> {
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
    std::fs::set_permissions(path, perms)
        .map_err(|e| ApiError::Internal(format!("chmod 600 {}: {e}", path.display())))
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

/// Resolve the actual API key value for a provider entry, using the same
/// env-first-then-secrets-file priority that `provider check` / `provider
/// list` use to decide `api_key_set` (see [`entry_has_key`] and
/// [`row_from_entry`]).
///
/// Priority:
/// 1. The configured `api_key_env` env var, if set and non-empty (env wins,
///    so CI / one-shot scripts stay deterministic).
/// 2. The stored secret in `$XVN_HOME/secrets/providers.toml` keyed by
///    provider name, if present and non-empty.
///
/// Returns `Ok(None)` only when neither source yields a key. Callers decide
/// whether a missing key is fatal (network kinds) or acceptable (no-auth
/// local kinds). This closes the divergence where the eval/optimizer RUN
/// path read the env var ONLY and failed with "no API key" even though the
/// secrets file held a valid key (`provider check` could see it but eval
/// could not).
pub async fn resolve_provider_key_value(xvn_home: &Path, entry: &ProviderEntry) -> ApiResult<Option<String>> {
    // 1. Env var first (highest priority).
    if !entry.api_key_env.is_empty() {
        if let Ok(v) = std::env::var(&entry.api_key_env) {
            if !v.is_empty() {
                return Ok(Some(v));
            }
        }
    }
    // 2. Fall back to the stored secret, keyed by provider name — the same
    //    file `provider check` reads.
    let secrets = load_providers_secrets(xvn_home).await?;
    if let Some(secret) = secrets.provider.get(&entry.name) {
        if !secret.api_key.is_empty() {
            return Ok(Some(secret.api_key.clone()));
        }
    }
    Ok(None)
}

fn kind_to_str(k: ProviderKind) -> &'static str {
    match k {
        ProviderKind::Anthropic => "anthropic",
        ProviderKind::OpenaiCompat => "openai-compat",
        ProviderKind::LocalCandle => "local-candle",
        ProviderKind::Ollama => "ollama",
        ProviderKind::LlamaCpp => "llama-cpp",
        ProviderKind::Vllm => "vllm",
    }
}

fn parse_kind(s: &str) -> ApiResult<ProviderKind> {
    match s {
        "anthropic" => Ok(ProviderKind::Anthropic),
        "openai-compat" => Ok(ProviderKind::OpenaiCompat),
        "local-candle" => Ok(ProviderKind::LocalCandle),
        "ollama" => Ok(ProviderKind::Ollama),
        "llama-cpp" => Ok(ProviderKind::LlamaCpp),
        "vllm" => Ok(ProviderKind::Vllm),
        other => Err(ApiError::Validation(format!(
            "invalid kind `{other}`; must be one of: anthropic | openai-compat | local-candle | ollama | llama-cpp | vllm"
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

    const MIN_CONFIG_NO_PROVIDERS: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

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

    // Mirrors the live xvn default.toml that reproduces the UI 500: a
    // [default_llm] referencing a provider absent from [[providers]], two
    // openai-compat providers (one with a `~`-prefixed enabled model).
    const DEPLOYED_LIKE_CONFIG: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[default_llm]
provider          = "anthropic"
base_url          = "https://api.anthropic.com"
model             = "claude-haiku-4-5"
api_key_env       = "ANTHROPIC_API_KEY"
temperature       = 0.0
reasoning_effort  = "low"
max_tokens        = 1024

[trader]
model_path = "models/x.gguf"
temperature = 0.0
forward_paper_temperature = 0.4
max_tokens = 512
[trader.vectors]
enabled = true
config = "off"

[backtest]
step = 24
horizon = 16
bootstrap_resamples = 10000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://data/decisions.db"

[data.alpaca]
rate_limit_rpm = 200

[[providers]]
name = "openrouter"
kind = "openai-compat"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "XVN_PROVIDER_OPENROUTER_KEY"
enabled_models = ["google/gemini-3.1-flash-lite", "deepseek/deepseek-v4-flash", "~openai/gpt-mini-latest"]

[[providers]]
name = "deepseek"
kind = "openai-compat"
base_url = "https://api.deepseek.com"
api_key_env = "XVN_PROVIDER_DEEPSEEK_KEY"
enabled_models = ["deepseek-v4-pro", "deepseek-v4-flash"]
"#;

    async fn ctx_in(dir: &TempDir) -> ApiContext {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        // Mirror engine api migrations so audit.record works in tests.
        sqlx::query(include_str!("../../../migrations/001_api_audit.sql"))
            .execute(&pool)
            .await
            .unwrap();
        ApiContext::new(pool, Actor::Cli { user: "test".into() }, dir.path().to_path_buf())
    }

    fn write_min_config(dir: &TempDir) -> std::path::PathBuf {
        let p = dir.path().join("default.toml");
        std::fs::write(&p, MIN_CONFIG).unwrap();
        p
    }

    fn write_min_config_no_providers(dir: &TempDir) -> std::path::PathBuf {
        let p = dir.path().join("default.toml");
        std::fs::write(&p, MIN_CONFIG_NO_PROVIDERS).unwrap();
        p
    }

    #[tokio::test]
    async fn list_returns_configured_anthropic_row() {
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
    async fn list_returns_no_rows_when_config_has_no_providers() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config_no_providers(&dir);
        let ctx = ctx_in(&dir).await;
        let report = list(&ctx, &path).await.unwrap();
        assert!(report.providers.is_empty());
        assert_eq!(report.default_model.as_deref(), Some("x"));
    }

    #[tokio::test]
    async fn add_creates_first_provider_when_config_has_no_providers_block() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config_no_providers(&dir);
        let ctx = ctx_in(&dir).await;
        let row = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "ollama".into(),
                kind: "ollama".into(),
                base_url: "".into(),
                api_key_env: "".into(),
                api_key: None,
            },
        )
        .await
        .expect("first provider add must create [[providers]]");
        assert_eq!(row.name, "ollama");
        assert_eq!(row.base_url, "http://localhost:11434");
        let report = list(&ctx, &path).await.unwrap();
        assert_eq!(report.providers.len(), 1);
        assert_eq!(report.providers[0].name, "ollama");
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
    async fn add_ollama_with_empty_key_succeeds() {
        // Repro for the UI "internal: internal error" on adding Ollama: the
        // form posts kind="ollama", base_url="http://localhost:11434", and an
        // EMPTY api_key + EMPTY api_key_env (Ollama needs no auth). This must
        // succeed, not 500.
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let res = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "ollama".into(),
                kind: "ollama".into(),
                base_url: "http://localhost:11434".into(),
                api_key_env: "".into(),
                api_key: Some("".into()),
            },
        )
        .await;
        let row = res.expect("ollama add must succeed");
        assert_eq!(row.name, "ollama");
        assert_eq!(row.kind, "ollama");
    }

    #[tokio::test]
    async fn add_vllm_with_empty_key_succeeds_and_defaults_to_localhost() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let row = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "vllm".into(),
                kind: "vllm".into(),
                base_url: "".into(),
                api_key_env: "".into(),
                api_key: None,
            },
        )
        .await
        .expect("vLLM add must support no-auth local servers");
        assert_eq!(row.name, "vllm");
        assert_eq!(row.kind, "vllm");
        assert_eq!(row.base_url, "http://localhost:8000/v1");
        assert_eq!(row.api_key_env, "");
        assert!(!row.api_key_set);
    }

    #[tokio::test]
    async fn add_ollama_against_deployed_like_state() {
        // Faithful repro of the live xvn config that 500s on "add Ollama":
        // [default_llm] points at a provider NOT in [[providers]], two
        // openai-compat providers, and an orphaned/invalid-named secrets file
        // (Gemini / "Gemini Custom" left behind by an older buggy add path).
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("default.toml");
        std::fs::write(&path, DEPLOYED_LIKE_CONFIG).unwrap();
        std::fs::create_dir_all(dir.path().join("secrets")).unwrap();
        std::fs::write(
            dir.path().join("secrets").join("providers.toml"),
            r#"
[provider.Gemini]
env_var = "GEMINI_API_KEY"
api_key = "x"

[provider."Gemini Custom"]
env_var = "XVN_PROVIDER_GEMINI_CUSTOM_KEY"
api_key = "x"

[provider.deepseek]
env_var = "XVN_PROVIDER_DEEPSEEK_KEY"
api_key = "x"

[provider.openrouter]
env_var = "XVN_PROVIDER_OPENROUTER_KEY"
api_key = "x"
"#,
        )
        .unwrap();
        let ctx = ctx_in(&dir).await;
        let res = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "ollama".into(),
                kind: "ollama".into(),
                base_url: "http://localhost:11434".into(),
                api_key_env: "".into(),
                api_key: Some("".into()),
            },
        )
        .await;
        let row = res.expect("ollama add must succeed against deployed-like state");
        assert_eq!(row.name, "ollama");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn add_on_unwritable_config_returns_actionable_error_not_internal() {
        // QA 2026-06-05 root cause: a root-owned (unwritable) default.toml made
        // the non-root runtime user fail every provider edit with EACCES, masked
        // as a generic 500 "internal error". A non-writable config must now yield
        // an actionable Validation error naming the file + fix — never Internal.
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        // Read-only: owner can still READ (list works) but WRITE hits EACCES,
        // exactly like the root-owned file under a non-root runtime user.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
        let ctx = ctx_in(&dir).await;
        let err = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "ollama".into(),
                kind: "ollama".into(),
                base_url: "http://localhost:11434".into(),
                api_key_env: "".into(),
                api_key: Some("".into()),
            },
        )
        .await
        .unwrap_err();
        match err {
            ApiError::Validation(msg) => {
                assert!(
                    msg.contains("not writable") && msg.contains("chown"),
                    "actionable message expected, got: {msg}"
                );
            }
            other => panic!("expected actionable Validation error, got {other:?}"),
        }
        // Restore perms so TempDir cleanup can remove the file.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
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

    /// Regression for the config-corruption bug: an invalid (uppercase) name
    /// must be rejected with a Validation error AND must NOT be written to
    /// default.toml. Before the fix, `add_inner` persisted the bad row first,
    /// then the post-write re-validation failed — leaving the file permanently
    /// unloadable.
    #[tokio::test]
    async fn add_rejects_uppercase_name_without_corrupting_config() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let before = std::fs::read_to_string(&path).unwrap();

        let err = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "Gemini".into(),
                kind: "openai-compat".into(),
                base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
                api_key_env: "GEMINI_API_KEY".into(),
                api_key: Some("k".into()),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");

        // The file is byte-unchanged and still loads — no corruption.
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(before, after, "default.toml was mutated by a rejected add");
        let report = list(&ctx, &path).await.expect("config still loads");
        assert_eq!(report.providers.len(), 1, "only the seeded anthropic row remains");
    }

    #[tokio::test]
    async fn add_rejects_name_over_32_chars() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let err = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "a".repeat(33),
                kind: "openai-compat".into(),
                base_url: "https://x.example.com/v1".into(),
                api_key_env: "K".into(),
                api_key: Some("k".into()),
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)), "got {err:?}");
    }

    /// A name with surrounding whitespace that trims to a valid slug is
    /// accepted and stored trimmed.
    #[tokio::test]
    async fn add_trims_surrounding_whitespace_in_name() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let row = add(
            &ctx,
            &path,
            AddProviderRequest {
                name: "  gemini  ".into(),
                kind: "openai-compat".into(),
                base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
                api_key_env: "GEMINI_API_KEY".into(),
                api_key: Some("k".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(row.name, "gemini");
        // Config still loads with both rows.
        let report = list(&ctx, &path).await.unwrap();
        assert_eq!(report.providers.len(), 2);
    }

    /// WU6 resilience: a single invalid `[[providers]]` row must NOT blank the
    /// list — the valid rows still load and the bad row is surfaced for repair.
    #[tokio::test]
    async fn list_surfaces_invalid_rows_instead_of_blanking() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("default.toml");
        let cfg = format!(
            "{MIN_CONFIG}\n[[providers]]\nname = \"Gemini\"\nkind = \"openai-compat\"\nbase_url = \"https://x.example.com/v1\"\napi_key_env = \"K\"\n"
        );
        std::fs::write(&path, &cfg).unwrap();
        let ctx = ctx_in(&dir).await;

        let report = list(&ctx, &path)
            .await
            .expect("list must not hard-fail on a single bad row");
        assert_eq!(report.providers.len(), 1, "valid anthropic row still listed");
        assert_eq!(report.providers[0].name, "anthropic");
        let invalid = report.invalid.expect("bad row surfaced, not hidden");
        assert_eq!(invalid.len(), 1);
        assert_eq!(invalid[0].name, "Gemini");
    }

    /// WU6 self-heal: the invalid row must be removable via the API so the
    /// operator can fix a corrupted config without hand-editing the file.
    #[tokio::test]
    async fn remove_can_delete_an_invalid_row() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("default.toml");
        let cfg = format!(
            "{MIN_CONFIG}\n[[providers]]\nname = \"Gemini\"\nkind = \"openai-compat\"\nbase_url = \"https://x.example.com/v1\"\napi_key_env = \"K\"\n"
        );
        std::fs::write(&path, &cfg).unwrap();
        let ctx = ctx_in(&dir).await;

        remove(&ctx, &path, "Gemini")
            .await
            .expect("an invalid row must be removable");
        let report = list(&ctx, &path).await.unwrap();
        assert!(report.invalid.is_none(), "bad row gone after removal");
        assert_eq!(report.providers.len(), 1);
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
    async fn remove_clears_default_when_referenced_provider_is_deleted() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        remove(&ctx, &path, "anthropic").await.unwrap();
        let report = list(&ctx, &path).await.unwrap();
        assert!(report.providers.is_empty());
        assert_eq!(report.default_model, None);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("[default_llm]"));
        assert!(!raw.contains("[intern]"));
    }

    #[tokio::test]
    async fn update_edits_provider_and_keeps_default_pointer() {
        let dir = TempDir::new().unwrap();
        let path = write_min_config(&dir);
        let ctx = ctx_in(&dir).await;
        let row = update(
            &ctx,
            &path,
            "anthropic",
            UpdateProviderRequest {
                kind: "anthropic".into(),
                base_url: "https://proxy.example/v1".into(),
                api_key_env: "ANTHROPIC_PROXY_KEY".into(),
                api_key: Some("sk-updated".into()),
                enabled_models: Some(vec!["claude-sonnet-4-6".into()]),
            },
        )
        .await
        .unwrap();
        assert_eq!(row.base_url, "https://proxy.example/v1");
        assert_eq!(row.enabled_models, vec!["claude-sonnet-4-6"]);
        assert!(row.is_default);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("base_url = \"https://proxy.example/v1\""));
        assert!(raw.contains("api_key_env = \"ANTHROPIC_PROXY_KEY\""));
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

    fn entry_for(name: &str, env_var: &str) -> ProviderEntry {
        ProviderEntry {
            name: name.to_string(),
            kind: ProviderKind::OpenaiCompat,
            base_url: "https://api.example.com/v1".into(),
            api_key_env: env_var.to_string(),
            enabled_models: vec!["google/gemini-3.1-flash-lite".into()],
        }
    }

    // T3 regression: the eval/optimizer RUN path used to read the provider key
    // from the env var ONLY, so a fresh container (no key bridged into env)
    // failed even when the key was persisted in `secrets/providers.toml` —
    // which `provider check` could already see. `resolve_provider_key_value`
    // now falls back to that file, closing the divergence.
    #[tokio::test]
    async fn resolve_key_falls_back_to_secrets_file_when_env_unset() {
        let dir = TempDir::new().unwrap();
        // Unique env var name so we don't collide with any real/other-test
        // process env. Ensure it is UNSET — this is the fresh-container case.
        let env_var = "XVN_PROVIDER_T3_OPENROUTER_KEY";
        std::env::remove_var(env_var);

        let entry = entry_for("openrouter", env_var);
        // Persist the key in the same secrets file `provider check` reads.
        upsert_provider_secret(dir.path(), "openrouter", env_var, "sk-secret-from-file")
            .await
            .unwrap();

        // Before the fix this returned None/errored (env-only). Now it
        // resolves from the file.
        let resolved = resolve_provider_key_value(dir.path(), &entry).await.unwrap();
        assert_eq!(resolved.as_deref(), Some("sk-secret-from-file"));
    }

    // Env var still takes priority over the stored secret (env overrides file).
    #[tokio::test]
    async fn resolve_key_prefers_env_over_secrets_file() {
        let dir = TempDir::new().unwrap();
        let env_var = "XVN_PROVIDER_T3_PRIORITY_KEY";
        std::env::set_var(env_var, "sk-from-env");

        let entry = entry_for("priorityprov", env_var);
        upsert_provider_secret(dir.path(), "priorityprov", env_var, "sk-from-file")
            .await
            .unwrap();

        let resolved = resolve_provider_key_value(dir.path(), &entry).await.unwrap();
        assert_eq!(resolved.as_deref(), Some("sk-from-env"));
        std::env::remove_var(env_var);
    }

    // Only when BOTH env and file lack the key do we return None (the caller
    // then emits the accurate "no API key … and no key stored" error).
    #[tokio::test]
    async fn resolve_key_returns_none_when_env_and_file_both_missing() {
        let dir = TempDir::new().unwrap();
        let env_var = "XVN_PROVIDER_T3_ABSENT_KEY";
        std::env::remove_var(env_var);

        let entry = entry_for("absentprov", env_var);
        // No secrets file written at all (NotFound → default empty map).
        let resolved = resolve_provider_key_value(dir.path(), &entry).await.unwrap();
        assert!(resolved.is_none(), "got {resolved:?}");
    }

    // --- QA U8: env-var naming -------------------------------------------

    #[test]
    fn default_api_key_env_for_is_pub_and_returns_convention() {
        // Visibility: callable from this test, which exercises the `pub`
        // export operator surfaces depend on (provider list / error text).
        // openai-compat default → XVN_PROVIDER_<NAME>_KEY with `-`→`_`.
        assert_eq!(
            default_api_key_env_for(ProviderKind::OpenaiCompat, "ollama-local"),
            "XVN_PROVIDER_OLLAMA_LOCAL_KEY"
        );
        assert_eq!(
            default_api_key_env_for(ProviderKind::Anthropic, "anthropic"),
            "ANTHROPIC_API_KEY"
        );
        // No-auth local kinds have no default env var.
        assert_eq!(default_api_key_env_for(ProviderKind::Ollama, "ollama"), "");
        assert_eq!(default_api_key_env_for(ProviderKind::LocalCandle, "x"), "");
    }

    #[test]
    fn missing_provider_key_message_names_configured_env_var() {
        // The configured api_key_env wins and is named verbatim.
        let msg = missing_provider_key_message(
            ProviderKind::Ollama,
            "ollama-local",
            "XVN_PROVIDER_OLLAMA_LOCAL_KEY",
        );
        assert!(
            msg.contains("Ollama provider key not found in environment")
                && msg.contains("Set XVN_PROVIDER_OLLAMA_LOCAL_KEY=<key>")
                && msg.contains("providers.toml"),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn missing_provider_key_message_falls_back_to_convention() {
        // Empty api_key_env → derive the convention via default_api_key_env_for.
        let msg = missing_provider_key_message(ProviderKind::OpenaiCompat, "deepseek", "");
        assert!(
            msg.contains("Set XVN_PROVIDER_DEEPSEEK_KEY=<key>"),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn missing_provider_key_message_handles_no_env_local_kind() {
        // A local kind with no api_key_env has no var to name — message
        // must stay actionable rather than printing `Set =<key>`.
        let msg = missing_provider_key_message(ProviderKind::Ollama, "ollama", "");
        assert!(
            !msg.contains("Set =<key>") && msg.contains("Settings → Providers"),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn effective_provider_carries_expected_api_key_env() {
        // effective_from_entry must populate the new field used by
        // `provider list` to name the var (QA U8).
        let entry = ProviderEntry {
            name: "ollama-local".into(),
            kind: ProviderKind::OpenaiCompat,
            base_url: "http://localhost:11434/v1".into(),
            api_key_env: String::new(),
            enabled_models: vec![],
        };
        let ep = effective_from_entry(&entry, &ProvidersSecretsFile::default());
        assert_eq!(ep.expected_api_key_env, "XVN_PROVIDER_OLLAMA_LOCAL_KEY");
    }
}
