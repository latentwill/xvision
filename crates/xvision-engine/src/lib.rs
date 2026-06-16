//! xvision-engine — strategy creation, bundling, agent execution.
//!
//! See: docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md

pub mod agent;
pub mod agents;
pub mod api;
pub mod authoring;
pub mod autooptimizer;
pub mod autoresearch;
pub mod baselines;
pub mod chat_session;
pub mod checkpoint;
pub mod diagnostics;
pub mod error;
pub mod eval;
pub mod focus;
pub mod guardrails;
pub mod mint;
pub mod nanochat;
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

// Tool-readiness diagnostics + launch gate.
pub use diagnostics::{
    assert_launchable, capability_diagnostics, diagnose, AgentDiagnostics, DiagnosticsError,
    StrategyDiagnostics, ToolDiagnostic, UnmetTool,
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

// Phase 4.3/4.4 tune-&-mint discipline: pure accept-gate + marketplace-mint-gate
// + holdout store + per-capability metric registry. dspy-free.
pub use mint::{
    check_accept, check_marketplace_mint, AcceptDecision, AcceptInputs, AcceptRefusal, EvalProof,
    HoldoutResult, HoldoutStore, MintDecision, MintInputs, MintRefusal, NewHoldoutResult, OverfitConfig,
};

// Re-export strategy risk types so consumers don't have to depend on
// xvision-core directly just to construct a Strategy.
pub use xvision_core::{Capital, RiskCaps};
