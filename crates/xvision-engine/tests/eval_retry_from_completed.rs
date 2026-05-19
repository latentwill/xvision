//! Regression tests for the `eval-rerun-from-completed` track
//! (2026-05-19).
//!
//! Operator can re-run a `Completed` eval against the same agent and
//! scenario to get a fresh trace ("Rerun" — re-test for stability or
//! after a code-level fix that doesn't change params). This is distinct
//! from "Retry" (recovery from `Failed` / `Cancelled`), and the engine
//! must classify the two so downstream lineage surfaces can tell them
//! apart.
//!
//! Pins:
//! 1. Source `Completed` is accepted and routes to `RetryReason::ManualRerun`.
//! 2. Source `Failed` / `Cancelled` still routes to
//!    `RetryReason::FailureRecovery` — the widening is purely additive.
//! 3. Source `Queued` / `Running` are still rejected with a
//!    classified `ApiError::Validation`.
//! 4. Idempotency on `(agent_id, scenario_id, mode, params_override)`
//!    holds for `Completed` source too — a double-click on Rerun
//!    coalesces onto a queued/running sibling rather than fanning out.
//! 5. Lineage: `RetryOutcome::source_run_id` points back to the source.
//! 6. The legacy `retry(...) -> RunDetail` signature still works — it
//!    just discards lineage.

use sqlx::sqlite::SqlitePoolOptions;
use xvision_engine::api::eval::{self, ListRunsRequest, RetryReason};
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::eval::{Run, RunMode, RunStatus, RunStore};

async fn ctx_with_eval_tables() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/014_eval_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "operator".into(),
        },
        dir.path().to_path_buf(),
    );
    (ctx, dir)
}

async fn seed_run(ctx: &ApiContext, status: RunStatus) -> Run {
    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued("agent-x".into(), "scenario-x".into(), RunMode::Backtest);
    store.create(&run).await.unwrap();
    if status != RunStatus::Queued {
        store.update_status(&run.id, status, None).await.unwrap();
    }
    store.get(&run.id).await.unwrap()
}

async fn seed_sibling(ctx: &ApiContext, source: &Run, sibling_status: RunStatus) -> Run {
    let store = RunStore::new(ctx.db.clone());
    let sibling = Run::new_queued(source.agent_id.clone(), source.scenario_id.clone(), source.mode);
    let sibling_id = sibling.id.clone();
    store.create(&sibling).await.unwrap();
    if sibling_status != RunStatus::Queued {
        store
            .update_status(&sibling_id, sibling_status, None)
            .await
            .unwrap();
    }
    store.get(&sibling_id).await.unwrap()
}

/// Source `Completed` → accepted, classified `ManualRerun`, lineage
/// breadcrumbs point back to the source.
///
/// `start_run` itself fails with `NotFound` in this harness (no
/// strategy is wired up), so we use the in-flight-sibling coalesce
/// path to assert the happy path end-to-end without needing a full
/// engine boot. A queued sibling with matching fingerprint exists →
/// retry returns that sibling's id, classified `ManualRerun`.
#[tokio::test]
async fn rerun_completed_classifies_as_manual_rerun_with_source_lineage() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Completed).await;
    let sibling = seed_sibling(&ctx, &source, RunStatus::Queued).await;

    let outcome = eval::retry_with_outcome(&ctx, &source.id)
        .await
        .expect("rerun of completed must succeed via in-flight coalesce");

    assert_eq!(outcome.reason, RetryReason::ManualRerun);
    assert_eq!(outcome.source_run_id, source.id);
    assert_eq!(outcome.detail.summary.id, sibling.id);
    assert_eq!(outcome.detail.summary.status, "queued");
    // Source agent + scenario + mode preserved.
    assert_eq!(outcome.detail.summary.agent_id, source.agent_id);
    assert_eq!(outcome.detail.summary.scenario_id, source.scenario_id);
}

/// Source `Failed` → still classified `FailureRecovery`. The 2026-05-19
/// widening is purely additive; the existing failure-recovery path
/// must not regress.
#[tokio::test]
async fn retry_failed_still_classifies_as_failure_recovery() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Failed).await;
    let sibling = seed_sibling(&ctx, &source, RunStatus::Queued).await;

    let outcome = eval::retry_with_outcome(&ctx, &source.id)
        .await
        .expect("retry of failed must succeed via in-flight coalesce");

    assert_eq!(outcome.reason, RetryReason::FailureRecovery);
    assert_eq!(outcome.source_run_id, source.id);
    assert_eq!(outcome.detail.summary.id, sibling.id);
}

