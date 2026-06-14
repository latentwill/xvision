//! `UnifiedEvent` — the single typed event model shared by the chat rail
//! and the trace dock.
//!
//! Phase 1.1 of the chat-rail / DSPy / strategy-agents wave
//! (`docs/superpowers/specs/2026-05-24-chat-rail-and-strategy-agents-evaluation.md`).
//!
//! ## Why an envelope, not a rewrite
//!
//! Two mature vocabularies already exist:
//!
//! - [`RunEvent`] (this crate) — agent-run observability, consumed by the
//!   trace dock over `/api/agent-runs/:id/stream`.
//! - `WizardEvent` (`xvision-dashboard`) — chat-rail authoring, streamed over
//!   `/api/chat-rail/chat`.
//!
//! The rail and dock currently project from these two *separate* streams with
//! two row models. `UnifiedEvent` is the one envelope both project from: it
//! carries universal addressing + ordering on the outside and reuses the
//! existing strongly-typed detail structs on the inside. It does **not**
//! replace `RunEvent`; it is the projection layer.
//!
//! The `From<RunEvent>` direction lives here (same crate). The
//! `From<WizardEvent>` direction lives in `xvision-dashboard` because
//! `observability` must not depend upward on the dashboard crate.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::events::{
    ArtifactWrittenEvent, BackpressureDroppedEvent, BrokerCallFinishedEvent, BrokerCallStartedEvent,
    CheckpointWrittenEvent, EngineEvent, MemoryRecallEvent, MemoryWriteEvent, ModelCallFinishedEvent,
    RunEvent, RunFinishedEvent, RunInterruptedEvent, RunStartedEvent, SidecarErrorEvent, SpanFinishedEvent,
    SpanStartedEvent, SupervisorNoteEvent, ToolCallCancelledEvent, ToolCallFailedEvent,
    ToolCallFinishedEvent, ToolCallStartedEvent,
};

/// Who or what produced the event. Lets the UI attribute a row to the
/// operator vs. the agent vs. an automated hook without parsing payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    /// A human operator (typed a message, approved a tool, edited focus).
    Operator,
    /// The agent / model (assistant output, tool requests).
    Agent,
    /// The harness itself (run lifecycle, persistence, backpressure).
    System,
    /// A registered hook (evidence capture, policy enforcement).
    Hook,
    /// The offline optimizer (`xvision-dspy`).
    Optimizer,
}

/// Which surface emitted the event. Provenance for debugging the dual-path
/// migration: a unified consumer can assert no row is sourced from a
/// deprecated path once the shim is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    ChatRail,
    AgentRun,
    Engine,
    Optimizer,
    Hook,
}

/// The scope an event is attached to. Mirrors the dashboard `ContextScope`
/// addressing as a flat `(kind, id)` pair so `observability` does not have to
/// depend on the engine crate's scope enum. `kind` is the snake_case scope
/// discriminant (`workspace`, `run`, `strategy`, …); `id` is the scoped id
/// when the scope names one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventScope {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl EventScope {
    pub fn workspace() -> Self {
        Self {
            kind: "workspace".into(),
            id: None,
        }
    }
    pub fn new(kind: impl Into<String>, id: Option<String>) -> Self {
        Self {
            kind: kind.into(),
            id,
        }
    }
}

/// Outcome of a server-side tool-policy check (Phase 2.3). Carried on a
/// [`UnifiedPayload::ToolPolicyChecked`] so the rail can render approval rows
/// and the server can prove enforcement happened before execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPolicyOutcome {
    /// Enabled + auto-approved: execution proceeds.
    AutoApproved,
    /// Enabled but requires operator approval before execution.
    NeedsApproval,
    /// Disabled or denied by mode (e.g. a write tool in Research mode).
    Denied,
}

/// The unified event envelope. Every chat-rail row and every trace-dock row
/// is a projection of one of these.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedEvent {
    /// Globally-unique id for this event (ULID). Stable across reconnects so
    /// the per-row reducer is idempotent.
    pub event_id: String,
    /// Owning chat session, when the event belongs to a rail conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Owning agent run, when the event belongs to an execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// Span this event is scoped to within a run, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    /// Causally-preceding event (e.g. a tool result points at its request).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_event_id: Option<String>,
    /// Monotonic per-session sequence number. The reducer orders + dedupes on
    /// `(session_id, seq)`; gaps signal a dropped event.
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub scope: EventScope,
    pub actor: Actor,
    pub source: EventSource,
    /// Optional content-addressed hash of a large payload offloaded to the
    /// blob store. The inline payload stays redacted/bounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_hash: Option<String>,
    pub payload: UnifiedPayload,
}

