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

pub const XVN_CONFIG_PATH_ENV: &str = "XVN_CONFIG_PATH";
pub const XVN_CONFIG_ENV: &str = "XVN_CONFIG";

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
    // `local-candle` is an in-process/no-network provider, so an empty
    // base_url is valid for that kind. Route-level provider CRUD still
    // rejects empty URLs for auth-bearing network providers before writing.
    #[garde(length(max = 512))]
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
    Ollama,
    LlamaCpp,
    Vllm,
}

impl From<DefaultLlmProvider> for ProviderKind {
    fn from(p: DefaultLlmProvider) -> Self {
        match p {
            DefaultLlmProvider::Anthropic => Self::Anthropic,
            DefaultLlmProvider::OpenaiCompat => Self::OpenaiCompat,
            DefaultLlmProvider::LocalCandle => Self::LocalCandle,
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

/// Canonical provider-name rule, shared by the config loader (the garde
/// validator below) and the route-level provider CRUD (`add`). A name is
/// `[a-z0-9-]+`, 1..=32 chars, and may not start with `_` (the leading
/// underscore namespace is reserved for internal rows).
///
/// Returning a plain `Result<(), String>` (rather than a `garde::Error`) lets
/// the HTTP `add` path reuse this *before* it writes to `default.toml`, so an
/// invalid name is rejected up front instead of being persisted and only caught
/// by the post-write re-validation — which used to leave the file corrupt.
pub fn validate_provider_name_str(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 32 {
        return Err("provider name must be 1..=32 chars".to_string());
    }
    if name.starts_with('_') {
        return Err("provider names starting with '_' are reserved".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("provider name must match [a-z0-9-]+".to_string());
    }
    Ok(())
}

fn validate_provider_name(name: &str, _ctx: &()) -> garde::Result {
    validate_provider_name_str(name).map_err(garde::Error::new)
}

// --- runtime ----------------------------------------------------------------

/// Which agent runtime drives LLM-backed slots.
///
/// As of WU-6 (Task 1.6 — trader LlmDispatch retirement), **Cline is the
/// ONLY runtime**. The `LlmDispatch` enum variant has been removed; the
/// trader unconditionally routes through the `xvision-agentd` sidecar.
/// The `LlmDispatch` *trait* (`crate::agent::llm::LlmDispatch`) is kept
/// for the optimizer mutator/judge, CLI eval, and provider dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentRuntime {
    #[default]
    Cline,
}

impl std::str::FromStr for AgentRuntime {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cline" => Ok(Self::Cline),
            other => Err(format!(
                "unknown agent runtime: {other} (only \"cline\" is valid; LlmDispatch was retired in WU-6)"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[garde(skip)]
    pub runtime: Runtime,
    /// Agent runtime selector. Always `Cline` since WU-6 retired LlmDispatch.
    /// `#[serde(default)]` so existing configs (including those that carried
    /// `agent_runtime = "cline"` explicitly) keep loading without error.
    #[serde(default)]
    #[garde(skip)]
    pub agent_runtime: AgentRuntime,
    #[serde(default)]
    #[garde(dive)]
    pub providers: Vec<ProviderEntry>,
    /// Optional workspace-level default LLM (used by chat-rail, wizard, and
    /// any agent slot that doesn't override its own provider/model). Configured
    /// via `[default_llm]` in the TOML config.
    #[serde(default)]
    #[garde(skip)]
    pub default_llm: Option<DefaultLlmConfig>,
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
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutorKind {
    Alpaca,
    Orderly,
    Byreal,
}

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct DefaultLlmConfig {
    #[garde(skip)]
    pub provider: DefaultLlmProvider,
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
pub enum DefaultLlmProvider {
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
    /// Ticker symbol (e.g. "BTC", "1000BONK", "SPY_MYTHOS").
    #[garde(length(min = 1, max = 32))]
    pub symbol: String,
    #[garde(skip)]
    pub enabled: bool,
    /// Asset category. The legacy field name `cluster` is accepted as an alias.
    #[garde(length(min = 1, max = 32))]
    #[serde(alias = "cluster")]
    pub category: String,
    /// Data source ("alpaca" | "orderly-only"). Ignored by this crate; present
    /// so the struct round-trips the generated whitelist without errors.
    #[garde(skip)]
    #[serde(default)]
    pub data: String,
    #[garde(skip)]
    #[serde(default)]
    pub venues: AssetVenues,
}

/// Per-venue symbol mappings. Both fields are optional so that alpaca-only
/// and orderly-only assets can omit the field they don't support.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AssetVenues {
    #[serde(default)]
    pub alpaca: Option<String>,
    #[serde(default)]
    pub orderly: Option<String>,
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

// --- byreal spot curated set ------------------------------------------------

/// Token category for a curated Solana-spot asset. `xstock` (Backed Finance
/// tokenized equities) is a plain SPL token; the tag drives display only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpotAssetKind {
    Spl,
    Xstock,
}

/// One operator-curated Solana-spot asset.
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct SpotAssetEntry {
    /// Ticker symbol used by the agent and `xvn spot` (e.g. "SOL", "AAPLx").
    #[garde(length(min = 1, max = 32))]
    pub symbol: String,
    /// SPL mint address (base58). xStocks are plain SPL mints.
    #[garde(length(min = 32, max = 64))]
    pub mint: String,
    #[garde(skip)]
    pub kind: SpotAssetKind,
    /// On-chain decimals for the mint (informational; byreal-cli auto-resolves
    /// decimals from UI amounts).
    #[garde(range(min = 0, max = 18))]
    pub decimals: u8,
}

/// Curated whitelist mapping `symbol → { mint, kind, decimals }`, plus the
/// USDC mint used as the quote leg of every buy/sell swap. Out-of-set symbols
/// are refused (whitelist, not a hint list).
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct SpotAssetConfig {
    /// USDC SPL mint used as the quote asset for buys (USDC→token) and sells.
    #[garde(length(min = 32, max = 64))]
    pub usdc_mint: String,
    #[garde(dive)]
    pub assets: Vec<SpotAssetEntry>,
}

impl SpotAssetConfig {
    /// Case-insensitive ticker lookup. `None` for symbols outside the curated set.
    pub fn resolve(&self, symbol: &str) -> Option<&SpotAssetEntry> {
        self.assets.iter().find(|a| a.symbol.eq_ignore_ascii_case(symbol))
    }
}

// --- risk -------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskConfig {
    #[garde(dive)]
    pub limits: RiskLimits,
    #[garde(dive)]
    pub stops: RiskStops,
    /// Per-venue deterministic broker constraints (e.g. minimum
    /// notional). xvision-core passes them through so `config/risk.toml`
    /// deserializes here; the engine reads them directly. See
    /// `risk-gate-min-notional` contract for details.
    #[garde(skip)]
    #[serde(default)]
    pub venues: std::collections::BTreeMap<String, RiskVenueLimits>,
    /// Perps-guard thresholds. Like `venues`, xvision-core mirrors this section
    /// only so `config/risk.toml` deserializes here too; the thresholds are
    /// a vestigial global mirror (the engine reads per-strategy `strategy.risk`).
    /// Absent `[perps]` ⇒ default (all `None`).
    #[garde(skip)]
    #[serde(default)]
    pub perps: RiskPerpsGuards,
}

/// Pass-through mirror of the `[perps]` guard config from `risk.toml`. Deliberately
/// NOT `deny_unknown_fields` so future perps thresholds don't break
/// xvision-core's loader. The engine reads per-strategy `strategy.risk` fields.
#[derive(Debug, Clone, Default, PartialEq, Validate, Serialize, Deserialize)]
pub struct RiskPerpsGuards {
    /// Funding-carry guard threshold (PR #985). Mirrored for deserialization.
    #[garde(skip)]
    #[serde(default)]
    pub max_funding_pay_8h: Option<f64>,
    /// Liquidation-distance guard threshold. Mirrored for deserialization.
    #[garde(skip)]
    #[serde(default)]
    pub min_liq_distance_pct: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Validate, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskVenueLimits {
    /// Minimum order notional in USD. `0.0` (the default) disables
    /// the venue-min-notional rule.
    #[garde(range(min = 0.0))]
    #[serde(default)]
    pub min_notional_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskLimits {
    #[garde(range(min = 0.1, max = 100.0))]
    pub max_position_pct_nav: f32,
    #[garde(range(min = 0.1, max = 500.0))]
    pub max_total_exposure_pct: f32,
    #[garde(range(min = 1, max = 50))]
    pub max_open_positions: u32,
    #[garde(range(min = 0.1, max = 100.0))]
    pub max_daily_loss_pct: f32,
}

/// `#[serde(try_from = "RiskStopsRaw")]` runs the F-6 cross-field
/// rule (`stop_loss_min_pct <= stop_loss_max_pct`) on every parse.
/// TOML loads, JSON API payloads, and any future caller pick up the
/// check without an explicit `validate_cross_field()` call.
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
#[serde(try_from = "RiskStopsRaw")]
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RiskStopsRaw {
    stop_loss_required: bool,
    stop_loss_min_pct: f32,
    stop_loss_max_pct: f32,
    take_profit_required: bool,
    take_profit_min_rr: f32,
}

impl TryFrom<RiskStopsRaw> for RiskStops {
    type Error = String;

    fn try_from(raw: RiskStopsRaw) -> Result<Self, Self::Error> {
        let stops = RiskStops {
            stop_loss_required: raw.stop_loss_required,
            stop_loss_min_pct: raw.stop_loss_min_pct,
            stop_loss_max_pct: raw.stop_loss_max_pct,
            take_profit_required: raw.take_profit_required,
            take_profit_min_rr: raw.take_profit_min_rr,
        };
        stops.validate_cross_field()?;
        Ok(stops)
    }
}

impl RiskStops {
    /// Cross-field invariant: the minimum stop-loss must not exceed
    /// the maximum. Previously implicit (callers happened not to
    /// invert them); F-6 enforces it at the validator boundary as a
    /// companion to `Validate::validate(&())`.
    pub fn validate_cross_field(&self) -> Result<(), String> {
        if self.stop_loss_min_pct > self.stop_loss_max_pct {
            return Err(format!(
                "stop_loss_min_pct ({:.2}) must be <= stop_loss_max_pct ({:.2})",
                self.stop_loss_min_pct, self.stop_loss_max_pct,
            ));
        }
        Ok(())
    }
}

impl RiskConfig {
    /// Run cross-field invariants on every nested type that has them.
    /// Used by the pre-persist seam alongside `Validate::validate`.
    pub fn validate_cross_field(&self) -> Result<(), String> {
        self.stops.validate_cross_field()
    }
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

pub fn runtime_config_path(xvn_home: &Path) -> PathBuf {
    for env_name in [XVN_CONFIG_PATH_ENV, XVN_CONFIG_ENV] {
        if let Ok(path) = std::env::var(env_name) {
            if !path.is_empty() {
                return PathBuf::from(path);
            }
        }
    }
    xvn_home.join("config").join("default.toml")
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

/// One `[[providers]]` row that failed validation and was dropped during a
/// lenient load. Surfaced so the operator can see / fix / remove it, instead of
/// the entire provider surface silently disappearing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidProvider {
    pub name: String,
    pub reason: String,
}

/// Per-row provider validation mirroring the garde rules that `#[garde(dive)]`
/// applies to each `ProviderEntry` (name charset/length, base_url/api_key_env
/// length caps). Used by the lenient loader to decide which rows to keep.
fn validate_provider_entry(p: &ProviderEntry) -> Result<(), String> {
    validate_provider_name_str(&p.name)?;
    if p.base_url.len() > 512 {
        return Err(format!("base_url exceeds 512 chars ({})", p.base_url.len()));
    }
    if p.api_key_env.len() > 64 {
        return Err(format!("api_key_env exceeds 64 chars ({})", p.api_key_env.len()));
    }
    Ok(())
}

/// Like [`load_runtime`], but tolerant of individually-invalid `[[providers]]`
/// rows: any provider entry that fails validation (bad name, oversize fields,
/// or a duplicate name) is DROPPED from the returned config and reported in the
/// second tuple element, rather than failing the entire load.
///
/// This is the resilience guarantee for the provider-settings surface — a
/// single malformed provider row can never blank the whole provider list, nor
/// wedge the config so badly that the offending row can't even be removed. The
/// strict [`load_runtime`] remains the authoritative loader for the engine's
/// run path; this leniency is scoped to provider CRUD / listing.
///
/// Non-provider config sections are still validated strictly: if anything
/// outside `[[providers]]` is invalid, this returns the same error
/// [`load_runtime`] would.
pub fn load_runtime_lenient(path: &Path) -> Result<(RuntimeConfig, Vec<InvalidProvider>), ConfigError> {
    // Parse WITHOUT the whole-struct garde validate (serde itself accepts e.g.
    // an uppercase provider name — only garde's `dive` rejects it). We validate
    // the provider rows individually below.
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
    let mut cfg: RuntimeConfig = toml::from_str(&s).map_err(|e| ConfigError::Parse {
        path: path.to_path_buf(),
        source: e,
    })?;

    // Partition providers into valid / invalid (dropping invalid + duplicate
    // rows). Dedup-by-name here too so a duplicate row can't wedge the load.
    let mut valid: Vec<ProviderEntry> = Vec::with_capacity(cfg.providers.len());
    let mut invalid: Vec<InvalidProvider> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for p in std::mem::take(&mut cfg.providers) {
        if let Err(reason) = validate_provider_entry(&p) {
            invalid.push(InvalidProvider { name: p.name, reason });
            continue;
        }
        if !seen.insert(p.name.clone()) {
            invalid.push(InvalidProvider {
                name: p.name,
                reason: "duplicate provider name".to_string(),
            });
            continue;
        }
        valid.push(p);
    }
    cfg.providers = valid;

    // Validate the cleaned config strictly. The providers vec now holds only
    // good rows, so the `dive` passes; every other section is checked exactly
    // as in `load_runtime`.
    cfg.validate().map_err(|report| ConfigError::Validation {
        path: path.to_path_buf(),
        report,
    })?;
    cfg.backtest
        .validate_step_vs_horizon()
        .map_err(|message| ConfigError::CrossField {
            path: path.to_path_buf(),
            message,
        })?;
    Ok((cfg, invalid))
}

pub fn load_whitelist(path: &Path) -> Result<WhitelistConfig, ConfigError> {
    read_toml(path)
}

/// Load the curated Byreal-spot asset set from a TOML file. Validated via `garde`.
pub fn load_spot_assets(path: &Path) -> Result<SpotAssetConfig, ConfigError> {
    read_toml(path)
}

/// Default location of the curated spot-asset config under `xvn_home`.
pub fn spot_assets_path(xvn_home: &Path) -> PathBuf {
    xvn_home.join("config").join("byreal_spot_assets.toml")
}

pub fn load_risk(path: &Path) -> Result<RiskConfig, ConfigError> {
    let cfg: RiskConfig = read_toml(path)?;
    // F-6: the `RiskStops` `try_from` shadow already runs the
    // `min <= max` invariant at TOML parse time. The explicit call
    // here is belt-and-suspenders for any future RiskConfig-level
    // cross-field rule and makes the loader symmetric with
    // `load_runtime`'s explicit `validate_step_vs_horizon` /
    // `validate_unique_provider_names` calls.
    cfg.validate_cross_field()
        .map_err(|message| ConfigError::CrossField {
            path: path.to_path_buf(),
            message,
        })?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── F-6: deny_unknown_fields + cross-field invariants ───────────

    fn baseline_risk_config() -> RiskConfig {
        RiskConfig {
            limits: RiskLimits {
                max_position_pct_nav: 10.0,
                max_total_exposure_pct: 100.0,
                max_open_positions: 3,
                max_daily_loss_pct: 5.0,
            },
            stops: RiskStops {
                stop_loss_required: true,
                stop_loss_min_pct: 0.5,
                stop_loss_max_pct: 5.0,
                take_profit_required: true,
                take_profit_min_rr: 1.5,
            },
            venues: std::collections::BTreeMap::new(),
            perps: RiskPerpsGuards::default(),
        }
    }

    #[test]
    fn risk_config_rejects_unknown_field() {
        let valid = serde_json::to_value(baseline_risk_config()).unwrap();
        let mut object = valid.as_object().unwrap().clone();
        object.insert("phantom".into(), serde_json::json!(true));
        let err = serde_json::from_value::<RiskConfig>(serde_json::Value::Object(object))
            .expect_err("deny_unknown_fields must reject `phantom`");
        assert!(err.to_string().contains("unknown field"));
        assert!(err.to_string().contains("phantom"));
    }

    #[test]
    fn risk_limits_rejects_unknown_field() {
        let valid = serde_json::to_value(baseline_risk_config().limits).unwrap();
        let mut object = valid.as_object().unwrap().clone();
        object.insert("max_widgets".into(), serde_json::json!(7));
        let err = serde_json::from_value::<RiskLimits>(serde_json::Value::Object(object))
            .expect_err("deny_unknown_fields must reject `max_widgets`");
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn risk_stops_rejects_unknown_field() {
        let valid = serde_json::to_value(baseline_risk_config().stops).unwrap();
        let mut object = valid.as_object().unwrap().clone();
        object.insert("trailing_stop".into(), serde_json::json!(0.5));
        let err = serde_json::from_value::<RiskStops>(serde_json::Value::Object(object))
            .expect_err("deny_unknown_fields must reject `trailing_stop`");
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn risk_stops_cross_field_accepts_min_le_max() {
        let cfg = baseline_risk_config();
        cfg.stops
            .validate_cross_field()
            .expect("0.5 <= 5.0 satisfies the cross-field rule");
    }

    #[test]
    fn risk_stops_cross_field_rejects_min_above_max() {
        let mut cfg = baseline_risk_config();
        cfg.stops.stop_loss_min_pct = 10.0;
        cfg.stops.stop_loss_max_pct = 2.0;
        let err = cfg
            .stops
            .validate_cross_field()
            .expect_err("10.0 > 2.0 must fail the cross-field rule");
        assert!(err.contains("stop_loss_min_pct"));
        assert!(err.contains("stop_loss_max_pct"));
    }

    #[test]
    fn risk_config_cross_field_delegates_to_stops() {
        let mut cfg = baseline_risk_config();
        cfg.stops.stop_loss_min_pct = 10.0;
        cfg.stops.stop_loss_max_pct = 2.0;
        cfg.validate_cross_field()
            .expect_err("RiskConfig::validate_cross_field must surface RiskStops failures");
    }

    #[test]
    fn risk_stops_deserialize_rejects_inverted_min_max() {
        // PR #302 review P2: the try_from shadow on RiskStops must
        // enforce min<=max on every parse path — TOML load, JSON
        // payload, anywhere else. Catches it BEFORE load_risk's
        // explicit `validate_cross_field` call would have a chance to.
        let bad = serde_json::json!({
            "stop_loss_required": true,
            "stop_loss_min_pct": 10.0,
            "stop_loss_max_pct": 2.0,
            "take_profit_required": true,
            "take_profit_min_rr": 1.5,
        });
        let err = serde_json::from_value::<RiskStops>(bad).expect_err("min > max must fail deserialization");
        let msg = err.to_string();
        assert!(msg.contains("stop_loss_min_pct"), "{msg}");
        assert!(msg.contains("stop_loss_max_pct"), "{msg}");
    }

    #[test]
    fn load_risk_rejects_inverted_min_max_via_explicit_validate_cross_field() {
        // PR #302 review P2: load_risk's explicit call surfaces
        // cross-field failures as ConfigError::CrossField (not as a
        // serde Validation error). The try_from shadow would catch
        // it first today, but the explicit call here is a
        // belt-and-suspenders contract — any future RiskConfig-level
        // rule that lives outside RiskStops should land here.
        //
        // We can't easily force the try_from to PASS while the
        // explicit call FAILS without diverging the two checks, so
        // this test exercises load_risk's error mapping by writing a
        // file with an inverted RiskStops and asserting the error is
        // either Parse (try_from fires) or CrossField (explicit call
        // fires) — both are acceptable outcomes of the load.
        let td = tempfile::tempdir().expect("tempdir");
        let path = td.path().join("risk.toml");
        let toml_text = r#"
[limits]
max_position_pct_nav = 10.0
max_total_exposure_pct = 100.0
max_open_positions = 3
max_daily_loss_pct = 5.0

[stops]
stop_loss_required = true
stop_loss_min_pct = 10.0
stop_loss_max_pct = 2.0
take_profit_required = true
take_profit_min_rr = 1.5
"#;
        std::fs::write(&path, toml_text).expect("write fixture");
        let err = load_risk(&path).expect_err("inverted min/max must reject");
        match err {
            ConfigError::Parse { .. } | ConfigError::CrossField { .. } => {}
            other => panic!("expected Parse or CrossField error, got {other:?}",),
        }
    }

    #[test]
    fn load_risk_accepts_valid_min_le_max() {
        let td = tempfile::tempdir().expect("tempdir");
        let path = td.path().join("risk.toml");
        let toml_text = r#"
[limits]
max_position_pct_nav = 10.0
max_total_exposure_pct = 100.0
max_open_positions = 3
max_daily_loss_pct = 5.0

[stops]
stop_loss_required = true
stop_loss_min_pct = 0.5
stop_loss_max_pct = 5.0
take_profit_required = true
take_profit_min_rr = 1.5
"#;
        std::fs::write(&path, toml_text).expect("write fixture");
        load_risk(&path).expect("0.5 <= 5.0 must load cleanly");
    }

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
    fn load_runtime_lenient_drops_invalid_provider_rows() {
        // One valid ("gemini") and one invalid ("Gemini", uppercase) provider
        // row. The strict loader rejects the whole file; the lenient loader
        // keeps the good row and reports the bad one — so a single malformed
        // row can't blank the provider surface.
        let toml_src = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "gemini"
kind = "openai-compat"
base_url = "https://generativelanguage.googleapis.com/v1beta/openai"
api_key_env = "GEMINI_API_KEY"

[[providers]]
name = "Gemini"
kind = "openai-compat"
base_url = "https://generativelanguage.googleapis.com/v1beta/openai"
api_key_env = "GEMINI_API_KEY"

[default_llm]
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
        let path = dir.path().join("with-bad-provider.toml");
        std::fs::write(&path, toml_src).unwrap();

        // Strict load rejects the entire file because of the "Gemini" row.
        assert!(
            load_runtime(&path).is_err(),
            "strict load must reject the invalid row"
        );

        // Lenient load keeps the good row and reports the bad one.
        let (cfg, invalid) = load_runtime_lenient(&path).expect("lenient load must succeed");
        let names: Vec<&str> = cfg.providers.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["gemini"], "only the valid row is kept");
        assert_eq!(invalid.len(), 1, "exactly one row reported invalid");
        assert_eq!(invalid[0].name, "Gemini");
        assert!(
            invalid[0].reason.contains("[a-z0-9-]"),
            "reason should name the rule, got {:?}",
            invalid[0].reason
        );
    }

    #[test]
    fn load_runtime_lenient_still_rejects_non_provider_errors() {
        // Leniency is scoped to [[providers]] — a broken non-provider section
        // (here: step < horizon) must still fail, exactly as strict load.
        let toml_src = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[default_llm]
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
step = 4
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
        let path = dir.path().join("bad-backtest.toml");
        std::fs::write(&path, toml_src).unwrap();
        assert!(
            load_runtime_lenient(&path).is_err(),
            "non-provider validation errors must still fail the lenient load"
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

[default_llm]
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

[default_llm]
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
        // All entries in the generated whitelist are enabled=true, so the
        // enabled count must equal the total asset count (≥ 102 Orderly + legacy).
        assert!(
            enabled.len() >= 102,
            "whitelist must contain at least 102 enabled assets (Orderly coverage), got {}",
            enabled.len()
        );
        // Legacy crypto symbols must still be present.
        let legacy = [
            "BTC", "ETH", "SOL", "LTC", "AVAX", "LINK", "AAVE", "UNI", "DOT", "DOGE", "BCH",
        ];
        for sym in &legacy {
            assert!(
                enabled.contains(sym),
                "legacy symbol {sym} must be present in whitelist"
            );
        }
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

    // --- agent runtime selector (Cline runtime unification, Stage 1) --------

    #[test]
    fn agent_runtime_defaults_to_llm_dispatch_until_flipped() {
        // Name kept stable for the contract's audit trail.
        // WU-6 removed LlmDispatch entirely; Cline is the only variant.
        assert_eq!(AgentRuntime::default(), AgentRuntime::Cline);
    }

    /// The shipped seed config (`config/default.toml`, copied into the deploy
    /// image and seeded to `$XVN_HOME/config/default.toml` by the entrypoint)
    /// must carry an explicit `agent_runtime = "cline"` line. Since WU-6
    /// retired LlmDispatch, the only valid value is "cline" — this test
    /// confirms the file parses without error and resolves to Cline.
    #[test]
    fn shipped_default_config_sets_agent_runtime_cline_explicitly() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/default.toml");
        let raw = std::fs::read_to_string(&path).expect("read workspace config/default.toml");
        assert!(
            raw.lines()
                .map(str::trim_start)
                .any(|l| l.starts_with("agent_runtime") && l.contains('=')),
            "config/default.toml must contain an explicit `agent_runtime = ...` line \
             (the eval resolver only honors Cline when the field is explicit or \
             XVN_AGENTD_BIN is set)"
        );
        let cfg = load_runtime(&path).expect("workspace config/default.toml must parse");
        assert_eq!(
            cfg.agent_runtime,
            AgentRuntime::Cline,
            "the shipped seed config must explicitly select the Cline runtime"
        );
    }

    #[test]
    fn agent_runtime_parses_from_str() {
        use std::str::FromStr;
        assert_eq!(AgentRuntime::from_str("cline").unwrap(), AgentRuntime::Cline);
        // LlmDispatch was retired in WU-6; the parser must now reject it.
        assert!(AgentRuntime::from_str("llm-dispatch").is_err());
        assert!(AgentRuntime::from_str("llm_dispatch").is_err());
        assert!(AgentRuntime::from_str("bogus").is_err());
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

[default_llm]
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
    fn runtime_config_loads_local_candle_with_empty_base_url() {
        let toml_src = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[[providers]]
name = "local-candle"
kind = "local-candle"
base_url = ""
api_key_env = ""

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
        let path = dir.path().join("local-candle.toml");
        std::fs::write(&path, toml_src).unwrap();
        let cfg = load_runtime(&path).unwrap();
        assert_eq!(cfg.providers.len(), 1);
        assert_eq!(cfg.providers[0].kind, ProviderKind::LocalCandle);
        assert_eq!(cfg.providers[0].base_url, "");
    }

    #[test]
    fn repo_default_toml_ships_with_seeded_starter_providers() {
        // The repo's default config seeds two key-less OpenAI-compatible
        // starters (gemini, nous-research). Operators paste keys to enable
        // them; any further providers are added via Settings -> Providers
        // (or `xvn provider add`).
        let cfg = load_runtime(&project_root().join("config/default.toml")).unwrap();
        let mut user_rows: Vec<&str> = cfg
            .providers
            .iter()
            .map(|p| p.name.as_str())
            .filter(|n| !n.starts_with('_'))
            .collect();
        user_rows.sort_unstable();
        assert_eq!(
            user_rows,
            vec!["gemini", "nous-research"],
            "default.toml should ship the two seeded starters, got {user_rows:?}"
        );
        assert!(
            cfg.providers.iter().all(|p| p.name != "_default_llm"),
            "default.toml should not synthesize `_default_llm` provider rows"
        );
    }

    #[test]
    fn validate_provider_name_str_rules() {
        // Valid: lowercase, digits, hyphens, 1..=32 chars.
        assert!(validate_provider_name_str("gemini").is_ok());
        assert!(validate_provider_name_str("nous-research").is_ok());
        assert!(validate_provider_name_str("a").is_ok());
        assert!(validate_provider_name_str(&"a".repeat(32)).is_ok());
        // Invalid: uppercase, spaces, empty, too long, leading underscore.
        assert!(validate_provider_name_str("Gemini").is_err());
        assert!(validate_provider_name_str("nous research").is_err());
        assert!(validate_provider_name_str("").is_err());
        assert!(validate_provider_name_str(&"a".repeat(33)).is_err());
        assert!(validate_provider_name_str("_internal").is_err());
        // The garde adapter must agree with the shared fn (same accept/reject).
        for name in ["gemini", "Gemini", "_x", "ok-1", "Bad Name"] {
            assert_eq!(
                validate_provider_name_str(name).is_ok(),
                validate_provider_name(name, &()).is_ok(),
                "garde adapter disagreed with shared fn for {name:?}"
            );
        }
    }

    #[test]
    fn does_not_auto_derive_default_llm_provider() {
        let cfg = load_runtime(&project_root().join("config/default.toml")).expect("must load");
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

[default_llm]
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

[default_llm]
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

[default_llm]
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
        for k in [Anthropic, OpenaiCompat, LocalCandle, Ollama, LlamaCpp, Vllm] {
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
        let v = toml::Value::try_from(ProviderKind::Vllm).unwrap();
        assert_eq!(v.as_str(), Some("vllm"));
    }

    const BAD_STEP_HORIZON: &str = r#"
[runtime]
mode = "backtest"
executor = "alpaca"
random_seed = 42

[default_llm]
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

    // --- byreal spot asset config --------------------------------------------

    #[test]
    fn loads_byreal_spot_assets_and_resolves_mint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("byreal_spot_assets.toml");
        std::fs::write(
            &path,
            r#"
usdc_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"

[[assets]]
symbol = "SOL"
mint = "So11111111111111111111111111111111111111112"
kind = "spl"
decimals = 9

[[assets]]
symbol = "AAPLx"
mint = "XsbEhLAtcf6HdfpFZ5xEMdqW8nfAvcsP5bdudRLJzJp"
kind = "xstock"
decimals = 8
"#,
        )
        .unwrap();

        let cfg = load_spot_assets(&path).expect("valid config loads");
        assert_eq!(cfg.usdc_mint, "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
        let sol = cfg.resolve("SOL").expect("SOL is curated");
        assert_eq!(sol.mint, "So11111111111111111111111111111111111111112");
        assert_eq!(sol.decimals, 9);
        assert_eq!(sol.kind, SpotAssetKind::Spl);
        assert!(cfg.resolve("aaplx").is_some()); // case-insensitive
        assert!(cfg.resolve("DOGE").is_none()); // whitelist: unknown refused
    }

    #[test]
    fn rejects_spot_assets_with_empty_mint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(
            &path,
            "usdc_mint = \"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v\"\n[[assets]]\nsymbol = \"SOL\"\nmint = \"\"\nkind = \"spl\"\ndecimals = 9\n",
        )
        .unwrap();
        assert!(matches!(
            load_spot_assets(&path),
            Err(ConfigError::Validation { .. })
        ));
    }
}
