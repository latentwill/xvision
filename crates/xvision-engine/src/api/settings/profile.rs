//! `/api/settings/profile` — operator profile (display name / handle).
//!
//! A minimal file-backed profile (mirroring `settings::memory`) persisted at
//! `$XVN_HOME/config/profile.toml`. It holds the operator's display name
//! (handle) which is used to stamp `creator` on newly created strategies and is
//! offered as the "set creator to my handle" value on the strategy detail page.
//!
//! A missing / empty file → an empty profile (no display name). Parse errors
//! degrade to an empty profile with a warn — never panic.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::api::{
    audit::{self, Outcome},
    ApiContext, ApiResult,
};

/// Persisted operator profile. A missing field / file yields an empty profile.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// The operator's display name / handle (e.g. `@alice`). `None` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl ProfileConfig {
    /// Load from disk. Missing / empty file → defaults. Parse errors →
    /// defaults + warn (best-effort; never panics).
    pub fn load_from_file(path: &Path) -> ProfileConfig {
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return ProfileConfig::default(),
        };
        if raw.trim().is_empty() {
            return ProfileConfig::default();
        }
        match toml::from_str::<ProfileConfig>(&raw) {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %path.display(),
                    "profile.toml parse error; using defaults"
                );
                ProfileConfig::default()
            }
        }
    }

    /// Write to disk, creating the parent dir.
    pub fn write_to_file(path: &Path, cfg: &ProfileConfig) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body =
            toml::to_string_pretty(cfg).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, body)
    }

    /// The trimmed display name when set and non-empty, else `None`.
    pub fn handle(&self) -> Option<String> {
        self.display_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }
}

/// Free function: load the profile for an `$XVN_HOME` without an `ApiContext`.
/// Used by the strategy create path to default `creator` from the profile.
pub fn load(xvn_home: &Path) -> ProfileConfig {
    ProfileConfig::load_from_file(&config_path_for(xvn_home))
}

fn config_path_for(xvn_home: &Path) -> PathBuf {
    xvn_home.join("config").join("profile.toml")
}

fn config_path(ctx: &ApiContext) -> PathBuf {
    config_path_for(&ctx.xvn_home)
}

/// Report DTO returned by both `get` and `set`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileReport {
    /// The persisted display name / handle, or `null` when unset.
    pub display_name: Option<String>,
    /// True when the config file exists (i.e. the operator has saved a profile).
    pub persisted: bool,
}

/// Partial-update request. `display_name: Some("")` (or whitespace) clears the
/// handle; `None` leaves it untouched (partial update).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateProfileRequest {
    #[serde(default)]
    pub display_name: Option<String>,
}

fn report_from_cfg(cfg: &ProfileConfig, persisted: bool) -> ProfileReport {
    ProfileReport {
        display_name: cfg.handle(),
        persisted,
    }
}

/// Read the operator profile. Missing file → an empty (unpersisted) profile.
pub async fn get(ctx: &ApiContext) -> ApiResult<ProfileReport> {
    let started = Instant::now();
    let path = config_path(ctx);
    let persisted = path.exists();
    let cfg = ProfileConfig::load_from_file(&path);
    let report = report_from_cfg(&cfg, persisted);

    let _ = audit::record(
        ctx,
        "settings",
        "profile.get",
        None,
        None,
        Outcome::Ok,
        started.elapsed().as_millis() as i64,
    )
    .await;

    Ok(report)
}

/// Partial update — only the provided field is changed. Seeds from disk (or
/// defaults) so omitted fields are preserved.
pub async fn set(ctx: &ApiContext, req: UpdateProfileRequest) -> ApiResult<ProfileReport> {
    let started = Instant::now();
    let path = config_path(ctx);

    let mut cfg = ProfileConfig::load_from_file(&path);
    if let Some(name) = req.display_name.as_deref() {
        // Trim; empty ≡ "clear the handle" → None.
        let t = name.trim();
        cfg.display_name = if t.is_empty() { None } else { Some(t.to_string()) };
    }

    ProfileConfig::write_to_file(&path, &cfg)
        .map_err(|e| crate::api::ApiError::Internal(format!("write profile.toml: {e}")))?;

    let report = report_from_cfg(&cfg, true);

    let _ = audit::record(
        ctx,
        "settings",
        "profile.set",
        cfg.handle().as_deref(),
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

    #[test]
    fn defaults_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let cfg = load(tmp.path());
        assert_eq!(cfg.handle(), None);
    }

    #[test]
    fn parse_error_yields_defaults() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config").join("profile.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "this is not valid toml = = =").unwrap();
        assert_eq!(load(tmp.path()).handle(), None);
    }

    #[tokio::test]
    async fn get_returns_empty_when_no_file() {
        let (ctx, _tmp) = test_ctx().await;
        let report = get(&ctx).await.unwrap();
        assert_eq!(report.display_name, None);
        assert!(!report.persisted);
    }

    #[tokio::test]
    async fn set_then_get_round_trips_and_trims() {
        let (ctx, _tmp) = test_ctx().await;
        let report = set(
            &ctx,
            UpdateProfileRequest {
                display_name: Some("  @alice  ".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(report.display_name.as_deref(), Some("@alice"));
        assert!(report.persisted);

        let after = get(&ctx).await.unwrap();
        assert_eq!(after.display_name.as_deref(), Some("@alice"));
        assert!(after.persisted);
        // The free-function loader agrees.
        assert_eq!(load(&ctx.xvn_home).handle().as_deref(), Some("@alice"));
    }

    #[tokio::test]
    async fn empty_display_name_clears_to_none() {
        let (ctx, _tmp) = test_ctx().await;
        set(
            &ctx,
            UpdateProfileRequest {
                display_name: Some("@bob".into()),
            },
        )
        .await
        .unwrap();
        let report = set(
            &ctx,
            UpdateProfileRequest {
                display_name: Some("   ".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(report.display_name, None);
    }
}
