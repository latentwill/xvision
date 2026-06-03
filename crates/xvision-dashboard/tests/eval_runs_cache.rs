//! Regression tests for the `GET /api/eval/runs/:id` 500ms TTL cache
//! introduced 2026-05-19 (`dashboard-eval-run-polling-reduction`).
//!
//! The cache exists to absorb burst polling: the dashboard's
//! eval-run-detail tab refetches at 2s while a run is in-flight, and the
//! 2026-05-19 api_audit logged 890 `get_run` calls against 64 `start`
//! calls. A short TTL collapses concurrent reads onto a single DB hit
//! without serving stale data, since the UI's poll cadence is already
//! several multiples longer than the TTL.
//!
//! The tests below pin three invariants:
//!   1. A queued (non-terminal) run's second `GET` is served from cache —
//!      proven by mutating the underlying row directly via `RunStore`
//!      between two HTTP requests inside the TTL window and asserting the
//!      cached body still reflects the pre-mutation state.
//!   2. A completed (terminal) run is never cached — the same direct-DB
//!      mutation between two HTTP requests is visible immediately, because
//!      the route bypasses the cache for terminal status.
//!   3. Cancelling a run invalidates the cache — a `GET` after `POST
//!      /:id/cancel` sees the new `cancelled` status, not the pre-cancel
//!      `running` cached value.

use axum::http::StatusCode;
use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::eval::{
    run::{Run, RunMode, RunStatus},
    store::RunStore,
};

const SCENARIO_ID: &str = "crypto-bull-q1-2025";

async fn boot() -> (TestServer, AppState, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, state, tmp)
}

/// A queued run's second `GET` within the TTL window returns the cached
/// (pre-mutation) body, proving the cache short-circuits the engine layer.
#[tokio::test]
async fn get_run_caches_within_ttl_for_queued_runs() {
    let (server, state, _tmp) = boot().await;
    let store = RunStore::new(state.pool.clone());

    let mut run = Run::new_queued("agent-x".into(), SCENARIO_ID.into(), RunMode::Backtest);
    run.status = RunStatus::Queued;
    store.create(&run).await.unwrap();

    // First call populates the cache.
    let first = server.get(&format!("/api/eval/runs/{}", run.id)).await;
    first.assert_status(StatusCode::OK);
    let first_body: serde_json::Value = first.json();
    assert_eq!(first_body["summary"]["status"], "queued");

    // Mutate the underlying row directly — bypasses the dashboard cache
    // entirely so we can prove the next HTTP read is served from cache
    // rather than re-fetching.
    store
        .update_status(&run.id, RunStatus::Running, None)
        .await
        .unwrap();

    // Second call within TTL: should still see "queued" because the cache
    // is masking the row we just bumped to Running.
    let second = server.get(&format!("/api/eval/runs/{}", run.id)).await;
    second.assert_status(StatusCode::OK);
    let second_body: serde_json::Value = second.json();
    assert_eq!(
        second_body["summary"]["status"], "queued",
        "expected cached response (queued) but got fresh fetch ({})",
        second_body["summary"]["status"]
    );

    store
        .update_status(&run.id, RunStatus::Completed, None)
        .await
        .unwrap();

    let terminal = server.get(&format!("/api/eval/runs/{}", run.id)).await;
    terminal.assert_status(StatusCode::OK);
    let terminal_body: serde_json::Value = terminal.json();
    assert_eq!(
        terminal_body["summary"]["status"], "completed",
        "terminal transition must bypass the queued cache entry"
    );
}

