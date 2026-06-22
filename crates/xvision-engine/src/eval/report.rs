//! Per-run aggregated report — the canonical payload behind
//! `xvn eval results` / `xvn eval show`.
//!
//! This module prefers the per-call `model_calls.input_token_count`,
//! `model_calls.output_token_count`, and `model_calls.cost_usd` rows associated
//! with an eval run (via the observability span join). When those rows are
//! absent, it falls back to `eval_runs.actual_input_tokens` /
//! `eval_runs.actual_output_tokens`, which are updated directly by the executor
//! and cover sidecar-backed optimizer evals that do not have linked model calls.
//!
//! All token / cost / wall-clock fields are `Option`: a run that failed
//! before any `model_call` row was written, or a run produced by a
//! pre-migration-018 build, will have `None`. Callers render that as
//! `null` in JSON / `—` in markdown — never as `0`, because zero tokens
//! and "no data" are operationally different signals.
//!
//! The contract that introduced this module is
//! `team/contracts/cli-report-actions-and-tokens.md` (Wave A, intake
//! `2026-05-20-cli-operator-safety-and-model-bakeoff.md`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::eval::behavior::{derive_behavior_summary, ActionCounts, BehaviorSummary};
use crate::eval::run::{Run, RunStatus};
use crate::eval::store::{DecisionRow, RunStore};

/// Aggregated token + cost figures for one eval run.
///
/// Sourced from `model_calls` when available, then from the run-level
/// `eval_runs.actual_*_tokens` columns as a fallback. The
/// `cost_estimate_complete` flag only describes per-call cost rows; run-level
/// token fallbacks carry no cost signal and are therefore incomplete.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RunTokenTotals {
    /// Sum of `model_calls.input_token_count` across the run, or
    /// `eval_runs.actual_input_tokens` when no model calls landed for this run.
    pub input_tokens: Option<u64>,
    /// Sum of `model_calls.output_token_count` across the run, or
    /// `eval_runs.actual_output_tokens` when no model calls landed for this run.
    pub output_tokens: Option<u64>,
    /// Sum of `model_calls.cost_usd`. `None` when every contributing row
    /// had `cost_usd = NULL`, or no model_calls landed at all.
    pub cost_usd_estimate: Option<f64>,
    /// `true` iff every model_call row contributing to `input_tokens` /
    /// `output_tokens` had a known `cost_usd` (no NULLs were skipped).
    /// When `false`, `cost_usd_estimate` is a lower bound. The struct
    /// default is `false` (bool default) — see [`RunTokenTotals::default`]
    /// for the no-rows / failure-mode case where the flag is paired with
    /// `cost_usd_estimate = None` and operationally means "no signal,
    /// no claim".
    pub cost_estimate_complete: bool,
    /// Count of model_call rows aggregated. Useful for debugging "why is
    /// my cost null" — when this is 0, the observability bus wasn't wired
    /// (typical for very old runs or pure-rust-baseline arms).
    pub model_call_count: u64,
}

/// Wall-clock duration for a run in milliseconds.
///
/// `Some(completed - started)` when both are present and the delta is
/// non-negative; `None` otherwise. We do not clamp negative deltas to 0 —
/// a non-negative invariant on the column pair is a separate concern.
pub fn wall_clock_ms(started_at: DateTime<Utc>, completed_at: Option<DateTime<Utc>>) -> Option<u64> {
    let c = completed_at?;
    let delta = c.signed_duration_since(started_at).num_milliseconds();
    if delta < 0 {
        return None;
    }
    Some(delta as u64)
}

