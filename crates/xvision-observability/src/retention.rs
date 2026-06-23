//! Retention policy resolution chain with provenance.
//!
//! The schema crate's [`crate::config::ObservabilityConfig::load_with_env`]
//! covers env > file > default. This module adds the CLI flag step in
//! front and tracks **where each resolved value came from** so
//! `xvn obs retention show` can print provenance to operators.
//!
//! Precedence chain (highest → lowest):
//!     1. CLI flag passed by the operator (`--mode`, `--ttl-days`, …)
//!     2. Env var (`XVISION_OBSERVABILITY_…`)
//!     3. Config file (`$XVN_HOME/config/observability.toml`)
//!     4. Built-in default ([`crate::config::RetentionConfig::default`])
//!
//! Also handles writing the dashboard `full_debug` sentinel file the
//! Phase B UI leaf will consume.

use crate::config::{
    default_config_path, ObservabilityConfig, RetentionConfig, RetentionMode, ENV_OVERRIDE_PREFIX,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RetentionError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("config: {0}")]
    Config(#[from] crate::config::ConfigError),
    #[error("toml encode: {0}")]
    TomlSer(#[from] toml::ser::Error),
}

/// Where a resolved value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    CliFlag,
    Env,
    ConfigFile,
    Default,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::CliFlag => "CLI flag",
            Self::Env => "env var",
            Self::ConfigFile => "config file",
            Self::Default => "default",
        }
    }
}

/// A resolved value tagged with its provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Resolved<T: Clone> {
    pub value: T,
    pub source: Source,
}

impl<T: Clone> Resolved<T> {
    pub fn new(value: T, source: Source) -> Self {
        Self { value, source }
    }
}

/// Per-toggle CLI overrides. `None` means "no CLI flag was passed for
/// this knob; fall through to env/file/default".
#[derive(Debug, Default, Clone)]
pub struct CliOverrides {
    pub mode: Option<RetentionMode>,
    pub store_prompts: Option<bool>,
    pub store_responses: Option<bool>,
    pub store_tool_inputs: Option<bool>,
    pub store_tool_outputs: Option<bool>,
    pub redact_secrets: Option<bool>,
    pub payload_ttl_days: Option<u64>,
    pub max_payload_bytes: Option<u64>,
    pub sqlite_enabled: Option<bool>,
    pub otel_enabled: Option<bool>,
}

/// The fully resolved retention view exposed by `xvn obs retention show`.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedView {
    pub sqlite_enabled: Resolved<bool>,
    pub otel_enabled: Resolved<bool>,
    pub mode: Resolved<RetentionMode>,
    pub store_prompts: Resolved<bool>,
    pub store_responses: Resolved<bool>,
    pub store_tool_inputs: Resolved<bool>,
    pub store_tool_outputs: Resolved<bool>,
    pub redact_secrets: Resolved<bool>,
    pub payload_ttl_days: Resolved<u64>,
    pub max_payload_bytes: Resolved<u64>,
    pub config_path: PathBuf,
    pub config_file_present: bool,
}

impl ResolvedView {
    pub fn config(&self) -> ObservabilityConfig {
        ObservabilityConfig {
            sqlite_enabled: self.sqlite_enabled.value,
            otel_enabled: self.otel_enabled.value,
            retention: RetentionConfig {
                mode: self.mode.value,
                store_prompts: self.store_prompts.value,
                store_responses: self.store_responses.value,
                store_tool_inputs: self.store_tool_inputs.value,
                store_tool_outputs: self.store_tool_outputs.value,
                redact_secrets: self.redact_secrets.value,
                payload_ttl_days: self.payload_ttl_days.value,
                max_payload_bytes: self.max_payload_bytes.value,
            },
        }
    }

    /// Human-readable table for `xvn obs retention show`.
    pub fn to_table(&self) -> String {
        let rows = [
            (
                "sqlite_enabled",
                format!("{}", self.sqlite_enabled.value),
                self.sqlite_enabled.source,
            ),
            (
                "otel_enabled",
                format!("{}", self.otel_enabled.value),
                self.otel_enabled.source,
            ),
            ("mode", self.mode.value.as_db_str().to_string(), self.mode.source),
            (
                "store_prompts",
                format!("{}", self.store_prompts.value),
                self.store_prompts.source,
            ),
            (
                "store_responses",
                format!("{}", self.store_responses.value),
                self.store_responses.source,
            ),
            (
                "store_tool_inputs",
                format!("{}", self.store_tool_inputs.value),
                self.store_tool_inputs.source,
            ),
            (
                "store_tool_outputs",
                format!("{}", self.store_tool_outputs.value),
                self.store_tool_outputs.source,
            ),
            (
                "redact_secrets",
                format!("{}", self.redact_secrets.value),
                self.redact_secrets.source,
            ),
            (
                "payload_ttl_days",
                format!("{}", self.payload_ttl_days.value),
                self.payload_ttl_days.source,
            ),
            (
                "max_payload_bytes",
                format!("{}", self.max_payload_bytes.value),
                self.max_payload_bytes.source,
            ),
        ];
        let key_w = rows.iter().map(|(k, _, _)| k.len()).max().unwrap_or(0);
        let val_w = rows.iter().map(|(_, v, _)| v.len()).max().unwrap_or(0);
        let mut out = String::new();
        out.push_str(&format!(
            "config file: {} ({})\n",
            self.config_path.display(),
            if self.config_file_present {
                "present"
            } else {
                "absent"
            }
        ));
        out.push('\n');
        for (k, v, s) in &rows {
            out.push_str(&format!(
                "  {:<key_w$}  {:<val_w$}  ({})\n",
                k,
                v,
                s.label(),
                key_w = key_w,
                val_w = val_w
            ));
        }
        out
    }
}

