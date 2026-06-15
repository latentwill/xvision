//! `/api/settings/*` — Settings tabs in v1.
//!
//! Brokers / daemon / identity are read-only snapshots. `providers` is
//! the CRUD surface for the workspace's registered LLM providers,
//! dispatched through by both the `xvn provider` CLI and the dashboard.
//! `danger` is the destructive-ops surface (wipe / regen / factory
//! reset), confirm-string gated and audit-logged.
//!
//! No secrets in responses: env-var values are never returned, only
//! "set" / "unset" presence flags.

pub mod brokers;
pub mod daemon;
pub mod danger;
pub mod identity;
pub mod memory;
pub mod observability;
pub mod profile;
pub mod providers;
pub mod providers_catalog;
