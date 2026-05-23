//! Phase 3.C findings — LLM-extracted structured insights about a
//! completed eval run. The shape mirrors the `eval_findings` table from
//! migration 002 plus the review-linked columns added in migration 017
//! (see `docs/superpowers/specs/2026-05-15-eval-review-agent.md`) plus
//! the V2E trace-surface additions from migration 026
//! (`eval-trace-surface-foundation`, 2026-05-21).
//!
//! # Data-defect findings (V2E, migration 027)
//!
//! The `data_defect` finding kind is registered here. Data-defect findings
//! are emitted by `xvision_data::validate::validate_ohlcv` at fixture-load
//! time and at scenario start. They always carry:
//!
//! - `kind = "data_defect"`
//! - `evidence.produced_by_check = "validator:ohlcv"`
//! - `evidence.evidence_cycle_ids = []` (data defects pre-exist the cycle)
//!
//! Severity mapping:
//! - `Error`-tier defects → `Severity::Critical`
//! - `Warning`-tier defects → `Severity::Warning`
//! - `Info`-tier defects → `Severity::Info`
//!
//! A scenario with any `Critical` data-defect finding requires
//! `--allow-defective-data` to proceed.
//!
//! # Volume-share excess findings (V2E, eval-cost-model-per-bar-and-volume-share)
//!
//! `volume_share_excess` kind: emitted by the backtest simulator when
//! `order_qty / bar_volume > volume_limit` in the `VolumeShare` slippage
//! model. Payload: `{ requested_qty, bar_volume, cap_binding_qty,
//! fill_share }`. `produced_by_check = "sim:volume_cap"`.
//!
//! ## Kind registry (open enum — string on the wire)
//!
//! | Kind | Produced by | Severity |
//! |------|-------------|----------|
//! | `regime_fit_mismatch` | LLM extractor | warning |
//! | `drawdown_concentration` | LLM extractor | warning |
//! | `overtrading` | LLM extractor | warning |
//! | `underperformance` | LLM extractor | warning |
//! | `risk_violation` | LLM extractor | critical |
//! | `win_rate_anomaly` | LLM extractor | info |
//! | `tail_risk` | LLM extractor | warning |
//! | `lookahead_suspected` | `prober:lookahead` (eval-lookahead-bias-prober) | critical |
//! | `inference_cost_dominates_return` | `metrics:cost_dominance` (eval-net-of-inference-cost-metric) | warning |
//! | `uniform_justification` | `smell:uniformity` (eval-honesty-smell-tests) | critical |
//! | `uniform_decision` | `smell:uniformity` (eval-honesty-smell-tests) | critical |
//! | `near_uniform_justification` | `smell:uniformity` (eval-honesty-smell-tests) | warning |
//! | `memory_recalled_into_bad_decision` | `memory:provenance` (memory-aware-eval-findings) | warning |
//! | `memory_recalled_into_good_decision` | `memory:provenance` (memory-aware-eval-findings, opt-in) | info |
//!
//! ## V2E item 25: `inference_cost_dominates_return`
//!
//! Emitted at run finalize when
//! `|inference_cost_quote_total| > k × |gross_return_quote|` (default k=0.5).
//! Annotate-only — does NOT gate or abort the run. The `produced_by_check`
//! field is set to `"metrics:cost_dominance"`. Evidence cycle ids are empty
//! because this is a per-run aggregate finding, not per-cycle.

pub mod extractor;
pub mod memory;
pub mod uniformity;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;
use xvision_data::validate::{defect_to_finding_evidence, DataDefect, DefectSeverity};

/// Current finding schema version. Bump when fields are added in a
/// backwards-incompatible way; additive `Option<_>` fields + `serde(default)`
/// do not bump the version (old rows just carry the zero value).
///
/// Version history:
///   "1"  — initial shape (migration 002)
///   "2"  — V2E trace-surface: `evidence_cycle_ids` + `produced_by_check`
///          (migration 026). Old rows loaded from disk with schema_version="1"
///          deserialize to empty `evidence_cycle_ids` and
///          `produced_by_check = "legacy"`.
pub const FINDING_SCHEMA_VERSION: &str = "2";

/// The `kind` string for a lookahead-bias finding produced by the two-pass
/// prober (`eval-lookahead-bias-prober` track).
pub const KIND_LOOKAHEAD_SUSPECTED: &str = "lookahead_suspected";

