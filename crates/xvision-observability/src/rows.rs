//! Row types matching migration 018. Every column in the migration has a
//! field here; the recorder writes via `INSERT` against these structs.
//!
//! We use plain types (not `sqlx::FromRow`) to keep the crate cheap to
//! build — sqlx's derive macros can dominate compile time. Manual row
//! mapping lives in the recorder (`agent-run-observability-event-bus`
//! leaf).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunRow {
    pub id: String,
    pub objective: String,
    pub strategy_id: Option<String>,
    pub eval_run_id: Option<String>,
    pub source_cli_job_id: Option<String>,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub retention_mode: String,
    pub sidecar_version: Option<String>,
    pub cline_sdk_version: Option<String>,
    pub protocol_version: Option<String>,
    pub skills_json: Option<String>,
    pub mcp_servers_json: Option<String>,
    pub otel_trace_id: Option<String>,
    pub final_artifact_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanRow {
    pub id: String,
    pub run_id: String,
    pub parent_span_id: Option<String>,
    pub otel_trace_id: Option<String>,
    pub otel_span_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub attributes_json: Option<String>,
    pub error_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRow {
    pub id: String,
    pub run_id: String,
    pub span_id: String,
    pub sequence: i64,
    pub kind: String,
    pub input_hash: String,
    pub output_hash: Option<String>,
    pub input_payload_ref: Option<String>,
    pub output_payload_ref: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCallRow {
    pub span_id: String,
    pub provider: String,
    pub model: String,
    pub input_token_count: Option<i64>,
    pub output_token_count: Option<i64>,
    pub cost_usd: Option<f64>,
    pub prompt_hash: String,
    pub response_hash: Option<String>,
    pub prompt_text: Option<String>,
    pub response_text: Option<String>,
    pub prompt_payload_ref: Option<String>,
    pub response_payload_ref: Option<String>,
    pub tool_calls_requested: Option<String>,
    pub capability_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRow {
    pub span_id: String,
    pub tool_name: String,
    pub origin: String,
    pub tool_version: Option<String>,
    pub tool_hash: Option<String>,
    pub input_hash: String,
    pub output_hash: Option<String>,
    /// Reconstructed plaintext tool input from the `tool_call_payload`
    /// side-row (mirrors `ModelCallRow::prompt_text`). `None` when no
    /// side-row exists (hash-only runs, or pre-payload tool calls).
    pub input_text: Option<String>,
    /// Reconstructed plaintext tool output from the `tool_call_payload`
    /// side-row (mirrors `ModelCallRow::response_text`).
    pub output_text: Option<String>,
    pub input_payload_ref: Option<String>,
    pub output_payload_ref: Option<String>,
    pub side_effect_level: String,
    pub risk_level: String,
    pub requires_approval: bool,
    pub approval_id: Option<String>,
    pub exit_code: Option<i64>,
    pub is_run_terminator: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRow {
    pub id: String,
    pub span_id: String,
    pub tool_call_id: String,
    pub reason: String,
    pub risk_level: String,
    pub requested_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decision: Option<String>,
    pub decided_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResultRow {
    pub span_id: String,
    pub command: String,
    pub cwd: Option<String>,
    pub stdout_ref: Option<String>,
    pub stderr_ref: Option<String>,
    pub exit_code: i64,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorNoteRow {
    pub id: String,
    pub run_id: String,
    pub role: String,
    pub content: String,
    pub severity: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRow {
    pub id: String,
    pub run_id: String,
    pub kind: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub hypothesis: Option<String>,
    pub recommendation: Option<String>,
    pub evidence_json: Option<String>,
    pub next_experiments_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRow {
    pub id: String,
    pub run_id: String,
    pub span_id: Option<String>,
    pub kind: String,
    pub payload_json: Option<String>,
    pub created_at: DateTime<Utc>,
}
