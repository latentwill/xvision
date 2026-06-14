//! bead-8wn: cross-source cost surface.
//!
//! - `GET /api/cost/rollup?since=<rfc3339>` — windowed combined spend
//!   (eval/agent-run token cost + optimizer cycle cost) plus the operator cap.
//!   Read-only.
//! - `GET /api/cost/budget` — the persisted operator-set daily budget cap
//!   (`{ daily_cap_usd: Option<f64> }`, `null` when UNSET). Read-only.
//! - `PUT /api/cost/budget` — set the daily budget cap. Mutation. A
//!   non-positive / NaN cap is a 400 (copying the autooptimizer_cycle.rs
//!   budget validation).
//!
//! HONESTY (§8.1/§8.9): every monetary field is `null` (not `0`) when its
//! source has no real cost data; the cap is `null` when UNSET (the dashboard
//! renders an em-dash, never a faked ceiling). All the null-vs-zero semantics
//! live in the engine `eval::cost` aggregators + `api::cost::combine_spend` —
//! these handlers are thin orchestration.

use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use xvision_engine::api::cost::{self, CostBudgetResponse, CostRollupResponse};
use xvision_engine::eval::cost::{get_daily_budget_cap, set_daily_budget_cap};

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct RollupParams {
    /// REQUIRED-in-spirit windowed lower bound on run/cycle start, RFC-3339
    /// (e.g. `2026-06-13T00:00:00Z`). Validated in the handler; invalid values
    /// surface as `DashboardError::Validation`. Absent/empty defaults to the
    /// trailing 24h (a daily budget surface) so a bare `/api/cost/rollup` is
    /// useful without a query string.
    pub since: Option<String>,
}

/// bead-8wn: parse the optional `?since=` value into an RFC-3339 lower bound,
/// mirroring the proven validation ladder in `eval_runs::parse_since` /
/// `autooptimizer.rs::get_ladder` (parse → 400 on error → `.with_timezone`).
///
/// `None` / `Some("")` defaults to `now - 24h` (the daily-budget window).
fn parse_since(raw: Option<&str>) -> Result<DateTime<Utc>, DashboardError> {
    match raw {
        Some(s) if !s.trim().is_empty() => Ok(DateTime::parse_from_rfc3339(s.trim())
            .map_err(|e| DashboardError::Validation {
                field: "since".into(),
                msg: format!("invalid RFC-3339 timestamp: {e}"),
            })?
            .with_timezone(&Utc)),
        _ => Ok(Utc::now() - chrono::Duration::hours(24)),
    }
}

/// `GET /api/cost/rollup?since=<rfc3339>` — windowed cross-source spend.
pub async fn rollup(
    State(state): State<AppState>,
    Query(params): Query<RollupParams>,
) -> Result<Json<CostRollupResponse>, DashboardError> {
    let since = parse_since(params.since.as_deref())?;
    let resp = cost::compute_rollup(&state.pool, since).await;
    Ok(Json(resp))
}

/// `GET /api/cost/budget` — the persisted operator-set daily budget cap.
pub async fn get_budget(State(state): State<AppState>) -> Result<Json<CostBudgetResponse>, DashboardError> {
    let daily_cap_usd = get_daily_budget_cap(&state.pool).await;
    Ok(Json(CostBudgetResponse { daily_cap_usd }))
}

#[derive(Debug, Deserialize)]
pub struct SetBudgetBody {
    /// The daily USD cap to persist. Must be finite and `> 0` (400 otherwise).
    pub daily_cap_usd: f64,
}

/// `PUT /api/cost/budget` — set the daily budget cap.
///
/// Validation copies `autooptimizer_cycle.rs:203-213`: a non-positive / NaN /
/// non-finite cap is a client error (400), not a silently-ignored one.
pub async fn put_budget(
    State(state): State<AppState>,
    Json(body): Json<SetBudgetBody>,
) -> Result<Json<CostBudgetResponse>, DashboardError> {
    let cap = body.daily_cap_usd;
    if !cap.is_finite() || cap <= 0.0 {
        return Err(DashboardError::Validation {
            field: "daily_cap_usd".into(),
            msg: "daily_cap_usd must be a finite positive USD value".into(),
        });
    }
    set_daily_budget_cap(&state.pool, cap, &Utc::now().to_rfc3339())
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?;
    Ok(Json(CostBudgetResponse {
        daily_cap_usd: Some(cap),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_since_rejects_garbage_with_validation() {
        let err = parse_since(Some("not-a-timestamp")).expect_err("garbage must 400");
        match err {
            DashboardError::Validation { field, .. } => assert_eq!(field, "since"),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn parse_since_accepts_rfc3339() {
        let ts = parse_since(Some("2026-06-13T00:00:00Z")).expect("valid rfc3339");
        assert_eq!(ts.to_rfc3339(), "2026-06-13T00:00:00+00:00");
    }

    #[test]
    fn parse_since_defaults_when_absent_or_empty() {
        // No filter string → a window opening ~24h ago (within the last day).
        let before = Utc::now() - chrono::Duration::hours(25);
        let after = Utc::now() - chrono::Duration::hours(23);
        for raw in [None, Some(""), Some("   ")] {
            let ts = parse_since(raw).expect("default window");
            assert!(
                ts > before && ts < after,
                "default since must be ~now-24h, got {ts}"
            );
        }
    }
}
