//! Phase 3.D read-only api::eval surface tests — list, get, scenarios.
//! The `run` dispatch is deferred to a follow-up PR.

mod common;

use common::{open_api_context as ctx_with_eval_tables, seeded_scenario_id};
use xvision_engine::api::eval::{self, ListRunsRequest, ScenarioSummary};
use xvision_engine::api::ApiContext;
use xvision_engine::eval::{Run, RunMode, RunStatus, RunStore};

#[tokio::test]
async fn list_returns_empty_for_fresh_pool() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let runs = eval::list(&ctx, ListRunsRequest::default()).await.unwrap();
    assert!(runs.is_empty());
}

#[tokio::test]
async fn list_returns_persisted_runs() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = seeded_scenario_id(&ctx).await;
    let r1 = Run::new_queued("h-A".into(), scenario_id.clone(), RunMode::Backtest);
    let r2 = Run::new_queued("h-B".into(), scenario_id, RunMode::Backtest);
    store.create(&r1).await.unwrap();
    store.create(&r2).await.unwrap();

    let runs = eval::list(&ctx, ListRunsRequest::default()).await.unwrap();
    assert_eq!(runs.len(), 2);
}

#[tokio::test]
async fn list_filters_by_agent_id() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = seeded_scenario_id(&ctx).await;
    let mut a = Run::new_queued("h-A".into(), scenario_id.clone(), RunMode::Backtest);
    let mut b = Run::new_queued("h-A".into(), scenario_id.clone(), RunMode::Backtest);
    let mut c = Run::new_queued("h-B".into(), scenario_id, RunMode::Backtest);
    a.agent_id = "h-A".into();
    b.agent_id = "h-A".into();
    c.agent_id = "h-B".into();
    store.create(&a).await.unwrap();
    store.create(&b).await.unwrap();
    store.create(&c).await.unwrap();

    let req = ListRunsRequest {
        agent_id: Some("h-A".into()),
        ..Default::default()
    };
    let runs = eval::list(&ctx, req).await.unwrap();
    assert_eq!(runs.len(), 2);
    for r in &runs {
        assert_eq!(r.agent_id, "h-A");
    }
}

#[tokio::test]
async fn list_filters_by_status() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = seeded_scenario_id(&ctx).await;
    let r1 = Run::new_queued("h".into(), scenario_id.clone(), RunMode::Backtest);
    let r2 = Run::new_queued("h".into(), scenario_id, RunMode::Backtest);
    store.create(&r1).await.unwrap();
    store.create(&r2).await.unwrap();
    store
        .update_status(&r1.id, RunStatus::Completed, None)
        .await
        .unwrap();

    let req = ListRunsRequest {
        status: Some(vec![RunStatus::Completed]),
        ..Default::default()
    };
    let runs = eval::list(&ctx, req).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, r1.id);
}

#[tokio::test]
async fn get_returns_persisted_run() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = seeded_scenario_id(&ctx).await;
    let run = Run::new_queued("h".into(), scenario_id.clone(), RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.unwrap();

    let back = eval::get(&ctx, &id).await.unwrap();
    assert_eq!(back.id, id);
    assert_eq!(back.scenario_id, scenario_id);
    assert_eq!(back.mode, RunMode::Backtest);
}

#[tokio::test]
async fn get_returns_not_found_for_unknown_id() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let r = eval::get(&ctx, "missing").await;
    assert!(matches!(r, Err(xvision_engine::api::ApiError::NotFound(_))));
}

#[tokio::test]
async fn cancel_is_idempotent_after_run_is_cancelled() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued("h".into(), seeded_scenario_id(&ctx).await, RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.unwrap();

    let first = eval::cancel(&ctx, &id).await.unwrap();
    assert_eq!(first.status, RunStatus::Cancelled);

    let second = eval::cancel(&ctx, &id).await.unwrap();
    assert_eq!(second.status, RunStatus::Cancelled);
    assert_eq!(second.error.as_deref(), Some("cancelled by user"));
}

