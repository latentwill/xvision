//! Agent records — workspace-level reusable templates that compose into
//! strategies. Agents are stored in SQLite (see migration `005_agents.sql`);
//! the API layer at `crates/xvision-engine/src/api/agents.rs` wraps the
//! store in audit-emitting handlers.
//!
//! See: `docs/superpowers/plans/2026-05-11-agents-page-v1.md`
//!
//! Distinct from `xvision_engine::agent` (singular) which handles agent
//! *execution* during a cycle. This module handles agent *records*.

pub mod model;
pub mod store;
pub mod templates;
pub mod validate;
pub mod validator;

#[cfg(test)]
mod max_tokens_resolution;

pub use model::{Agent, AgentSlot};
// Canonical per-model metadata table lives in `xvision-core::providers`
// so non-engine crates (CLI, dashboard) can resolve auto-tokens without
// linking the engine. The engine re-exports the names it consumes.
pub use xvision_core::providers::{lookup_model, ModelClass, ModelMetadata};

pub use store::{AgentStore, ListFilter, NewAgent, UpdateAgent};
pub use templates::{builtin_templates, AgentTemplate};
pub use validate::{validate_agent, Severity, ValidationDiagnostic};
pub use validator::{
    lint_agents, validate_prompt_schema, validate_prompt_schema_slots, LintFinding, PromptSchemaDriftError,
    ACTION_SCHEMA_ENUM,
};