/// `produced_by_check` value for lookahead-bias findings.
pub const PRODUCED_BY_LOOKAHEAD_PROBER: &str = "prober:lookahead";

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Finding {
    pub id: String,
    pub run_id: String,
    /// Open enum: `regime_fit_mismatch`, `drawdown_concentration`,
    /// `overtrading`, `underperformance`, `risk_violation`, `win_rate_anomaly`,
    /// `tail_risk`, `lookahead_suspected`, or any LLM-proposed new kind.
    /// Validation belongs to downstream consumers.
    ///
    /// See the module-level kind registry for the full list.
    pub kind: String,
    pub severity: Severity,
    pub summary: String,
    /// LLM-extracted evidence blob — open-ended JSON. Typed as `unknown` on
    /// the wire so consumers narrow with a runtime guard if they need fields.
    #[cfg_attr(feature = "ts-export", ts(type = "unknown"))]
    pub evidence: serde_json::Value,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub extracted_at: DateTime<Utc>,
    pub schema_version: String,
    // --- V2E trace-surface fields (migration 026). Default empty / "legacy"
    // so rows with schema_version="1" continue to round-trip unchanged.
    /// ULIDs of the `cycles` rows whose data motivated this finding.
    /// Empty for legacy findings or findings produced without cycle-level
    /// evidence (e.g. aggregated metrics checks). `None` serialises as absent;
    /// consumers should treat absent and empty-array the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional, type = "Array<string>"))]
    pub evidence_cycle_ids: Option<Vec<String>>,
    /// Identifier of the check that produced this finding (e.g.
    /// `"lookahead_prober"`, `"broker_rule_engine"`, `"candle_integrity"`).
    /// Legacy rows (schema_version="1") carry `"legacy"`. Absent on wire
    /// means legacy; consumers should treat `None` as `"legacy"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub produced_by_check: Option<String>,
    // --- Review-linked v2 fields (migration 017). All optional so legacy
    // extractor rows continue to round-trip unchanged and so callers that
    // only need the v1 shape can leave them unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub eval_review_id: Option<String>,
    /// Review finding category: `performance | risk | regime | behavior |
    /// execution | data_quality | anomaly | opportunity` (open enum). The
    /// engine track maps this to legacy `kind` for compatibility.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub review_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub recommendation: Option<String>,
    #[cfg_attr(feature = "ts-export", ts(type = "string | null", optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
}

impl Finding {
    /// Construct a `lookahead_suspected` finding for a given run.
    pub fn lookahead_suspected(
        run_id: &str,
        cycle_id: &str,
        indicator_name: Option<&str>,
        pass_1_action: &str,
        pass_2_action: Option<&str>,
        snapshot_index: usize,
    ) -> Self {
        let now = Utc::now();
        Finding {
            id: Ulid::new().to_string(),
            run_id: run_id.to_string(),
            kind: KIND_LOOKAHEAD_SUSPECTED.to_string(),
            severity: Severity::Critical,
            summary: format!(
                "Lookahead bias suspected at cycle {} (snapshot {}): action '{}' was identical \
                 when bar t was withheld — the algorithm may read future bar data.",
                cycle_id, snapshot_index, pass_1_action,
            ),
            evidence: serde_json::json!({
                "cycle_id": cycle_id,
                "indicator_name": indicator_name,
                "pass_1_action": pass_1_action,
                "pass_2_action": pass_2_action,
                "snapshot_index": snapshot_index,
            }),
            extracted_at: now,
            schema_version: FINDING_SCHEMA_VERSION.to_string(),
            evidence_cycle_ids: Some(vec![cycle_id.to_string()]),
            produced_by_check: Some(PRODUCED_BY_LOOKAHEAD_PROBER.to_string()),
            eval_review_id: None,
            review_type: None,
            confidence: None,
            title: Some("Lookahead bias suspected".to_string()),
            description: Some(format!(
                "Two-pass prober detected that the algorithm produced action '{}' at \
                 snapshot {} even when bar t was withheld (bars[..=t-1] only). \
                 This indicates the decision did not depend on bar t, suggesting \
                 the algorithm may be using future bar data (bars[t+1..]) or \
                 pre-computed indicators that incorporate future information.",
                pass_1_action, snapshot_index,
            )),
            recommendation: Some(
                "Audit the algorithm's indicator computation to ensure no bar beyond \
                 the current bar is accessed. Run `xvn eval probe-lookahead --run <run_id>` \
                 for a full post-hoc analysis."
                    .to_string(),
            ),
            created_at: Some(now),
        }
    }
}

/// Evidence payload for the `inference_cost_dominates_return` finding kind
/// (V2E item 25). Persisted as the `evidence` JSON blob on the `Finding`.
///
/// `produced_by_check = "metrics:cost_dominance"`.
/// `evidence_cycle_ids` is empty — this is a per-run aggregate.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InferenceCostDominatesReturnPayload {
    /// `|inference_cost_quote_total| / |gross_return_quote|`. Values > threshold
    /// triggered the finding.
    pub ratio: f64,
    /// The configured threshold k (default 0.5).
    pub threshold: f64,
    /// Gross trading return in USD (capital × gross_return_pct / 100).
    pub gross_return_quote: f64,
    /// Total LLM inference cost for this run in USD.
    pub inference_cost_quote_total: f64,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Critical => "critical",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "info" => Some(Severity::Info),
            "warning" => Some(Severity::Warning),
            "critical" => Some(Severity::Critical),
            _ => None,
        }
    }
}

