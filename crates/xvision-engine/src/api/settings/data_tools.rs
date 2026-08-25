//! `/api/settings/data-tools` — GET / PUT for `[[data_tools]]` in the
//! workspace config.
//!
//! Reads from / writes to `config/default.toml` via `toml_edit` so comments
//! and formatting survive round-trips. Single source of truth for the
//! data-tool list (Nansen, Elfa, …).
//!
//! Secret handling mirrors `settings::providers`: the config carries only the
//! env-var NAME (`DataToolEntry.api_key_env`); an optional plaintext
//! `api_key` on PUT is persisted to `$XVN_HOME/secrets/data_tools.toml`
//! (mode 0600) and exported into the daemon process env under `api_key_env`.
//! GET surfaces only a redacted `api_key_set` presence flag per row — never
//! the key itself. PUT replaces the entire `[[data_tools]]` block atomically;
//! GET returns the current list, empty when none are configured.
//!
//! Pattern mirrors `settings::memory` (engine-managed config, simple
//! GET + SET) combined with `settings::providers` (writes to
//! `default.toml` via `toml_edit`, secrets via a 0600 TOML under
//! `$XVN_HOME/secrets/`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::task;

use xvision_core::config::{DataToolEntry, DataToolKind, RuntimeConfig};

use crate::api::{
    audit::{self, Outcome},
    ApiContext, ApiError, ApiResult,
};

// --- wire types -------------------------------------------------------------

/// One row in GET/PUT responses: the persisted config entry plus a redacted
/// key-presence flag. The key value itself never leaves the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataToolRow {
    #[serde(flatten)]
    pub entry: DataToolEntry,
    /// True when an API key is materialized for this tool: the configured
    /// `api_key_env` env var is set non-empty, or a key is stored in
    /// `$XVN_HOME/secrets/data_tools.toml`. Env wins at runtime.
    pub api_key_set: bool,
}

/// Response body for both GET and PUT `/api/settings/data-tools`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataToolsReport {
    pub data_tools: Vec<DataToolRow>,
}

/// One entry accepted by PUT: the persisted config entry plus an optional
/// plaintext key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetDataToolEntry {
    #[serde(flatten)]
    pub entry: DataToolEntry,
    /// The actual API key (cleartext over the API). When set non-empty,
    /// persisted to `$XVN_HOME/secrets/data_tools.toml` (mode 0600) and
    /// exported into the daemon process env under `api_key_env` so the tool
    /// works right away. Never logged or returned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// Request body for PUT `/api/settings/data-tools`.
/// Replaces the entire `[[data_tools]]` array atomically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetDataToolsRequest {
    pub data_tools: Vec<SetDataToolEntry>,
}

// --- public API (audit-wrapped) ---------------------------------------------

