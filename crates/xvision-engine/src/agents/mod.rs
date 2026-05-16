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

#[cfg(test)]
mod max_tokens_resolution;

pub use model::{Agent, AgentSlot};
// Canonical per-model metadata table lives in `xvision-core::providers`
// so non-engine crates (CLI, dashboard) can resolve auto-tokens without
// linking the engine. The engine re-exports the names it consumes.
pub use xvision_core::providers::{lookup_model, ModelClass, ModelMetadata};

/// Resolve an `AgentSlot.max_tokens` to a concrete budget the dispatcher
/// sends to the provider:
///
/// - `Some(n)` with `n > 0`: honored, clamped to `output_token_ceiling`.
/// - `None` or `Some(0)`: auto — `recommended_visible_output +
///   reasoning_token_default`, clamped.
///
/// The sentinel for `0` lets the store layer round-trip `None` ↔
/// `INTEGER NOT NULL DEFAULT 0` without a schema migration.
pub fn resolve_max_tokens(explicit: Option<u32>, meta: &ModelMetadata) -> u32 {
    meta.resolve(explicit)
}

pub use store::{AgentStore, ListFilter, NewAgent, UpdateAgent};
pub use templates::{builtin_templates, AgentTemplate};
pub use validate::{validate_agent, Severity, ValidationDiagnostic};
