//! bead-008: `GET /api/eval/runs?since=<rfc3339>` — validated, inclusive
//! lower bound on `started_at`.
//!
//! Contract (shared with `/api/agent-runs`):
//! - `since` value is RFC-3339 (e.g. `2026-06-06T00:00:00Z`).
//! - INCLUSIVE lower bound: returns rows WHERE `started_at >= since`.
//! - Absent/empty `since` => no filter (first-paint behavior unchanged).
//! - Invalid `since` => HTTP 400 with `DashboardError::Validation { field: "since", .. }`.
//!
//! These tests seed `eval_runs` rows directly into the already-migrated
//! `AppState` pool (only the NOT NULL columns), bypassing the heavier
//! launchable-strategy fixture — we only need list-shaped rows here.

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

/// Raw INSERT covering only the NOT NULL eval_runs columns. `scenario_id`
/// stays NULL (allowed since the scenario-less Live runs migration) so we
/// don't have to seed a `scenarios` row for the FK.
async fn seed(pool: &sqlx::SqlitePool, id: &str, started_at: &str) {
    sqlx::query(
        "INSERT INTO eval_runs (id, agent_id, scenario_id, mode, status, started_at) \
         VALUES (?, 'agent-x', NULL, 'backtest', 'completed', ?)",
    )
    .bind(id)
    .bind(started_at)
    .execute(pool)
    .await
    .expect("seed eval_runs row");
}

#[tokio::test]
async fn since_filters_out_older_rows_inclusive_boundary() {
    let (server, state, _tmp) = boot().await;
    seed(&state.pool, "old", "2026-06-01T00:00:00Z").await;
    seed(&state.pool, "boundary", "2026-06-06T00:00:00Z").await;
    seed(&state.pool, "newer", "2026-06-10T00:00:00Z").await;

    let resp = server.get("/api/eval/runs?since=2026-06-06T00:00:00Z").await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    let items = v["items"].as_array().unwrap();
    // Inclusive boundary: exact-match row kept, older dropped. Newest-first.
    assert_eq!(items.len(), 2, "expected newer + boundary, got {items:?}");
    assert_eq!(items[0]["id"].as_str().unwrap(), "newer");
    assert_eq!(items[1]["id"].as_str().unwrap(), "boundary");
    // total reflects the post-filter count.
    assert_eq!(v["total"].as_u64().unwrap(), 2);
}

#[tokio::test]
async fn absent_since_returns_all() {
    let (server, state, _tmp) = boot().await;
    seed(&state.pool, "a", "2026-06-01T00:00:00Z").await;
    seed(&state.pool, "b", "2026-06-10T00:00:00Z").await;

    let resp = server.get("/api/eval/runs").await;
    resp.assert_status_ok();
    let v: serde_json::Value = resp.json();
    assert_eq!(v["items"].as_array().unwrap().len(), 2);
    assert_eq!(v["total"].as_u64().unwrap(), 2);
}

#[tokio::test]
async fn invalid_since_returns_400_validation() {
    let (server, state, _tmp) = boot().await;
    seed(&state.pool, "a", "2026-06-01T00:00:00Z").await;

    let resp = server.get("/api/eval/runs?since=not-a-timestamp").await;
    resp.assert_status(StatusCode::BAD_REQUEST);
    let v: serde_json::Value = resp.json();
    assert_eq!(v["code"].as_str(), Some("validation"));
    assert_eq!(v["field"].as_str(), Some("since"));
}
