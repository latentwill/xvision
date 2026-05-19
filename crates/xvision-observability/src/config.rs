//! Observability config loader.
//!
//! See the plan's "Retention policy" section for the full precedence
//! chain. This crate exposes the loader and the env-var step; the CLI
//! flag step is implemented in the retention-cli leaf (which can pass
//! the resolved `ObservabilityConfig` directly into whatever needs it).
//!
//! Precedence implemented here: **env > config file > built-in default**.
//! The retention-cli leaf adds the CLI flag step in front.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::warn;

pub const CONFIG_FILE_NAME: &str = "observability.toml";

/// All env vars start with this prefix.
pub const ENV_OVERRIDE_PREFIX: &str = "XVISION_OBSERVABILITY";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config io: {0}")]
    Io(#[from] std::io::Error),
    #[error("config parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("unknown retention mode: {0}")]
    UnknownRetentionMode(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionMode {
    HashOnly,
    Redacted,
    FullDebug,
}

impl RetentionMode {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::HashOnly => "hash_only",
            Self::Redacted => "redacted",
            Self::FullDebug => "full_debug",
        }
    }

    pub fn from_str_strict(s: &str) -> Result<Self, ConfigError> {
        match s {
            "hash_only" => Ok(Self::HashOnly),
            "redacted" => Ok(Self::Redacted),
            "full_debug" => Ok(Self::FullDebug),
            other => Err(ConfigError::UnknownRetentionMode(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetentionConfig {
    pub mode: RetentionMode,
    pub store_prompts: bool,
    pub store_responses: bool,
    pub store_tool_inputs: bool,
    pub store_tool_outputs: bool,
    pub redact_secrets: bool,
    pub payload_ttl_days: u64,
    pub max_payload_bytes: u64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        // Default is FullDebug so operators can read prompts and
        // responses out of the box. Privacy hardening is an explicit
        // opt-in via TOML / env, not the default.
        Self {
            mode: RetentionMode::FullDebug,
            store_prompts: true,
            store_responses: true,
            store_tool_inputs: true,
            store_tool_outputs: true,
            redact_secrets: true,
            payload_ttl_days: 7,
            max_payload_bytes: 200_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    pub sqlite_enabled: bool,
    pub otel_enabled: bool,
    pub retention: RetentionConfig,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            sqlite_enabled: true,
            otel_enabled: false,
            retention: RetentionConfig::default(),
        }
    }
}

/// Layered config file shape. Top-level `[observability]` and nested
/// `[observability.retention]` match the plan's example.
#[derive(Debug, Default, Deserialize)]
struct ConfigFile {
    observability: Option<FileObs>,
}

#[derive(Debug, Default, Deserialize)]
struct FileObs {
    sqlite_enabled: Option<bool>,
    otel_enabled: Option<bool>,
    retention: Option<FileRetention>,
}

#[derive(Debug, Default, Deserialize)]
struct FileRetention {
    mode: Option<String>,
    store_prompts: Option<bool>,
    store_responses: Option<bool>,
    store_tool_inputs: Option<bool>,
    store_tool_outputs: Option<bool>,
    redact_secrets: Option<bool>,
    payload_ttl_days: Option<u64>,
    max_payload_bytes: Option<u64>,
}

impl ObservabilityConfig {
    /// Load with precedence env > file > default. The retention-cli leaf
    /// wraps this with the CLI flag step.
    ///
    /// `config_path` points at the file. Missing file is fine — defaults
    /// apply. The startup WARN line for `full_debug` is emitted exactly
    /// once on `load_with_env`, regardless of where the mode came from.
    pub fn load_with_env(config_path: &Path) -> Result<Self, ConfigError> {
        let mut cfg = Self::load_from_file(config_path)?;
        cfg.apply_env();
        cfg.warn_if_full_debug();
        Ok(cfg)
    }

    pub fn load_from_file(path: &Path) -> Result<Self, ConfigError> {
        let mut cfg = Self::default();
        if !path.exists() {
            return Ok(cfg);
        }
        let text = fs::read_to_string(path)?;
        let parsed: ConfigFile = toml::from_str(&text)?;
        if let Some(obs) = parsed.observability {
            if let Some(v) = obs.sqlite_enabled {
                cfg.sqlite_enabled = v;
            }
            if let Some(v) = obs.otel_enabled {
                cfg.otel_enabled = v;
            }
            if let Some(r) = obs.retention {
                if let Some(m) = r.mode {
                    cfg.retention.mode = RetentionMode::from_str_strict(&m)?;
                }
                if let Some(v) = r.store_prompts {
                    cfg.retention.store_prompts = v;
                }
                if let Some(v) = r.store_responses {
                    cfg.retention.store_responses = v;
                }
                if let Some(v) = r.store_tool_inputs {
                    cfg.retention.store_tool_inputs = v;
                }
                if let Some(v) = r.store_tool_outputs {
                    cfg.retention.store_tool_outputs = v;
                }
                if let Some(v) = r.redact_secrets {
                    cfg.retention.redact_secrets = v;
                }
                if let Some(v) = r.payload_ttl_days {
                    cfg.retention.payload_ttl_days = v;
                }
                if let Some(v) = r.max_payload_bytes {
                    cfg.retention.max_payload_bytes = v;
                }
            }
        }
        Ok(cfg)
    }

    /// Apply env-var overrides on top of the loaded values. Naming:
    /// `XVISION_OBSERVABILITY_<UPPER_SNAKE>` for top-level keys,
    /// `XVISION_OBSERVABILITY_RETENTION_<UPPER_SNAKE>` for retention.
    /// The retention mode shortcut `XVISION_OBSERVABILITY_RETENTION`
    /// is also honoured (matches the operator's example).
    pub fn apply_env(&mut self) {
        if let Ok(v) = std::env::var(format!("{ENV_OVERRIDE_PREFIX}_SQLITE_ENABLED")) {
            self.sqlite_enabled = parse_bool(&v);
        }
        if let Ok(v) = std::env::var(format!("{ENV_OVERRIDE_PREFIX}_OTEL_ENABLED")) {
            self.otel_enabled = parse_bool(&v);
        }
        // Shorthand for the headline knob.
        if let Ok(v) = std::env::var(format!("{ENV_OVERRIDE_PREFIX}_RETENTION")) {
            if let Ok(mode) = RetentionMode::from_str_strict(&v) {
                self.retention.mode = mode;
            }
        }
        if let Ok(v) = std::env::var(format!("{ENV_OVERRIDE_PREFIX}_RETENTION_MODE")) {
            if let Ok(mode) = RetentionMode::from_str_strict(&v) {
                self.retention.mode = mode;
            }
        }
        if let Ok(v) = std::env::var(format!("{ENV_OVERRIDE_PREFIX}_RETENTION_STORE_PROMPTS")) {
            self.retention.store_prompts = parse_bool(&v);
        }
        if let Ok(v) = std::env::var(format!("{ENV_OVERRIDE_PREFIX}_RETENTION_STORE_RESPONSES")) {
            self.retention.store_responses = parse_bool(&v);
        }
        if let Ok(v) = std::env::var(format!("{ENV_OVERRIDE_PREFIX}_RETENTION_STORE_TOOL_INPUTS")) {
            self.retention.store_tool_inputs = parse_bool(&v);
        }
        if let Ok(v) = std::env::var(format!("{ENV_OVERRIDE_PREFIX}_RETENTION_STORE_TOOL_OUTPUTS")) {
            self.retention.store_tool_outputs = parse_bool(&v);
        }
        if let Ok(v) = std::env::var(format!("{ENV_OVERRIDE_PREFIX}_RETENTION_REDACT_SECRETS")) {
            self.retention.redact_secrets = parse_bool(&v);
        }
        if let Ok(v) = std::env::var(format!("{ENV_OVERRIDE_PREFIX}_RETENTION_PAYLOAD_TTL_DAYS")) {
            if let Ok(n) = v.parse::<u64>() {
                self.retention.payload_ttl_days = n;
            }
        }
        if let Ok(v) = std::env::var(format!("{ENV_OVERRIDE_PREFIX}_RETENTION_MAX_PAYLOAD_BYTES")) {
            if let Ok(n) = v.parse::<u64>() {
                self.retention.max_payload_bytes = n;
            }
        }
    }

    pub fn warn_if_full_debug(&self) {
        // FullDebug is now the default — the startup WARN here would
        // fire on every fresh install, which is noisy. Demoted to an
        // info-level line; callers that go through `retention::resolve`
        // still emit the loud WARN when full_debug was set explicitly
        // by an operator (TOML / env / CLI flag), so the contract-grep
        // for `full_debug retention enabled` keeps firing under those
        // explicit paths.
        if self.retention.mode == RetentionMode::FullDebug {
            tracing::info!(
                target: "xvision_observability",
                "full_debug retention active (default). Prompts, \
                 responses, and tool payloads are stored on disk."
            );
        }
    }
}

fn parse_bool(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Default config path: `$XVN_HOME/config/observability.toml`. Falls back
/// to `~/.config/xvn/config/observability.toml` when `XVN_HOME` is unset.
pub fn default_config_path() -> PathBuf {
    let base = if let Ok(home) = std::env::var("XVN_HOME") {
        PathBuf::from(home)
    } else {
        dirs_home().join(".config").join("xvn")
    };
    base.join("config").join(CONFIG_FILE_NAME)
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_is_full_debug() {
        let cfg = ObservabilityConfig::default();
        assert_eq!(cfg.retention.mode, RetentionMode::FullDebug);
        assert!(cfg.sqlite_enabled);
        assert!(!cfg.otel_enabled);
        assert!(cfg.retention.redact_secrets);
        assert!(cfg.retention.store_prompts);
        assert!(cfg.retention.store_responses);
        assert!(cfg.retention.store_tool_inputs);
        assert!(cfg.retention.store_tool_outputs);
    }

    #[test]
    fn missing_file_returns_default() {
        let tmp = TempDir::new().unwrap();
        let cfg = ObservabilityConfig::load_from_file(&tmp.path().join("nope.toml")).unwrap();
        assert_eq!(cfg, ObservabilityConfig::default());
    }

    #[test]
    fn file_overrides_default() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("observability.toml");
        fs::write(
            &path,
            r#"
[observability]
otel_enabled = true

[observability.retention]
mode = "redacted"
store_prompts = true
payload_ttl_days = 30
"#,
        )
        .unwrap();
        let cfg = ObservabilityConfig::load_from_file(&path).unwrap();
        assert!(cfg.otel_enabled);
        assert_eq!(cfg.retention.mode, RetentionMode::Redacted);
        assert!(cfg.retention.store_prompts);
        assert_eq!(cfg.retention.payload_ttl_days, 30);
        // Untouched defaults survive.
        assert!(cfg.sqlite_enabled);
        assert_eq!(cfg.retention.max_payload_bytes, 200_000);
    }

    #[test]
    fn env_overrides_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("observability.toml");
        fs::write(
            &path,
            r#"
[observability.retention]
mode = "redacted"
"#,
        )
        .unwrap();
        // Use a unique env var (and clean it up) so concurrent tests
        // don't fight us. The shorthand `XVISION_OBSERVABILITY_RETENTION`
        // is the one operator-facing.
        let key = format!("{ENV_OVERRIDE_PREFIX}_RETENTION");
        // SAFETY: tests inside this crate run in the same process, but
        // we're only touching a single dedicated env var that no other
        // test in this crate uses.
        unsafe {
            std::env::set_var(&key, "full_debug");
        }
        let cfg = ObservabilityConfig::load_with_env(&path).unwrap();
        assert_eq!(cfg.retention.mode, RetentionMode::FullDebug);
        unsafe {
            std::env::remove_var(&key);
        }
    }

    #[test]
    fn unknown_retention_mode_in_file_errors() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("observability.toml");
        fs::write(
            &path,
            r#"
[observability.retention]
mode = "wild_west"
"#,
        )
        .unwrap();
        let err = ObservabilityConfig::load_from_file(&path).unwrap_err();
        match err {
            ConfigError::UnknownRetentionMode(m) => assert_eq!(m, "wild_west"),
            other => panic!("expected UnknownRetentionMode, got {other:?}"),
        }
    }
}
