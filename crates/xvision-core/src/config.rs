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

// --- providers --------------------------------------------------------------

/// One LLM provider, referenced by name from slot configs and arm specs.
/// `api_key_env` may be the empty string for endpoints that don't require auth
/// (local llama.cpp / Ollama / vLLM in --no-auth mode).
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct ProviderEntry {
    #[garde(custom(validate_provider_name))]
    pub name: String,
    #[garde(skip)]
    pub kind: ProviderKind,
    #[garde(length(min = 1, max = 512))]
    pub base_url: String,
    #[garde(length(max = 64))]
    pub api_key_env: String,
    /// Subset of the provider's catalog the operator has explicitly
    /// enabled for the chat-rail / wizard dropdown. Empty means
    /// "nothing picked yet" — the UI surfaces a prompt to open Settings
    /// → Providers → Manage models. Especially load-bearing for
    /// OpenRouter, which exposes hundreds of routes.
    #[serde(default)]
    #[garde(skip)]
    pub enabled_models: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    Anthropic,
    OpenaiCompat,
    LocalCandle,
}

impl From<InternProvider> for ProviderKind {
    fn from(p: InternProvider) -> Self {
        match p {
            InternProvider::Anthropic => Self::Anthropic,
            InternProvider::OpenaiCompat => Self::OpenaiCompat,
            InternProvider::LocalCandle => Self::LocalCandle,
        }
    }
}

impl ProviderEntry {
    /// True iff this entry's kind/base_url/api_key_env triple matches the
    /// supplied tuple. Used by auto-derivation to skip when the user has
    /// already declared an equivalent row.
    pub fn matches_triple(&self, kind: ProviderKind, base_url: &str, api_key_env: &str) -> bool {
        self.kind == kind && self.base_url == base_url && self.api_key_env == api_key_env
    }
}

fn validate_provider_name(name: &String, _ctx: &()) -> garde::Result {
    if name.is_empty() || name.len() > 32 {
        return Err(garde::Error::new("provider name must be 1..=32 chars"));
    }
    if name.starts_with('_') {
        // The leading-underscore namespace is reserved for internal rows.
        return Err(garde::Error::new(
            "provider names starting with '_' are reserved",
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(garde::Error::new(
            "provider name must match [a-z0-9-]+",
        ));
    }
    Ok(())
}

// --- runtime ----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[garde(skip)]
    pub runtime: Runtime,
    #[serde(default)]
    #[garde(dive)]
    pub providers: Vec<ProviderEntry>,
    /// Optional workspace-level default LLM (used by chat-rail, wizard, and
    /// any agent slot that doesn't override its own provider/model). Accepts
    /// `[default_llm]` (canonical) or `[intern]` (legacy alias kept for one
    /// release for backward compatibility with existing user configs).
    #[serde(default, alias = "intern")]
    #[garde(dive)]
    pub default_llm: Option<Intern>,
    #[garde(dive)]
    pub trader: Trader,
    #[garde(dive)]
    pub backtest: Backtest,
    #[garde(skip)]
    pub paths: Paths,
    /// Market-data fetcher knobs. Optional with a fully-defaulted struct so
    /// older config files (and tests using inline TOML) keep loading.
    #[serde(default)]
    #[garde(dive)]
    pub data: Data,
}

/// Top-level `[data]` section. Holds per-fetcher knobs; today only Alpaca.
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize, Default)]
pub struct Data {
    #[serde(default)]
    #[garde(dive)]
    pub alpaca: AlpacaData,
}

/// `[data.alpaca]` knobs read by `xvision-engine` when constructing the
/// `AlpacaBarsFetcher`. Defaults match Alpaca's free-tier crypto-data limit.
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct AlpacaData {
    #[serde(default = "AlpacaData::default_rate_limit_rpm")]
    #[garde(range(min = 1, max = 10_000))]
    pub rate_limit_rpm: u32,
}

impl AlpacaData {
    pub const DEFAULT_RATE_LIMIT_RPM: u32 = 200;

    fn default_rate_limit_rpm() -> u32 {
        Self::DEFAULT_RATE_LIMIT_RPM
    }
}

impl Default for AlpacaData {
    fn default() -> Self {
        Self {
            rate_limit_rpm: Self::DEFAULT_RATE_LIMIT_RPM,
        }
    }
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
    validate_unique_provider_names(&cfg).map_err(|msg| ConfigError::CrossField {
        path: path.to_path_buf(),
        message: msg,
    })?;
    Ok(cfg)
}

