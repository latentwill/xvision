//! Nanochat filter agent subsystem — trained model management, autoresearch
//! run/experiment tracking, validation primitives, and dispatch.
//!
//! Developer surface name: `nanochat` / `NanochatStore` / `autoresearch_*`.
//! Operator surface name: "Nanochat model" / "Autoresearcher" (tab on the
//! Optimizer page). See the terminology lock in
//! `docs/superpowers/specs/2026-06-13-nanochat-filter-agent.md`.

pub mod store;
pub mod validate;
