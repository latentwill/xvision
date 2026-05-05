//! TOML config loader for `default.toml`, `whitelist.toml`, `risk.toml`.
//!
//! Three separate files because they have different lifecycles:
//! - `default.toml` — checked in, edited per environment via env-var overrides
//! - `whitelist.toml` — checked in, edited rarely (asset universe changes)
//! - `risk.toml` — checked in, edited via PR with risk-management review
//!
//! All errors flow through `ConfigError` — no `panic!` in the load path.

use std::path::{Path, PathBuf};

use garde::Validate;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    NotFound(PathBuf),
    #[error("io error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("toml parse error in {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("validation failed for {path}: {report}")]
    Validation { path: PathBuf, report: garde::Report },
    #[error("cross-field validation failed for {path}: {message}")]
    CrossField { path: PathBuf, message: String },
}

// --- runtime ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[garde(skip)]
    pub runtime: Runtime,
    #[garde(dive)]
    pub intern: Intern,
    #[garde(dive)]
    pub trader: Trader,
    #[garde(dive)]
    pub backtest: Backtest,
    #[garde(skip)]
    pub paths: Paths,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Runtime {
    pub mode: RunMode,
    pub executor: ExecutorKind,
    pub random_seed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    Backtest,
    Paper,
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutorKind {
    Alpaca,
    Orderly,
}

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct Intern {
    #[garde(skip)]
    pub provider: InternProvider,
    #[garde(skip)]
    pub base_url: String,
    #[garde(length(min = 1))]
    pub model: String,
    #[garde(skip)]
    pub api_key_env: String,
    #[garde(range(min = 0.0, max = 2.0))]
    pub temperature: f32,
    #[garde(skip)]
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[garde(range(min = 1, max = 16384))]
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InternProvider {
    Anthropic,
    OpenaiCompat,
    LocalCandle,
    /// Subprocess to the `acpx` CLI driving an Agent Client Protocol
    /// harness (codex / claude / openclaw / pi). See FOLLOWUPS F21.
    Acpx,
}

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct Trader {
    #[garde(length(min = 1))]
    pub model_path: String,
    #[garde(range(min = 0.0, max = 2.0))]
    pub temperature: f32,
    #[garde(range(min = 0.0, max = 2.0))]
    pub forward_paper_temperature: f32,
    #[garde(range(min = 1, max = 8192))]
    pub max_tokens: u32,
    #[garde(dive)]
    pub vectors: VectorsConfig,
}

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct VectorsConfig {
    #[garde(skip)]
    pub enabled: bool,
    #[garde(skip)]
    pub config: VectorArm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorArm {
    Off,
    On,
    Random,
    Orthogonal,
    RegimeConditioned,
}

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct Backtest {
    #[garde(range(min = 1, max = 1000))]
    pub step: u32,
    #[garde(range(min = 1, max = 1000))]
    pub horizon: u32,
    #[garde(range(min = 100, max = 1_000_000))]
    pub bootstrap_resamples: u32,
    #[garde(range(min = 1, max = 1000))]
    pub bootstrap_block_size: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paths {
    pub data_root: String,
    pub vectors: String,
    pub probes: String,
    pub sqlite_url: String,
}

impl Backtest {
    /// Tier 1 fix #4: `step >= horizon` enforced post-parse so the message is
    /// actionable.
    pub fn validate_step_vs_horizon(&self) -> Result<(), String> {
        if self.step < self.horizon {
            Err(format!(
                "backtest.step ({}) must be >= backtest.horizon ({}) — Tier 1 fix #4",
                self.step, self.horizon
            ))
        } else {
            Ok(())
        }
    }
}

// --- whitelist --------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct WhitelistConfig {
    #[garde(dive)]
    pub assets: Vec<AssetEntry>,
}

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct AssetEntry {
    #[garde(length(min = 1, max = 10))]
    pub symbol: String,
    #[garde(skip)]
    pub enabled: bool,
    #[garde(length(min = 1, max = 32))]
    pub cluster: String,
    #[garde(skip)]
    pub venues: AssetVenues,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetVenues {
    pub alpaca: String,
    pub orderly: String,
}

impl WhitelistConfig {
    pub fn enabled_symbols(&self) -> Vec<&str> {
        self.assets
            .iter()
            .filter(|a| a.enabled)
            .map(|a| a.symbol.as_str())
            .collect()
    }
}

// --- risk -------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct RiskConfig {
    #[garde(dive)]
    pub limits: RiskLimits,
    #[garde(dive)]
    pub stops: RiskStops,
}

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct RiskLimits {
    #[garde(range(min = 0.1, max = 100.0))]
    pub max_position_pct_nav: f32,
    #[garde(range(min = 0.1, max = 500.0))]
    pub max_total_exposure_pct: f32,
    #[garde(range(min = 1, max = 50))]
    pub max_open_positions: u32,
    #[garde(range(min = 0.1, max = 100.0))]
    pub max_daily_loss_pct: f32,
    #[garde(range(min = 1, max = 50))]
    pub max_correlation_cluster: u32,
}

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct RiskStops {
    #[garde(skip)]
    pub stop_loss_required: bool,
    #[garde(range(min = 0.01, max = 50.0))]
    pub stop_loss_min_pct: f32,
    #[garde(range(min = 0.01, max = 50.0))]
    pub stop_loss_max_pct: f32,
    #[garde(skip)]
    pub take_profit_required: bool,
    #[garde(range(min = 0.5, max = 10.0))]
    pub take_profit_min_rr: f32,
}

// --- loader -----------------------------------------------------------------

fn read_toml<T: for<'de> Deserialize<'de> + Validate<Context = ()>>(path: &Path) -> Result<T, ConfigError> {
    let bytes = std::fs::read(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ConfigError::NotFound(path.to_path_buf()),
        _ => ConfigError::Io {
            path: path.to_path_buf(),
            source: e,
        },
    })?;
    let s = String::from_utf8(bytes).map_err(|e| ConfigError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })?;
    let parsed: T = toml::from_str(&s).map_err(|e| ConfigError::Parse {
        path: path.to_path_buf(),
        source: e,
    })?;
    parsed.validate().map_err(|report| ConfigError::Validation {
        path: path.to_path_buf(),
        report,
    })?;
    Ok(parsed)
}

pub fn load_runtime(path: &Path) -> Result<RuntimeConfig, ConfigError> {
    let cfg: RuntimeConfig = read_toml(path)?;
    cfg.backtest
        .validate_step_vs_horizon()
        .map_err(|msg| ConfigError::CrossField {
            path: path.to_path_buf(),
            message: msg,
        })?;
    Ok(cfg)
}

pub fn load_whitelist(path: &Path) -> Result<WhitelistConfig, ConfigError> {
    read_toml(path)
}

pub fn load_risk(path: &Path) -> Result<RiskConfig, ConfigError> {
    read_toml(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_root() -> PathBuf {
        // crates/xianvec-core -> ../..
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn loads_repo_default_toml() {
        let cfg =
            load_runtime(&project_root().join("config/default.toml")).expect("config/default.toml must load");
        assert_eq!(cfg.intern.temperature, 0.0, "Tier 1 fix #1");
        assert_eq!(cfg.trader.temperature, 0.0, "Tier 1 fix #2");
        assert!(cfg.backtest.step >= cfg.backtest.horizon, "Tier 1 fix #4");
    }

    #[test]
    fn loads_repo_whitelist_toml() {
        let cfg = load_whitelist(&project_root().join("config/whitelist.toml"))
            .expect("config/whitelist.toml must load");
        let enabled = cfg.enabled_symbols();
        assert_eq!(enabled, vec!["BTC"], "v1 BTC-only");
    }

    #[test]
    fn loads_repo_risk_toml() {
        let cfg = load_risk(&project_root().join("config/risk.toml")).expect("config/risk.toml must load");
        assert!(cfg.limits.max_position_pct_nav > 0.0);
        assert!(cfg.stops.stop_loss_required, "v1 requires stops");
    }

    #[test]
    fn rejects_missing_file() {
        match load_runtime(Path::new("/nonexistent/path/default.toml")) {
            Err(ConfigError::NotFound(_)) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn rejects_step_smaller_than_horizon() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, BAD_STEP_HORIZON).unwrap();
        match load_runtime(&path) {
            Err(ConfigError::CrossField { message, .. }) => {
                assert!(message.contains("step"), "actual: {message}");
            }
            other => panic!("expected CrossField, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_toml_syntax() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "not = valid toml = syntax").unwrap();
        match load_runtime(&path) {
            Err(ConfigError::Parse { .. }) => {}
            other => panic!("expected Parse, got {other:?}"),
        }
    }

    const BAD_STEP_HORIZON: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[intern]
provider = "anthropic"
base_url = "https://api.anthropic.com"
model = "x"
api_key_env = "K"
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
step = 8
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"
"#;
}
