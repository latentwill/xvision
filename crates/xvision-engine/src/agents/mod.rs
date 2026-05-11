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

pub use model::{Agent, AgentSlot};
