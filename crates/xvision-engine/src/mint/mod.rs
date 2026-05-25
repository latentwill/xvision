//! Tune-&-mint discipline (Phase 4.3 + 4.4).
//!
//! This module owns the *pure, testable gating logic* that sits between an
//! optimization run and (a) accepting its winning snapshot into a child agent,
//! and (b) minting that child to marketplace metadata. It is deliberately split
//! from the dashboard routes that drive it: the routes fetch facts (run,
//! snapshot, holdout result, lineage, eval proof) and the strategy swap; this
//! module decides.
//!
//! ## HARD INVARIANT — no `xvision-dspy`
//!
//! Like the sibling [`crate::optimization`] store, this module lives in the
//! deploy-critical runtime crate and MUST NOT import `xvision-dspy` / `dspy-rs`.
//! Metric values are scalars produced by the eval harness on the CLI side and
//! persisted via [`holdout::HoldoutStore`]; nothing here reaches into the
//! optimizer's type system.
//!
//! ## Pieces
//!
//! - [`metrics`] — the per-capability required-metric registry (trader / filter
//!   batteries). Pure constants + lookup helpers.
//! - [`holdout`] — the `optimization_holdout_results` store (migration 046) plus
//!   the overfit-detection statistic. Records one paired train/holdout result
//!   per snapshot; computes the overfit verdict deterministically.
//! - [`gate`] — the pure accept-gate ([`gate::check_accept`]) and
//!   marketplace-mint-gate ([`gate::check_marketplace_mint`]) decision functions
//!   with their typed refusals.
//!
//! ## The discipline (what the gates enforce)
//!
//! 1. A candidate CANNOT be accepted without a holdout result UNLESS a
//!    documented `override_reason` is supplied (recorded). →
//!    [`gate::check_accept`].
//! 2. An overfit warning (train ≫ holdout beyond the threshold) BLOCKS
//!    marketplace minting unless waived with a recorded reason. →
//!    [`gate::check_marketplace_mint`] + [`holdout::HoldoutStore::waive_overfit`].
//! 3. A child CANNOT be minted to marketplace metadata without (a) optimization
//!    lineage, (b) eval proof, (c) no unwaived overfit warning, (d) the
//!    capability's required-metric set covered. → [`gate::check_marketplace_mint`].

pub mod gate;
pub mod holdout;
pub mod metrics;

pub use gate::{
    check_accept, check_marketplace_mint, AcceptDecision, AcceptInputs, AcceptRefusal, EvalProof,
    MintDecision, MintInputs, MintRefusal,
};
pub use holdout::{
    capability_required_metrics, detect_overfit, metric_coverage_gap, HoldoutError, HoldoutResult,
    HoldoutStore, NewHoldoutResult, OverfitConfig, DEFAULT_OVERFIT_THRESHOLD,
};
pub use metrics::{
    is_known_capability, missing_metrics, required_metrics, FILTER_REQUIRED_METRICS,
    TRADER_REQUIRED_METRICS,
};
