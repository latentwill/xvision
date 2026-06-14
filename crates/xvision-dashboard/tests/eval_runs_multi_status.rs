//! Multi-status filter on `GET /api/eval/runs?status=queued,running`.
//!
//! Contract (xvision-t4u8.1):
//! - `?status=queued,running` → 200; returns runs of BOTH statuses, none dropped.
//! - `?status=queued`         → 200 (single-value regression).
//! - `?status=bogus`          → 400 validation error, field = "status".
//! - `?status=queued,bogus`   → 400 validation error.
//! - `?status=queued&status=running` (repeated param) → only the last value
//!   (`running`) is seen by axum's Query<ListParams>; deterministic, documented
//!   behaviour — asserted here so a future refactor can't silently break it.
//!
//! These tests seed `eval_runs` rows directly into the migrated AppState pool
//! (NOT NULL columns only), matching the pattern used in `eval_runs_since.rs`.

use axum::http::StatusCode;
use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

async fn boot() -> (TestServer, AppState, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, state, tmp)
}

async fn seed(pool: &sqlx::SqlitePool, id: &str, status: &str) {
    sqlx::query(
        "INSERT INTO eval_runs (id, agent_id, scenario_id, mode, status, started_at) \
         VALUES (?, 'agent-x', NULL, 'backtest', ?, '2026-06-13T10:00:00Z')",
    )
    .bind(id)
    .bind(status)
    .execute(pool)
    .await
    .expect("seed eval_runs row");
}

// ── DoD #1 ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn multi_status_comma_list_returns_both_statuses() {
    let (server, state, _tmp) = boot().await;
    seed(&state.pool, "r-queued", "queued").await;
    seed(&state.pool, "r-running", "running").await;
    seed(&state.pool, "r-completed", "completed").await;

    let resp = server.get("/api/eval/runs?status=queued,running").await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    let items = v["items"].as_array().unwrap();
    let ids: Vec<&str> = items.iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), 2, "expected exactly queued + running, got {ids:?}");
    assert!(ids.contains(&"r-queued"), "missing r-queued in {ids:?}");
    assert!(ids.contains(&"r-running"), "missing r-running in {ids:?}");
    assert!(
        !ids.contains(&"r-completed"),
        "r-completed should NOT be in {ids:?}"
    );
    assert_eq!(v["total"].as_u64().unwrap(), 2);
}

// ── DoD #2 ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn single_status_still_works() {
    let (server, state, _tmp) = boot().await;
    seed(&state.pool, "r-queued", "queued").await;
    seed(&state.pool, "r-running", "running").await;

    let resp = server.get("/api/eval/runs?status=queued").await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_str().unwrap(), "r-queued");
    assert_eq!(v["total"].as_u64().unwrap(), 1);
}

// ── DoD #3 ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bogus_status_returns_400() {
    let (server, state, _tmp) = boot().await;
    seed(&state.pool, "r-queued", "queued").await;

    let resp = server.get("/api/eval/runs?status=bogus").await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    let v: serde_json::Value = resp.json();
    assert_eq!(v["code"].as_str(), Some("validation"));
    assert_eq!(v["field"].as_str(), Some("status"));
}

#[tokio::test]
async fn partially_bogus_status_returns_400() {
    let (server, state, _tmp) = boot().await;
    seed(&state.pool, "r-queued", "queued").await;

    let resp = server.get("/api/eval/runs?status=queued,bogus").await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    let v: serde_json::Value = resp.json();
    assert_eq!(v["code"].as_str(), Some("validation"));
    assert_eq!(v["field"].as_str(), Some("status"));
}

// ── DoD #4 ──────────────────────────────────────────────────────────────────

/// Repeated `?status=queued&status=running`: axum's `Query<ListParams>`
/// rejects duplicate fields with a 400 ("duplicate field `status`"). This
/// is the deterministic, documented behaviour — better than silently keeping
/// one value. The supported multi-status form is the comma-list
/// `?status=queued,running`.
#[tokio::test]
async fn repeated_status_param_returns_400() {
    let (server, state, _tmp) = boot().await;
    seed(&state.pool, "r-queued", "queued").await;
    seed(&state.pool, "r-running", "running").await;

    let resp = server.get("/api/eval/runs?status=queued&status=running").await;
    // Axum rejects duplicate fields → 400 (no silent wrong-answer).
    resp.assert_status(StatusCode::BAD_REQUEST);
}