/// The kind-tagged payload. Agent-run-derived kinds reuse the existing
/// `RunEvent` detail structs (strong typing, zero drift); net-new kinds for
/// the rail, focus chain, tool policy, optimization, and typed errors get
/// their own small structs.
///
/// Adjacently tagged (`{ "kind": "...", "data": { … } }`) — **not** internally
/// tagged. Several reused detail structs (`CheckpointWrittenEvent`,
/// `ArtifactWrittenEvent`, `EngineEvent`) carry their own `kind` field, which
/// collides with an internal `tag = "kind"` (serde rejects it as a duplicate
/// field). Nesting the payload under `data` keeps the envelope discriminant
/// free of the inner structs' fields. The TS mirror models this as
/// `{ kind, data }`; unit variants omit `data`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum UnifiedPayload {
    // ── Session lifecycle (rail-originated) ─────────────────────────────
    SessionCreated { scope_label: String },
    SessionResumed { from_seq: u64 },
    SessionInterrupted { reason: String },
    SessionCompleted,
    SessionFailed { message: String },

    // ── Run lifecycle (agent-run, reused from RunEvent) ─────────────────
    RunStarted(RunStartedEvent),
    RunFinished(RunFinishedEvent),
    RunInterrupted(RunInterruptedEvent),
    SpanStarted(SpanStartedEvent),
    SpanFinished(SpanFinishedEvent),
    ModelCallFinished(ModelCallFinishedEvent),

    // ── Assistant output ────────────────────────────────────────────────
    AssistantMessageStarted,
    AssistantTokenDelta { text: String },
    AssistantContentBlock { block: serde_json::Value },
    AssistantMessageDone { draft_id: Option<String> },

    // ── Tool lifecycle ──────────────────────────────────────────────────
    ToolRequested(ToolCallStartedEvent),
    ToolPolicyChecked(ToolPolicyChecked),
    ToolApproved { span_id: String, approver: String },
    ToolStarted { span_id: String },
    ToolDelta { span_id: String, text: String },
    ToolFinished(ToolCallFinishedEvent),
    ToolFailed(ToolCallFailedEvent),
    ToolCancelled(ToolCallCancelledEvent),
    ToolDenied(ToolDenied),

    // ── Broker (xvision-specific, reused) ───────────────────────────────
    BrokerCallStarted(BrokerCallStartedEvent),
    BrokerCallFinished(BrokerCallFinishedEvent),

    // ── Checkpoints ─────────────────────────────────────────────────────
    CheckpointCreated(CheckpointWrittenEvent),
    CheckpointRestored(CheckpointRestored),
    CheckpointRestoreFailed(CheckpointRestoreFailed),

    // ── Focus chain ─────────────────────────────────────────────────────
    FocusLoaded(FocusEvent),
    FocusEdited(FocusEvent),
    FocusInjected(FocusEvent),

    // ── Optimization (offline; surfaced live in the rail) ───────────────
    OptimizationCandidateStarted(OptimizationCandidate),
    OptimizationCandidateMetric(OptimizationCandidateMetric),
    OptimizationCandidateSelected(OptimizationCandidate),
    OptimizationCompleted(OptimizationCompleted),

    // ── Provenance / supervision (reused) ───────────────────────────────
    MemoryRecall(MemoryRecallEvent),
    MemoryWrite(MemoryWriteEvent),
    ArtifactWritten(ArtifactWrittenEvent),
    SupervisorNote(SupervisorNoteEvent),
    EngineEvent(EngineEvent),

    // ── Errors (typed, never silent) ────────────────────────────────────
    ErrorMissingCapability(TypedError),
    ErrorMissingTool(TypedError),
    ErrorInvalidSchema(TypedError),
    ErrorProviderUnavailable(TypedError),
    ErrorPolicyDenied(TypedError),
    ErrorPersistenceFailed(TypedError),
    SidecarError(SidecarErrorEvent),
    BackpressureDropped(BackpressureDroppedEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPolicyChecked {
    pub span_id: String,
    pub tool_name: String,
    pub outcome: ToolPolicyOutcome,
    /// `research` | `act` — the mode in force when the check ran.
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDenied {
    pub span_id: String,
    pub tool_name: String,
    /// Stable machine code, e.g. `write_tool_in_research_mode`,
    /// `tool_disabled`, `tool_not_registered`.
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRestored {
    pub checkpoint_id: String,
    pub run_id: Option<String>,
    pub session_id: Option<String>,
    /// Artifacts rewound (e.g. `strategy`, `agent_slot`, `policy`, `focus`).
    pub restored: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRestoreFailed {
    pub checkpoint_id: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusEvent {
    pub scope_kind: String,
    pub scope_id: Option<String>,
    pub path: String,
    /// Content-addressed hash of the focus file at this event.
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationCandidate {
    pub optimization_id: String,
    pub candidate_index: u32,
    pub optimizer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationCandidateMetric {
    pub optimization_id: String,
    pub candidate_index: u32,
    pub metric: String,
    pub value: f64,
    /// `train` | `holdout`.
    pub split: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationCompleted {
    pub optimization_id: String,
    pub selected_candidate_index: Option<u32>,
    pub minted_agent_id: Option<String>,
}

/// A typed, never-silent error. Every short-circuit class in the plan maps to
/// one of the `Error*` payloads carrying this, so the UI can render
/// remediation and the CLI can return a distinct exit code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedError {
    /// Stable machine code (e.g. `missing_capability_optimizer`).
    pub code: String,
    /// Human-readable summary.
    pub message: String,
    /// Operator remediation hint, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
}

impl UnifiedEvent {
    /// SSE `event:` name for this payload — the snake_case discriminant of
    /// [`UnifiedPayload`]. Kept in lockstep with the serde tag so the
    /// frontend subscribes to the same name it reads inside the JSON.
    pub fn event_name(&self) -> &'static str {
        payload_event_name(&self.payload)
    }

    /// Lifecycle-critical events the bus must never silently drop.
    pub fn is_lifecycle_critical(&self) -> bool {
        matches!(
            self.payload,
            UnifiedPayload::RunStarted(_)
                | UnifiedPayload::RunFinished(_)
                | UnifiedPayload::RunInterrupted(_)
                | UnifiedPayload::SessionFailed { .. }
                | UnifiedPayload::SidecarError(_)
                | UnifiedPayload::ErrorPersistenceFailed(_)
        )
    }

    /// True for terminal events that close a stream.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.payload,
            UnifiedPayload::RunFinished(_)
                | UnifiedPayload::RunInterrupted(_)
                | UnifiedPayload::SessionCompleted
                | UnifiedPayload::SessionFailed { .. }
        )
    }
}

fn payload_event_name(p: &UnifiedPayload) -> &'static str {
    use UnifiedPayload::*;
    match p {
        SessionCreated { .. } => "session_created",
        SessionResumed { .. } => "session_resumed",
        SessionInterrupted { .. } => "session_interrupted",
        SessionCompleted => "session_completed",
        SessionFailed { .. } => "session_failed",
        RunStarted(_) => "run_started",
        RunFinished(_) => "run_finished",
        RunInterrupted(_) => "run_interrupted",
        SpanStarted(_) => "span_started",
        SpanFinished(_) => "span_finished",
        ModelCallFinished(_) => "model_call_finished",
        AssistantMessageStarted => "assistant_message_started",
        AssistantTokenDelta { .. } => "assistant_token_delta",
        AssistantContentBlock { .. } => "assistant_content_block",
        AssistantMessageDone { .. } => "assistant_message_done",
        ToolRequested(_) => "tool_requested",
        ToolPolicyChecked(_) => "tool_policy_checked",
        ToolApproved { .. } => "tool_approved",
        ToolStarted { .. } => "tool_started",
        ToolDelta { .. } => "tool_delta",
        ToolFinished(_) => "tool_finished",
        ToolFailed(_) => "tool_failed",
        ToolCancelled(_) => "tool_cancelled",
        ToolDenied(_) => "tool_denied",
        BrokerCallStarted(_) => "broker_call_started",
        BrokerCallFinished(_) => "broker_call_finished",
        CheckpointCreated(_) => "checkpoint_created",
        CheckpointRestored(_) => "checkpoint_restored",
        CheckpointRestoreFailed(_) => "checkpoint_restore_failed",
        FocusLoaded(_) => "focus_loaded",
        FocusEdited(_) => "focus_edited",
        FocusInjected(_) => "focus_injected",
        OptimizationCandidateStarted(_) => "optimization_candidate_started",
        OptimizationCandidateMetric(_) => "optimization_candidate_metric",
        OptimizationCandidateSelected(_) => "optimization_candidate_selected",
        OptimizationCompleted(_) => "optimization_completed",
        MemoryRecall(_) => "memory_recall",
        MemoryWrite(_) => "memory_write",
        ArtifactWritten(_) => "artifact_written",
        SupervisorNote(_) => "supervisor_note",
        EngineEvent(_) => "engine_event",
        ErrorMissingCapability(_) => "error_missing_capability",
        ErrorMissingTool(_) => "error_missing_tool",
        ErrorInvalidSchema(_) => "error_invalid_schema",
        ErrorProviderUnavailable(_) => "error_provider_unavailable",
        ErrorPolicyDenied(_) => "error_policy_denied",
        ErrorPersistenceFailed(_) => "error_persistence_failed",
        SidecarError(_) => "sidecar_error",
        BackpressureDropped(_) => "backpressure_dropped",
    }
}

/// Projects `RunEvent`s emitted during one agent run into [`UnifiedEvent`]s,
/// assigning a monotonic per-session sequence number and stamping the owning
/// session/scope. One projector instance per (session, run) so `seq` is
/// stable and gap-detectable on the consumer.
///
/// `next_event_id` is injected so callers control id generation (ULID in
/// production, deterministic in tests).
pub struct RunEventProjector {
    session_id: Option<String>,
    run_id: String,
    scope: EventScope,
    seq: u64,
}

impl RunEventProjector {
    pub fn new(session_id: Option<String>, run_id: impl Into<String>, scope: EventScope) -> Self {
        Self {
            session_id,
            run_id: run_id.into(),
            scope,
            seq: 0,
        }
    }

    /// Current sequence cursor (the seq the next projected event will use).
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Project one `RunEvent` into a `UnifiedEvent`, advancing `seq`.
    pub fn project(&mut self, event_id: impl Into<String>, ev: RunEvent, ts: DateTime<Utc>) -> UnifiedEvent {
        let span_id = ev.span_id().map(|s| s.to_string());
        let (actor, payload) = run_event_to_payload(ev);
        let out = UnifiedEvent {
            event_id: event_id.into(),
            session_id: self.session_id.clone(),
            run_id: Some(self.run_id.clone()),
            span_id,
            parent_event_id: None,
            seq: self.seq,
            ts,
            scope: self.scope.clone(),
            actor,
            source: EventSource::AgentRun,
            blob_hash: None,
            payload,
        };
        self.seq += 1;
        out
    }
}

/// Map a `RunEvent` to its unified payload + the actor that produced it.
fn run_event_to_payload(ev: RunEvent) -> (Actor, UnifiedPayload) {
    match ev {
        RunEvent::RunStarted(e) => (Actor::System, UnifiedPayload::RunStarted(e)),
        RunEvent::RunFinished(e) => (Actor::System, UnifiedPayload::RunFinished(e)),
        RunEvent::RunInterrupted(e) => (Actor::System, UnifiedPayload::RunInterrupted(e)),
        RunEvent::SpanStarted(e) => (Actor::Agent, UnifiedPayload::SpanStarted(e)),
        RunEvent::SpanFinished(e) => (Actor::Agent, UnifiedPayload::SpanFinished(e)),
        RunEvent::ModelCallFinished(e) => (Actor::Agent, UnifiedPayload::ModelCallFinished(e)),
        RunEvent::ToolCallStarted(e) => (Actor::Agent, UnifiedPayload::ToolRequested(e)),
        RunEvent::ToolCallFinished(e) => (Actor::Agent, UnifiedPayload::ToolFinished(e)),
        RunEvent::ToolCallFailed(e) => (Actor::Agent, UnifiedPayload::ToolFailed(e)),
        RunEvent::ToolCallCancelled(e) => (Actor::Agent, UnifiedPayload::ToolCancelled(e)),
        RunEvent::BrokerCallStarted(e) => (Actor::System, UnifiedPayload::BrokerCallStarted(e)),
        RunEvent::BrokerCallFinished(e) => (Actor::System, UnifiedPayload::BrokerCallFinished(e)),
        RunEvent::CheckpointWritten(e) => (Actor::System, UnifiedPayload::CheckpointCreated(e)),
        RunEvent::AssistantTextDelta(e) => (
            Actor::Agent,
            UnifiedPayload::AssistantTokenDelta { text: e.delta_text },
        ),
        RunEvent::SupervisorNote(e) => (Actor::System, UnifiedPayload::SupervisorNote(e)),
        RunEvent::ArtifactWritten(e) => (Actor::Agent, UnifiedPayload::ArtifactWritten(e)),
        RunEvent::SidecarError(e) => (Actor::System, UnifiedPayload::SidecarError(e)),
        RunEvent::BackpressureDropped(e) => (Actor::System, UnifiedPayload::BackpressureDropped(e)),
        RunEvent::MemoryRecall(e) => (Actor::System, UnifiedPayload::MemoryRecall(e)),
        RunEvent::MemoryWrite(e) => (Actor::System, UnifiedPayload::MemoryWrite(e)),
        RunEvent::EngineEvent(e) => (Actor::System, UnifiedPayload::EngineEvent(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{RunFinishedEvent, RunStartedEvent, ToolCallStartedEvent};
    use crate::types::{RiskLevel, RunStatus, SideEffectLevel, ToolOrigin};

    fn ts() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-24T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn sample_envelope(payload: UnifiedPayload) -> UnifiedEvent {
        UnifiedEvent {
            event_id: "ev_1".into(),
            session_id: Some("sess_1".into()),
            run_id: Some("run_1".into()),
            span_id: None,
            parent_event_id: None,
            seq: 7,
            ts: ts(),
            scope: EventScope::new("strategy", Some("strat_abc".into())),
            actor: Actor::Operator,
            source: EventSource::ChatRail,
            blob_hash: None,
            payload,
        }
    }

    #[test]
    fn envelope_round_trips_through_json() {
        let ev = sample_envelope(UnifiedPayload::AssistantTokenDelta { text: "hello".into() });
        let json = serde_json::to_string(&ev).unwrap();
        let back: UnifiedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.event_id, "ev_1");
        assert_eq!(back.seq, 7);
        assert_eq!(back.scope.kind, "strategy");
        assert!(matches!(back.payload, UnifiedPayload::AssistantTokenDelta { .. }));
    }

    #[test]
    fn payload_kind_tag_matches_event_name() {
        // The serde `kind` tag and event_name() must never drift.
        let ev = sample_envelope(UnifiedPayload::ToolDenied(ToolDenied {
            span_id: "sp".into(),
            tool_name: "create_strategy".into(),
            code: "write_tool_in_research_mode".into(),
            message: "denied".into(),
        }));
        let v: serde_json::Value = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["payload"]["kind"], "tool_denied");
        assert_eq!(ev.event_name(), "tool_denied");
    }

    #[test]
    fn every_payload_has_a_distinct_event_name() {
        // Guards against two variants mapping to the same SSE name.
        use std::collections::HashSet;
        let names = [
            payload_event_name(&UnifiedPayload::SessionCreated {
                scope_label: "x".into(),
            }),
            payload_event_name(&UnifiedPayload::AssistantMessageStarted),
            payload_event_name(&UnifiedPayload::AssistantMessageDone { draft_id: None }),
            payload_event_name(&UnifiedPayload::ToolApproved {
                span_id: "s".into(),
                approver: "op".into(),
            }),
            payload_event_name(&UnifiedPayload::FocusLoaded(FocusEvent {
                scope_kind: "strategy".into(),
                scope_id: None,
                path: "f".into(),
                content_hash: None,
            })),
            payload_event_name(&UnifiedPayload::ErrorMissingTool(TypedError {
                code: "c".into(),
                message: "m".into(),
                remediation: None,
            })),
        ];
        let set: HashSet<_> = names.iter().collect();
        assert_eq!(set.len(), names.len(), "duplicate event names: {names:?}");
    }

    #[test]
    fn projector_assigns_monotonic_seq_and_run_id() {
        let mut proj = RunEventProjector::new(
            Some("sess_9".into()),
            "run_9",
            EventScope::new("run", Some("run_9".into())),
        );
        let started = RunEvent::RunStarted(RunStartedEvent {
            run_id: "run_9".into(),
            objective: "obj".into(),
            strategy_id: None,
            eval_run_id: None,
            source_cli_job_id: None,
            started_at: ts(),
            retention_mode: "summary".into(),
            trajectory_mode: None,
            sidecar_version: None,
            cline_sdk_version: None,
            protocol_version: None,
            skills_json: None,
            mcp_servers_json: None,
        });
        let tool = RunEvent::ToolCallStarted(ToolCallStartedEvent {
            span_id: "sp_1".into(),
            tool_name: "create_strategy".into(),
            origin: ToolOrigin::Mcp("xvn".into()),
            tool_version: None,
            tool_hash: None,
            side_effect_level: SideEffectLevel::ExternalWrite,
            risk_level: RiskLevel::StrategyMutation,
            requires_approval: true,
            is_run_terminator: false,
            input_hash: "h".into(),
            input_payload_ref: None,
            input_text: None,
        });
        let finished = RunEvent::RunFinished(RunFinishedEvent {
            run_id: "run_9".into(),
            finished_at: ts(),
            status: RunStatus::Completed,
            final_artifact_id: None,
            error: None,
        });

        let e0 = proj.project("ev0", started, ts());
        let e1 = proj.project("ev1", tool, ts());
        let e2 = proj.project("ev2", finished, ts());

        assert_eq!((e0.seq, e1.seq, e2.seq), (0, 1, 2));
        assert_eq!(e0.run_id.as_deref(), Some("run_9"));
        assert_eq!(e0.source, EventSource::AgentRun);
        // span-scoped event keeps its span id on the envelope
        assert_eq!(e1.span_id.as_deref(), Some("sp_1"));
        assert!(matches!(e1.payload, UnifiedPayload::ToolRequested(_)));
        assert!(e2.is_terminal());
        assert!(e0.is_lifecycle_critical());
    }

    #[test]
    fn checkpoint_variant_round_trips_despite_inner_kind_field() {
        // CheckpointWrittenEvent has its OWN `kind` field (model_step|tool_step)
        // which collides with the envelope's serde(tag="kind"). This test proves
        // whether the inner kind survives a round-trip.
        let ckpt = crate::events::CheckpointWrittenEvent {
            checkpoint_id: "ck1".into(),
            run_id: "run1".into(),
            span_id: "sp1".into(),
            sequence: 3,
            kind: "model_step".into(),
            input_hash: "ih".into(),
            output_hash: None,
            input_payload_ref: None,
            output_payload_ref: None,
        };
        let ev = sample_envelope(UnifiedPayload::CheckpointCreated(ckpt));
        let json = serde_json::to_string(&ev).unwrap();
        let back: UnifiedEvent = serde_json::from_str(&json).unwrap();
        match back.payload {
            UnifiedPayload::CheckpointCreated(c) => {
                assert_eq!(c.kind, "model_step", "inner kind corrupted; json was: {json}");
                assert_eq!(c.checkpoint_id, "ck1");
            }
            other => panic!("wrong payload kind after round-trip: {other:?}; json={json}"),
        }
    }

    #[test]
    fn typed_errors_round_trip_with_remediation() {
        let ev = sample_envelope(UnifiedPayload::ErrorMissingCapability(TypedError {
            code: "missing_capability_optimizer".into(),
            message: "agent has no trader capability".into(),
            remediation: Some("add a trader-capability slot before optimizing".into()),
        }));
        let json = serde_json::to_string(&ev).unwrap();
        let back: UnifiedEvent = serde_json::from_str(&json).unwrap();
        match back.payload {
            UnifiedPayload::ErrorMissingCapability(e) => {
                assert_eq!(e.code, "missing_capability_optimizer");
                assert!(e.remediation.is_some());
            }
            other => panic!("wrong payload: {other:?}"),
        }
    }
}
