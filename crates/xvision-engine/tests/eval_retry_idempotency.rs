//! Regression tests for `api::eval::retry` idempotency predicate.
//!
//! QA finding #9 (`qa/2026-05-17-comprehensive-codebase-review.md`): the
//! retry handler's docstring promises the in-flight sibling lookup is keyed
//! on `(agent_id, scenario_id, mode, params_override)`, but the
//! implementation previously dropped `params_override` from the equality
//! check — so a queued or running run with the SAME agent+scenario+mode but
//! a DIFFERENT params override was incorrectly returned as the "sibling",
//! silently coalescing two distinct workloads into one.
//!
//! These tests pin the documented contract:
//! 1. A sibling with a different `params_override` does NOT coalesce —
//!    retry must start a new run (and surface any downstream error from
//!    that path rather than masquerading as the unrelated sibling).
//! 2. A true sibling — identical `params_override`, including `None` and
//!    semantically-equal JSON with reordered keys — still coalesces.

use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use xvision_engine::api::eval::{self, ListRunsRequest};
use xvision_engine::api::{Actor, ApiContext, ApiError};
use xvision_engine::eval::{Run, RunMode, RunStatus, RunStore};

async fn ctx_with_eval_tables() -> (ApiContext, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("eval_retry_idempotency.sqlite");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
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
    sqlx::query(include_str!("../migrations/022_eval_runs_agents_agent_id.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/015_eval_decisions_reasoning.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let ctx = ApiContext::new(
        pool,
        Actor::Cli {
            user: "operator".into(),
        },
        dir.path().to_path_buf(),
    );
    (ctx, dir)
}

async fn seed_failed(ctx: &ApiContext, params: Option<serde_json::Value>) -> Run {
    let store = RunStore::new(ctx.db.clone());
    let mut run = Run::new_queued("agent-x".into(), "scenario-x".into(), RunMode::Backtest);
    run.params_override = params;
    store.create(&run).await.unwrap();
    store
        .update_status(&run.id, RunStatus::Failed, Some("provider 5xx"))
        .await
        .unwrap();
    store.get(&run.id).await.unwrap()
}

async fn seed_sibling_queued(ctx: &ApiContext, failed: &Run, params: Option<serde_json::Value>) -> Run {
    let store = RunStore::new(ctx.db.clone());
    let mut sibling = Run::new_queued(failed.agent_id.clone(), failed.scenario_id.clone(), failed.mode);
    sibling.params_override = params;
    store.create(&sibling).await.unwrap();
    store.get(&sibling.id).await.unwrap()
}

async fn seed_sibling_running(ctx: &ApiContext, failed: &Run, params: Option<serde_json::Value>) -> Run {
    let store = RunStore::new(ctx.db.clone());
    let sibling = seed_sibling_queued(ctx, failed, params).await;
    store
        .update_status(&sibling.id, RunStatus::Running, None)
        .await
        .unwrap();
    store.get(&sibling.id).await.unwrap()
}

async fn assert_retry_takes_start_path(ctx: &ApiContext, failed_id: &str) {
    let err = eval::retry(ctx, failed_id)
        .await
        .expect_err("different params_override must not return an in-flight sibling");
    assert!(
        matches!(err, ApiError::NotFound(_)),
        "retry should fall through to start_run and fail on missing strategy, got {err:?}"
    );

    let runs = eval::list(ctx, ListRunsRequest::default()).await.unwrap();
    assert_eq!(
        runs.len(),
        2,
        "retry must not coalesce and must not persist a new run when start_run fails"
    );
}

/// Regression: when the only in-flight run with the same
/// (agent_id, scenario_id, mode) has a DIFFERENT `params_override`, retry
/// must NOT coalesce onto it. Before the fix, the predicate dropped
/// `params_override` and would silently return the unrelated sibling's
/// detail; afterwards, the predicate falls through to `start_run`, which
/// — in this test harness without strategies wired up — produces an
/// `ApiError::NotFound` for the missing strategy. The point is that the
/// sibling is NOT returned: a different params override is a different
/// workload.
#[tokio::test]
async fn retry_does_not_coalesce_when_params_override_differs() {
    let (ctx, _d) = ctx_with_eval_tables().await;

    // Failed source has params { "alpha": 1 }
    let failed = seed_failed(&ctx, Some(json!({"alpha": 1}))).await;
    // Sibling Queued has params { "alpha": 2 } — same (agent, scenario, mode),
    // different params_override. Must NOT be treated as a sibling.
    let _sibling = seed_sibling_queued(&ctx, &failed, Some(json!({"alpha": 2}))).await;

    assert_retry_takes_start_path(&ctx, &failed.id).await;
}

/// Happy path: a true sibling — identical `params_override` — still
/// coalesces. Guards against the fix over-correcting and breaking the
/// double-click-retry behavior the docstring promises.
#[tokio::test]
async fn retry_coalesces_when_params_override_matches() {
    let (ctx, _d) = ctx_with_eval_tables().await;

    let params = Some(json!({"alpha": 1, "beta": "x"}));
    let failed = seed_failed(&ctx, params.clone()).await;
    let sibling = seed_sibling_queued(&ctx, &failed, params).await;

    let detail = eval::retry(&ctx, &failed.id)
        .await
        .expect("matching-params sibling coalesces, no start_run path taken");
    assert_eq!(detail.summary.id, sibling.id);
    assert_eq!(detail.summary.status, "queued");

    let runs = eval::list(&ctx, ListRunsRequest::default()).await.unwrap();
    assert_eq!(runs.len(), 2, "no third run should be created");
}

/// Running siblings are also in-flight: identical `params_override` must
/// coalesce just like queued siblings.
#[tokio::test]
async fn retry_coalesces_when_running_params_override_matches() {
    let (ctx, _d) = ctx_with_eval_tables().await;

    let params = Some(json!({"alpha": 1, "beta": "x"}));
    let failed = seed_failed(&ctx, params.clone()).await;
    let sibling = seed_sibling_running(&ctx, &failed, params).await;

    let detail = eval::retry(&ctx, &failed.id)
        .await
        .expect("matching-params running sibling coalesces, no start_run path taken");
    assert_eq!(detail.summary.id, sibling.id);
    assert_eq!(detail.summary.status, "running");

    let runs = eval::list(&ctx, ListRunsRequest::default()).await.unwrap();
    assert_eq!(runs.len(), 2, "no third run should be created");
}

/// Running siblings with a different `params_override` are a different
/// workload and must not be returned by retry.
#[tokio::test]
async fn retry_does_not_coalesce_when_running_params_override_differs() {
    let (ctx, _d) = ctx_with_eval_tables().await;

    let failed = seed_failed(&ctx, Some(json!({"alpha": 1}))).await;
    let _sibling = seed_sibling_running(&ctx, &failed, Some(json!({"alpha": 2}))).await;

    assert_retry_takes_start_path(&ctx, &failed.id).await;
}

/// Happy path: both source and sibling have `params_override == None`.
/// This is the most common case (no overrides set) and must coalesce.
#[tokio::test]
async fn retry_coalesces_when_params_override_both_none() {
    let (ctx, _d) = ctx_with_eval_tables().await;

    let failed = seed_failed(&ctx, None).await;
    let sibling = seed_sibling_queued(&ctx, &failed, None).await;

    let detail = eval::retry(&ctx, &failed.id)
        .await
        .expect("None==None sibling coalesces");
    assert_eq!(detail.summary.id, sibling.id);
}

/// Semantic equality: JSON objects with the same keys+values but
/// different textual key order must still coalesce. `serde_json::Value`
/// uses a Map whose equality is order-independent, so this works without
/// explicit canonicalization — pin the behavior so a future Map swap
/// (e.g. enabling `preserve_order`) can't silently break the contract.
#[tokio::test]
async fn retry_coalesces_when_params_override_keys_reordered() {
    let (ctx, _d) = ctx_with_eval_tables().await;

    let failed = seed_failed(&ctx, Some(json!({"alpha": 1, "beta": 2}))).await;
    // Build the sibling's params via a different insertion order. With
    // `serde_json` default features (BTreeMap-backed), this is the same
    // value either way; with `preserve_order` enabled, IndexMap's PartialEq
    // is still order-independent.
    let sibling = seed_sibling_queued(&ctx, &failed, Some(json!({"beta": 2, "alpha": 1}))).await;

    let detail = eval::retry(&ctx, &failed.id)
        .await
        .expect("semantically-equal JSON coalesces");
    assert_eq!(detail.summary.id, sibling.id);
}

/// A sibling whose `params_override` is `Some(...)` while source is
/// `None` (or vice versa) is a different workload. Pin the asymmetry.
#[tokio::test]
async fn retry_does_not_coalesce_when_one_side_is_none() {
    let (ctx, _d) = ctx_with_eval_tables().await;

    let failed = seed_failed(&ctx, None).await;
    let _sibling = seed_sibling_queued(&ctx, &failed, Some(json!({"alpha": 1}))).await;

    assert_retry_takes_start_path(&ctx, &failed.id).await;
}
