//! bead-8wn: cross-source cost surface — windowed spend rollup + persisted
//! operator-set daily budget cap.
//!
//! ## Honesty contract (§8.1/§8.9)
//!
//! Every monetary field is an `Option<f64>` that is `null` (not `0`) when its
//! source has no real cost data:
//!
//! * `eval_cost_usd` — `Σ model_calls.cost_usd` over agent runs (eval-linked
//!   AND standalone) started in the window. `null` when the observability
//!   tables are absent, no priced call exists, or every call ran against an
//!   unpriced model. Computed by [`crate::eval::cost::aggregate_inference_cost_since`].
//! * `optimizer_cost_usd` — `Σ cycle_cost.cost_usd` over cycles whose cost row
//!   was written in the window (mirrors the dashboard `session_cost_usd`).
//!   `null` when no cycle in the window has a metered cost.
//!   Computed by [`crate::eval::cost::aggregate_optimizer_cost_since`].
//! * `spend_usd` — the combined total. `null` ONLY when BOTH sources are
//!   `null` (genuinely no cost data anywhere in the window). When at least one
//!   source is known, `spend_usd` is the sum of the known components — we do
//!   not let an unknown half NULL out a known half.
//! * `daily_cap_usd` — the operator-set cap, or `null` when UNSET. An UNSET
//!   cap means the dashboard renders NO denominator (em-dash), never a faked
//!   ceiling. Read from `cost_budget` via [`crate::eval::cost::get_daily_budget_cap`].
//!
//! The combine rule never fabricates `Some(0.0)`: a precise `$0.00` total is a
//! worse signal than "unknown" when the underlying price is missing.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::eval::cost::{
    aggregate_inference_cost_since, aggregate_optimizer_cost_since, get_daily_budget_cap,
};

/// Windowed cross-source spend rollup. The FE budget strip builds on this exact
/// shape: each `*_usd` field is `null` when its source has no cost data.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostRollupResponse {
    /// Echo of the validated `?since` lower bound (RFC-3339, UTC).
    pub since: String,
    /// Combined eval/agent-run + optimizer spend in the window. `null` when
    /// BOTH component sources are unknown; otherwise the sum of the known
    /// components (an unknown half does not NULL out a known half).
    pub spend_usd: Option<f64>,
    /// Eval + agent-run inference cost (token cost) over runs started in the
    /// window. `null` (unknown) when no priced call exists.
    pub eval_cost_usd: Option<f64>,
    /// Optimizer (autooptimizer cycle) cost over cycles in the window. `null`
    /// (unknown) when no metered cycle exists.
    pub optimizer_cost_usd: Option<f64>,
    /// Operator-set daily budget cap, or `null` when UNSET (render em-dash,
    /// never a faked ceiling).
    pub daily_cap_usd: Option<f64>,
}

/// The persisted operator-set daily budget cap. `daily_cap_usd` is `null` when
/// UNSET.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CostBudgetResponse {
    pub daily_cap_usd: Option<f64>,
}

/// Combine the two windowed cost sources into a `spend_usd` total without
/// fabricating a number. `None` only when BOTH are `None`; otherwise the sum
/// of whichever components are known.
pub fn combine_spend(eval_cost: Option<f64>, optimizer_cost: Option<f64>) -> Option<f64> {
    match (eval_cost, optimizer_cost) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
    }
}

/// Compute the full windowed rollup for `since`. Thin orchestration over the
/// `eval::cost` aggregators + the persisted cap; all the honesty semantics live
/// in those leaf functions and [`combine_spend`].
pub async fn compute_rollup(pool: &SqlitePool, since: DateTime<Utc>) -> CostRollupResponse {
    let eval_cost_usd = aggregate_inference_cost_since(pool, since).await;
    let optimizer_cost_usd = aggregate_optimizer_cost_since(pool, since).await;
    let daily_cap_usd = get_daily_budget_cap(pool).await;
    CostRollupResponse {
        since: since.to_rfc3339(),
        spend_usd: combine_spend(eval_cost_usd, optimizer_cost_usd),
        eval_cost_usd,
        optimizer_cost_usd,
        daily_cap_usd,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_spend_none_when_both_unknown() {
        // Both sources unknown → total is unknown (null), NOT $0.00.
        assert_eq!(combine_spend(None, None), None);
    }

    #[test]
    fn combine_spend_sums_when_both_known() {
        assert_eq!(combine_spend(Some(1.5), Some(2.5)), Some(4.0));
    }

    #[test]
    fn combine_spend_keeps_known_half_when_other_unknown() {
        // A known half must survive an unknown half — don't NULL out real data.
        assert_eq!(combine_spend(Some(3.0), None), Some(3.0));
        assert_eq!(combine_spend(None, Some(7.0)), Some(7.0));
    }
}
