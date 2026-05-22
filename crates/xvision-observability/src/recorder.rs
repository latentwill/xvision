//! Two recorder traits live in this module:
//!
//! 1. [`AgentRunRecorder`] — the bus-subscribed sink for `RunEvent`s.
//!    `SqliteRecorder` (canonical, writes the rows in migration 018) and
//!    `NoopRecorder` implement it. Used by the harness and the sidecar
//!    path to consume bus events asynchronously.
//!
//! 2. [`Recorder`] (Phase D) — a synchronous, capability-handler-facing
//!    sink with 7 row-typed methods, threaded through
//!    `dispatch_capability` as `&dyn Recorder`. `HarnessRecorder` wraps
//!    the existing `ObsEmitter` + DB write path; `EvalRecorder` mirrors
//!    each row into both an in-memory trace buffer AND the `xvn.db`
//!    recorder tables. This closes F-11(f) (eval-driven runs producing
//!    empty recorder tables) structurally — both surfaces share one
//!    dispatch entry point and one trait shape.
//!
//! **Attribute API guardrail:** the recorder trait deliberately does NOT
//! accept raw payload strings as attributes. Hashes, counts, ids — never
//! the full prompt. The events module enforces this at the type level by
//! constructing `*_hash` columns; this trait is the consumer.

use crate::events::{EngineEvent, RunEvent};
use crate::rows::{
    ApprovalRow, ArtifactRow, CheckpointRow, SandboxResultRow, SupervisorNoteRow, ToolCallRow,
};
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RecorderError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] sqlx::Error),
    #[error("recorder: {0}")]
    Internal(String),
}

/// Attribute values that the recorder is allowed to attach to a span /
/// row / OTel attribute. The enum has **no** `From<&str>` /
/// `From<String>` impl, by design — recorder attribute APIs only accept
/// hashes, ids, counts, and flags. A careless
/// `attribute.set("prompt", &prompt)` cannot leak to a remote OTel
/// collector because the only way to construct an `Attribute` carrying
/// text is via [`Attribute::hash`] or [`Attribute::id`], both of which
/// signal that the caller has already replaced the payload with a
/// fixed-shape token.
///
/// This is the Phase A guardrail. The Phase B `OtelTeeRecorder`
/// (`agent-run-observability-otel-bridge` leaf) takes `Attribute`, not
/// `&str`, so the constraint is load-bearing once the OTel bridge ships.
///
/// ```compile_fail
/// use xvision_observability::Attribute;
/// // A raw payload string is not a valid attribute — only hashes/ids/counts.
/// // This must NOT compile: no `From<&str>` impl exists, by design.
/// let _: Attribute = "sk-very-secret-prompt-text".into();
/// ```
///
/// ```compile_fail
/// use xvision_observability::Attribute;
/// // Same constraint via the `From<String>` path — also not implemented.
/// let payload = String::from("a giant prompt body");
/// let _: Attribute = payload.into();
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribute {
    /// A content hash (sha256 hex, opaque blob ref, etc.).
    Hash(String),
    /// An identifier — run id, span id, tool name. Bounded cardinality
    /// is the caller's responsibility; the type does not enforce it.
    Id(String),
    /// A numeric measurement — token count, byte size, exit code.
    Count(i64),
    /// A boolean flag — requires_approval, is_run_terminator, etc.
    Flag(bool),
}

impl Attribute {
    pub fn hash(value: impl Into<String>) -> Self {
        Self::Hash(value.into())
    }

    pub fn id(value: impl Into<String>) -> Self {
        Self::Id(value.into())
    }

    pub fn count(value: i64) -> Self {
        Self::Count(value)
    }

    pub fn flag(value: bool) -> Self {
        Self::Flag(value)
    }
}

#[async_trait]
pub trait AgentRunRecorder: Send + Sync {
    /// Called by the bus consumer for each `RunEvent`. Implementations
    /// MUST be tolerant of out-of-order arrivals within reason
    /// (e.g. `SpanFinished` arriving before `SpanStarted` should be
    /// surfaced as an internal error, not panic).
    async fn handle_event(&self, event: &RunEvent) -> Result<(), RecorderError>;

    /// Called by the supervisor when the sidecar gives up — recorder
    /// marks every still-open span on this run as `interrupted`. The
    /// bus also delivers a `RunInterrupted` event after this hook, so
    /// implementations should leave the run record open here and update
    /// it on the event.
    async fn mark_interrupted(&self, run_id: &str) -> Result<(), RecorderError>;
}

/// Off-mode / test recorder. Records every event into an in-memory buffer
/// so tests can assert what the bus delivered without touching SQLite.
#[derive(Debug, Default)]
pub struct NoopRecorder {
    pub events: tokio::sync::Mutex<Vec<RunEvent>>,
}

impl NoopRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn snapshot(&self) -> Vec<RunEvent> {
        self.events.lock().await.clone()
    }
}

#[async_trait]
impl AgentRunRecorder for NoopRecorder {
    async fn handle_event(&self, event: &RunEvent) -> Result<(), RecorderError> {
        self.events.lock().await.push(event.clone());
        Ok(())
    }

    async fn mark_interrupted(&self, _run_id: &str) -> Result<(), RecorderError> {
        Ok(())
    }
}

// =========================================================================
// Phase D — unified `Recorder` trait + row-typed sink for the capability
// dispatcher.
// =========================================================================

/// Row-typed alias for an engine-level event. Distinct from
/// [`RunEvent`] (which is the broader event-bus envelope) — this is the
/// shape the Phase D `Recorder` trait carries through
/// `record_event`, and it round-trips through the `events` recorder
/// table via [`EventRow`].
pub type AgentEvent = EngineEvent;

