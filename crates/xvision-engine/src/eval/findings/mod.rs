//! Phase 3.C findings — LLM-extracted structured insights about a
//! completed eval run. The shape mirrors the `eval_findings` table from
//! migration 002.

pub mod extractor;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub evidence: serde_json::Value,
    pub extracted_at: DateTime<Utc>,
    pub schema_version: String,
}

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
