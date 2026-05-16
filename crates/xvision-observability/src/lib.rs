//! Agent run observability — canonical Rust ledger.
//!
//! See `docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md`
//! for the architecture and the IPC emission boundary that this crate is
//! designed to feed once Phase B lands.
//!
//! Phase A scope (this leaf): row types, redactor, blob store, config
//! loader. **No event bus, no recorder, no emission** — those are added by
//! `agent-run-observability-event-bus` next.

pub mod blobs;
pub mod config;
pub mod janitor;
pub mod redactor;
pub mod retention;
pub mod rows;
pub mod types;

pub use blobs::{BlobRef, BlobStore, BlobStoreError};
pub use config::{
    default_config_path, ObservabilityConfig, RetentionConfig, RetentionMode,
    CONFIG_FILE_NAME, ENV_OVERRIDE_PREFIX,
};
pub use janitor::{
    expire_old_payload_refs, run_once as run_janitor_once, spawn_periodic as spawn_janitor,
    truncate_to_max_bytes, JanitorConfig, JanitorError, JanitorStats,
};
pub use redactor::{Redactor, RedactionMatch};
pub use retention::{
    clear_config, full_debug_sentinel_path, resolve as resolve_retention, write_config,
    CliOverrides, Resolved, ResolvedView, RetentionError, Source,
};
pub use rows::{
    AgentRunRow, ApprovalRow, ArtifactRow, CheckpointRow, EventRow, ModelCallRow,
    SandboxResultRow, SpanRow, SupervisorNoteRow, ToolCallRow,
};
pub use types::{
    CapabilityPath, RiskLevel, RunStatus, SideEffectLevel, SpanKind, SpanStatus,
    ToolOrigin,
};