#[tokio::test]
async fn scenarios_returns_canonical_set() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let summaries: Vec<ScenarioSummary> = eval::scenarios(&ctx).await.unwrap();
    assert!(summaries.len() >= 4, "canonical set expected >= 4");
    // Scenarios are asset-free now — the traded asset comes from the
    // strategy, so the summary no longer carries an asset universe.
    for s in &summaries {
        assert!(!s.regime_tags.is_empty());
    }
    // Unique IDs.
    let mut ids: Vec<&str> = summaries.iter().map(|s| s.id.as_str()).collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "duplicate scenario id detected");
}

#[tokio::test]
async fn list_writes_audit_row() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let _ = eval::list(&ctx, ListRunsRequest::default()).await.unwrap();
    let (domain, op, outcome): (String, String, String) =
        sqlx::query_as("SELECT domain, operation, outcome FROM api_audit")
            .fetch_one(&ctx.db)
            .await
            .unwrap();
    assert_eq!(domain, "eval");
    assert_eq!(op, "list");
    assert_eq!(outcome, "ok");
}

#[tokio::test]
async fn get_writes_audit_row() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued("h".into(), seeded_scenario_id(&ctx).await, RunMode::Backtest);
    let id = run.id.clone();
    store.create(&run).await.unwrap();
    let _ = eval::get(&ctx, &id).await.unwrap();

    let (domain, op, target, outcome): (String, String, Option<String>, String) =
        sqlx::query_as("SELECT domain, operation, target, outcome FROM api_audit WHERE operation = 'get'")
            .fetch_one(&ctx.db)
            .await
            .unwrap();
    assert_eq!(domain, "eval");
    assert_eq!(op, "get");
    assert_eq!(target.as_deref(), Some(id.as_str()));
    assert_eq!(outcome, "ok");
}

#[tokio::test]
async fn scenarios_writes_audit_row() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let _ = eval::scenarios(&ctx).await.unwrap();
    let (domain, op): (String, String) =
        sqlx::query_as("SELECT domain, operation FROM api_audit WHERE operation = 'scenarios'")
            .fetch_one(&ctx.db)
            .await
            .unwrap();
    assert_eq!(domain, "eval");
    assert_eq!(op, "scenarios");
}

// ── eval::retry ─────────────────────────────────────────────────────────────

use xvision_engine::api::ApiError;

async fn seed_run(ctx: &ApiContext, status: RunStatus) -> Run {
    let store = RunStore::new(ctx.db.clone());
    let run = Run::new_queued("agent-x".into(), seeded_scenario_id(ctx).await, RunMode::Backtest);
    store.create(&run).await.unwrap();
    if status != RunStatus::Queued {
        let err = if status == RunStatus::Failed {
            Some("provider 5xx")
        } else {
            None
        };
        store.update_status(&run.id, status, err).await.unwrap();
    }
    store.get(&run.id).await.unwrap()
}

#[tokio::test]
async fn retry_rejects_unknown_run() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let err = eval::retry(&ctx, "01NOPE").await.unwrap_err();
    assert!(matches!(err, ApiError::NotFound(_)), "got {err:?}");
}

#[tokio::test]
async fn retry_accepts_completed_run() {
    // `eval-rerun-from-completed` (2026-05-19): the gate now accepts
    // `Completed` sources for "Rerun" semantics. This test pins the
    // widening — the previous assertion (Completed → Validation) was
    // a pre-2026-05-19 invariant that no longer holds.
    //
    // In this harness there is no strategy wired up for `agent-x`, so
    // `start_run` falls through to NotFound. The point of this test
    // is that the GATE is crossed — i.e. the error is no longer
    // `Validation { msg contains "completed" }`.
    let (ctx, _d) = ctx_with_eval_tables().await;
    let run = seed_run(&ctx, RunStatus::Completed).await;
    let err = eval::retry(&ctx, &run.id).await.unwrap_err();
    assert!(
        !matches!(&err, ApiError::Validation(msg) if msg.contains("completed")),
        "Completed must no longer be rejected by the gate; got {err:?}"
    );
    // Specifically, the downstream `start_run` error surfaces — proving
    // the gate accepted the source.
    assert!(
        matches!(err, ApiError::NotFound(_)),
        "expected start_run NotFound (no strategy in harness); got {err:?}"
    );
}

