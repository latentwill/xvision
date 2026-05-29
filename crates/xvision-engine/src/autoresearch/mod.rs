//! Autoresearcher module — implements AR-1's mutator + lineage + numeric
//! gate + CycleSeal substrate per
//! `docs/superpowers/plans/2026-05-09-autoresearcher-1-mutator-lineage-gate-seal.md`.
//!
//! This is the scaffold landed by AR-1 Task 1. Each submodule is a
//! placeholder filled in by a later AR-1 task (see the plan's task
//! table). Task 1 declares them up front so subsequent task PRs can
//! land in parallel without colliding on this `mod.rs`.
//!
//! Note: the original plan placed `program_view` under `src/bundle/`,
//! but no `bundle` module exists in `xvision-engine` today. The
//! program view is hosted here under `autoresearch/program_view`
//! instead — it is logically part of the autoresearcher's mutation
//! surface and the rest of the codebase doesn't currently reference a
//! bundle namespace.
//!
//! Existing HTTP-surface autoresearch entry points live at
//! `src/api/autoresearch.rs` and are unrelated to this module — that
//! file is the dashboard API; this module is the cryptographic + LLM
//! substrate the API will eventually delegate to.

pub mod blob_store;
pub mod config;
pub mod content_hash;
pub mod gate;
pub mod lineage;
pub mod mutator;
pub mod program_view;
pub mod progress;
pub mod seal;
pub mod session;
pub mod validator;
