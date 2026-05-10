//! Phase 3.D read-only api::eval surface tests — list, get, scenarios.
//! The `run` dispatch is deferred to a follow-up PR.

use sqlx::SqlitePool;
use xvision_engine::api::eval::{self, ListRunsRequest, ScenarioSummary};
use xvision_engine::api::{Actor, ApiContext};
use xvision_engine::eval::{Run, RunMode, RunStatus, RunStore};

async fn ctx_with_eval_tables() -> (ApiContext, tempfile::TempDir) {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/001_api_audit.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(include_str!("../migrations/002_eval.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let ctx = ApiContext {
        db: pool,
        actor: Actor::Cli {
            user: "operator".into(),
        },
        xvn_home: dir.path().to_path_buf(),
    };
    (ctx, dir)
}

#[tokio::test]
async fn list_returns_empty_for_fresh_pool() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let runs = eval::list(&ctx, ListRunsRequest::default())
        .await
        .unwrap();
    assert!(runs.is_empty());
}

#[tokio::test]
async fn list_returns_persisted_runs() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let r1 = Run::new_queued("h-A".into(), "scen-A".into(), RunMode::Backtest);
    let r2 = Run::new_queued("h-B".into(), "scen-B".into(), RunMode::Paper);
    store.create(&r1).await.unwrap();
    store.create(&r2).await.unwrap();

    let runs = eval::list(&ctx, ListRunsRequest::default())
        .await
        .unwrap();
    assert_eq!(runs.len(), 2);
}

#[tokio::test]
async fn list_filters_by_strategy_bundle_hash() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let mut a = Run::new_queued("h-A".into(), "s".into(), RunMode::Backtest);
    let mut b = Run::new_queued("h-A".into(), "s".into(), RunMode::Paper);
    let mut c = Run::new_queued("h-B".into(), "s".into(), RunMode::Backtest);
    a.strategy_bundle_hash = "h-A".into();
    b.strategy_bundle_hash = "h-A".into();
    c.strategy_bundle_hash = "h-B".into();
    store.create(&a).await.unwrap();
    store.create(&b).await.unwrap();
    store.create(&c).await.unwrap();

    let req = ListRunsRequest {
        strategy_bundle_hash: Some("h-A".into()),
        ..Default::default()
    };
    let runs = eval::list(&ctx, req).await.unwrap();
    assert_eq!(runs.len(), 2);
    for r in &runs {
        assert_eq!(r.strategy_bundle_hash, "h-A");
    }
}

#[tokio::test]
async fn list_filters_by_status() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let r1 = Run::new_queued("h".into(), "s".into(), RunMode::Backtest);
    let r2 = Run::new_queued("h".into(), "s".into(), RunMode::Backtest);
    store.create(&r1).await.unwrap();
    store.create(&r2).await.unwrap();
    store
        .update_status(&r1.id, RunStatus::Completed, None)
        .await
        .unwrap();

    let req = ListRunsRequest {
        status: Some(RunStatus::Completed),
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
    let run = Run::new_queued("h".into(), "scen-x".into(), RunMode::Paper);
    let id = run.id.clone();
    store.create(&run).await.unwrap();

    let back = eval::get(&ctx, &id).await.unwrap();
    assert_eq!(back.id, id);
    assert_eq!(back.scenario_id, "scen-x");
    assert_eq!(back.mode, RunMode::Paper);
}

#[tokio::test]
async fn get_returns_not_found_for_unknown_id() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let r = eval::get(&ctx, "missing").await;
    assert!(matches!(
        r,
        Err(xvision_engine::api::ApiError::NotFound(_))
    ));
}

#[tokio::test]
async fn scenarios_returns_canonical_set() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let summaries: Vec<ScenarioSummary> = eval::scenarios(&ctx).await.unwrap();
    assert!(summaries.len() >= 4, "canonical set expected >= 4");
    // BTC-only constraint surfaces in the summary too.
    for s in &summaries {
        assert!(!s.regime_tags.is_empty());
        assert!(s.asset_universe.contains(&"BTC/USD".to_string()));
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
    let run = Run::new_queued("h".into(), "s".into(), RunMode::Paper);
    let id = run.id.clone();
    store.create(&run).await.unwrap();
    let _ = eval::get(&ctx, &id).await.unwrap();

    let (domain, op, target, outcome): (String, String, Option<String>, String) = sqlx::query_as(
        "SELECT domain, operation, target, outcome FROM api_audit WHERE operation = 'get'",
    )
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