/// Source `Cancelled` → still classified `FailureRecovery`. Pins the
/// PR #260 (2026-05-18) widening alongside the new completed-source
/// widening.
#[tokio::test]
async fn retry_cancelled_still_classifies_as_failure_recovery() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Cancelled).await;
    let sibling = seed_sibling(&ctx, &source, RunStatus::Queued).await;

    let outcome = eval::retry_with_outcome(&ctx, &source.id)
        .await
        .expect("retry of cancelled must succeed via in-flight coalesce");

    assert_eq!(outcome.reason, RetryReason::FailureRecovery);
    assert_eq!(outcome.source_run_id, source.id);
    assert_eq!(outcome.detail.summary.id, sibling.id);
}

/// Source `Queued` → rejected with `ApiError::Validation`. The error
/// message lists the accepted set so the operator can self-diagnose.
#[tokio::test]
async fn retry_rejects_queued_source() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Queued).await;

    let err = eval::retry_with_outcome(&ctx, &source.id)
        .await
        .expect_err("queued source has nothing to retry");

    match err {
        ApiError::Validation(msg) => {
            assert!(
                msg.contains("failed") && msg.contains("cancelled") && msg.contains("completed"),
                "validation message should list the accepted set; got: {msg}"
            );
        }
        other => panic!("expected ApiError::Validation, got {other:?}"),
    }
}

/// Source `Running` → rejected with `ApiError::Validation`. Same
/// rationale as queued — the existing in-flight run is what the
/// operator should be watching.
#[tokio::test]
async fn retry_rejects_running_source() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Running).await;

    let err = eval::retry_with_outcome(&ctx, &source.id)
        .await
        .expect_err("running source has nothing to retry");

    assert!(matches!(err, ApiError::Validation(_)));
}

/// A double-rerun while a sibling is still queued is idempotent. The
/// second call must NOT enqueue a third row — it returns the in-flight
/// queued id with `RetryReason::ManualRerun` again.
#[tokio::test]
async fn double_rerun_of_completed_is_idempotent_on_queued_sibling() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Completed).await;
    let sibling = seed_sibling(&ctx, &source, RunStatus::Queued).await;

    let first = eval::retry_with_outcome(&ctx, &source.id).await.unwrap();
    let second = eval::retry_with_outcome(&ctx, &source.id).await.unwrap();

    assert_eq!(first.detail.summary.id, sibling.id);
    assert_eq!(second.detail.summary.id, sibling.id);
    assert_eq!(second.reason, RetryReason::ManualRerun);

    let runs = eval::list(&ctx, ListRunsRequest::default()).await.unwrap();
    assert_eq!(
        runs.len(),
        2,
        "no third row should be created — completed source + queued sibling only"
    );
}

/// A double-rerun while a sibling is still running is also idempotent
/// — coalesces onto the running sibling.
#[tokio::test]
async fn rerun_of_completed_coalesces_onto_running_sibling() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Completed).await;
    let sibling = seed_sibling(&ctx, &source, RunStatus::Running).await;

    let outcome = eval::retry_with_outcome(&ctx, &source.id).await.unwrap();

    assert_eq!(outcome.detail.summary.id, sibling.id);
    assert_eq!(outcome.reason, RetryReason::ManualRerun);
    assert_eq!(outcome.detail.summary.status, "running");
}

/// When no in-flight sibling exists, the rerun falls through to
/// `start_run`. The test harness has no strategy wired up so that path
/// fails with `NotFound` — but the point is that the COMPLETED status
/// gate accepts the source. Before the widening, this returned
/// `ApiError::Validation` from the gate; now it should bubble the
/// downstream `start_run` error, proving the gate was crossed.
#[tokio::test]
async fn rerun_of_completed_falls_through_to_start_run_when_no_sibling() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Completed).await;

    let err = eval::retry_with_outcome(&ctx, &source.id)
        .await
        .expect_err("no sibling + no strategy wired up → start_run fails");

    assert!(
        matches!(err, ApiError::NotFound(_)),
        "gate accepted Completed; start_run failure (NotFound for strategy) is the expected downstream outcome in this harness — got {err:?}"
    );

    // Crucially: no new row was persisted because start_run aborted.
    let runs = eval::list(&ctx, ListRunsRequest::default()).await.unwrap();
    assert_eq!(runs.len(), 1, "only the completed source remains");
}

/// The legacy `retry(...) -> RunDetail` form still works — it just
/// discards the lineage breadcrumbs that `retry_with_outcome` returns.
/// Keeps the existing dashboard route + CLI consumers unchanged.
#[tokio::test]
async fn legacy_retry_signature_still_returns_run_detail_for_completed_source() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let source = seed_run(&ctx, RunStatus::Completed).await;
    let sibling = seed_sibling(&ctx, &source, RunStatus::Queued).await;

    let detail = eval::retry(&ctx, &source.id).await.unwrap();
    assert_eq!(detail.summary.id, sibling.id);
    assert_eq!(detail.summary.status, "queued");
}
