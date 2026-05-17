//! Agent run observability — canonical Rust ledger.
//!
//! See `docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md`
//! for the architecture and the IPC emission boundary that this crate is
//! designed to feed once Phase B lands.
//!
//! Phase A scope: row types, redactor, blob store, config loader, event
//! bus + recorder trait + SqliteRecorder + NoopRecorder, retention CLI +
//! janitor.

pub mod blobs;
pub mod bus;
pub mod config;
pub mod events;
pub mod janitor;
#[cfg(feature = "otel")]
pub mod otel;
pub mod recorder;
pub mod redactor;
pub mod retention;
pub mod rows;
pub mod sqlite;
pub mod types;

pub use blobs::{BlobRef, BlobStore, BlobStoreError};
pub use bus::RunEventBus;
pub use config::{
    default_config_path, ObservabilityConfig, RetentionConfig, RetentionMode,
    CONFIG_FILE_NAME, ENV_OVERRIDE_PREFIX,
};
pub use events::{
    ArtifactWrittenEvent, AssistantTextDeltaEvent, BackpressureDroppedEvent,
    CheckpointWrittenEvent, ModelCallFinishedEvent, RunEvent, RunFinishedEvent,
    RunInterruptedEvent, RunStartedEvent, SidecarErrorEvent, SpanFinishedEvent,
    SpanStartedEvent, SupervisorNoteEvent, ToolCallCancelledEvent,
    ToolCallFailedEvent, ToolCallFinishedEvent, ToolCallStartedEvent,
};
pub use janitor::{
    expire_old_payload_refs, run_once as run_janitor_once, spawn_periodic as spawn_janitor,
    truncate_to_max_bytes, JanitorConfig, JanitorError, JanitorStats,
};
pub use recorder::{AgentRunRecorder, Attribute, NoopRecorder, RecorderError};
pub use redactor::{Redactor, RedactionMatch};
pub use retention::{
    clear_config, full_debug_sentinel_path, resolve as resolve_retention, write_config,
    CliOverrides, Resolved, ResolvedView, RetentionError, Source,
};
pub use rows::{
    AgentRunRow, ApprovalRow, ArtifactRow, CheckpointRow, EventRow, ModelCallRow,
    SandboxResultRow, SpanRow, SupervisorNoteRow, ToolCallRow,
};
pub use sqlite::SqliteRecorder;
pub use types::{
    CapabilityPath, RiskLevel, RunStatus, SideEffectLevel, SpanKind, SpanStatus,
    ToolOrigin,
};

#[cfg(feature = "otel")]
pub use otel::{
    add_attribute as otel_add_attribute, attr as otel_attr, attribute_to_kv,
    build_resource as otel_build_resource, init_otel_pipeline, shutdown_otel_pipeline,
    OtelIds, OtelInitError, OtelTeeRecorder, ENV_OTLP_ENDPOINT,
    ENV_RESOURCE_ATTRIBUTES, ENV_SERVICE_NAME,
};
