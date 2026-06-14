//! Engine-layer multi-status filter tests (xvision-t4u8.1 DoD #5).
//!
//! Verifies that `list_summaries_paged` (and `list`) with
//! `ListRunsRequest { status: Some(vec![...]) }` returns rows matching
//! ANY of the supplied statuses in a SINGLE engine query — not an
//! N-query loop at the route layer.

mod common;

use common::{open_api_context as ctx_with_eval_tables, seeded_scenario_id};
use xvision_engine::api::eval::{self, ListRunsRequest};
use xvision_engine::eval::run::RunStatus as RS;
use xvision_engine::eval::{Run, RunMode, RunStore};

// ── DoD #5a: list_summaries_paged with multi-status Vec ─────────────────────

#[tokio::test]
async fn list_summaries_paged_multi_status_returns_matching_any() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = seeded_scenario_id(&ctx).await;

    let r_queued = Run::new_queued("h".into(), scenario_id.clone(), RunMode::Backtest);
    let r_running = Run::new_queued("h".into(), scenario_id.clone(), RunMode::Backtest);
    let r_completed = Run::new_queued("h".into(), scenario_id.clone(), RunMode::Backtest);
    let r_failed = Run::new_queued("h".into(), scenario_id.clone(), RunMode::Backtest);

    store.create(&r_queued).await.unwrap();
    store.create(&r_running).await.unwrap();
    store.create(&r_completed).await.unwrap();
    store.create(&r_failed).await.unwrap();

    // Transition statuses
    store
        .update_status(&r_running.id, RS::Running, None)
        .await
        .unwrap();
    store
        .update_status(&r_completed.id, RS::Completed, None)
        .await
        .unwrap();
    store.update_status(&r_failed.id, RS::Failed, None).await.unwrap();

    let req = ListRunsRequest {
        status: Some(vec![RS::Queued, RS::Running]),
        ..Default::default()
    };
    let page = eval::list_summaries_paged(&ctx, req).await.unwrap();

    let statuses: Vec<&str> = page.items.iter().map(|s| s.status.as_str()).collect();
    assert_eq!(
        page.total, 2,
        "expected total=2 (queued+running), got total={} items={statuses:?}",
        page.total
    );
    assert_eq!(
        page.items.len(),
        2,
        "expected 2 items, got {}: {statuses:?}",
        page.items.len()
    );
    for s in &page.items {
        assert!(
            s.status == "queued" || s.status == "running",
            "unexpected status {:?} in multi-status result",
            s.status
        );
    }
}

// ── DoD #5b: list() with single-element Vec behaves like old single-status ──

#[tokio::test]
async fn list_single_status_vec_behaves_like_old_single() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = seeded_scenario_id(&ctx).await;

    let r1 = Run::new_queued("h".into(), scenario_id.clone(), RunMode::Backtest);
    let r2 = Run::new_queued("h".into(), scenario_id.clone(), RunMode::Backtest);
    store.create(&r1).await.unwrap();
    store.create(&r2).await.unwrap();
    store.update_status(&r1.id, RS::Completed, None).await.unwrap();

    let req = ListRunsRequest {
        status: Some(vec![RS::Completed]),
        ..Default::default()
    };
    let runs = eval::list(&ctx, req).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, r1.id);
}

// ── DoD #5c: None status still returns all ──────────────────────────────────

#[tokio::test]
async fn list_no_status_filter_returns_all() {
    let (ctx, _d) = ctx_with_eval_tables().await;
    let store = RunStore::new(ctx.db.clone());
    let scenario_id = seeded_scenario_id(&ctx).await;

    let r1 = Run::new_queued("h".into(), scenario_id.clone(), RunMode::Backtest);
    let r2 = Run::new_queued("h".into(), scenario_id.clone(), RunMode::Backtest);
    store.create(&r1).await.unwrap();
    store.create(&r2).await.unwrap();
    store.update_status(&r1.id, RS::Completed, None).await.unwrap();

    let req = ListRunsRequest::default(); // status = None
    let runs = eval::list(&ctx, req).await.unwrap();
    assert_eq!(runs.len(), 2);
}

// ── DoD #5d: RunStatus::parse_list unit tests ────────────────────────────────

#[test]
fn parse_list_single_valid() {
    let result = RS::parse_list("queued").unwrap();
    assert_eq!(result, vec![RS::Queued]);
}

#[test]
fn parse_list_multi_valid() {
    let result = RS::parse_list("queued,running").unwrap();
    assert_eq!(result, vec![RS::Queued, RS::Running]);
}

#[test]
fn parse_list_with_whitespace() {
    let result = RS::parse_list("queued, running").unwrap();
    assert_eq!(result, vec![RS::Queued, RS::Running]);
}

#[test]
fn parse_list_all_statuses() {
    let result = RS::parse_list("queued,running,completed,failed,cancelled").unwrap();
    assert_eq!(
        result,
        vec![RS::Queued, RS::Running, RS::Completed, RS::Failed, RS::Cancelled]
    );
}

#[test]
fn parse_list_bogus_returns_err() {
    let err = RS::parse_list("bogus").unwrap_err();
    assert!(err.contains("bogus"), "error should mention the bad token: {err}");
}

#[test]
fn parse_list_partial_bogus_returns_err() {
    let err = RS::parse_list("queued,bogus").unwrap_err();
    assert!(err.contains("bogus"), "error should mention the bad token: {err}");
}

#[test]
fn parse_list_empty_returns_err() {
    let err = RS::parse_list("").unwrap_err();
    assert!(!err.is_empty());
}