/// Phase D unified recorder. Both the harness path
/// (`HarnessRecorder`) and the eval-executor path (`EvalRecorder`)
/// implement this trait; the capability dispatcher
/// (`xvision_engine::agent::dispatch_capability::dispatch_capability`)
/// takes `&dyn Recorder` and stays oblivious to which surface it's
/// running on.
///
/// All methods take `&self` (not `&mut self`) — the SQLite write path
/// is internally synchronized via the post-#522 `busy_timeout`. Phase C
/// can evaluate filters concurrently inside the dispatcher, so a `&mut`
/// receiver would require external locking the dispatcher doesn't own.
///
/// The 7 methods correspond one-to-one with the 7 recorder tables added
/// by migrations 018 / 020 / 022:
/// `tool_calls`, `events`, `supervisor_notes`, `approvals`,
/// `sandbox_results`, `checkpoints`, `artifacts`.
///
/// **F-11(f) closure:** pre-Phase-D, the harness path wrote rows to all
/// 7 tables but the eval-executor path wrote to none — operators saw
/// empty recorder tables on every eval run. After Phase D, both paths
/// share `dispatch_capability` and `&dyn Recorder`, so any new emission
/// is automatically symmetric.
pub trait Recorder: Send + Sync {
    /// Record a single tool-call row. Producer is responsible for
    /// having populated `span_id` (correlates to a `tool.call` span).
    fn record_tool_call(&self, call: ToolCallRow);

    /// Record an engine-level event scoped to a run (and optionally a
    /// span). Producer-defined `kind` string; redacted `payload_json`
    /// when present.
    fn record_event(&self, event: AgentEvent);

    /// Record a supervisor / guard / risk note (free-form `content`
    /// with a `severity` triage).
    fn record_supervisor_note(&self, note: SupervisorNoteRow);

    /// Record an approval row (request open, decision close, or both —
    /// the row carries the lifecycle).
    fn record_approval(&self, approval: ApprovalRow);

    /// Record a sandbox-exec result (command + stdout/stderr blob refs
    /// + exit code).
    fn record_sandbox_result(&self, result: SandboxResultRow);

    /// Record a checkpoint row (deterministic input/output hashes per
    /// span — used by replay).
    fn record_checkpoint(&self, checkpoint: CheckpointRow);

    /// Record an artifact row (final analyst-style output, plus the
    /// structured next-experiments payload).
    fn record_artifact(&self, artifact: ArtifactRow);
}

/// Test-only `Recorder` that counts every method call. Useful as the
/// `&dyn Recorder` argument in the Phase D `recorder_trait_basics`
/// dispatch tests — assertions check call counts per capability
/// invocation without spinning up SQLite.
#[derive(Debug, Default)]
pub struct CountingRecorder {
    pub tool_calls: std::sync::atomic::AtomicUsize,
    pub events: std::sync::atomic::AtomicUsize,
    pub supervisor_notes: std::sync::atomic::AtomicUsize,
    pub approvals: std::sync::atomic::AtomicUsize,
    pub sandbox_results: std::sync::atomic::AtomicUsize,
    pub checkpoints: std::sync::atomic::AtomicUsize,
    pub artifacts: std::sync::atomic::AtomicUsize,
}

impl CountingRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> RecorderCounts {
        use std::sync::atomic::Ordering::Relaxed;
        RecorderCounts {
            tool_calls: self.tool_calls.load(Relaxed),
            events: self.events.load(Relaxed),
            supervisor_notes: self.supervisor_notes.load(Relaxed),
            approvals: self.approvals.load(Relaxed),
            sandbox_results: self.sandbox_results.load(Relaxed),
            checkpoints: self.checkpoints.load(Relaxed),
            artifacts: self.artifacts.load(Relaxed),
        }
    }
}

/// Snapshot of [`CountingRecorder`]'s atomic counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecorderCounts {
    pub tool_calls: usize,
    pub events: usize,
    pub supervisor_notes: usize,
    pub approvals: usize,
    pub sandbox_results: usize,
    pub checkpoints: usize,
    pub artifacts: usize,
}

impl Recorder for CountingRecorder {
    fn record_tool_call(&self, _call: ToolCallRow) {
        self.tool_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    fn record_event(&self, _event: AgentEvent) {
        self.events.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    fn record_supervisor_note(&self, _note: SupervisorNoteRow) {
        self.supervisor_notes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    fn record_approval(&self, _approval: ApprovalRow) {
        self.approvals.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    fn record_sandbox_result(&self, _result: SandboxResultRow) {
        self.sandbox_results
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    fn record_checkpoint(&self, _checkpoint: CheckpointRow) {
        self.checkpoints
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    fn record_artifact(&self, _artifact: ArtifactRow) {
        self.artifacts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// `Recorder` implementation that drops every call. Useful for code
/// paths that haven't yet been wired to a real recorder (legacy unit
/// tests, dispatch fixtures) — the trait stays threaded through the
/// dispatcher so additions only need to choose a real implementor.
#[derive(Debug, Default)]
pub struct NullRecorder;

impl Recorder for NullRecorder {
    fn record_tool_call(&self, _call: ToolCallRow) {}
    fn record_event(&self, _event: AgentEvent) {}
    fn record_supervisor_note(&self, _note: SupervisorNoteRow) {}
    fn record_approval(&self, _approval: ApprovalRow) {}
    fn record_sandbox_result(&self, _result: SandboxResultRow) {}
    fn record_checkpoint(&self, _checkpoint: CheckpointRow) {}
    fn record_artifact(&self, _artifact: ArtifactRow) {}
}
