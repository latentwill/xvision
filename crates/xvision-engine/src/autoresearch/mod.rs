//! Autoresearch training orchestration.
//!
//! Developer-surface module. Operator-surface name: "Autoresearcher"
//! (a tab on the Optimizer page). Distinct from the autooptimizer (strategy
//! evolution) — this module trains models.

pub mod experiment;
pub mod promotion;
pub mod run_config;
pub mod training_gate;
pub mod worktree;