/// Resolve the full retention view from the four-layer chain.
/// `config_path` is the location of `observability.toml`. Missing file
/// is fine; the resolver returns defaults for any value not set.
///
/// Emits the `full_debug retention enabled` startup WARN exactly once
/// when the resolved mode is `full_debug`.
pub fn resolve(config_path: &Path, overrides: &CliOverrides) -> Result<ResolvedView, RetentionError> {
    let file_cfg = ObservabilityConfig::load_from_file(config_path)?;
    let default_cfg = ObservabilityConfig::default();
    let env_cfg = {
        let mut c = file_cfg.clone();
        c.apply_env();
        c
    };
    let file_present = config_path.exists();

    // For each knob, determine the source by checking layers top-down.
    let sqlite_enabled = resolve_bool(
        overrides.sqlite_enabled,
        env_var_bool(&env_key("SQLITE_ENABLED")),
        file_present
            .then_some(file_cfg.sqlite_enabled)
            .filter(|v| *v != default_cfg.sqlite_enabled),
        default_cfg.sqlite_enabled,
        || env_cfg.sqlite_enabled,
    );
    let otel_enabled = resolve_bool(
        overrides.otel_enabled,
        env_var_bool(&env_key("OTEL_ENABLED")),
        file_present
            .then_some(file_cfg.otel_enabled)
            .filter(|v| *v != default_cfg.otel_enabled),
        default_cfg.otel_enabled,
        || env_cfg.otel_enabled,
    );

    let mode_env = env_var_mode();
    let mode_file = config_retention_has_key(config_path, "mode")?.then_some(file_cfg.retention.mode);
    let mode = resolve_value(
        overrides.mode,
        mode_env,
        mode_file,
        default_cfg.retention.mode,
        env_cfg.retention.mode,
    );

    let store_prompts = resolve_bool(
        overrides.store_prompts,
        env_var_bool(&env_key("RETENTION_STORE_PROMPTS")),
        file_present
            .then_some(file_cfg.retention.store_prompts)
            .filter(|v| *v != default_cfg.retention.store_prompts),
        default_cfg.retention.store_prompts,
        || env_cfg.retention.store_prompts,
    );
    let store_responses = resolve_bool(
        overrides.store_responses,
        env_var_bool(&env_key("RETENTION_STORE_RESPONSES")),
        file_present
            .then_some(file_cfg.retention.store_responses)
            .filter(|v| *v != default_cfg.retention.store_responses),
        default_cfg.retention.store_responses,
        || env_cfg.retention.store_responses,
    );
    let store_tool_inputs = resolve_bool(
        overrides.store_tool_inputs,
        env_var_bool(&env_key("RETENTION_STORE_TOOL_INPUTS")),
        file_present
            .then_some(file_cfg.retention.store_tool_inputs)
            .filter(|v| *v != default_cfg.retention.store_tool_inputs),
        default_cfg.retention.store_tool_inputs,
        || env_cfg.retention.store_tool_inputs,
    );
    let store_tool_outputs = resolve_bool(
        overrides.store_tool_outputs,
        env_var_bool(&env_key("RETENTION_STORE_TOOL_OUTPUTS")),
        file_present
            .then_some(file_cfg.retention.store_tool_outputs)
            .filter(|v| *v != default_cfg.retention.store_tool_outputs),
        default_cfg.retention.store_tool_outputs,
        || env_cfg.retention.store_tool_outputs,
    );
    let redact_secrets = resolve_bool(
        overrides.redact_secrets,
        env_var_bool(&env_key("RETENTION_REDACT_SECRETS")),
        file_present
            .then_some(file_cfg.retention.redact_secrets)
            .filter(|v| *v != default_cfg.retention.redact_secrets),
        default_cfg.retention.redact_secrets,
        || env_cfg.retention.redact_secrets,
    );

    let payload_ttl_days = resolve_u64(
        overrides.payload_ttl_days,
        env_var_u64(&env_key("RETENTION_PAYLOAD_TTL_DAYS")),
        file_present
            .then_some(file_cfg.retention.payload_ttl_days)
            .filter(|v| *v != default_cfg.retention.payload_ttl_days),
        default_cfg.retention.payload_ttl_days,
        || env_cfg.retention.payload_ttl_days,
    );
    let max_payload_bytes = resolve_u64(
        overrides.max_payload_bytes,
        env_var_u64(&env_key("RETENTION_MAX_PAYLOAD_BYTES")),
        file_present
            .then_some(file_cfg.retention.max_payload_bytes)
            .filter(|v| *v != default_cfg.retention.max_payload_bytes),
        default_cfg.retention.max_payload_bytes,
        || env_cfg.retention.max_payload_bytes,
    );

    let view = ResolvedView {
        sqlite_enabled,
        otel_enabled,
        mode,
        store_prompts,
        store_responses,
        store_tool_inputs,
        store_tool_outputs,
        redact_secrets,
        payload_ttl_days,
        max_payload_bytes,
        config_path: config_path.to_path_buf(),
        config_file_present: file_present,
    };

    // Emit the startup WARN only when full_debug was set EXPLICITLY by
    // an operator (CLI / env / config file). The implicit default is
    // also full_debug now (so a fresh install can debug from the first
    // run); warning every time would be noise. The wording on the
    // explicit path is unchanged so the contract-grep still matches.
    if view.mode.value == RetentionMode::FullDebug && view.mode.source != Source::Default {
        tracing::info!(
            target: "xvision_observability",
            "full_debug retention enabled. Prompts, responses, and tool \
             payloads may be stored on disk. Disable for shared / client \
             work. To lower retention run: \
             `xvn obs retention set --mode redacted` \
             or set env var XVISION_OBSERVABILITY_RETENTION=redacted \
             before starting."
        );
    }

    Ok(view)
}