/// Sum tokens + cost for the given eval run.
///
/// The primary source is the observability join path (mirrors
/// [`crate::eval::cost::aggregate_eval_run_inference_cost`]):
/// `eval_runs.id → agent_runs.eval_run_id → spans.run_id → model_calls.span_id`.
///
/// If that join yields zero rows, fall back to `eval_runs.actual_*_tokens`.
/// Optimizer-spawned evals routed through the sidecar persist those columns even
/// when their provider calls are not linked into `model_calls`.
pub async fn aggregate_run_token_totals(pool: &SqlitePool, eval_run_id: &str) -> RunTokenTotals {
    let row: Result<Option<(Option<i64>, Option<i64>, Option<f64>, i64, i64)>, _> = sqlx::query_as(
        "SELECT \
            SUM(mc.input_token_count)  AS sum_in,  \
            SUM(mc.output_token_count) AS sum_out, \
            SUM(mc.cost_usd)           AS sum_cost, \
            SUM(CASE WHEN mc.cost_usd IS NULL THEN 1 ELSE 0 END) AS null_cost_rows, \
            COUNT(*) AS rows \
         FROM model_calls mc \
         JOIN spans s     ON s.id = mc.span_id \
         JOIN agent_runs ar ON ar.id = s.run_id \
         WHERE ar.eval_run_id = ?",
    )
    .bind(eval_run_id)
    .fetch_optional(pool)
    .await;

    let Ok(Some((sum_in, sum_out, sum_cost, null_cost_rows, rows))) = row else {
        return aggregate_run_actual_tokens(pool, eval_run_id).await;
    };

    if rows <= 0 {
        return aggregate_run_actual_tokens(pool, eval_run_id).await;
    }

    let mut input_tokens = sum_in.and_then(|n| u64::try_from(n).ok());
    let mut output_tokens = sum_out.and_then(|n| u64::try_from(n).ok());
    if input_tokens.is_none() || output_tokens.is_none() {
        let (actual_input, actual_output) = load_run_actual_tokens(pool, eval_run_id).await;
        if input_tokens.is_none() {
            input_tokens = actual_input.and_then(|n| u64::try_from(n).ok());
        }
        if output_tokens.is_none() {
            output_tokens = actual_output.and_then(|n| u64::try_from(n).ok());
        }
    }
    // `cost_estimate_complete` is true only when *every* row had a known
    // cost; one NULL row tips it to false. A run with zero rows is handled
    // above; we only get here when rows > 0.
    let cost_estimate_complete = null_cost_rows == 0;
    let cost_usd_estimate = sum_cost.filter(|v| v.is_finite());

    RunTokenTotals {
        input_tokens,
        output_tokens,
        cost_usd_estimate,
        cost_estimate_complete,
        model_call_count: rows as u64,
    }
}

async fn load_run_actual_tokens(pool: &SqlitePool, eval_run_id: &str) -> (Option<i64>, Option<i64>) {
    let row: Result<Option<(Option<i64>, Option<i64>)>, _> = sqlx::query_as(
        "SELECT actual_input_tokens, actual_output_tokens \
         FROM eval_runs WHERE id = ?",
    )
    .bind(eval_run_id)
    .fetch_optional(pool)
    .await;

    row.ok().flatten().unwrap_or((None, None))
}

async fn aggregate_run_actual_tokens(pool: &SqlitePool, eval_run_id: &str) -> RunTokenTotals {
    let (actual_input, actual_output) = load_run_actual_tokens(pool, eval_run_id).await;
    if actual_input.is_none() && actual_output.is_none() {
        return RunTokenTotals::default();
    }

    RunTokenTotals {
        input_tokens: actual_input.and_then(|n| u64::try_from(n).ok()),
        output_tokens: actual_output.and_then(|n| u64::try_from(n).ok()),
        cost_usd_estimate: None,
        cost_estimate_complete: false,
        model_call_count: 0,
    }
}

/// Aggregated per-run report — the canonical payload for `xvn eval
/// results` (and its `show` alias).
///
/// JSON shape is **append-only**: existing CLI consumers parse the
/// historical run-object as-is via `#[serde(flatten)]`; new fields are
/// additive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub run_id: String,
    pub status: RunStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Wall-clock duration in milliseconds. `None` when `completed_at`
    /// is missing or the delta would be negative.
    pub wall_clock_ms: Option<u64>,
    pub action_counts: ActionCounts,
    pub decisions: u32,
    /// Sum of `action_counts.opens() + action_counts.closes()` — every
    /// open and every explicit close is one trade leg the executor would
    /// have routed.
    pub trades: u32,
    pub direct_flips: u32,
    pub repeated_opens: u32,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd_estimate: Option<f64>,
    pub cost_estimate_complete: bool,
}

