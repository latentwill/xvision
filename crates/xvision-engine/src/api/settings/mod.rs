//! Settings — read-only surfaces in v1.
//!
//! This module exposes "what's configured?" for the dashboard's Settings tabs.
//! The Provider CRUD surface (and the danger-zone wipe) are intentionally
//! deferred — they live with the llm-providers Phase 2+ track and a follow-up
//! cleanup plan respectively.
//!
//! No mutations here. No secrets in responses (env vars are surfaced as
//! "set" / "unset" flags, never values).

pub mod brokers;
pub mod daemon;
pub mod identity;
