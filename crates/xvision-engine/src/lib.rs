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
pub mod error;
pub mod eval;
pub mod focus;
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

// Phase 2.5 chat-rail checkpoints: content-addressed snapshot + verbatim
// restore of a session's mutable authoring artifacts.
pub use checkpoint::{
    CapturedArtifact, Checkpoint, CheckpointError, CheckpointKind, Checkpointer, RestoreOutcome,
};

// Re-export strategy risk types so consumers don't have to depend on
// xvision-core directly just to construct a Strategy.
pub use xvision_core::{Capital, RiskCaps};
