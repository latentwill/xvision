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
pub mod bus_subscriber;
pub mod config;
pub mod eval_recorder;
pub mod events;
pub mod export;
pub mod harness_recorder;
pub mod janitor;
#[cfg(feature = "otel")]
pub mod otel;
pub mod recorder;
pub mod redactor;
pub mod retention;
pub mod rows;
pub mod sqlite;
pub mod trajectory;
pub mod types;
pub mod unified_event;

pub use blobs::{BlobRef, BlobStore, BlobStoreError};
pub use bus::RunEventBus;
pub use bus_subscriber::{BroadcastSubscriber, SharedBroadcastSubscriber, RUN_CHANNEL_CAPACITY};
pub use config::{
    default_config_path, ObservabilityConfig, RetentionConfig, RetentionMode, CONFIG_FILE_NAME,
    ENV_OVERRIDE_PREFIX,
};
pub use eval_recorder::{EvalRecorder, TraceBuf, TraceBufCounts};
pub use events::{
    ArtifactWrittenEvent, AssistantTextDeltaEvent, BackpressureDroppedEvent, BrokerCallFinishedEvent,
    BrokerCallOutcome, BrokerCallStartedEvent, BrokerSide, CheckpointWrittenEvent, EngineEvent,
    MemoryRecallEvent, MemoryRecallItem, MemoryWriteEvent, ModelCallFinishedEvent, RunEvent,
    RunFinishedEvent, RunInterruptedEvent, RunStartedEvent, SidecarErrorEvent, SpanFinishedEvent,
    SpanStartedEvent, SupervisorNoteEvent, ToolCallCancelledEvent, ToolCallFailedEvent,
    ToolCallFinishedEvent, ToolCallStartedEvent,
};
pub use export::{
    build_export, build_export_with_blobs, build_report, find_blob_owner, render_report, AgentRunExport,
    AgentRunReport, ExportError, ExportEvent, ExportTotals, FinalArtifact, SpanNode, SCHEMA_VERSION,
};
pub use harness_recorder::HarnessRecorder;
pub use janitor::{
    expire_old_payload_refs, gc_orphaned_blobs, run_once as run_janitor_once,
    spawn_periodic as spawn_janitor, truncate_to_max_bytes, GcReport, JanitorConfig, JanitorError,
    JanitorStats, GC_MIN_AGE_SECS,
};
pub use recorder::{
    AgentEvent, AgentRunRecorder, Attribute, CountingRecorder, NoopRecorder, NullRecorder, Recorder,
    RecorderCounts, RecorderError,
};
pub use redactor::{RedactionMatch, Redactor};
pub use retention::{
    clear_config, full_debug_sentinel_path, resolve as resolve_retention, write_config, CliOverrides,
    Resolved, ResolvedView, RetentionError, Source,
};
pub use rows::{
    AgentRunRow, ApprovalRow, ArtifactRow, CheckpointRow, EventRow, ModelCallRow, SandboxResultRow, SpanRow,
    SupervisorNoteRow, ToolCallRow,
};
pub use sqlite::SqliteRecorder;
pub use types::{
    CapabilityPath, RiskLevel, RunStatus, SideEffectLevel, SpanAttributes, SpanKind, SpanStatus, ToolOrigin,
};
pub use unified_event::{
    Actor, CheckpointRestoreFailed, CheckpointRestored, EventScope, EventSource, FocusEvent,
    OptimizationCandidate, OptimizationCandidateMetric, OptimizationCompleted, RunEventProjector, ToolDenied,
    ToolPolicyChecked, ToolPolicyOutcome, TypedError, UnifiedEvent, UnifiedPayload,
};

#[cfg(feature = "otel")]
pub use otel::{
    add_attribute as otel_add_attribute, attr as otel_attr, attribute_to_kv,
    build_resource as otel_build_resource, init_otel_pipeline, shutdown_otel_pipeline, OtelIds,
    OtelInitError, OtelTeeRecorder, ENV_OTLP_ENDPOINT, ENV_RESOURCE_ATTRIBUTES, ENV_SERVICE_NAME,
};
