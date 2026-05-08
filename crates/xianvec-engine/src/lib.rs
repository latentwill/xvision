//! xianvec-engine — strategy creation, bundling, agent execution.
//!
//! See: docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md

pub mod agent;
pub mod baselines;
pub mod bundle;
pub mod error;
pub mod templates;
pub mod tokens;
pub mod tools;

pub use bundle::StrategyBundle;
pub use error::EngineError;