fn validate_unique_provider_names(cfg: &RuntimeConfig) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for p in &cfg.providers {
        if !seen.insert(p.name.as_str()) {
            return Err(format!("duplicate provider name `{}`", p.name));
        }
    }
    Ok(())
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
        // crates/xvision-core -> ../..
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
        assert_eq!(
            cfg.default_llm.as_ref().unwrap().temperature,
            0.0,
            "Tier 1 fix #1"
        );
        assert_eq!(cfg.trader.temperature, 0.0, "Tier 1 fix #2");
        assert!(cfg.backtest.step >= cfg.backtest.horizon, "Tier 1 fix #4");
        assert_eq!(
            cfg.data.alpaca.rate_limit_rpm,
            AlpacaData::DEFAULT_RATE_LIMIT_RPM,
            "default.toml ships with the documented Alpaca rate limit"
        );
    }

    #[test]
    fn data_alpaca_section_defaults_when_omitted() {
        // Older configs that don't yet have a `[data.alpaca]` section must
        // still load (serde default fills the gap with 200 rpm).
        let toml_src = r#"
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
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no-data.toml");
        std::fs::write(&path, toml_src).unwrap();
        let cfg = load_runtime(&path).unwrap();
        assert_eq!(
            cfg.data.alpaca.rate_limit_rpm,
            AlpacaData::DEFAULT_RATE_LIMIT_RPM,
            "missing [data.alpaca] section must default-fill"
        );
    }

    #[test]
    fn runtime_config_loads_without_default_llm() {
        let toml_src = r#"
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
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no-default-llm.toml");
        std::fs::write(&path, toml_src).unwrap();
        let cfg = load_runtime(&path).unwrap();
        assert!(cfg.default_llm.is_none());
        assert!(cfg.providers.is_empty());
    }

    #[test]
    fn data_alpaca_rate_limit_rpm_round_trips() {
        let toml_src = r#"
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
step = 24
horizon = 16
bootstrap_resamples = 1000
bootstrap_block_size = 8

[paths]
data_root = "data"
vectors = "data/vectors"
probes = "data/probes"
sqlite_url = "sqlite://x.db"

[data.alpaca]
rate_limit_rpm = 600
"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rpm-override.toml");
        std::fs::write(&path, toml_src).unwrap();
        let cfg = load_runtime(&path).unwrap();
        assert_eq!(cfg.data.alpaca.rate_limit_rpm, 600);
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

    // --- providers (Plan #7 Phase 1) ----------------------------------------

    #[test]
    fn runtime_config_round_trips_with_providers() {
        let toml_src = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "anthropic"
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"

[[providers]]
name = "ollama-local"
kind = "openai-compat"
base_url = "http://localhost:11434/v1"
api_key_env = ""

[intern]
provider = "anthropic"
base_url = "https://api.anthropic.com"
model = "claude-haiku-4-5"
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
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("with-providers.toml");
        std::fs::write(&path, toml_src).unwrap();
        let cfg = load_runtime(&path).unwrap();
        // Two declared rows must round-trip; no provider rows are synthesized.
        assert_eq!(cfg.providers.len(), 2);
        assert!(cfg.providers.iter().any(|p| p.name == "anthropic"));
        assert!(cfg.providers.iter().any(|p| p.name == "ollama-local"));
    }

    #[test]
    fn repo_default_toml_ships_with_no_user_providers() {
        // The repo's default config no longer seeds [[providers]]; users add
        // their own via Settings -> Providers (or `xvn provider add`).
        let cfg = load_runtime(&project_root().join("config/default.toml")).unwrap();
        let user_rows: Vec<&str> = cfg
            .providers
            .iter()
            .map(|p| p.name.as_str())
            .filter(|n| !n.starts_with('_'))
            .collect();
        assert!(
            user_rows.is_empty(),
            "default.toml should ship without user provider rows, got {user_rows:?}"
        );
        assert!(
            cfg.providers.iter().all(|p| p.name != "_default_llm"),
            "default.toml should not synthesize `_default_llm` provider rows"
        );
    }

    #[test]
    fn does_not_auto_derive_default_llm_provider() {
        let cfg = load_runtime(&project_root().join("config/default.toml"))
            .expect("must load");
        assert!(cfg.providers.iter().all(|p| p.name != "_default_llm"));
    }

    #[test]
    fn declared_default_matching_provider_round_trips_without_synthetic() {
        let toml_src = r#"
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
model = "claude-haiku-4-5"
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
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("user-already-declared.toml");
        std::fs::write(&path, toml_src).unwrap();
        let cfg = load_runtime(&path).unwrap();
        assert_eq!(cfg.providers.len(), 1, "synthetic must not be added");
        assert_eq!(cfg.providers[0].name, "anthropic");
    }

    #[test]
    fn rejects_duplicate_provider_names() {
        let toml_src = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "p"
kind = "anthropic"
base_url = "https://a.example"
api_key_env = "A"

[[providers]]
name = "p"
kind = "openai-compat"
base_url = "https://b.example"
api_key_env = "B"

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
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dup-names.toml");
        std::fs::write(&path, toml_src).unwrap();
        match load_runtime(&path) {
            Err(ConfigError::CrossField { message, .. }) => {
                assert!(message.contains("duplicate provider name"), "actual: {message}");
            }
            other => panic!("expected CrossField, got {other:?}"),
        }
    }

    #[test]
    fn rejects_provider_name_with_underscore_prefix() {
        let toml_src = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "_mine"
kind = "anthropic"
base_url = "https://a.example"
api_key_env = "A"

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
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reserved-name.toml");
        std::fs::write(&path, toml_src).unwrap();
        match load_runtime(&path) {
            Err(ConfigError::Validation { .. }) => {}
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn provider_kind_round_trips_via_serde() {
        use ProviderKind::*;
        for k in [Anthropic, OpenaiCompat, LocalCandle] {
            let s = toml::to_string(&ProviderEntry {
                name: "p".into(),
                kind: k,
                base_url: "https://example.com".into(),
                api_key_env: "X".into(),
                enabled_models: Vec::new(),
            })
            .unwrap();
            let back: ProviderEntry = toml::from_str(&s).unwrap();
            assert_eq!(back.kind, k, "round trip failed for {:?}", k);
        }
    }

    #[test]
    fn provider_kind_serializes_to_kebab_case() {
        let v = toml::Value::try_from(ProviderKind::OpenaiCompat).unwrap();
        assert_eq!(v.as_str(), Some("openai-compat"));
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
