//! `/api/eval/*` route handlers.
//!
//! Today this module only hosts `review` — the older `eval_runs.rs` lives
//! as a flat sibling and is not migrated as part of this track to avoid
//! scope creep. Future tracks that touch the eval-runs surface can pull
//! it into `eval::runs` if they decide the directory layout is worth the
//! churn.

pub mod agent_profiles;
pub mod review;
