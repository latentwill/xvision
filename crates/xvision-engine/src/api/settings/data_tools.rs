//! `/api/settings/data-tools` — GET / PUT for `[[data_tools]]` in the
//! workspace config.
//!
//! Reads from / writes to `config/default.toml` via `toml_edit` so comments
//! and formatting survive round-trips. Single source of truth for the
//! data-tool list (Nansen, Elfa, …).
//!
//! No secret redaction: `DataToolEntry.api_key_env` holds only the env-var
//! NAME, never the actual key value. PUT replaces the entire `[[data_tools]]`
//! block atomically; GET returns the current list, empty when none are
//! configured (back-compat default).
//!
//! Pattern mirrors `settings::memory` (engine-managed config, simple
//! GET + SET) combined with `settings::providers` (writes to
//! `default.toml` via `toml_edit`).

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

/// Response body for both GET and PUT `/api/settings/data-tools`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataToolsReport {
    pub data_tools: Vec<DataToolEntry>,
}

/// Request body for PUT `/api/settings/data-tools`.
/// Replaces the entire `[[data_tools]]` array atomically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetDataToolsRequest {
    pub data_tools: Vec<DataToolEntry>,
}

// --- public API (audit-wrapped) ---------------------------------------------

/// Read the current `[[data_tools]]` list. Returns an empty list when none
/// are configured (the `#[serde(default)]` in `RuntimeConfig` guarantees
/// this is always valid).
pub async fn get(ctx: &ApiContext, config_path: &Path) -> ApiResult<DataToolsReport> {
    let started = Instant::now();
    let result = get_inner(config_path).await;

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
pub async fn set(
    ctx: &ApiContext,
    config_path: &Path,
    req: SetDataToolsRequest,
) -> ApiResult<DataToolsReport> {
    let started = Instant::now();
    let args = serde_json::to_string(&serde_json::json!({ "count": req.data_tools.len() })).ok();
    let result = set_inner(config_path, req).await;

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

async fn get_inner(config_path: &Path) -> ApiResult<DataToolsReport> {
    let cfg = load_cfg(config_path).await?;
    Ok(DataToolsReport {
        data_tools: cfg.data_tools,
    })
}

async fn set_inner(config_path: &Path, req: SetDataToolsRequest) -> ApiResult<DataToolsReport> {
    // Basic field-length pre-checks (mirrors the garde annotations on
    // DataToolEntry). Full garde validation happens via load_runtime after the
    // write, which re-validates the whole config and rejects any invalid state.
    for entry in &req.data_tools {
        if entry.base_url.len() > 512 {
            return Err(ApiError::Validation(format!(
                "base_url too long ({} chars, max 512)",
                entry.base_url.len()
            )));
        }
        if entry.api_key_env.len() > 64 {
            return Err(ApiError::Validation(format!(
                "api_key_env too long ({} chars, max 64)",
                entry.api_key_env.len()
            )));
        }
    }

    let entries = req.data_tools.clone();
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

    // Re-validate the resulting config so the file is never left in an
    // invalid state (mirrors the providers pattern).
    let _ = load_cfg(config_path).await?;

    get_inner(config_path).await
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
                data_tools: vec![DataToolEntry {
                    kind: DataToolKind::Nansen,
                    base_url: "https://api.nansen.ai/v1".to_string(),
                    api_key_env: "NANSEN_API_KEY".to_string(),
                    enabled: true,
                    budget_credits_per_run: Some(100),
                    nansen_lookahead_lag_days: Some(1),
                }],
            },
        )
        .await
        .unwrap();

        assert_eq!(put_report.data_tools.len(), 1);
        let entry = &put_report.data_tools[0];
        assert_eq!(entry.kind, DataToolKind::Nansen);
        assert_eq!(entry.base_url, "https://api.nansen.ai/v1");
        assert_eq!(entry.api_key_env, "NANSEN_API_KEY");
        assert!(entry.enabled);
        assert_eq!(entry.budget_credits_per_run, Some(100));
        assert_eq!(entry.nansen_lookahead_lag_days, Some(1));

        // Subsequent GET reflects the persisted value.
        let get_report = get(&ctx, &config_path).await.unwrap();
        assert_eq!(get_report.data_tools.len(), 1);
        let got = &get_report.data_tools[0];
        assert_eq!(got.kind, DataToolKind::Nansen);
        assert_eq!(got.api_key_env, "NANSEN_API_KEY");
        assert_eq!(got.budget_credits_per_run, Some(100));
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
                    DataToolEntry {
                        kind: DataToolKind::Nansen,
                        base_url: "https://api.nansen.ai/v1".to_string(),
                        api_key_env: "NANSEN_API_KEY".to_string(),
                        enabled: true,
                        budget_credits_per_run: None,
                        nansen_lookahead_lag_days: None,
                    },
                    DataToolEntry {
                        kind: DataToolKind::Elfa,
                        base_url: "https://api.elfa.ai/v1".to_string(),
                        api_key_env: "ELFA_API_KEY".to_string(),
                        enabled: false,
                        budget_credits_per_run: None,
                        nansen_lookahead_lag_days: None,
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
                data_tools: vec![DataToolEntry {
                    kind: DataToolKind::Elfa,
                    base_url: "https://api.elfa.ai/v2".to_string(),
                    api_key_env: "ELFA_KEY_V2".to_string(),
                    enabled: true,
                    budget_credits_per_run: Some(50),
                    nansen_lookahead_lag_days: None,
                }],
            },
        )
        .await
        .unwrap();

        assert_eq!(report.data_tools.len(), 1);
        assert_eq!(report.data_tools[0].kind, DataToolKind::Elfa);
        assert_eq!(report.data_tools[0].base_url, "https://api.elfa.ai/v2");

        let get_report = get(&ctx, &config_path).await.unwrap();
        assert_eq!(get_report.data_tools.len(), 1);
        assert_eq!(get_report.data_tools[0].kind, DataToolKind::Elfa);
    }

    #[tokio::test]
    async fn put_empty_list_clears_data_tools() {
        let (ctx, _tmp, config_path) = test_ctx_with_config("").await;

        // Seed with one entry.
        set(
            &ctx,
            &config_path,
            SetDataToolsRequest {
                data_tools: vec![DataToolEntry {
                    kind: DataToolKind::Nansen,
                    base_url: "https://api.nansen.ai/v1".to_string(),
                    api_key_env: "NANSEN_API_KEY".to_string(),
                    enabled: true,
                    budget_credits_per_run: None,
                    nansen_lookahead_lag_days: None,
                }],
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
}
