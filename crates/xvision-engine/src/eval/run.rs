//! Per-run metadata. One `Run` row per `xvn eval run` invocation. The full
//! eval engine plan goes through this type for every status transition,
//! metric finalization, and listing surface (CLI / MCP / dashboard).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::eval::live_config::LiveConfig;

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

/// Who queued a run — the deployment-source discriminator backing CT5's
/// `LiveDeploymentSummary.source` and `awm`'s Cancel-gate (only `Human`-sourced
/// runs may be cancelled from the dashboard strip). Persisted in the
/// `eval_runs.source` column (migration 065, DB default `'human'`). Set at
/// queue time: the operator queue path (POST /api/eval/runs) keeps the
/// `Human` default; the autooptimizer eval adapter sets `Optimizer`. The
/// strategy `agent_id` is NOT a reliable discriminator (the optimizer reuses
/// `strategy.manifest.id`), so this explicit column is required. See
/// docs/superpowers/specs/2026-06-13-ct5-live-deployment-contract.md §9.2.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentSource {
    /// Operator-queued (the human queue path). The persisted-column default.
    #[default]
    Human,
    /// Queued by the autooptimizer eval adapter.
    Optimizer,
}

impl DeploymentSource {
    /// On-disk string form (matches the `eval_runs.source` column values).
    pub fn as_str(&self) -> &'static str {
        match self {
            DeploymentSource::Human => "human",
            DeploymentSource::Optimizer => "optimizer",
        }
    }

    /// Parse the on-disk string. Returns `None` for unknown values so callers
    /// can fall back to the tolerant `'human'` default on a malformed read.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "human" => Some(DeploymentSource::Human),
            "optimizer" => Some(DeploymentSource::Optimizer),
            _ => None,
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

    /// Parse a comma-separated list of status tokens into a `Vec<RunStatus>`.
    ///
    /// Each token is trimmed of whitespace before matching. An empty input
    /// string and any unrecognised token are errors; the error message names
    /// the offending token so callers can surface it in a user-facing message.
    ///
    /// Valid tokens: `queued`, `running`, `completed`, `failed`, `cancelled`.
    ///
    /// # Examples
    /// ```text
    /// RunStatus::parse_list("queued,running")  // Ok([Queued, Running])
    /// RunStatus::parse_list("queued, running") // Ok([Queued, Running])
    /// RunStatus::parse_list("bogus")           // Err("unknown status 'bogus'")
    /// RunStatus::parse_list("queued,bogus")    // Err("unknown status 'bogus'")
    /// RunStatus::parse_list("")                // Err("status list must not be empty")
    /// ```
    pub fn parse_list(s: &str) -> Result<Vec<Self>, String> {
        let tokens: Vec<&str> = s.split(',').map(str::trim).collect();
        if tokens.is_empty() || (tokens.len() == 1 && tokens[0].is_empty()) {
            return Err("status list must not be empty".to_string());
        }
        tokens
            .into_iter()
            .map(|t| RunStatus::parse(t).ok_or_else(|| format!("unknown run status '{t}'")))
            .collect()
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
    /// Per-run opt-in for the post-completion auto review path.
    #[serde(default)]
    pub auto_fire_review: bool,
    /// Provider/model preference to use when an operator manually fires a
    /// review, or when a future LLM auto-review worker is enabled.
    #[serde(default)]
    pub review_model: Option<ReviewModel>,
    /// Upper bound for review-generated chart annotations. Defaults to 8
    /// when absent.
    #[serde(default)]
    pub max_annotations_per_review: Option<u32>,
    /// Launch envelope for a Live run. Backtests keep this as `None`; Live
    /// rows persist it in `eval_runs.live_config_json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_config: Option<LiveConfig>,
    /// A1 per-run (per-run) pause flag. When `true`, the live executor skips
    /// the broker submit for each cycle but keeps iterating the run — an
    /// ADDITIVE skip alongside the global `SafetyManager` pause. Resume
    /// clears it. Persisted in `eval_runs.paused` (migration 061); pre-061
    /// rows read back as `false`.
    #[serde(default)]
    pub paused: bool,
    /// RFC3339 timestamp of the most recent pause (migration 061's
    /// `eval_runs.paused_at`); `None` when never paused or after resume.
    /// Overlaid by `RunStore::get` alongside `paused`; the harmless `None`
    /// default applies everywhere else (and on pre-061 rows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_at: Option<String>,
    /// A3 one-shot "flatten positions" request flag. When `true`, the live
    /// executor closes ALL open broker positions on its next cycle (the same
    /// close path A2 uses on cancel) and then clears the flag — WITHOUT
    /// terminating the run. Persisted in `eval_runs.flatten_requested`
    /// (migration 062); pre-062 rows read back as `false`. Overlaid by
    /// `RunStore::get` alongside `paused`; the harmless `false` default applies
    /// everywhere else.
    #[serde(default)]
    pub flatten_requested: bool,
    /// CT5 deployment-source discriminator (`eval_runs.source`, migration 065).
    /// `Human` for the operator queue path, `Optimizer` for the autooptimizer
    /// eval adapter. Set at queue time; drives `awm`'s Cancel-gate. Defaults to
    /// `Human` (the DB column default) so backtests and pre-065 rows are
    /// behaviorally unchanged.
    #[serde(default)]
    pub source: DeploymentSource,
    /// CT5 per-run mark-to-market unrealized PnL in USD (`eval_runs.unrealized_pnl_usd`,
    /// migration 065), written by the live loop's buffered equity flush. `None`
    /// when unavailable / pre-first-fill — HONESTY MANDATE (§8.1): an
    /// unsourceable value surfaces as NULL ("—" in the UI), NEVER a faked 0.
    /// Backtests leave this `None` with no behavior change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unrealized_pnl_usd: Option<f64>,
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
            auto_fire_review: false,
            review_model: None,
            max_annotations_per_review: Some(8),
            live_config: None,
            paused: false,
            paused_at: None,
            flatten_requested: false,
            source: DeploymentSource::Human,
            unrealized_pnl_usd: None,
        }
    }

    pub fn with_live_config(mut self, config: LiveConfig) -> Self {
        self.live_config = Some(config);
        self
    }

    /// Set the deployment-source discriminator (CT5). The operator queue path
    /// keeps the `Human` default; the autooptimizer eval adapter calls this
    /// with `DeploymentSource::Optimizer` at run creation.
    pub fn with_source(mut self, source: DeploymentSource) -> Self {
        self.source = source;
        self
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewModel {
    pub provider: String,
    pub model: String,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BaselineMetrics {
    /// Total return as a percentage of starting capital. E.g. `6.80` means +6.80%.
    pub return_pct: f64,
    /// Annualised Sharpe ratio. `0.0` when flat or < 2 bars.
    pub sharpe: f64,
}

/// Strategy outperformance (return_pct delta) versus each of the five baselines.
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
    /// Defaults to `0.0` when deserializing legacy rows that predate this field.
    #[serde(default)]
    pub random_direction: f64,
}

/// All five automatic baselines computed over the same bar slice the strategy
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
    /// Coin-flip long/short at 100 bps per bar, seeded for reproducibility.
    /// Defaults to `BaselineMetrics { return_pct: 0.0, sharpe: 0.0 }` when
    /// deserializing legacy rows that predate this field.
    #[serde(default)]
    pub random_direction: BaselineMetrics,
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
    /// Fraction of CLOSED ROUND-TRIPS that realized positive PnL
    /// (`wins / realized_count`). A round-trip is one position open→flat
    /// cycle, closed by the trader (`flat`/flip) or a deterministic SL/TP
    /// exit. `0.0` when no round-trip closed.
    pub win_rate: f64,
    /// Count of FILL LEGS that crossed the book — opens, closes, SL/TP forced
    /// exits, and partial-TP1 slices each count one. An open+close round-trip
    /// is `2` here. This is leg-count semantics (NOT round-trips); `win_rate`
    /// is the round-trip view. See the counter doc in `backtest.rs`.
    pub n_trades: u32,
    /// Count of LLM-pipeline decision slots, including synthesized SL/TP exit
    /// rows. Cadence-gated and filter-suppressed bars do not increment it (no
    /// decision occurred). Filter wake/suppression accounting lives separately
    /// in `xvision_filters::events::FilterSummary`.
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
    /// Live/forward-test only: bars where dispatch was skipped because
    /// the agent was still processing a previous bar.
    #[serde(default)]
    pub skipped_dispatches: u64,
    /// Live/forward-test only: decisions accepted but flagged delayed
    /// because the bar was stale (age > stale-data-max-age-ms).
    #[serde(default)]
    pub delayed_decisions: u64,
    /// Live/forward-test only: agents force-cancelled via --max-agent-ms.
    #[serde(default)]
    pub forced_cancels: u64,
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

    // ── CT5 Wave 3a: DeploymentSource discriminator on Run ──────────────────

    #[test]
    fn deployment_source_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&DeploymentSource::Human).unwrap(),
            "\"human\""
        );
        assert_eq!(
            serde_json::to_string(&DeploymentSource::Optimizer).unwrap(),
            "\"optimizer\""
        );
    }

    #[test]
    fn deployment_source_parse_roundtrip_and_default() {
        assert_eq!(DeploymentSource::parse("human"), Some(DeploymentSource::Human));
        assert_eq!(
            DeploymentSource::parse("optimizer"),
            Some(DeploymentSource::Optimizer)
        );
        assert_eq!(DeploymentSource::parse("???"), None);
        // The persisted-column default is 'human'.
        assert_eq!(DeploymentSource::default(), DeploymentSource::Human);
        assert_eq!(DeploymentSource::Human.as_str(), "human");
        assert_eq!(DeploymentSource::Optimizer.as_str(), "optimizer");
    }

    #[test]
    fn new_queued_defaults_to_human_source_and_null_unrealized_pnl() {
        // The human queue path keeps the default; backtests are unaffected.
        let run = Run::new_queued("agent".into(), "scenario".into(), RunMode::Backtest);
        assert_eq!(run.source, DeploymentSource::Human);
        assert_eq!(run.unrealized_pnl_usd, None);
    }

    #[test]
    fn with_source_sets_optimizer_discriminator() {
        // The optimizer eval adapter sets the discriminator at run creation.
        let run = Run::new_queued("agent".into(), "scenario".into(), RunMode::Backtest)
            .with_source(DeploymentSource::Optimizer);
        assert_eq!(run.source, DeploymentSource::Optimizer);
    }

    /// B27 regression: old eval rows whose `metrics_json` was serialized before
    /// `random_direction` was added to `BaselinesReport` / `BaselineRelative`
    /// must deserialize cleanly (defaulting the missing field to zero / default
    /// `BaselineMetrics`) rather than returning a serde error and dropping the
    /// entire metrics blob.
    #[test]
    fn baselines_report_without_random_direction_deserializes_with_default() {
        // Simulates the legacy JSON that triggered the WARN in row_to_run():
        // a MetricsSummary that contains a `baselines` object missing the
        // `random_direction` key in both BaselinesReport and BaselineRelative.
        let legacy_json = r#"{
            "total_return_pct": 5.0,
            "sharpe": 1.2,
            "max_drawdown_pct": 3.0,
            "win_rate": 0.6,
            "n_trades": 10,
            "n_decisions": 20,
            "baselines": {
                "buy_hold":              {"return_pct": 2.0, "sharpe": 0.5},
                "always_flat":           {"return_pct": 0.0, "sharpe": 0.0},
                "simple_trend":          {"return_pct": 1.5, "sharpe": 0.8},
                "simple_mean_reversion": {"return_pct": 1.0, "sharpe": 0.6},
                "relative_to": {
                    "buy_hold":              3.0,
                    "always_flat":           5.0,
                    "simple_trend":          3.5,
                    "simple_mean_reversion": 4.0
                }
            }
        }"#;

        let result = serde_json::from_str::<MetricsSummary>(legacy_json);
        assert!(result.is_ok(), "expected Ok but got: {:?}", result.err());
        let ms = result.unwrap();
        let baselines = ms.baselines.expect("baselines should be Some");
        // random_direction should default to BaselineMetrics { return_pct: 0.0, sharpe: 0.0 }
        assert_eq!(baselines.random_direction.return_pct, 0.0);
        assert_eq!(baselines.random_direction.sharpe, 0.0);
        // BaselineRelative.random_direction should default to 0.0
        assert_eq!(baselines.relative_to.random_direction, 0.0);
        // Other fields from the legacy JSON must survive intact
        assert_eq!(ms.total_return_pct, 5.0);
        assert_eq!(baselines.buy_hold.return_pct, 2.0);
        assert_eq!(baselines.relative_to.buy_hold, 3.0);
    }
}
