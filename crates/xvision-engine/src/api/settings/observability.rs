//! `/api/settings/observability` — agent-run trace retention surface.
//!
//! Read path returns the current `RetentionMode` together with the
//! per-toggle effective flags so the Settings → General page can render
//! a single-radio "what shows up in traces" picker without needing to
//! understand the granular `observability.toml` knobs.
//!
//! Write path persists the chosen mode to
//! `$XVN_HOME/config/observability.toml` via the existing
//! `xvision_observability::write_config` helper — the same on-disk file
//! the `xvn obs retention set` CLI writes — so the two surfaces agree.
//! Switching modes preserves the mode's canonical toggle defaults
//! (full_debug = all stores on, redacted = stores on + redact, hash_only
//! = all stores off); custom per-toggle tweaks made via the CLI live on
//! disk and are surfaced read-only here.

use std::path::PathBuf;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use xvision_observability::{write_config, ObservabilityConfig, RetentionMode};

use crate::api::{
    audit::{self, Outcome},
    ApiContext, ApiError, ApiResult,
};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetentionModeDto {
    FullDebug,
    Redacted,
    HashOnly,
}

impl From<RetentionMode> for RetentionModeDto {
    fn from(m: RetentionMode) -> Self {
        match m {
            RetentionMode::FullDebug => Self::FullDebug,
            RetentionMode::Redacted => Self::Redacted,
            RetentionMode::HashOnly => Self::HashOnly,
        }
    }
}

