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
pub mod validate;

pub use model::{Agent, AgentSlot};
pub use store::{AgentStore, ListFilter, NewAgent, UpdateAgent};
pub use validate::{validate_agent, Severity, ValidationDiagnostic};