#[tokio::test]
async fn retry_rejects_running_run() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let run = seed_run(&ctx, RunStatus::Running).await;
    let err = eval::retry(&ctx, &run.id).await.unwrap_err();
    let ApiError::Validation(msg) = err else {
        panic!("expected Validation, got {err:?}");
    };
    assert!(msg.contains("running"), "{msg}");
}

#[tokio::test]
async fn retry_accepts_cancelled_run() {
    // Operator intent on Cancel is reversible — retry must requeue the
    // same (agent_id, scenario_id, mode) shape just like a failed run.
    // This harness has no strategy file for `agent-x`, so seed the
    // in-flight retry sibling directly and assert the gate accepts
    // Cancelled by coalescing onto that queued workload.
    let (ctx, _d) = ctx_with_eval_tables().await;
    let run = seed_run(&ctx, RunStatus::Cancelled).await;
    let store = RunStore::new(ctx.db.clone());
    let sibling = Run::new_queued(run.agent_id.clone(), run.scenario_id.clone(), run.mode);
    store.create(&sibling).await.unwrap();

    let detail = eval::retry(&ctx, &run.id).await.unwrap();
    assert_eq!(detail.summary.status, "queued");
    assert_eq!(detail.summary.id, sibling.id);
    assert_eq!(detail.summary.agent_id, run.agent_id);
    assert_eq!(detail.summary.scenario_id, run.scenario_id);
}

#[tokio::test]
async fn retry_returns_inflight_sibling_idempotently() {
    // A failed run plus a Queued sibling sharing (agent_id, scenario_id, mode):
    // the retry endpoint must return the sibling rather than start a third run.
    let (ctx, _d) = ctx_with_eval_tables().await;
    let failed = seed_run(&ctx, RunStatus::Failed).await;
    let store = RunStore::new(ctx.db.clone());
    let sibling = Run::new_queued(failed.agent_id.clone(), failed.scenario_id.clone(), failed.mode);
    store.create(&sibling).await.unwrap();

    let detail = eval::retry(&ctx, &failed.id).await.unwrap();
    assert_eq!(detail.summary.id, sibling.id);
    assert_eq!(detail.summary.status, "queued");

    // No third run created.
    let runs = eval::list(&ctx, ListRunsRequest::default()).await.unwrap();
    assert_eq!(
        runs.len(),
        2,
        "expected 2 runs (failed + sibling), got {}",
        runs.len()
    );
}

#[tokio::test]
async fn retry_writes_audit_row_on_rejection() {
    // `Running` is still gated (the in-flight run is what the operator
    // should be watching). Pre-2026-05-19 this test used `Completed`
    // as the rejection case, but the gate now accepts Completed for
    // "Rerun" semantics.
    let (ctx, _d) = ctx_with_eval_tables().await;
    let run = seed_run(&ctx, RunStatus::Running).await;
    let _ = eval::retry(&ctx, &run.id).await;

    let (domain, op, target, outcome): (String, String, Option<String>, String) =
        sqlx::query_as("SELECT domain, operation, target, outcome FROM api_audit WHERE operation = 'retry'")
            .fetch_one(&ctx.db)
            .await
            .unwrap();
    assert_eq!(domain, "eval");
    assert_eq!(op, "retry");
    assert_eq!(target.as_deref(), Some(run.id.as_str()));
    assert!(outcome.starts_with("error"), "got outcome {outcome}");
}
