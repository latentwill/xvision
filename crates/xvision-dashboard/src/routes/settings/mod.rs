//! `/api/settings/*` — Settings tabs in v1.
//!
//! Brokers / daemon / identity are read-only snapshots. `providers` is the
//! only CRUD surface — list / show / add / remove. The danger-zone wipe
//! lives elsewhere (deferred per a follow-up cleanup plan).

pub mod brokers;
pub mod daemon;
pub mod identity;
pub mod providers;
