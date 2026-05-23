//! Per-run metadata. One `Run` row per `xvn eval run` invocation. The full
//! eval engine plan goes through this type for every status transition,
//! metric finalization, and listing surface (CLI / MCP / dashboard).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Reason a run was aborted by the safety subsystem. Persisted to the
/// `eval_runs.error` column so the dashboard can render a human-readable
/// reason alongside the `Cancelled` terminal status.
///
/// Extend with new variants rather than adding a parallel error type (per
/// the v2b-broker-wallet-kill-switch contract Notes section).
#[derive(Debug, Clone, PartialEq)]
pub enum RunAbort {
    /// Global pause was active when the submit was attempted.
    SafetyPaused { reason: String },
    /// A per-run safety limit was breached at submit time.
    SafetyLimit { kind: String, value: f64, limit: f64 },
    /// A Paper-labelled scenario tried to submit to a Live-configured broker.
    VenueLabelMismatch {
        scenario_label: String,
        broker_label: String,
    },
}

impl RunAbort {
    /// Stable reason string written to the run's `error` column.
    /// Format: `"aborted: <tag> <detail>"`.
    pub fn reason(&self) -> String {
        match self {
            RunAbort::SafetyPaused { reason } => {
                format!("aborted: safety_paused — {reason}")
            }
            RunAbort::SafetyLimit { kind, value, limit } => {
                format!("aborted: safety_limit — {kind} value={value:.4} limit={limit:.4}")
            }
            RunAbort::VenueLabelMismatch {
                scenario_label,
                broker_label,
            } => {
                format!("aborted: venue_label_mismatch — scenario={scenario_label} broker={broker_label}")
            }
        }
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    Backtest,
    Live,
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
            RunMode::Live => "live",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "backtest" => Some(RunMode::Backtest),
            "live" => Some(RunMode::Live),
            // Legacy DB read-only alias: pre-collapse runs persisted `mode = 'paper'`.
            // The intake's "retire paper mode with prejudice" decision relabels them
            // as Backtest on read. New writes never emit "paper". See
            // team/archive/2026-05-22-conductor-pass/contracts/executor-refactor.md.
            "paper" => Some(RunMode::Backtest),
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
///
/// Net-of-inference-cost fields (V2E item 25):
/// - `gross_return_pct` is a method alias for `total_return_pct` (the stored
///   field name). `total_return_pct` continues to serialize under that name
///   for one release (backward compat); V2F will rename it on the wire.
/// - `inference_cost_quote_total` — sum of all per-decision inference cost
///   quotes (USD). `None` when pricing data is unavailable for the model.
/// - `net_return_pct` — `gross_return_pct − (inference_cost_quote_total /
///   capital_initial × 100)`. `None` when `inference_cost_quote_total` is
///   unavailable.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MetricsSummary {
    /// Gross trading return as a percentage of starting capital.
    /// Deprecated wire name: `total_return_pct` (kept for one release).
    /// Use `MetricsSummary::gross_return_pct()` as the forward-looking accessor.
    /// Deserialization accepts `gross_return_pct` as an alias so JSON written
    /// by future code that uses the new name can still round-trip.
    #[serde(alias = "gross_return_pct")]
    pub total_return_pct: f64,
    pub sharpe: f64,
    pub max_drawdown_pct: f64,
    pub win_rate: f64,
    pub n_trades: u32,
    pub n_decisions: u32,
    /// Total LLM inference cost for all decisions in this run (USD).
    /// `None` when the model's pricing isn't in the catalog — in that case
    /// `net_return_pct` is also `None` and a `MissingPricingData` finding
    /// fires at run-finalize time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_cost_quote_total: Option<f64>,
    /// Net return after subtracting LLM inference cost from gross return.
    /// Math: `total_return_pct − (inference_cost_quote_total / capital_initial × 100)`.
    /// `None` when `inference_cost_quote_total` is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_return_pct: Option<f64>,
    /// Automatic baseline comparison computed over the same bar slice the
    /// strategy saw. `None` for old runs that predate baselines support or
    /// for paper-mode runs where bars are not available post-hoc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baselines: Option<BaselinesReport>,
}

impl MetricsSummary {
    /// Returns gross trading return (= `total_return_pct`). Canonical forward-
    /// looking accessor name; the underlying field is still serialized as
    /// `total_return_pct` for one release.
    pub fn gross_return_pct(&self) -> f64 {
        self.total_return_pct
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_mode_as_str_backtest_returns_backtest() {
        // executor-collapse-paper-mode: post-collapse the wire string is
        // "backtest", not "paper". New writes emit "backtest"; legacy
        // "paper" rows route through `parse(...)` to `Backtest` on read.
        assert_eq!(RunMode::Backtest.as_str(), "backtest");
    }

    #[test]
    fn run_mode_as_str_live_returns_live() {
        assert_eq!(RunMode::Live.as_str(), "live");
    }

    #[test]
    fn run_mode_parse_paper_returns_backtest_legacy_alias() {
        // The legacy alias is the deliberate backward-compatibility seam
        // (see comment in `RunMode::parse`). Pre-collapse rows with
        // `mode = 'paper'` continue to load as Backtest.
        assert_eq!(RunMode::parse("paper"), Some(RunMode::Backtest));
    }

    #[test]
    fn run_mode_parse_backtest_returns_backtest() {
        assert_eq!(RunMode::parse("backtest"), Some(RunMode::Backtest));
    }

    #[test]
    fn run_mode_parse_live_returns_live() {
        assert_eq!(RunMode::parse("live"), Some(RunMode::Live));
    }

    #[test]
    fn run_mode_parse_unknown_returns_none() {
        assert_eq!(RunMode::parse("???"), None);
        assert_eq!(RunMode::parse(""), None);
    }
}
