//! xvision-engine — strategy creation, bundling, agent execution.
//!
//! See: docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md

pub mod agent;
pub mod agents;
pub mod api;
pub mod authoring;
pub mod baselines;
pub mod chat_session;
pub mod error;
pub mod eval;
pub mod providers;
pub mod search;
pub mod skills;
pub mod strategies;
pub mod templates;
pub mod tokens;
pub mod tools;

pub use error::EngineError;
pub use strategies::Strategy;

// Re-export strategy risk types so consumers don't have to depend on
// xvision-core directly just to construct a Strategy.
pub use xvision_core::{Capital, RiskCaps};
