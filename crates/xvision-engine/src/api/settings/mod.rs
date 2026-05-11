//! `/api/settings/*` — Settings tabs in v1.
//!
//! Brokers / daemon / identity are read-only snapshots. `providers` is the
//! only CRUD surface in this module — single source of truth for the
//! workspace's registered LLM providers, dispatched through by both the
//! `xvn provider` CLI and the dashboard's Settings/Providers route.
//!
//! The danger-zone wipe lives elsewhere (deferred per a follow-up cleanup
//! plan). No secrets in responses: env-var values are never returned,
//! only "set" / "unset" presence flags.

pub mod brokers;
pub mod daemon;
pub mod identity;
pub mod providers;
