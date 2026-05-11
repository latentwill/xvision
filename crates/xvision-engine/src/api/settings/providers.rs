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
    /// True if removing this entry would orphan the `[intern]` workspace
    /// default slot. UI should disable the delete button.
    pub referenced_by_intern: bool,
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

// --- inner impls (no auditing) ---------------------------------------------

async fn list_inner(config_path: &Path, xvn_home: &Path) -> ApiResult<ProvidersReport> {
    let cfg = load_cfg(config_path).await?;
    let intern_kind: ProviderKind = cfg.intern.provider.into();
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
    let intern_kind: ProviderKind = cfg.intern.provider.into();
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
    let _ = load_cfg(config_path).await?;
    show_inner(config_path, xvn_home, &name).await
}

async fn remove_inner(config_path: &Path, xvn_home: &Path, name: &str) -> ApiResult<()> {
    let cfg = load_cfg(config_path).await?;
    let intern_kind: ProviderKind = cfg.intern.provider.into();
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
    if entry.matches_triple(intern_kind, &cfg.intern.base_url, &cfg.intern.api_key_env) {
        return Err(ApiError::Conflict(format!(
            "cannot remove `{name}`: referenced by [intern] (workspace default Intern slot). \
             Edit [intern] to point at a different provider first."
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
    let referenced_by_intern =
        entry.matches_triple(intern_kind, &cfg.intern.base_url, &cfg.intern.api_key_env);
    ProviderRow {
        name: entry.name.clone(),
        kind: kind_to_str(entry.kind).into(),
        base_url: entry.base_url.clone(),
        api_key_env: entry.api_key_env.clone(),
        api_key_set,
        synthetic: entry.name.starts_with('_'),
        referenced_by_intern,
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
        assert!(p.referenced_by_intern);
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
                api_key: None,
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
                api_key: None,
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
                api_key: None,
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
                api_key: None,
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
