//! `RunEvent` — the canonical event vocabulary produced by the
//! `xvision-agent-client` IPC handler (Phase B) and consumed by recorder
//! subscribers.
//!
//! The vocabulary maps 1:1 to the IPC notification table in the
//! observability plan. Each variant carries enough data for the
//! `SqliteRecorder` to write the corresponding row(s) without needing
//! to round-trip back to the producer.

use crate::types::{CapabilityPath, RiskLevel, RunStatus, SideEffectLevel, SpanKind, SpanStatus, ToolOrigin};
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

    /// One broker submit → fill/reject cycle. Started on
    /// `BrokerSurface::submit_order` entry, finished on its return
    /// (success or typed broker error). `qa-trace-broker-spans`.
    BrokerCallStarted(BrokerCallStartedEvent),
    BrokerCallFinished(BrokerCallFinishedEvent),

    CheckpointWritten(CheckpointWrittenEvent),

    /// Persisted only as a span-attached `events` row (not its own table)
    /// to keep the timeline reconstructable. Stream-only payloads are
    /// not stored — the delta text is on a separate SSE channel.
    AssistantTextDelta(AssistantTextDeltaEvent),

    SupervisorNote(SupervisorNoteEvent),
    ArtifactWritten(ArtifactWrittenEvent),

    SidecarError(SidecarErrorEvent),
    BackpressureDropped(BackpressureDroppedEvent),

    /// V2D auto-recall hit set, bound to the per-decision identifier the
    /// recall fed into. `memory-provenance-in-decisions-trace`: ledger
    /// events were previously run-level only (`tracing::info!` logs that
    /// landed nowhere persistent). This variant threads `decision_id`
    /// through so the eval-review surface can answer "which memories
    /// drove decision N." Persisted into the `events` table by
    /// `SqliteRecorder` (no schema migration — the `events` table
    /// already accepts arbitrary `(kind, payload_json)` rows).
    ///
    /// Disjoint from `trace-dock-emitters` event-kinds — that wave adds
    /// new orthogonal event variants for the trace dock; this one is
    /// scoped to memory provenance.
    MemoryRecall(MemoryRecallEvent),

    /// V2D Observation write, bound to the decision that produced it.
    /// Persisted as a generic `events` row so flywheel surfaces can
    /// correlate recall -> model call -> remembered Observation.
    MemoryWrite(MemoryWriteEvent),

    /// Bar-level engine lifecycle event. Persists as a row in the
    /// migration-018 `events` table (no dedicated table), which is the
    /// schema-018 design for sparse/extensible bar-level signals such as
    /// `decision_started`, `decision_completed`, `fill_attempted`,
    /// `guardrail_fired`, `early_stop_triggered`, `flat_skip_fired`.
    ///
    /// Added by F43 (`trace-dock-emitters`) to close the eval-traces
    /// audit F-11(b) gap: the `events` table had no writer in the
    /// observability crate before this contract.
    EngineEvent(EngineEvent),
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
    /// Optional run-level trajectory mode for Cline sidecar runs. Omitted by
    /// older/non-Cline producers, in which case the DB default remains `live`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trajectory_mode: Option<String>,
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
    /// Full plaintext prompt/request body. Sidecar model-call spans must keep
    /// this visible for trace review; callers that cannot retain plaintext
    /// leave it `None` and still populate `prompt_hash`.
    pub prompt_text: Option<String>,
    /// Full plaintext assistant/model response body. For tool-use turns this
    /// may be a JSON summary of the streamed tool-call deltas.
    pub response_text: Option<String>,
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
    /// Full plaintext tool input (args JSON) BEFORE blob persistence.
    /// Carried on the event so the recorder can write a
    /// `tool_call_payload` side-row, mirroring `prompt_text` on
    /// `ModelCallFinishedEvent`. NEVER persisted as a `tool_calls`
    /// column — the recorder consumes it into the `events` table only.
    /// Producers under `hash_only` leave it `None`.
    #[serde(default)]
    pub input_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFinishedEvent {
    pub span_id: String,
    pub output_hash: Option<String>,
    pub output_payload_ref: Option<String>,
    pub exit_code: Option<i64>,
    /// Full plaintext tool output BEFORE blob persistence. Same
    /// recorder-only side-row contract as
    /// `ToolCallStartedEvent::input_text` / `response_text` on the
    /// model-call event. Producers under `hash_only` leave it `None`.
    #[serde(default)]
    pub output_text: Option<String>,
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
///
/// `delta_text` carries the actual chunk text so the trace dock can render
/// the assistant body as it streams, instead of just a character count.
/// Producers should cap individual chunks at
/// `ObservabilityConfig::retention.max_payload_bytes` and truncate with a
/// trailing `…` if exceeded — the recorder still stamps the original
/// `delta_len` for monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantTextDeltaEvent {
    pub span_id: String,
    pub run_id: String,
    pub delta_len: usize,
    #[serde(default)]
    pub delta_text: String,
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

/// One V2D `memory_recall` hit set, scoped to a single decision inside a
/// run. The recorder persists it into the `events` table as
/// `kind = "memory_recall"` with the full payload serialized into
/// `payload_json`; `run_id`/`decision_id`/`memory_item_ids[]` are
/// reconstructable for the eval-review join.
///
/// Item shape mirrors `xvision_memory::types::MemoryMatch` without
/// pulling the dep into observability — the event payload carries plain
/// primitives so consumers don't need the memory crate to deserialize.
/// Embeddings are intentionally NOT carried; the operator surfaces use
/// the `id` to deep-link back to the memory store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecallEvent {
    pub run_id: String,
    /// Stable cycle correlation key used by flywheel surfaces to stitch
    /// capture -> observe -> recall -> outcome across event families.
    /// New emitters populate this as `<run_id>:<decision_id>`; optional
    /// for backward compatibility with already-persisted payload_json.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flywheel_cycle_id: Option<String>,
    /// Per-decision identifier the recall fed into. Encoded as the
    /// engine's `cycle_idx: i64` (the per-decision integer carried on
    /// `SlotInput` and threaded through `MemoryRecorder::recall`). The
    /// tuple `(run_id, decision_id)` uniquely keys the recall to its
    /// owning decision; consumers can also denormalize against
    /// `scenario_id` via the run's surrounding context if needed.
    pub decision_id: i64,
    /// Memory namespace queried (e.g. `agent:<id>` or `global`). Lets
    /// the dashboard render namespace-scoped deep links without
    /// re-deriving the namespace.
    pub namespace: String,
    /// Per-item recall payload. Ordered by the recall score the store
    /// returned; consumers can re-sort if they want. Empty when recall
    /// completed but returned zero hits — the event is still emitted so
    /// the timeline records the attempt.
    pub items: Vec<MemoryRecallItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecallItem {
    /// Memory store id of the recalled item. Used by the dashboard to
    /// build the "Open Pattern" deep link.
    pub id: String,
    /// Cosine similarity (or whatever scoring fn the store applied)
    /// between the query and the item. Higher is more relevant.
    pub score: f32,
    /// First ~160 chars of the item's text. The recorder DOES NOT carry
    /// the full body — that lives in `memory_items.text`. Operators who
    /// want the full text follow the `id` deep link.
    pub text_preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryWriteEvent {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flywheel_cycle_id: Option<String>,
    pub decision_id: i64,
    pub namespace: String,
    pub memory_item_id: String,
    pub text_preview: String,
}

/// One bar-level engine lifecycle signal. Recorded as a single row in
/// the `events` table.
///
/// `kind` is a free-form snake_case string the producer chooses; the
/// dashboard projects it verbatim. F43 introduces these known kinds:
///
///   - `decision_started` — opening a per-decision pipeline iteration
///   - `decision_completed` — closing the same
///   - `fill_attempted` — broker submit / paper-fill attempt was made
///   - `guardrail_fired` — guardrail rewrote / blocked a trader action
///   - `early_stop_triggered` — flat-degeneracy early-stop policy fired
///   - `flat_skip_fired` — trader-noop-skip short-circuited the LLM
///   - `preflight_warning` — pre-run preflight surfaced a warning
///   - `broker_rule_violation` — broker rule rejected / warned an order
///   - `cost_cap_warning` — max-tokens / cost guard surfaced a warning
///
/// `payload_json` is producer-defined. F43 emits structured payloads
/// with `decision_index`, `asset`, action/severity etc. — see
/// `xvision-engine` call sites for the canonical shape per kind.
///
/// Producers MUST scrub secrets out of any free-text fields before
/// serializing into `payload_json` — the writer trusts the producer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineEvent {
    pub run_id: String,
    /// Optional span this event is scoped to. `None` when the event is
    /// run-scoped (e.g. early-stop policy fire that isn't bracketed by
    /// a specific span). The dashboard joins on this to surface event
    /// rows in the SpanInspector when present.
    pub span_id: Option<String>,
    /// Producer-defined kind string. See struct docs for known values.
    pub kind: String,
    /// Producer-defined structured payload. Caller is responsible for
    /// keeping this redacted — the writer does not scan.
    pub payload_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Side of a broker submit. Mirrors `xvision_execution::Side`, plus
/// the higher-level `CloseFlat` / `ShortOpen` intents the executor
/// derives from the trader's action. Operators look at this column in
/// the trace dock to spot missing short fills (#14 in the round-2
/// intake).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerSide {
    Buy,
    Sell,
    Close,
    Short,
}

/// Terminal state of a broker submit. `Filled` covers both full and
/// partial fills (the qty / price columns disambiguate). `Rejected`
/// carries a broker-side reason; `Cancelled` is operator-initiated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerCallOutcome {
    Filled,
    Rejected,
    Cancelled,
    /// Transport / 5xx / timeout — distinct from `Rejected` (which is a
    /// well-formed broker NACK).
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerCallStartedEvent {
    pub span_id: String,
    pub run_id: String,
    pub side: BrokerSide,
    pub symbol: String,
    pub qty: f64,
    /// Reference / intended price at submit time. Fill price lands on
    /// the matching [`BrokerCallFinishedEvent`].
    pub intended_price: Option<f64>,
    /// `market` / `limit` / `stop_limit` etc. Producer-defined string;
    /// the dashboard renders it verbatim.
    pub order_type: String,
    /// e.g. `alpaca-paper`, `alpaca-live`, `orderly`. Lets operators
    /// distinguish paper vs. real fills at a glance.
    pub venue: String,
    /// Client-side dedupe key the producer set on the broker submit
    /// (e.g. `<run_id>-<decision_idx>`). Lets traces correlate to
    /// decision rows without joining on (run_id, decision_idx).
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerCallFinishedEvent {
    pub span_id: String,
    pub outcome: BrokerCallOutcome,
    pub fill_price: Option<f64>,
    pub fill_qty: Option<f64>,
    pub fee: Option<f64>,
    /// Broker venue's order id on success; `None` on transport failure
    /// before an id was assigned.
    pub broker_order_id: Option<String>,
    /// Short error class — e.g. `broker_rejected`, `broker_auth`,
    /// `broker_unsupported`, `broker_insufficient_funds`,
    /// `broker_timeout`. Only set on `Rejected` / `Failed` outcomes.
    pub error_class: Option<String>,
    /// Verbatim broker / transport message. Truncated upstream if it
    /// exceeds the observability payload cap.
    pub error_message: Option<String>,
    /// Severity tag for the trace dock. `Some("warn")` means the
    /// broker rejected the order but the run continues — the agent
    /// gets the error fed back on the next decision cycle.
    /// `Some("error")` means the run terminated. Added by
    /// `agent-error-feedback-self-healing`; older producers leave
    /// it `None` and the dashboard falls back to deriving from the
    /// outcome.
    #[serde(default)]
    pub severity: Option<String>,
}

impl RunEvent {
    /// Convenience for tests + recorder routing. Returns the run id this
    /// event belongs to, or `""` if the event only carries a `span_id`
    /// (see [`Self::span_id`] for those variants — the bus resolves them
    /// via its span→run map).
    pub fn run_id(&self) -> &str {
        match self {
            Self::RunStarted(e) => &e.run_id,
            Self::RunFinished(e) => &e.run_id,
            Self::RunInterrupted(e) => &e.run_id,
            Self::SpanStarted(e) => &e.run_id,
            Self::SpanFinished(_e) => "",
            Self::ModelCallFinished(_) => "",
            Self::ToolCallStarted(_) => "",
            Self::ToolCallFinished(_) => "",
            Self::ToolCallFailed(_) => "",
            Self::ToolCallCancelled(_) => "",
            Self::BrokerCallStarted(e) => &e.run_id,
            Self::BrokerCallFinished(_) => "",
            Self::CheckpointWritten(e) => &e.run_id,
            Self::AssistantTextDelta(e) => &e.run_id,
            Self::SupervisorNote(e) => &e.run_id,
            Self::ArtifactWritten(e) => &e.run_id,
            Self::SidecarError(e) => &e.run_id,
            Self::BackpressureDropped(e) => &e.run_id,
            Self::MemoryRecall(e) => &e.run_id,
            Self::MemoryWrite(e) => &e.run_id,
            Self::EngineEvent(e) => &e.run_id,
        }
    }

    /// Returns the span this event is scoped to, if any. Span-scoped
    /// events omit `run_id` to keep the payload small; the bus routes
    /// them to a run via the span→run map it builds from `SpanStarted`.
    pub fn span_id(&self) -> Option<&str> {
        match self {
            Self::SpanStarted(e) => Some(&e.span_id),
            Self::SpanFinished(e) => Some(&e.span_id),
            Self::ModelCallFinished(e) => Some(&e.span_id),
            Self::ToolCallStarted(e) => Some(&e.span_id),
            Self::ToolCallFinished(e) => Some(&e.span_id),
            Self::ToolCallFailed(e) => Some(&e.span_id),
            Self::ToolCallCancelled(e) => Some(&e.span_id),
            Self::BrokerCallStarted(e) => Some(&e.span_id),
            Self::BrokerCallFinished(e) => Some(&e.span_id),
            Self::CheckpointWritten(e) => Some(&e.span_id),
            Self::AssistantTextDelta(e) => Some(&e.span_id),
            Self::EngineEvent(e) => e.span_id.as_deref(),
            Self::RunStarted(_)
            | Self::RunFinished(_)
            | Self::RunInterrupted(_)
            | Self::SupervisorNote(_)
            | Self::ArtifactWritten(_)
            | Self::SidecarError(_)
            | Self::BackpressureDropped(_)
            | Self::MemoryRecall(_)
            | Self::MemoryWrite(_) => None,
        }
    }

    /// Lifecycle-critical events that must never be silently dropped:
    /// losing one of them leaves the run/spans open in SQLite forever
    /// or hides a sidecar crash. The bus delivers these with
    /// backpressure (awaits a slot) rather than `try_send`.
    pub fn is_lifecycle_critical(&self) -> bool {
        matches!(
            self,
            Self::RunStarted(_) | Self::RunFinished(_) | Self::RunInterrupted(_) | Self::SidecarError(_)
        )
    }
}