#[tokio::test]
async fn get_run_bypasses_cached_non_terminal_when_run_fails() {
    let (server, state, _tmp) = boot().await;
    let store = RunStore::new(state.pool.clone());

    let mut run = Run::new_queued("agent-x".into(), SCENARIO_ID.into(), RunMode::Backtest);
    run.status = RunStatus::Running;
    store.create(&run).await.unwrap();

    let first = server.get(&format!("/api/eval/runs/{}", run.id)).await;
    first.assert_status(StatusCode::OK);
    let first_body: serde_json::Value = first.json();
    assert_eq!(first_body["summary"]["status"], "running");
    assert!(
        state.eval_run_cache_get(&run.id).is_some(),
        "running status should populate the cache"
    );

    store
        .update_status(&run.id, RunStatus::Failed, Some("fixture failure"))
        .await
        .unwrap();

    let failed = server.get(&format!("/api/eval/runs/{}", run.id)).await;
    failed.assert_status(StatusCode::OK);
    let failed_body: serde_json::Value = failed.json();
    assert_eq!(
        failed_body["summary"]["status"], "failed",
        "failed transition must bypass the running cache entry"
    );
    assert!(
        state.eval_run_cache_get(&run.id).is_none(),
        "terminal failed status should evict the cached non-terminal body"
    );
}

/// Terminal-status runs (`completed | failed | cancelled`) are never
/// inserted into the cache. We assert this directly via the state
/// accessor — after a `GET` returns a `completed` body, the cache lookup
/// for that id must still miss.
#[tokio::test]
async fn get_run_does_not_cache_terminal_status() {
    let (server, state, _tmp) = boot().await;
    let store = RunStore::new(state.pool.clone());

    let mut run = Run::new_queued("agent-x".into(), SCENARIO_ID.into(), RunMode::Backtest);
    run.status = RunStatus::Queued;
    store.create(&run).await.unwrap();
    store
        .update_status(&run.id, RunStatus::Completed, None)
        .await
        .unwrap();

    let first = server.get(&format!("/api/eval/runs/{}", run.id)).await;
    first.assert_status(StatusCode::OK);
    let first_body: serde_json::Value = first.json();
    assert_eq!(first_body["summary"]["status"], "completed");

    // The post-fetch cache lookup must miss — terminal responses bypass
    // the put path entirely.
    assert!(
        state.eval_run_cache_get(&run.id).is_none(),
        "terminal status should not be cached"
    );

    // Sanity: a fresh non-terminal run on the same state populates the
    // cache after one GET — proving the assertion above is meaningful.
    let mut live = Run::new_queued("agent-x".into(), SCENARIO_ID.into(), RunMode::Backtest);
    live.status = RunStatus::Queued;
    store.create(&live).await.unwrap();
    let _ = server.get(&format!("/api/eval/runs/{}", live.id)).await;
    assert!(
        state.eval_run_cache_get(&live.id).is_some(),
        "queued status should populate the cache"
    );
}

/// Cancelling a run invalidates the cache: a subsequent `GET` returns
/// the new `cancelled` status rather than the pre-cancel cached body.
#[tokio::test]
async fn cancel_invalidates_get_run_cache() {
    let (server, state, _tmp) = boot().await;
    let store = RunStore::new(state.pool.clone());

    let mut run = Run::new_queued("agent-x".into(), SCENARIO_ID.into(), RunMode::Backtest);
    run.status = RunStatus::Running;
    store.create(&run).await.unwrap();
    store
        .update_status(&run.id, RunStatus::Running, None)
        .await
        .unwrap();

    // Warm the cache.
    let first = server.get(&format!("/api/eval/runs/{}", run.id)).await;
    first.assert_status(StatusCode::OK);
    let first_body: serde_json::Value = first.json();
    assert_eq!(first_body["summary"]["status"], "running");

    // Cancel via the HTTP route — it invokes
    // `state.eval_run_cache_invalidate(&id)` after the engine flip.
    let cancel = server.post(&format!("/api/eval/runs/{}/cancel", run.id)).await;
    cancel.assert_status(StatusCode::OK);

    // Read-back: should see the post-cancel status, not the cached
    // `running` snapshot. Terminal status, so bypass kicks in too —
    // belt-and-suspenders for the invariant.
    let after = server.get(&format!("/api/eval/runs/{}", run.id)).await;
    let after_body: serde_json::Value = after.json();
    assert_eq!(
        after_body["summary"]["status"], "cancelled",
        "expected cancel to invalidate cache, got {}",
        after_body["summary"]["status"]
    );
}
