//! HTTP-level regression tests for the
//! `POST /api/eval/runs/:id/retry` route under the new
//! `eval-rerun-from-completed` widening (2026-05-19).
//!
//! Today the route accepts source runs in `Failed | Cancelled | Completed`.
//! Runs in `Queued` / `Running` are rejected with `400 validation`. A
//! double-click on Rerun coalesces onto the in-flight sibling with
//! `202` instead of starting a third row.

use axum::http::StatusCode;
use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

async fn boot() -> (TestServer, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state)).unwrap();
    (server, tmp)
}

/// `Completed` source + a queued sibling → coalesce with `202`. The
/// route returns the sibling's id, no new row is persisted. Pins the
/// "Rerun" semantics: a double-click on Rerun does NOT fan out.
#[tokio::test]
async fn retry_returns_202_for_completed_source_with_queued_sibling() {
    use xvision_engine::eval::{
        run::{Run, RunMode, RunStatus},
        store::RunStore,
    };

    let (server, tmp) = boot().await;
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}/xvn.db", tmp.path().display()))
        .await
        .unwrap();
    let store = RunStore::new(pool);

    let mut completed = Run::new_queued("agent-x".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    completed.status = RunStatus::Queued;
    store.create(&completed).await.unwrap();
    store
        .update_status(&completed.id, RunStatus::Completed, None)
        .await
        .unwrap();

    let sibling = Run::new_queued(
        completed.agent_id.clone(),
        completed.scenario_id.clone(),
        completed.mode,
    );
    let sibling_id = sibling.id.clone();
    store.create(&sibling).await.unwrap();

    let response = server
        .post(&format!("/api/eval/runs/{}/retry", completed.id))
        .await;
    response.assert_status(StatusCode::ACCEPTED);
    let body: serde_json::Value = response.json();
    assert_eq!(body["summary"]["id"], sibling_id);
    assert_eq!(body["summary"]["status"], "queued");

    // No third run was created — just the completed source and the queued sibling.
    let list = server.get("/api/eval/runs").await;
    let items = list.json::<serde_json::Value>()["items"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(
        items, 2,
        "expected 2 runs (completed source + sibling), got {items}"
    );
}

/// `Queued` source → `400 validation`. The body's `code` field is
/// `"validation"` so the frontend can classify the toast.
#[tokio::test]
async fn retry_rejects_queued_source_with_400_validation() {
    use xvision_engine::eval::{
        run::{Run, RunMode},
        store::RunStore,
    };

    let (server, tmp) = boot().await;
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}/xvn.db", tmp.path().display()))
        .await
        .unwrap();
    let store = RunStore::new(pool);

    let run = Run::new_queued("agent-x".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    let run_id = run.id.clone();
    store.create(&run).await.unwrap();
    // Leave it Queued.

    let response = server.post(&format!("/api/eval/runs/{run_id}/retry")).await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

/// `Running` source → `400 validation`. Same rationale as Queued — the
/// existing in-flight run is what the operator should be watching.
#[tokio::test]
async fn retry_rejects_running_source_with_400_validation() {
    use xvision_engine::eval::{
        run::{Run, RunMode, RunStatus},
        store::RunStore,
    };

    let (server, tmp) = boot().await;
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}/xvn.db", tmp.path().display()))
        .await
        .unwrap();
    let store = RunStore::new(pool);

    let run = Run::new_queued("agent-x".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    let run_id = run.id.clone();
    store.create(&run).await.unwrap();
    store
        .update_status(&run_id, RunStatus::Running, None)
        .await
        .unwrap();

    let response = server.post(&format!("/api/eval/runs/{run_id}/retry")).await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

/// `Failed` source still works — the widening is additive. Pins the
/// PR #260 (2026-05-18) behavior alongside the new completed-source case.
#[tokio::test]
async fn retry_still_returns_202_for_failed_source_with_queued_sibling() {
    use xvision_engine::eval::{
        run::{Run, RunMode, RunStatus},
        store::RunStore,
    };

    let (server, tmp) = boot().await;
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}/xvn.db", tmp.path().display()))
        .await
        .unwrap();
    let store = RunStore::new(pool);

    let failed = Run::new_queued("agent-x".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
    store.create(&failed).await.unwrap();
    store
        .update_status(&failed.id, RunStatus::Failed, Some("provider 5xx"))
        .await
        .unwrap();

    let sibling = Run::new_queued(failed.agent_id.clone(), failed.scenario_id.clone(), failed.mode);
    let sibling_id = sibling.id.clone();
    store.create(&sibling).await.unwrap();

    let response = server.post(&format!("/api/eval/runs/{}/retry", failed.id)).await;
    response.assert_status(StatusCode::ACCEPTED);
    let body: serde_json::Value = response.json();
    assert_eq!(body["summary"]["id"], sibling_id);
}
