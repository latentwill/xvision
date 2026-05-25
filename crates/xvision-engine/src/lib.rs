//! xvision-engine — strategy creation, bundling, agent execution.
//!
//! See: docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md

pub mod agent;
pub mod agents;
pub mod api;
pub mod authoring;
pub mod baselines;
pub mod chat_session;
pub mod checkpoint;
pub mod diagnostics;
pub mod error;
pub mod eval;
pub mod focus;
pub mod guardrails;
pub mod optimization;
pub mod providers;
pub mod safety;
pub mod search;
pub mod skills;
pub mod strategies;
pub mod strategies_folder;
pub mod tokens;
pub mod tools;

pub use error::EngineError;
pub use focus::FocusDoc;
pub use strategies::Strategy;

// Phase 4.1 capability-completeness diagnostics: typed readiness statuses
// + launch gate. dspy-free (the optimizable set is a hardcoded mirror).
pub use diagnostics::{
    assert_launchable, capability_diagnostics, diagnose, AgentDiagnostics, CapabilityDiagnostic,
    CapabilityStatus, DiagnosticsError, StrategyDiagnostics, UnmetRequirement,
};

// Phase 4.2 no-short-circuit execution guardrails: pure detectors that map
// a missing prerequisite to a distinct code + remediation + typed-error
// event payload, so a skipped step never reads as a silent success.
pub use guardrails::{ShortCircuit, ShortCircuitReport, SHORT_CIRCUIT_CODES};

// Phase 2.5 chat-rail checkpoints: content-addressed snapshot + verbatim
// restore of a session's mutable authoring artifacts.
pub use checkpoint::{
    CapturedArtifact, Checkpoint, CheckpointError, CheckpointKind, Checkpointer, RestoreOutcome,
};

// Re-export strategy risk types so consumers don't have to depend on
// xvision-core directly just to construct a Strategy.
pub use xvision_core::{Capital, RiskCaps};