// ── Data-defect finding constructor ──────────────────────────────────────────

impl Finding {
    /// Construct a `data_defect` finding from a `DataDefect` detected by the
    /// OHLCV validator.
    ///
    /// - `kind = "data_defect"`
    /// - `produced_by_check = "validator:ohlcv"`
    /// - `evidence_cycle_ids = []` (data defects pre-exist the cycle)
    /// - Severity is mapped from the defect's tier: Error → Critical,
    ///   Warning → Warning, Info → Info.
    pub fn from_data_defect(run_id: &str, defect: &DataDefect) -> Self {
        let severity = match defect.severity() {
            DefectSeverity::Error => Severity::Critical,
            DefectSeverity::Warning => Severity::Warning,
            DefectSeverity::Info => Severity::Info,
        };
        let summary = data_defect_summary(defect);
        let evidence = defect_to_finding_evidence(defect);
        Finding {
            id: Ulid::new().to_string(),
            run_id: run_id.to_string(),
            kind: "data_defect".to_string(),
            severity,
            summary,
            evidence,
            extracted_at: Utc::now(),
            schema_version: FINDING_SCHEMA_VERSION.to_string(),
            evidence_cycle_ids: Some(vec![]),
            produced_by_check: Some("validator:ohlcv".to_string()),
            eval_review_id: None,
            review_type: None,
            confidence: None,
            title: None,
            description: None,
            recommendation: None,
            created_at: None,
        }
    }
}

fn data_defect_summary(defect: &DataDefect) -> String {
    match defect {
        DataDefect::NonMonotonicTimestamp { at, prev_ts, this_ts } => {
            format!("bar[{at}] timestamp {this_ts} is not after previous timestamp {prev_ts}")
        }
        DataDefect::DuplicateTimestamp { at, ts } => {
            format!("bar[{at}] has duplicate timestamp {ts}")
        }
        DataDefect::MissingBar {
            at,
            expected_ts,
            gap_bars,
        } => {
            format!("bar[{at}] has a gap: {gap_bars} missing bar(s) before {expected_ts}")
        }
        DataDefect::OhlcViolation { at, ts, kind } => {
            format!("bar[{at}] at {ts} violates OHLC invariant: {kind:?}")
        }
        DataDefect::NegativeOrNanField { at, ts, field } => {
            format!("bar[{at}] at {ts} has negative or NaN value for field '{field}'")
        }
        DataDefect::ZeroVolumeBar { at, ts } => {
            format!("bar[{at}] at {ts} has zero volume")
        }
        DataDefect::WickShockOutlier { at, ts, sigma } => {
            format!("bar[{at}] at {ts} is a wick-shock outlier (sigma={sigma:.1})")
        }
    }
}

/// Build a `volume_share_excess` finding (V2E — eval-cost-model-per-bar-and-volume-share).
///
/// Emitted once per binding cycle when `order_qty / bar_volume > volume_limit`
/// in the `VolumeShare` slippage model.
///
/// - `produced_by_check`: `"sim:volume_cap"` (simulator volume-cap gate)
/// - `evidence_cycle_ids`: contains the `cycle_id` whose order hit the cap
pub fn make_volume_share_excess_finding(
    run_id: &str,
    cycle_id: u32,
    requested_qty: f64,
    bar_volume: f64,
    cap_binding_qty: f64,
    fill_share: f64,
) -> Finding {
    Finding {
        id: Ulid::new().to_string(),
        run_id: run_id.to_owned(),
        kind: "volume_share_excess".to_owned(),
        severity: Severity::Warning,
        summary: format!(
            "Order qty {:.6} exceeds volume cap ({:.4} of bar volume {:.2}); capped to {:.6}",
            requested_qty, fill_share, bar_volume, cap_binding_qty,
        ),
        evidence: serde_json::json!({
            "requested_qty": requested_qty,
            "bar_volume": bar_volume,
            "cap_binding_qty": cap_binding_qty,
            "fill_share": fill_share,
        }),
        extracted_at: Utc::now(),
        schema_version: FINDING_SCHEMA_VERSION.to_owned(),
        evidence_cycle_ids: Some(vec![cycle_id.to_string()]),
        produced_by_check: Some("sim:volume_cap".to_owned()),
        eval_review_id: None,
        review_type: None,
        confidence: None,
        title: Some("Volume share cap binding".to_owned()),
        description: Some(
            "The requested order size exceeds the configured volume_limit fraction of bar volume. \
             Fill was capped; partial fill semantics apply."
                .to_owned(),
        ),
        recommendation: None,
        created_at: None,
    }
}
