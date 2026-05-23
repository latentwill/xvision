//! Agent records — workspace-level reusable templates that compose into
//! strategies. Agents are stored in SQLite (see migration `005_agents.sql`);
//! the API layer at `crates/xvision-engine/src/api/agents.rs` wraps the
//! store in audit-emitting handlers.
//!
//! See: `docs/superpowers/plans/2026-05-11-agents-page-v1.md`
//!
//! Distinct from `xvision_engine::agent` (singular) which handles agent
//! *execution* during a cycle. This module handles agent *records*.

pub mod capability;
pub mod model;
pub mod store;
pub mod templates;
pub mod validate;
pub mod validator;

#[cfg(test)]
mod max_tokens_resolution;

pub use capability::Capability;
pub use model::{default_capabilities, Agent, AgentSlot, InputsPolicy};
// Canonical per-model metadata table lives in `xvision-core::providers`
// so non-engine crates (CLI, dashboard) can resolve auto-tokens without
// linking the engine. The engine re-exports the names it consumes.
pub use xvision_core::providers::{lookup_model, ModelClass, ModelMetadata};

pub use store::{AgentStore, ListFilter, NewAgent, ScopeFilter, ScopePatch, UpdateAgent};
pub use templates::{builtin_templates, AgentTemplate};
pub use validate::{
    validate_agent, validate_agent_for_save, AuditFinding, Severity, ValidationDiagnostic,
    DEFAULT_PLACEHOLDER_PROMPT,
};
pub use validator::{
    lint_agents, validate_prompt_schema, validate_prompt_schema_slots, LintFinding, PromptSchemaDriftError,
    ACTION_SCHEMA_ENUM,
};

/// Test-only helper: disable `validate_agent_for_save`'s content-quality
/// gate by setting `XVISION_DISABLE_AGENT_SAVE_GATE=1` in this process's
/// environment. Production callers must NEVER use this; it exists so
/// integration tests that exercise behavior unrelated to prompt quality
/// don't have to fabricate ≥200-char prompts at every fixture site.
///
/// The bypass is read by `validate::validate_agent_for_save`. Tests that
/// specifically pin the save-gate behavior (e.g. `agent_save_validate.rs`)
/// must NOT call this — and because each `tests/*.rs` file runs as its
/// own binary process, env vars don't leak across them.
#[doc(hidden)]
pub fn disable_save_gate_for_tests() {
    std::env::set_var("XVISION_DISABLE_AGENT_SAVE_GATE", "1");
}
