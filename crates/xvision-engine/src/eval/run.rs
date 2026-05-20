//! Per-run metadata. One `Run` row per `xvn eval run` invocation. The full
//! eval engine plan goes through this type for every status transition,
//! metric finalization, and listing surface (CLI / MCP / dashboard).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    Backtest,
    Paper,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl RunStatus {
    /// On-disk string form (matches the CHECK-able strings the migration
    /// describes for the `eval_runs.status` column).
    pub fn as_str(&self) -> &'static str {
        match self {
            RunStatus::Queued => "queued",
            RunStatus::Running => "running",
            RunStatus::Completed => "completed",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(RunStatus::Queued),
            "running" => Some(RunStatus::Running),
            "completed" => Some(RunStatus::Completed),
            "failed" => Some(RunStatus::Failed),
            "cancelled" => Some(RunStatus::Cancelled),
            _ => None,
        }
    }

    /// True for the two terminal states. Once a run is terminal it never
    /// transitions again.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled
        )
    }
}

impl RunMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunMode::Backtest => "backtest",
            RunMode::Paper => "paper",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "backtest" => Some(RunMode::Backtest),
            "paper" => Some(RunMode::Paper),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: String, // ULID
    /// Strategy bundle artifact hash. See migration 014: this column was
    /// renamed from `strategy_bundle_hash` and still carries the bundle
    /// value, NOT the workspace `agents.agent_id`. The long-lived agent
    /// ULID lives in `agents_agent_id` below (migration 022).
    pub agent_id: String,
    /// Long-lived workspace `agents.agent_id` ULID of the calling agent.
    /// `None` for rows older than migration 022 (no backfill — see F-11
    /// in `team/intake/2026-05-16-eval-review-and-v2a.md`). New runs
    /// populate this at start via the strategy's first AgentRef.
    #[serde(default)]
    pub agents_agent_id: Option<String>,
    pub scenario_id: String,
    pub params_override: Option<serde_json::Value>,
    pub mode: RunMode,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub metrics: Option<MetricsSummary>,
    pub error: Option<String>,
    pub estimated_total_tokens: Option<u64>,
    pub actual_input_tokens: Option<u64>,
    pub actual_output_tokens: Option<u64>,

    // ── Candle integrity + manifest (migration 027) ────────────────────────
    /// SHA-256 hex digest of the raw Parquet bytes loaded for this run.
    /// `None` for runs created before migration 027 or for paper-mode runs
    /// where bars are not pinned to a Parquet snapshot.
    #[serde(default)]
    pub bars_content_hash: Option<String>,
    /// SHA-256 hex digest of the JSON-canonical `DataManifest` for this run.
    /// Used by `ComparisonReport::build` to refuse mismatched-manifest compares.
    /// `None` for pre-migration rows; populated at run-start for new runs.
    #[serde(default)]
    pub manifest_canonical: Option<String>,
    /// Full JSON-serialized `DataManifest` for this run.
    /// `None` for pre-migration rows.
    #[serde(default)]
    pub bars_manifest: Option<serde_json::Value>,
}

impl Run {
    /// Construct a fresh `Queued` run with a generated ULID and `started_at = now`.
    pub fn new_queued(agent_id: String, scenario_id: String, mode: RunMode) -> Self {
        Self {
            id: Ulid::new().to_string(),
            agent_id,
            agents_agent_id: None,
            scenario_id,
            params_override: None,
            mode,
            status: RunStatus::Queued,
            started_at: Utc::now(),
            completed_at: None,
            metrics: None,
            error: None,
            estimated_total_tokens: None,
            actual_input_tokens: None,
            actual_output_tokens: None,
            bars_content_hash: None,
            manifest_canonical: None,
            bars_manifest: None,
        }
    }
}

/// Per-baseline performance numbers for one of the four automatic baselines.
///
/// Stored inside `MetricsSummary.baselines` (packed into `metrics_json` on
/// the `eval_runs` row). No separate DB column or migration is required —
/// old rows without the key simply deserialize with `baselines: None`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaselineMetrics {
    /// Total return as a percentage of starting capital. E.g. `6.80` means +6.80%.
    pub return_pct: f64,
    /// Annualised Sharpe ratio. `0.0` when flat or < 2 bars.
    pub sharpe: f64,
}

/// Strategy outperformance (return_pct delta) versus each of the four baselines.
/// Positive = the strategy beat the baseline on raw total return.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaselineRelative {
    pub buy_hold: f64,
    pub always_flat: f64,
    pub simple_trend: f64,
    pub simple_mean_reversion: f64,
}

/// All four automatic baselines computed over the same bar slice the strategy
/// saw, plus the per-baseline return delta (`strategy_return_pct −
/// baseline_return_pct`).
///
/// Serialized as `{"baselines": ..., "relative_to": ...}` and packed into
/// `MetricsSummary.baselines` (stored in the existing `metrics_json` column).
/// No DB migration required; old rows deserialize with `baselines: None`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaselinesReport {
    pub buy_hold: BaselineMetrics,
    pub always_flat: BaselineMetrics,
    pub simple_trend: BaselineMetrics,
    pub simple_mean_reversion: BaselineMetrics,
    /// `strategy_return_pct − baseline_return_pct` for each baseline.
    pub relative_to: BaselineRelative,
}

/// Headline metrics the eval engine computes after a run completes.
/// Persisted as `metrics_json` on the `eval_runs` row by `RunStore::finalize`.
///
/// The `baselines` field is optional and backward-compatible: old rows that
/// were finalized before baselines were introduced deserialize with
/// `baselines: None`. New runs always populate it.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsSummary {
    pub total_return_pct: f64,
    pub sharpe: f64,
    pub max_drawdown_pct: f64,
    pub win_rate: f64,
    pub n_trades: u32,
    pub n_decisions: u32,
    /// Automatic baseline comparison computed over the same bar slice the
    /// strategy saw. `None` for old runs that predate baselines support or
    /// for paper-mode runs where bars are not available post-hoc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baselines: Option<BaselinesReport>,
}