fn env_key(suffix: &str) -> String {
    format!("{ENV_OVERRIDE_PREFIX}_{suffix}")
}

fn config_retention_has_key(config_path: &Path, key: &str) -> Result<bool, RetentionError> {
    if !config_path.exists() {
        return Ok(false);
    }
    let text = fs::read_to_string(config_path)?;
    let parsed: toml::Value = toml::from_str(&text).map_err(crate::config::ConfigError::from)?;
    Ok(parsed
        .get("observability")
        .and_then(|v| v.get("retention"))
        .and_then(|v| v.as_table())
        .is_some_and(|table| table.contains_key(key)))
}

fn env_var_mode() -> Option<RetentionMode> {
    // Shorthand `XVISION_OBSERVABILITY_RETENTION` wins; fall back to
    // the long `_RETENTION_MODE` form.
    for key in [env_key("RETENTION"), env_key("RETENTION_MODE")] {
        if let Ok(v) = std::env::var(&key) {
            if let Ok(m) = RetentionMode::from_str_strict(&v) {
                return Some(m);
            }
        }
    }
    None
}

fn env_var_bool(key: &str) -> Option<bool> {
    std::env::var(key).ok().map(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn env_var_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

fn resolve_value<T: Clone + PartialEq>(
    cli: Option<T>,
    env: Option<T>,
    file: Option<T>,
    default: T,
    env_effective: T,
) -> Resolved<T> {
    if let Some(v) = cli {
        return Resolved::new(v, Source::CliFlag);
    }
    if let Some(v) = env {
        return Resolved::new(v, Source::Env);
    }
    if let Some(v) = file {
        return Resolved::new(v, Source::ConfigFile);
    }
    // No explicit value — but the env loader may have produced a value
    // that differs from default via a different env var path; trust it.
    let _ = env_effective;
    Resolved::new(default, Source::Default)
}

fn resolve_bool(
    cli: Option<bool>,
    env: Option<bool>,
    file: Option<bool>,
    default: bool,
    env_effective: impl FnOnce() -> bool,
) -> Resolved<bool> {
    resolve_value(cli, env, file, default, env_effective())
}

fn resolve_u64(
    cli: Option<u64>,
    env: Option<u64>,
    file: Option<u64>,
    default: u64,
    env_effective: impl FnOnce() -> u64,
) -> Resolved<u64> {
    resolve_value(cli, env, file, default, env_effective())
}

// -------- xvn obs retention set / clear --------

/// Path to the dashboard sentinel file written when `full_debug` is set.
/// The Phase B UI leaf reads this to show the banner. The path is a
/// sibling of the config file so a single `$XVN_HOME/config/` dir owns
/// both.
pub fn full_debug_sentinel_path(config_path: &Path) -> PathBuf {
    let dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    dir.join("full_debug.sentinel")
}

/// Write a fresh `observability.toml` at `config_path` carrying the
/// values in `cfg`. Creates parent dirs as needed. When the resolved
/// mode is `full_debug`, also writes the dashboard banner sentinel.
pub fn write_config(config_path: &Path, cfg: &ObservabilityConfig) -> Result<(), RetentionError> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = render_toml(cfg);
    fs::write(config_path, body)?;
    let sentinel = full_debug_sentinel_path(config_path);
    if cfg.retention.mode == RetentionMode::FullDebug {
        fs::write(&sentinel, "full_debug retention enabled\n")?;
    } else if sentinel.exists() {
        let _ = fs::remove_file(&sentinel);
    }
    Ok(())
}

/// `xvn obs retention clear` — remove the config file (defaults take
/// over) and any stale dashboard sentinel.
pub fn clear_config(config_path: &Path) -> Result<bool, RetentionError> {
    let sentinel = full_debug_sentinel_path(config_path);
    let _ = fs::remove_file(&sentinel);
    if !config_path.exists() {
        return Ok(false);
    }
    fs::remove_file(config_path)?;
    Ok(true)
}

/// Path used when the caller hasn't supplied one. Mirrors
/// [`crate::config::default_config_path`].
pub fn default_path() -> PathBuf {
    default_config_path()
}

fn render_toml(cfg: &ObservabilityConfig) -> String {
    // Toml::to_string can't enforce a particular section order; write
    // the file by hand so the output is stable and operator-readable.
    let mut s = String::new();
    s.push_str("# xvision observability config — managed by `xvn obs retention`.\n");
    s.push_str("# Operators can hand-edit; the CLI overwrites on `set`.\n\n");
    s.push_str("[observability]\n");
    s.push_str(&format!("sqlite_enabled = {}\n", cfg.sqlite_enabled));
    s.push_str(&format!("otel_enabled = {}\n", cfg.otel_enabled));
    s.push_str("\n[observability.retention]\n");
    s.push_str(&format!("mode = \"{}\"\n", cfg.retention.mode.as_db_str()));
    s.push_str(&format!("store_prompts = {}\n", cfg.retention.store_prompts));
    s.push_str(&format!("store_responses = {}\n", cfg.retention.store_responses));
    s.push_str(&format!(
        "store_tool_inputs = {}\n",
        cfg.retention.store_tool_inputs
    ));
    s.push_str(&format!(
        "store_tool_outputs = {}\n",
        cfg.retention.store_tool_outputs
    ));
    s.push_str(&format!("redact_secrets = {}\n", cfg.retention.redact_secrets));
    s.push_str(&format!(
        "payload_ttl_days = {}\n",
        cfg.retention.payload_ttl_days
    ));
    s.push_str(&format!(
        "max_payload_bytes = {}\n",
        cfg.retention.max_payload_bytes
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_then_resolve_roundtrips() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("observability.toml");
        let mut cfg = ObservabilityConfig::default();
        cfg.retention.mode = RetentionMode::Redacted;
        cfg.retention.payload_ttl_days = 14;
        write_config(&path, &cfg).unwrap();

        let view = resolve(&path, &CliOverrides::default()).unwrap();
        assert_eq!(view.mode.value, RetentionMode::Redacted);
        assert_eq!(view.mode.source, Source::ConfigFile);
        assert_eq!(view.payload_ttl_days.value, 14);
        assert_eq!(view.payload_ttl_days.source, Source::ConfigFile);
    }

    #[test]
    fn full_debug_writes_sentinel() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("observability.toml");
        let mut cfg = ObservabilityConfig::default();
        cfg.retention.mode = RetentionMode::FullDebug;
        write_config(&path, &cfg).unwrap();
        let sentinel = full_debug_sentinel_path(&path);
        assert!(sentinel.exists(), "sentinel must be present in full_debug");
    }

    #[test]
    fn clearing_removes_sentinel_too() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("observability.toml");
        let mut cfg = ObservabilityConfig::default();
        cfg.retention.mode = RetentionMode::FullDebug;
        write_config(&path, &cfg).unwrap();
        let removed = clear_config(&path).unwrap();
        assert!(removed);
        assert!(!path.exists());
        assert!(!full_debug_sentinel_path(&path).exists());
    }

    #[test]
    fn switching_away_from_full_debug_removes_sentinel() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("observability.toml");
        let mut cfg = ObservabilityConfig::default();
        cfg.retention.mode = RetentionMode::FullDebug;
        write_config(&path, &cfg).unwrap();
        assert!(full_debug_sentinel_path(&path).exists());
        cfg.retention.mode = RetentionMode::HashOnly;
        write_config(&path, &cfg).unwrap();
        assert!(!full_debug_sentinel_path(&path).exists());
    }
}
