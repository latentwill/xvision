//! Agent run observability — canonical Rust ledger.
//!
//! See `docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md`
//! for the architecture and the IPC emission boundary that this crate is
//! designed to feed once Phase B lands.
//!
//! Phase A scope: row types, redactor, blob store, config loader, event
//! bus + recorder trait + SqliteRecorder + NoopRecorder.

pub mod blobs;
pub mod bus;
pub mod config;
pub mod events;
pub mod recorder;
pub mod redactor;
pub mod rows;
pub mod sqlite;
pub mod types;

pub use blobs::{BlobRef, BlobStore, BlobStoreError};
pub use bus::RunEventBus;
pub use config::{
    ObservabilityConfig, RetentionConfig, RetentionMode, CONFIG_FILE_NAME,
    ENV_OVERRIDE_PREFIX,
};
pub use events::{
    ArtifactWrittenEvent, AssistantTextDeltaEvent, BackpressureDroppedEvent,
    CheckpointWrittenEvent, ModelCallFinishedEvent, RunEvent, RunFinishedEvent,
    RunInterruptedEvent, RunStartedEvent, SidecarErrorEvent, SpanFinishedEvent,
    SpanStartedEvent, SupervisorNoteEvent, ToolCallCancelledEvent,
    ToolCallFailedEvent, ToolCallFinishedEvent, ToolCallStartedEvent,
};
pub use recorder::{AgentRunRecorder, Attribute, NoopRecorder, RecorderError};
pub use redactor::{Redactor, RedactionMatch};
pub use rows::{
    AgentRunRow, ApprovalRow, ArtifactRow, CheckpointRow, EventRow, ModelCallRow,
    SandboxResultRow, SpanRow, SupervisorNoteRow, ToolCallRow,
};
pub use sqlite::SqliteRecorder;
pub use types::{
    CapabilityPath, RiskLevel, RunStatus, SideEffectLevel, SpanKind, SpanStatus,
    ToolOrigin,
};
