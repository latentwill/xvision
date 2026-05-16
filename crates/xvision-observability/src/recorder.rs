//! `AgentRunRecorder` — bus-subscribed sink for `RunEvent`s.
//!
//! Implementations: `SqliteRecorder` (canonical, writes the rows in
//! migration 018), `NoopRecorder` (tests / off-mode). OTel will arrive as
//! `OtelTeeRecorder` in the Phase B `agent-run-observability-otel-bridge`
//! leaf and will subscribe to the same bus.
//!
//! **Attribute API guardrail:** the recorder trait deliberately does NOT
//! accept raw payload strings as attributes. Hashes, counts, ids — never
//! the full prompt. The events module enforces this at the type level by
//! constructing `*_hash` columns; this trait is the consumer.

use crate::events::RunEvent;
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
