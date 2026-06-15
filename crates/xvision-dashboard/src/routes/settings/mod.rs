//! `/api/settings/*` — Settings tabs in v1.
//!
//! Brokers / daemon / identity are read-only snapshots. `providers` is
//! the CRUD surface for registered LLM providers. `danger` is the
//! destructive-ops surface (wipe DB / regen identity / factory reset),
//! confirm-string gated and audit-logged.

pub mod brokers;
pub mod daemon;
pub mod danger;
pub mod identity;
pub mod memory;
pub mod observability;
pub mod profile;
pub mod providers;