impl From<RetentionModeDto> for RetentionMode {
    fn from(m: RetentionModeDto) -> Self {
        match m {
            RetentionModeDto::FullDebug => Self::FullDebug,
            RetentionModeDto::Redacted => Self::Redacted,
            RetentionModeDto::HashOnly => Self::HashOnly,
        }
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityReport {
    pub mode: RetentionModeDto,
    pub store_prompts: bool,
    pub store_responses: bool,
    pub store_tool_inputs: bool,
    pub store_tool_outputs: bool,
    pub redact_secrets: bool,
    pub payload_ttl_days: u64,
    pub max_payload_bytes: u64,
    /// True when the persisted config file exists. False means defaults
    /// are in force — the UI uses this to render "Default" vs "Custom".
    pub persisted: bool,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateObservabilityRequest {
    pub mode: RetentionModeDto,
}

fn config_path(ctx: &ApiContext) -> PathBuf {
    // Honor `XVN_HOME` via the context so dashboard + CLI agree on the
    // location even when tests point at a tempdir.
    ctx.xvn_home.join("config").join("observability.toml")
}

fn report_from_cfg(cfg: &ObservabilityConfig, persisted: bool) -> ObservabilityReport {
    ObservabilityReport {
        mode: cfg.retention.mode.into(),
        store_prompts: cfg.retention.store_prompts,
        store_responses: cfg.retention.store_responses,
        store_tool_inputs: cfg.retention.store_tool_inputs,
        store_tool_outputs: cfg.retention.store_tool_outputs,
        redact_secrets: cfg.retention.redact_secrets,
        payload_ttl_days: cfg.retention.payload_ttl_days,
        max_payload_bytes: cfg.retention.max_payload_bytes,
        persisted,
    }
}

/// Read the effective retention config. Missing file → defaults
/// (matches `ObservabilityConfig::default()`).
pub async fn get(ctx: &ApiContext) -> ApiResult<ObservabilityReport> {
    let started = Instant::now();
    let path = config_path(ctx);
    let persisted = path.exists();
    let cfg = ObservabilityConfig::load_from_file(&path)
        .map_err(|e| ApiError::Internal(format!("read observability.toml: {e}")))?;
    let report = report_from_cfg(&cfg, persisted);

    let _ = audit::record(
        ctx,
        "settings",
        "observability.get",
        None,
        None,
        Outcome::Ok,
        started.elapsed().as_millis() as i64,
    )
    .await;

    Ok(report)
}

/// Set the retention mode. The new mode's canonical toggle defaults are
/// applied (full_debug enables all stores; redacted enables stores +
/// redact; hash_only disables stores). TTL / max-payload-bytes are
/// preserved from disk so an operator who tuned them via the CLI doesn't
/// lose those tweaks when toggling the headline mode in the UI.
pub async fn set_mode(ctx: &ApiContext, req: UpdateObservabilityRequest) -> ApiResult<ObservabilityReport> {
    let started = Instant::now();
    let path = config_path(ctx);

    // Seed from disk so existing TTL / max_payload_bytes / redact toggle
    // survive a mode change. Missing file → defaults.
    let mut cfg = ObservabilityConfig::load_from_file(&path)
        .map_err(|e| ApiError::Internal(format!("read observability.toml: {e}")))?;

    let new_mode: RetentionMode = req.mode.into();
    cfg.retention.mode = new_mode;
    // Apply the canonical store-flag defaults for the chosen mode so the
    // UI's single picker has predictable behavior. The granular toggles
    // remain editable via `xvn obs retention set` for power users.
    match new_mode {
        RetentionMode::FullDebug => {
            cfg.retention.store_prompts = true;
            cfg.retention.store_responses = true;
            cfg.retention.store_tool_inputs = true;
            cfg.retention.store_tool_outputs = true;
            cfg.retention.redact_secrets = true;
        }
        RetentionMode::Redacted => {
            cfg.retention.store_prompts = true;
            cfg.retention.store_responses = true;
            cfg.retention.store_tool_inputs = true;
            cfg.retention.store_tool_outputs = true;
            cfg.retention.redact_secrets = true;
        }
        RetentionMode::HashOnly => {
            cfg.retention.store_prompts = false;
            cfg.retention.store_responses = false;
            cfg.retention.store_tool_inputs = false;
            cfg.retention.store_tool_outputs = false;
            cfg.retention.redact_secrets = true;
        }
    }

    write_config(&path, &cfg).map_err(|e| ApiError::Internal(format!("write observability.toml: {e}")))?;

    let report = report_from_cfg(&cfg, true);

    let _ = audit::record(
        ctx,
        "settings",
        "observability.set_mode",
        Some(cfg.retention.mode.as_db_str()),
        None,
        Outcome::Ok,
        started.elapsed().as_millis() as i64,
    )
    .await;

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Actor;
    use sqlx::SqlitePool;
    use tempfile::TempDir;

    async fn test_ctx() -> (ApiContext, TempDir) {
        let tmp = TempDir::new().unwrap();
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let ctx = ApiContext::new(pool, Actor::Cli { user: "test".into() }, tmp.path().to_path_buf());
        (ctx, tmp)
    }

    #[tokio::test]
    async fn get_returns_default_when_no_file() {
        let (ctx, _tmp) = test_ctx().await;
        let report = get(&ctx).await.unwrap();
        // The default in `ObservabilityConfig::default()` is currently
        // `HashOnly`. The sibling track
        // `observability-retention-default-full-debug` flips this. This
        // test deliberately asserts whatever `default()` says so the two
        // tracks coexist cleanly — once the default flips, this test
        // continues to pass without an edit here.
        let expected: RetentionModeDto = ObservabilityConfig::default().retention.mode.into();
        assert_eq!(report.mode, expected);
        assert!(!report.persisted);
    }

    #[tokio::test]
    async fn set_full_debug_persists_and_flips_stores_on() {
        let (ctx, _tmp) = test_ctx().await;
        let report = set_mode(
            &ctx,
            UpdateObservabilityRequest {
                mode: RetentionModeDto::FullDebug,
            },
        )
        .await
        .unwrap();
        assert_eq!(report.mode, RetentionModeDto::FullDebug);
        assert!(report.store_prompts);
        assert!(report.store_responses);
        assert!(report.store_tool_inputs);
        assert!(report.store_tool_outputs);
        assert!(report.persisted);

        // Round-trip through `get`.
        let after = get(&ctx).await.unwrap();
        assert_eq!(after.mode, RetentionModeDto::FullDebug);
        assert!(after.persisted);
    }

    #[tokio::test]
    async fn set_hash_only_disables_stores() {
        let (ctx, _tmp) = test_ctx().await;
        let report = set_mode(
            &ctx,
            UpdateObservabilityRequest {
                mode: RetentionModeDto::HashOnly,
            },
        )
        .await
        .unwrap();
        assert_eq!(report.mode, RetentionModeDto::HashOnly);
        assert!(!report.store_prompts);
        assert!(!report.store_responses);
        assert!(!report.store_tool_inputs);
        assert!(!report.store_tool_outputs);
        assert!(report.redact_secrets);
    }

    #[tokio::test]
    async fn set_preserves_ttl_and_max_bytes() {
        let (ctx, _tmp) = test_ctx().await;
        // Seed a config with custom TTL via the file path the API uses.
        let path = config_path(&ctx);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut seed = ObservabilityConfig::default();
        seed.retention.payload_ttl_days = 30;
        seed.retention.max_payload_bytes = 999_000;
        write_config(&path, &seed).unwrap();

        let after = set_mode(
            &ctx,
            UpdateObservabilityRequest {
                mode: RetentionModeDto::FullDebug,
            },
        )
        .await
        .unwrap();
        assert_eq!(after.payload_ttl_days, 30);
        assert_eq!(after.max_payload_bytes, 999_000);
    }
}