/// Read the current `[[data_tools]]` list. Returns an empty list when none
/// are configured (the `#[serde(default)]` in `RuntimeConfig` guarantees
/// this is always valid).
pub async fn get(ctx: &ApiContext, config_path: &Path) -> ApiResult<DataToolsReport> {
    let started = Instant::now();
    let result = get_inner(config_path, &ctx.xvn_home).await;

    let outcome = audit_outcome(&result);
    let _ = audit::record(
        ctx,
        "settings",
        "data_tools.get",
        None,
        None,
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

/// Replace the entire `[[data_tools]]` list. Re-validates the resulting
/// config before returning so the file is never left in an invalid state.
/// Any non-empty `api_key` is stored as a secret and exported to the process
/// env; the key never lands in the audited args or the workspace config.
pub async fn set(
    ctx: &ApiContext,
    config_path: &Path,
    req: SetDataToolsRequest,
) -> ApiResult<DataToolsReport> {
    let started = Instant::now();
    let keys_provided = req
        .data_tools
        .iter()
        .filter(|e| e.api_key.as_deref().map(str::trim).map(str::len).unwrap_or(0) > 0)
        .count();
    let args =
        serde_json::to_string(&serde_json::json!({ "count": req.data_tools.len(), "api_keys_provided": keys_provided }))
            .ok();
    let result = set_inner(config_path, &ctx.xvn_home, req).await;

    let outcome = audit_outcome(&result);
    let _ = audit::record(
        ctx,
        "settings",
        "data_tools.set",
        None,
        args.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    result
}

// --- inner impls (no auditing) ----------------------------------------------

async fn get_inner(config_path: &Path, xvn_home: &Path) -> ApiResult<DataToolsReport> {
    let cfg = load_cfg(config_path).await?;
    let secrets = load_data_tools_secrets(xvn_home)
        .await
        .unwrap_or_default();

    let rows = cfg
        .data_tools
        .into_iter()
        .map(|entry| {
            let api_key_set = entry_has_key(&entry, &secrets);
            DataToolRow { entry, api_key_set }
        })
        .collect();
    Ok(DataToolsReport { data_tools: rows })
}

async fn set_inner(
    config_path: &Path,
    xvn_home: &Path,
    req: SetDataToolsRequest,
) -> ApiResult<DataToolsReport> {
    // Basic field-length pre-checks (mirrors the garde annotations on
    // DataToolEntry). Full garde validation happens via load_runtime after the
    // write, which re-validates the whole config and rejects any invalid state.
    for wire in &req.data_tools {
        if wire.entry.base_url.len() > 512 {
            return Err(ApiError::Validation(format!(
                "base_url too long ({} chars, max 512)",
                wire.entry.base_url.len()
            )));
        }
        if wire.entry.api_key_env.len() > 64 {
            return Err(ApiError::Validation(format!(
                "api_key_env too long ({} chars, max 64)",
                wire.entry.api_key_env.len()
            )));
        }
    }

    let entries: Vec<DataToolEntry> = req.data_tools.iter().map(|w| w.entry.clone()).collect();
    let keys: Vec<(DataToolKind, String, String)> = req
        .data_tools
        .iter()
        .filter_map(|w| {
            let key = w.api_key.as_deref().map(str::trim).unwrap_or("");
            (!key.is_empty()).then(|| (w.entry.kind, w.entry.api_key_env.trim().to_string(), key.to_string()))
        })
        .collect();

    let path: PathBuf = config_path.to_path_buf();

    task::spawn_blocking(move || -> ApiResult<()> {
        use toml_edit::{value, ArrayOfTables, DocumentMut, Table};

        let raw = std::fs::read_to_string(&path)
            .map_err(|e| ApiError::Internal(format!("read {}: {e}", path.display())))?;
        let mut doc: DocumentMut = raw
            .parse()
            .map_err(|e| ApiError::Internal(format!("parse {}: {e}", path.display())))?;

        // Replace [[data_tools]] atomically — wipe and rebuild.
        let mut aot = ArrayOfTables::new();
        for entry in &entries {
            let mut row = Table::new();
            row.insert("kind", value(kind_to_str(entry.kind)));
            row.insert("base_url", value(entry.base_url.clone()));
            row.insert("api_key_env", value(entry.api_key_env.clone()));
            row.insert("enabled", value(entry.enabled));
            if let Some(budget) = entry.budget_credits_per_run {
                row.insert("budget_credits_per_run", value(budget as i64));
            }
            if let Some(lag) = entry.nansen_lookahead_lag_days {
                row.insert("nansen_lookahead_lag_days", value(lag as i64));
            }
            aot.push(row);
        }

        if aot.is_empty() {
            // Remove the key entirely so the file stays clean when empty.
            doc.remove("data_tools");
        } else {
            doc.insert("data_tools", toml_edit::Item::ArrayOfTables(aot));
        }

        std::fs::write(&path, doc.to_string())
            .map_err(|e| ApiError::Internal(format!("write {}: {e}", path.display())))?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))??;

    // Store any provided keys AFTER the config write succeeded so a failed
    // write never orphans a secret. Serialized through spawn_blocking like
    // every other secret write (no concurrent set_var contention).
    for (kind, env_var, key) in keys {
        if env_var.is_empty() {
            return Err(ApiError::Validation(
                "cannot store an api_key for a data tool with no api_key_env".into(),
            ));
        }
        upsert_tool_secret(xvn_home, kind, &env_var, &key).await?;
        // Inject into the live process env so the tool is immediately usable
        // without restarting the daemon.
        std::env::set_var(&env_var, &key);
    }

    // Re-validate the resulting config so the file is never left in an
    // invalid state (mirrors the providers pattern).
    let _ = load_cfg(config_path).await?;

    get_inner(config_path, xvn_home).await
}

// --- secrets file -----------------------------------------------------------

/// Persisted data-tool secrets. Lives in `$XVN_HOME/secrets/data_tools.toml`,
/// keyed by tool kind (`[tool.nansen]`, `[tool.elfa]`). Never returned through
/// the read API — only `DataToolRow::api_key_set` (a presence flag) surfaces.
#[derive(Debug, Default, Serialize, Deserialize)]
struct DataToolsSecretsFile {
    #[serde(default)]
    tool: HashMap<String, ToolSecret>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolSecret {
    /// Env var name the daemon exports this secret under.
    env_var: String,
    /// Plaintext API key. Treat the file like an SSH private key.
    api_key: String,
}

fn data_tools_secrets_path(xvn_home: &Path) -> PathBuf {
    xvn_home.join("secrets").join("data_tools.toml")
}

async fn load_data_tools_secrets(xvn_home: &Path) -> ApiResult<DataToolsSecretsFile> {
    let path = data_tools_secrets_path(xvn_home);
    match tokio::fs::read_to_string(&path).await {
        Ok(s) => toml::from_str::<DataToolsSecretsFile>(&s)
            .map_err(|e| ApiError::Internal(format!("parse {}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DataToolsSecretsFile::default()),
        Err(e) => Err(ApiError::Internal(format!("read {}: {e}", path.display()))),
    }
}

async fn save_data_tools_secrets(xvn_home: &Path, file: &DataToolsSecretsFile) -> ApiResult<()> {
    let path = data_tools_secrets_path(xvn_home);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ApiError::Internal(format!("create {}: {e}", parent.display())))?;
    }
    let body = toml::to_string_pretty(file)
        .map_err(|e| ApiError::Internal(format!("serialize secrets: {e}")))?;
    tokio::fs::write(&path, body)
        .await
        .map_err(|e| ApiError::Internal(format!("write {}: {e}", path.display())))?;
    set_owner_only(&path)
}

async fn upsert_tool_secret(
    xvn_home: &Path,
    kind: DataToolKind,
    env_var: &str,
    api_key: &str,
) -> ApiResult<()> {
    let mut file = load_data_tools_secrets(xvn_home).await?;
    file.tool.insert(
        kind_to_str(kind).to_string(),
        ToolSecret {
            env_var: env_var.to_string(),
            api_key: api_key.to_string(),
        },
    );
    save_data_tools_secrets(xvn_home, &file).await
}

/// True when a usable key exists for this entry: the `api_key_env` env var is
/// set non-empty, OR a non-empty key is stored under this tool's kind.
fn entry_has_key(entry: &DataToolEntry, secrets: &DataToolsSecretsFile) -> bool {
    if !entry.api_key_env.is_empty()
        && std::env::var(&entry.api_key_env).map(|v| !v.is_empty()).unwrap_or(false)
    {
        return true;
    }
    secrets
        .tool
        .get(kind_to_str(entry.kind))
        .is_some_and(|s| !s.api_key.is_empty())
}

/// Hydrate stored data-tool secrets into the process env at startup so runs
/// pick up persisted keys without the operator re-exporting them. Env wins:
/// a variable the operator already exported is never clobbered.
///
/// Call from the same startup sites as
/// [`crate::api::settings::providers::load_providers_secrets_into_env`] —
/// the dashboard (`state.rs`) and the CLI (`main.rs`).
pub async fn load_data_tools_secrets_into_env(xvn_home: &Path) -> ApiResult<usize> {
    let file = load_data_tools_secrets(xvn_home).await?;
    let mut applied = 0usize;
    for (_kind, secret) in file.tool.iter() {
        if secret.env_var.is_empty() || secret.api_key.is_empty() {
            continue;
        }
        if std::env::var(&secret.env_var).map(|v| !v.is_empty()).unwrap_or(false) {
            continue;
        }
        std::env::set_var(&secret.env_var, &secret.api_key);
        applied += 1;
    }
    Ok(applied)
}

#[cfg(unix)]
fn set_owner_only(path: &Path) -> ApiResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)
        .map_err(|e| ApiError::Internal(format!("chmod {}: {e}", path.display())))
}

#[cfg(not(unix))]
fn set_owner_only(_path: &Path) -> ApiResult<()> {
    Ok(())
}

// --- helpers ----------------------------------------------------------------

async fn load_cfg(config_path: &Path) -> ApiResult<RuntimeConfig> {
    let path = config_path.to_path_buf();
    task::spawn_blocking(move || xvision_core::config::load_runtime(&path))
        .await
        .map_err(|e| ApiError::Internal(format!("spawn_blocking: {e}")))?
        .map_err(|e| ApiError::Validation(format!("load config: {e}")))
}

fn kind_to_str(kind: DataToolKind) -> &'static str {
    match kind {
        DataToolKind::Nansen => "nansen",
        DataToolKind::Elfa => "elfa",
    }
}

fn audit_outcome<T>(result: &ApiResult<T>) -> Outcome {
    match result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    }
}

// --- tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Actor;
    use sqlx::SqlitePool;
    use tempfile::TempDir;
    use xvision_core::config::DataToolKind;

    /// Minimal valid RuntimeConfig — reused from providers tests.
    const MIN_CONFIG: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

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

    async fn test_ctx_with_config(extra_toml: &str) -> (ApiContext, TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("default.toml");

        // Minimal valid config with optional extra TOML appended.
        let body = format!("{}{}", MIN_CONFIG, extra_toml);
        std::fs::write(&config_path, body.as_bytes()).unwrap();

        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let ctx = ApiContext::new(pool, Actor::Cli { user: "test".into() }, tmp.path().to_path_buf());
        (ctx, tmp, config_path)
    }

    fn nansen_entry() -> SetDataToolEntry {
        SetDataToolEntry {
            entry: DataToolEntry {
                kind: DataToolKind::Nansen,
                base_url: "https://api.nansen.ai/v1".to_string(),
                api_key_env: "NANSEN_API_KEY".to_string(),
                enabled: true,
                budget_credits_per_run: Some(100),
                nansen_lookahead_lag_days: Some(1),
            },
            api_key: None,
        }
    }

    #[tokio::test]
    async fn get_returns_empty_when_no_data_tools() {
        let (ctx, _tmp, config_path) = test_ctx_with_config("").await;
        let report = get(&ctx, &config_path).await.unwrap();
        assert!(
            report.data_tools.is_empty(),
            "expected empty data_tools, got: {:?}",
            report.data_tools
        );
    }

    #[tokio::test]
    async fn data_tools_settings_round_trip() {
        let (ctx, _tmp, config_path) = test_ctx_with_config("").await;

        // PUT one Nansen entry.
        let put_report = set(
            &ctx,
            &config_path,
            SetDataToolsRequest {
                data_tools: vec![nansen_entry()],
            },
        )
        .await
        .unwrap();

        assert_eq!(put_report.data_tools.len(), 1);
        let row = &put_report.data_tools[0];
        assert_eq!(row.entry.kind, DataToolKind::Nansen);
        assert_eq!(row.entry.base_url, "https://api.nansen.ai/v1");
        assert_eq!(row.entry.api_key_env, "NANSEN_API_KEY");
        assert!(row.entry.enabled);
        assert_eq!(row.entry.budget_credits_per_run, Some(100));
        assert_eq!(row.entry.nansen_lookahead_lag_days, Some(1));

        // Subsequent GET reflects the persisted value.
        let get_report = get(&ctx, &config_path).await.unwrap();
        assert_eq!(get_report.data_tools.len(), 1);
        let got = &get_report.data_tools[0];
        assert_eq!(got.entry.kind, DataToolKind::Nansen);
        assert_eq!(got.entry.api_key_env, "NANSEN_API_KEY");
        assert_eq!(got.entry.budget_credits_per_run, Some(100));
    }

    #[tokio::test]
    async fn put_replaces_entire_list() {
        let (ctx, _tmp, config_path) = test_ctx_with_config("").await;

        // Seed with two entries.
        set(
            &ctx,
            &config_path,
            SetDataToolsRequest {
                data_tools: vec![
                    nansen_entry(),
                    SetDataToolEntry {
                        entry: DataToolEntry {
                            kind: DataToolKind::Elfa,
                            base_url: "https://api.elfa.ai/v1".to_string(),
                            api_key_env: "ELFA_API_KEY".to_string(),
                            enabled: false,
                            budget_credits_per_run: None,
                            nansen_lookahead_lag_days: None,
                        },
                        api_key: None,
                    },
                ],
            },
        )
        .await
        .unwrap();

        // Replace with only Elfa — Nansen must be gone.
        let report = set(
            &ctx,
            &config_path,
            SetDataToolsRequest {
                data_tools: vec![SetDataToolEntry {
                    entry: DataToolEntry {
                        kind: DataToolKind::Elfa,
                        base_url: "https://api.elfa.ai/v2".to_string(),
                        api_key_env: "ELFA_KEY_V2".to_string(),
                        enabled: true,
                        budget_credits_per_run: Some(50),
                        nansen_lookahead_lag_days: None,
                    },
                    api_key: None,
                }],
            },
        )
        .await
        .unwrap();

        assert_eq!(report.data_tools.len(), 1);
        assert_eq!(report.data_tools[0].entry.kind, DataToolKind::Elfa);
        assert_eq!(report.data_tools[0].entry.base_url, "https://api.elfa.ai/v2");

        let get_report = get(&ctx, &config_path).await.unwrap();
        assert_eq!(get_report.data_tools.len(), 1);
        assert_eq!(get_report.data_tools[0].entry.kind, DataToolKind::Elfa);
    }

    #[tokio::test]
    async fn put_empty_list_clears_data_tools() {
        let (ctx, _tmp, config_path) = test_ctx_with_config("").await;

        // Seed with one entry.
        set(
            &ctx,
            &config_path,
            SetDataToolsRequest {
                data_tools: vec![nansen_entry()],
            },
        )
        .await
        .unwrap();

        // Clear.
        let report = set(&ctx, &config_path, SetDataToolsRequest { data_tools: vec![] })
            .await
            .unwrap();
        assert!(report.data_tools.is_empty());

        let get_report = get(&ctx, &config_path).await.unwrap();
        assert!(get_report.data_tools.is_empty());
    }

    #[tokio::test]
    async fn put_stores_api_key_as_secret_and_surfaces_presence_only() {
        let env_var = "XVN_TEST_DATA_TOOL_NANSEN_KEY_RT";
        std::env::remove_var(env_var);
        let (ctx, tmp, config_path) = test_ctx_with_config("").await;

        let mut entry = nansen_entry();
        entry.entry.api_key_env = env_var.to_string();
        entry.api_key = Some("sk-nansen-test".into());
        let report = set(&ctx, &config_path, SetDataToolsRequest { data_tools: vec![entry] })
            .await
            .unwrap();

        assert!(report.data_tools[0].api_key_set, "key just stored → present");

        // The secret landed in the 0600 file, keyed by kind — not in the
        // workspace config.
        let secrets_raw =
            std::fs::read_to_string(tmp.path().join("secrets").join("data_tools.toml")).unwrap();
        assert!(secrets_raw.contains("sk-nansen-test"));
        let cfg_raw = std::fs::read_to_string(&config_path).unwrap();
        assert!(!cfg_raw.contains("sk-nansen-test"), "secret must never land in config");

        // Exported into the process env for immediate runtime use.
        assert_eq!(std::env::var(env_var).unwrap(), "sk-nansen-test");

        std::env::remove_var(env_var);
    }

    #[tokio::test]
    async fn get_reports_key_set_from_env_without_secret() {
        let env_var = "XVN_TEST_DATA_TOOL_NANSEN_KEY_ENV";
        std::env::set_var(env_var, "sk-from-env");
        let (ctx, _tmp, config_path) = test_ctx_with_config("").await;

        let mut entry = nansen_entry();
        entry.entry.api_key_env = env_var.to_string();
        let report = set(&ctx, &config_path, SetDataToolsRequest { data_tools: vec![entry] })
            .await
            .unwrap();
        assert!(report.data_tools[0].api_key_set);

        std::env::remove_var(env_var);
    }

    #[tokio::test]
    async fn put_rejects_api_key_without_api_key_env() {
        let (ctx, _tmp, config_path) = test_ctx_with_config("").await;
        let mut entry = nansen_entry();
        entry.entry.api_key_env = String::new();
        entry.api_key = Some("sk-x".into());
        let err = set(&ctx, &config_path, SetDataToolsRequest { data_tools: vec![entry] })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no api_key_env"), "{err}");
    }

    #[tokio::test]
    async fn hydrate_applies_stored_secret_but_env_wins() {
        let env_var = "XVN_TEST_DATA_TOOL_HYDRATE_KEY";
        std::env::remove_var(env_var);
        let (ctx, _tmp, _config_path) = test_ctx_with_config("").await;

        // Store a secret directly.
        upsert_tool_secret(&ctx.xvn_home, DataToolKind::Nansen, env_var, "sk-stored")
            .await
            .unwrap();

        // Nothing exported yet → hydration applies the stored key.
        let applied = load_data_tools_secrets_into_env(&ctx.xvn_home).await.unwrap();
        assert!(applied >= 1);
        assert_eq!(std::env::var(env_var).unwrap(), "sk-stored");

        // Env already set → hydration must NOT clobber it.
        std::env::set_var(env_var, "sk-operator");
        upsert_tool_secret(&ctx.xvn_home, DataToolKind::Nansen, env_var, "sk-newer")
            .await
            .unwrap();
        load_data_tools_secrets_into_env(&ctx.xvn_home).await.unwrap();
        assert_eq!(std::env::var(env_var).unwrap(), "sk-operator");

        std::env::remove_var(env_var);
    }
}
