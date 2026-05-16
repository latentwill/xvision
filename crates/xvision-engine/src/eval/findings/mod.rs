//! Phase 3.C findings — LLM-extracted structured insights about a
//! completed eval run. The shape mirrors the `eval_findings` table from
//! migration 002 plus the review-linked columns added in migration 017
//! (see `docs/superpowers/specs/2026-05-15-eval-review-agent.md`).

pub mod extractor;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    /// `tail_risk`, or any LLM-proposed new kind. Validation belongs to
    /// downstream consumers.
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
    // --- Review-linked v2 fields (migration 017). All optional so legacy
    // extractor rows continue to round-trip unchanged and so callers that
    // only need the v1 shape can leave them unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval_review_id: Option<String>,
    /// Review finding category: `performance | risk | regime | behavior |
    /// execution | data_quality | anomaly | opportunity` (open enum). The
    /// engine track maps this to legacy `kind` for compatibility.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub review_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<String>,
    #[cfg_attr(feature = "ts-export", ts(type = "string | null"))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
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
