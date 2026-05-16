//! `RunEvent` — the canonical event vocabulary produced by the
//! `xvision-agent-client` IPC handler (Phase B) and consumed by recorder
//! subscribers.
//!
//! The vocabulary maps 1:1 to the IPC notification table in the
//! observability plan. Each variant carries enough data for the
//! `SqliteRecorder` to write the corresponding row(s) without needing
//! to round-trip back to the producer.

use crate::types::{
    CapabilityPath, RiskLevel, RunStatus, SideEffectLevel, SpanKind, SpanStatus,
    ToolOrigin,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Carried out-of-band when the bus has to drop an event under
/// backpressure. The recorder writes a `supervisor_notes` warn row
/// referencing this counter so gaps are visible in the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedEvents {
    pub run_id_hash: u64,
    pub dropped: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunEvent {
    RunStarted(RunStartedEvent),
    RunFinished(RunFinishedEvent),
    /// Sidecar crash mid-run. Recorder marks every open span as
    /// `interrupted` and bumps the run status.
    RunInterrupted(RunInterruptedEvent),

    SpanStarted(SpanStartedEvent),
    SpanFinished(SpanFinishedEvent),

    ModelCallFinished(ModelCallFinishedEvent),
    ToolCallStarted(ToolCallStartedEvent),
    ToolCallFinished(ToolCallFinishedEvent),
    ToolCallFailed(ToolCallFailedEvent),
    ToolCallCancelled(ToolCallCancelledEvent),

    CheckpointWritten(CheckpointWrittenEvent),

    /// Persisted only as a span-attached `events` row (not its own table)
    /// to keep the timeline reconstructable. Stream-only payloads are
    /// not stored — the delta text is on a separate SSE channel.
    AssistantTextDelta(AssistantTextDeltaEvent),

    SupervisorNote(SupervisorNoteEvent),
    ArtifactWritten(ArtifactWrittenEvent),

    SidecarError(SidecarErrorEvent),
    BackpressureDropped(BackpressureDroppedEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStartedEvent {
    pub run_id: String,
    pub objective: String,
    pub strategy_id: Option<String>,
    pub eval_run_id: Option<String>,
    pub source_cli_job_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub retention_mode: String,
    pub sidecar_version: Option<String>,
    pub cline_sdk_version: Option<String>,
    pub protocol_version: Option<String>,
    pub skills_json: Option<String>,
    pub mcp_servers_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunFinishedEvent {
    pub run_id: String,
    pub finished_at: DateTime<Utc>,
    pub status: RunStatus,
    pub final_artifact_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInterruptedEvent {
    pub run_id: String,
    pub finished_at: DateTime<Utc>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanStartedEvent {
    pub span_id: String,
    pub run_id: String,
    pub parent_span_id: Option<String>,
    pub kind: SpanKind,
    pub name: String,
    pub started_at: DateTime<Utc>,
    pub otel_trace_id: Option<String>,
    pub otel_span_id: Option<String>,
    pub attributes_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanFinishedEvent {
    pub span_id: String,
    pub ended_at: DateTime<Utc>,
    pub status: SpanStatus,
    pub error_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCallFinishedEvent {
    pub span_id: String,
    pub provider: String,
    pub model: String,
    pub input_token_count: Option<i64>,
    pub output_token_count: Option<i64>,
    pub cost_usd: Option<f64>,
    pub prompt_hash: String,
    pub response_hash: Option<String>,
    pub prompt_payload_ref: Option<String>,
    pub response_payload_ref: Option<String>,
    pub tool_calls_requested: Option<String>,
    pub capability_path: Option<CapabilityPath>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallStartedEvent {
    pub span_id: String,
    pub tool_name: String,
    pub origin: ToolOrigin,
    pub tool_version: Option<String>,
    pub tool_hash: Option<String>,
    pub side_effect_level: SideEffectLevel,
    pub risk_level: RiskLevel,
    pub requires_approval: bool,
    pub is_run_terminator: bool,
    pub input_hash: String,
    pub input_payload_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFinishedEvent {
    pub span_id: String,
    pub output_hash: Option<String>,
    pub output_payload_ref: Option<String>,
    pub exit_code: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFailedEvent {
    pub span_id: String,
    pub error_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallCancelledEvent {
    pub span_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointWrittenEvent {
    pub checkpoint_id: String,
    pub run_id: String,
    pub span_id: String,
    pub sequence: i64,
    /// `model_step` or `tool_step`.
    pub kind: String,
    pub input_hash: String,
    pub output_hash: Option<String>,
    pub input_payload_ref: Option<String>,
    pub output_payload_ref: Option<String>,
}

/// Streamed to the dashboard SSE channel; intentionally not persisted in
/// its own table. Recorder may optionally write a coarse `events` row if
/// retention policy asks for it; default is to discard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantTextDeltaEvent {
    pub span_id: String,
    pub run_id: String,
    pub delta_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorNoteEvent {
    pub run_id: String,
    /// planner|reviewer|guard|system
    pub role: String,
    pub content: String,
    /// info|warn|error
    pub severity: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactWrittenEvent {
    pub artifact_id: String,
    pub run_id: String,
    /// final|intermediate
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
pub struct SidecarErrorEvent {
    pub run_id: String,
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackpressureDroppedEvent {
    pub run_id: String,
    pub dropped: u32,
    pub note: String,
}

impl RunEvent {
    /// Convenience for tests + recorder routing. Returns the run id this
    /// event belongs to.
    pub fn run_id(&self) -> &str {
        match self {
            Self::RunStarted(e) => &e.run_id,
            Self::RunFinished(e) => &e.run_id,
            Self::RunInterrupted(e) => &e.run_id,
            Self::SpanStarted(e) => &e.run_id,
            Self::SpanFinished(_e) => {
                // SpanFinished doesn't carry run_id directly to keep the
                // event small. The bus routes by span_id → run_id via the
                // recorder's open-span table. For tests, fall back to "".
                ""
            }
            Self::ModelCallFinished(_) => "",
            Self::ToolCallStarted(_) => "",
            Self::ToolCallFinished(_) => "",
            Self::ToolCallFailed(_) => "",
            Self::ToolCallCancelled(_) => "",
            Self::CheckpointWritten(e) => &e.run_id,
            Self::AssistantTextDelta(e) => &e.run_id,
            Self::SupervisorNote(e) => &e.run_id,
            Self::ArtifactWritten(e) => &e.run_id,
            Self::SidecarError(e) => &e.run_id,
            Self::BackpressureDropped(e) => &e.run_id,
        }
    }
}
