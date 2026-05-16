//! Eval Review Agent data model. See
//! `docs/superpowers/specs/2026-05-15-eval-review-agent.md`.
//!
//! Scope for this module is the persistence-layer shape only:
//!
//! * [`AgentProfile`] — review-agent persona seeded by migration 016
//!   (`fast-trader-agent`, `reasoning-agent`, `risk-agent`,
//!   `research-agent`). The engine track composes the operational prompt
//!   from `system_prompt` + a strict-JSON contract at request time.
//! * [`EvalReview`] — parent artifact persisted per (run × profile)
//!   review request. Status transitions: queued → running → (completed |
//!   failed). The raw model JSON is preserved on `raw_output_json` for
//!   audit; normalized findings live on `eval_findings` rows linked back
//!   via `eval_review_id`.
//!
//! Review-linked finding columns are added to [`super::Finding`] so review
//! findings remain first-class rows, not nested inside `raw_output_json`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

impl ReviewStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewStatus::Queued => "queued",
            ReviewStatus::Running => "running",
            ReviewStatus::Completed => "completed",
            ReviewStatus::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(ReviewStatus::Queued),
            "running" => Some(ReviewStatus::Running),
            "completed" => Some(ReviewStatus::Completed),
            "failed" => Some(ReviewStatus::Failed),
            _ => None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, ReviewStatus::Completed | ReviewStatus::Failed)
    }
}

/// Strict verdict tag the review must return. Persisted as a plain string
/// so the DB stays schema-loose if downstream profiles need to evolve the
/// allowed values without a migration.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Promising,
    Weak,
    Failed,
    Inconclusive,
}

impl ReviewVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReviewVerdict::Promising => "promising",
            ReviewVerdict::Weak => "weak",
            ReviewVerdict::Failed => "failed",
            ReviewVerdict::Inconclusive => "inconclusive",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "promising" => Some(ReviewVerdict::Promising),
            "weak" => Some(ReviewVerdict::Weak),
            "failed" => Some(ReviewVerdict::Failed),
            "inconclusive" => Some(ReviewVerdict::Inconclusive),
            _ => None,
        }
    }
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    /// Persona tag (`fast-trader`, `reasoning`, `risk`, `research`, or
    /// operator-defined). Open enum on the wire and in storage so custom
    /// profiles can be added without a migration.
    #[serde(rename = "type")]
    pub profile_type: String,
    pub provider: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: u32,
    pub system_prompt: String,
    pub enabled: bool,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub updated_at: DateTime<Utc>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvalReview {
    pub id: String,
    pub eval_run_id: String,
    pub agent_profile_id: String,
    pub status: ReviewStatus,
    pub verdict: Option<ReviewVerdict>,
    pub confidence: Option<f64>,
    pub score: Option<i32>,
    pub summary: Option<String>,
    /// Raw strict-JSON reply preserved verbatim for audit. Engine track
    /// is responsible for slicing the prose around the JSON before
    /// persisting; the data layer just stores whatever string is handed
    /// in.
    #[cfg_attr(feature = "ts-export", ts(type = "string | null"))]
    pub raw_output_json: Option<String>,
    pub error: Option<String>,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub updated_at: DateTime<Utc>,
}

impl EvalReview {
    /// Construct a fresh `Queued` review with a generated ULID and
    /// `created_at = updated_at = now`. Callers persist via
    /// `RunStore::create_review` and advance the state machine through
    /// `RunStore::update_review_status` / `RunStore::complete_review`.
    pub fn new_queued(eval_run_id: String, agent_profile_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: Ulid::new().to_string(),
            eval_run_id,
            agent_profile_id,
            status: ReviewStatus::Queued,
            verdict: None,
            confidence: None,
            score: None,
            summary: None,
            raw_output_json: None,
            error: None,
            created_at: now,
            updated_at: now,
        }
    }
}