/// Compose a full `RunReport` from a `Run`, its decisions, and the
/// observability join. The behavior summary is also returned so callers
/// (e.g. `xvn eval show --behavior`) can avoid a second pass.
pub async fn compute_run_report(pool: &SqlitePool, run: &Run) -> (RunReport, BehaviorSummary) {
    let store = RunStore::new(pool.clone());
    let decisions: Vec<DecisionRow> = store.read_decisions(&run.id).await.unwrap_or_default();
    let behavior = derive_behavior_summary(&decisions);
    let totals = aggregate_run_token_totals(pool, &run.id).await;

    let report = RunReport {
        run_id: run.id.clone(),
        status: run.status,
        started_at: Some(run.started_at),
        completed_at: run.completed_at,
        wall_clock_ms: wall_clock_ms(run.started_at, run.completed_at),
        action_counts: behavior.action_counts.clone(),
        decisions: decisions.len() as u32,
        trades: behavior.action_counts.trades(),
        direct_flips: behavior.direct_flips,
        repeated_opens: behavior.repeated_opens,
        input_tokens: totals.input_tokens,
        output_tokens: totals.output_tokens,
        cost_usd_estimate: totals.cost_usd_estimate,
        cost_estimate_complete: totals.cost_estimate_complete,
    };
    (report, behavior)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn wall_clock_returns_none_when_completed_missing() {
        let started = Utc::now();
        assert_eq!(wall_clock_ms(started, None), None);
    }

    #[test]
    fn wall_clock_returns_delta_ms() {
        let started = Utc::now();
        let completed = started + Duration::milliseconds(1_234);
        assert_eq!(wall_clock_ms(started, Some(completed)), Some(1_234));
    }

    #[test]
    fn wall_clock_returns_none_on_negative_delta() {
        // Bad data — completed_at before started_at. Defensive: we
        // surface None rather than rolling around u64::MAX.
        let started = Utc::now();
        let completed = started - Duration::milliseconds(500);
        assert_eq!(wall_clock_ms(started, Some(completed)), None);
    }

    #[tokio::test]
    async fn aggregate_run_token_totals_falls_back_to_eval_run_actuals() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE eval_runs (
                id TEXT PRIMARY KEY,
                actual_input_tokens INTEGER,
                actual_output_tokens INTEGER
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO eval_runs (id, actual_input_tokens, actual_output_tokens)
             VALUES ('run-1', 410969, 34665)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let totals = aggregate_run_token_totals(&pool, "run-1").await;

        assert_eq!(totals.input_tokens, Some(410_969));
        assert_eq!(totals.output_tokens, Some(34_665));
        assert_eq!(totals.cost_usd_estimate, None);
        assert!(!totals.cost_estimate_complete);
        assert_eq!(totals.model_call_count, 0);
    }

    #[tokio::test]
    async fn aggregate_run_token_totals_uses_actuals_when_model_call_tokens_are_null() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE eval_runs (
                id TEXT PRIMARY KEY,
                actual_input_tokens INTEGER,
                actual_output_tokens INTEGER
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("CREATE TABLE agent_runs (id TEXT PRIMARY KEY, eval_run_id TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE spans (id TEXT PRIMARY KEY, run_id TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE model_calls (
                span_id TEXT,
                input_token_count INTEGER,
                output_token_count INTEGER,
                cost_usd REAL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO eval_runs (id, actual_input_tokens, actual_output_tokens)
             VALUES ('run-1', 410969, 34665)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO agent_runs (id, eval_run_id) VALUES ('agent-run-1', 'run-1')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO spans (id, run_id) VALUES ('span-1', 'agent-run-1')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO model_calls (span_id, input_token_count, output_token_count, cost_usd) VALUES ('span-1', NULL, NULL, NULL)")
            .execute(&pool)
            .await
            .unwrap();

        let totals = aggregate_run_token_totals(&pool, "run-1").await;

        assert_eq!(totals.input_tokens, Some(410_969));
        assert_eq!(totals.output_tokens, Some(34_665));
        assert_eq!(totals.model_call_count, 1);
        assert!(!totals.cost_estimate_complete);
    }

    #[tokio::test]
    async fn aggregate_run_token_totals_prefers_model_call_tokens_when_present() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE eval_runs (
                id TEXT PRIMARY KEY,
                actual_input_tokens INTEGER,
                actual_output_tokens INTEGER
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("CREATE TABLE agent_runs (id TEXT PRIMARY KEY, eval_run_id TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE spans (id TEXT PRIMARY KEY, run_id TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE model_calls (
                span_id TEXT,
                input_token_count INTEGER,
                output_token_count INTEGER,
                cost_usd REAL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO eval_runs (id, actual_input_tokens, actual_output_tokens)
             VALUES ('run-1', 999999, 999999)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO agent_runs (id, eval_run_id) VALUES ('agent-run-1', 'run-1')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO spans (id, run_id) VALUES ('span-1', 'agent-run-1')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO model_calls (span_id, input_token_count, output_token_count, cost_usd) VALUES ('span-1', 100, 25, 0.01)")
            .execute(&pool)
            .await
            .unwrap();

        let totals = aggregate_run_token_totals(&pool, "run-1").await;

        assert_eq!(totals.input_tokens, Some(100));
        assert_eq!(totals.output_tokens, Some(25));
        assert_eq!(totals.cost_usd_estimate, Some(0.01));
        assert!(totals.cost_estimate_complete);
        assert_eq!(totals.model_call_count, 1);
    }
}
