//! Integration tests for autoresearch run routes:
//!   POST /api/autoresearch/runs                — start (mutating+auth+gate)
//!   POST /api/autoresearch/runs/:id/stop       — stop  (mutating+auth)
//!   GET  /api/autoresearch/runs                — list  (readonly)
//!   GET  /api/autoresearch/runs/:id            — detail (readonly)
//!   GET  /api/autoresearch/runs/:id/stream     — SSE  (readonly)
//!   GET  /api/autoresearch/runs/:id/experiments— experiments list (readonly, ASC)
//!
//! Auth note: `require_auth_middleware` EXEMPTS loopback clients. axum-test's
//! TestServer connects as loopback, so the mutating start/stop routes reach the
//! handler without a token — which is what the gate/validation/concurrency tests
//! want. To exercise the 401 path we use `tower::ServiceExt::oneshot` with an
//! injected non-loopback `ConnectInfo` (the same pattern as nanochat_routes.rs /
//! auth_session.rs).

mod support;

use std::net::SocketAddr;
use std::sync::Mutex;

use axum::{
    body::Body,
    extract::connect_info::ConnectInfo,
    http::{Request, StatusCode},
};
use axum_test::TestServer;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

// XVN_ENABLE_LOCAL_TRAINING is process-global; cargo runs these tests in parallel.
// Serialize every test that reads/writes it through this lock so one test's
// mutation cannot be observed by another. Held across the request .await (the
// per-test current-thread runtime makes that sound).
static ENV_LOCK: Mutex<()> = Mutex::new(());

async fn boot() -> (TestServer, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init state");
    state.run_dashboard_migrations().await.unwrap();
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, tmp, state)
}

fn valid_start_body() -> Value {
    json!({
        "run_tag": "jun12a",
        "source_strategy_id": "strat-01JKTEST",
        "label_strategy": "price_forward",
        "label_config": {
            "price_forward_threshold": 0.003,
            "price_forward_horizon_bars": 12
        },
        "min_cycle_count": 10,
        "train_wall_clock_sec": 60
    })
}

// ── POST /api/autoresearch/runs ───────────────────────────────────────────────

#[tokio::test]
async fn start_run_returns_403_when_training_gate_unset() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::remove_var("XVN_ENABLE_LOCAL_TRAINING");

    // TestServer is loopback → auth-exempt → reaches the handler, which checks
    // the training gate and rejects with 403.
    let (server, _tmp, _state) = boot().await;

    let res = server
        .post("/api/autoresearch/runs")
        .json(&valid_start_body())
        .await;

    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
    let body: Value = res.json();
    assert_eq!(body["code"], "forbidden");
}

#[tokio::test]
async fn start_run_requires_auth() {
    // A non-loopback client with no token must be rejected by the auth
    // middleware BEFORE the handler runs. Use oneshot to inject a public IP.
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init state");
    state.run_dashboard_migrations().await.unwrap();
    let app = build_router(state);

    let mut request = Request::builder()
        .method("POST")
        .uri("/api/autoresearch/runs")
        .header("content-type", "application/json")
        .body(Body::from(valid_start_body().to_string()))
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo::<SocketAddr>("203.0.113.5:12345".parse().unwrap()));

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn start_run_rejects_invalid_run_tag() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("XVN_ENABLE_LOCAL_TRAINING", "1");

    let (server, _tmp, _state) = boot().await;

    let mut body = valid_start_body();
    body["run_tag"] = json!("INVALID TAG with spaces");

    let res = server
        .post("/api/autoresearch/runs")
        .json(&body)
        .await;

    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    let resp: Value = res.json();
    assert_eq!(resp["field"], "run_tag");
}

#[tokio::test]
async fn start_run_rejects_zero_min_cycle_count() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("XVN_ENABLE_LOCAL_TRAINING", "1");

    let (server, _tmp, _state) = boot().await;

    let mut body = valid_start_body();
    body["min_cycle_count"] = json!(0);

    let res = server
        .post("/api/autoresearch/runs")
        .json(&body)
        .await;

    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn start_run_rejects_second_concurrent_run() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("XVN_ENABLE_LOCAL_TRAINING", "1");

    let (server, _tmp, state) = boot().await;

    // Manually insert a 'running' run so the concurrency guard fires before
    // any worktree creation.
    sqlx::query(
        "INSERT INTO autoresearch_runs
         (run_id, run_tag, label_strategy, label_config, git_branch,
          worktree_path, status, started_at)
         VALUES ('existing-run', 'may01a', 'price_forward', '{}',
                 'autoresearch/may01a', '.worktrees/autoresearch-may01a',
                 'running', '2026-06-14T00:00:00Z')",
    )
    .execute(&state.pool)
    .await
    .unwrap();

    let res = server
        .post("/api/autoresearch/runs")
        .json(&valid_start_body())
        .await;

    assert_eq!(res.status_code(), StatusCode::CONFLICT);
    let body: Value = res.json();
    assert_eq!(body["code"], "conflict");
}

// ── GET /api/autoresearch/runs ────────────────────────────────────────────────

#[tokio::test]
async fn list_runs_returns_empty_array_initially() {
    let (server, _tmp, _state) = boot().await;
    let res = server.get("/api/autoresearch/runs").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body: Value = res.json();
    assert_eq!(body["runs"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn list_runs_requires_no_auth() {
    // GET is on readonly_router — no auth required.
    let (server, _tmp, _state) = boot().await;
    let res = server.get("/api/autoresearch/runs").await;
    assert_ne!(res.status_code(), StatusCode::UNAUTHORIZED);
}

// ── GET /api/autoresearch/runs/:id ───────────────────────────────────────────

#[tokio::test]
async fn get_run_detail_404_for_unknown() {
    let (server, _tmp, _state) = boot().await;
    let res = server.get("/api/autoresearch/runs/does-not-exist").await;
    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

// ── GET /api/autoresearch/runs/:id/experiments ────────────────────────────────

#[tokio::test]
async fn experiments_returned_in_created_at_asc_order() {
    let (server, _tmp, state) = boot().await;

    // Insert a run and two experiments in reverse insertion order.
    sqlx::query(
        "INSERT INTO autoresearch_runs
         (run_id, run_tag, label_strategy, label_config, git_branch,
          worktree_path, status, started_at)
         VALUES ('run-ord-01', 'jun01a', 'price_forward', '{}',
                 'autoresearch/jun01a', '.worktrees/autoresearch-jun01a',
                 'completed', '2026-06-14T00:00:00Z')",
    )
    .execute(&state.pool)
    .await
    .unwrap();

    for (eid, ts) in [
        ("exp-b", "2026-06-14T00:02:00Z"),
        ("exp-a", "2026-06-14T00:01:00Z"),
    ] {
        sqlx::query(
            "INSERT INTO autoresearch_experiments
             (experiment_id, run_id, git_commit, val_acc, status, description, created_at)
             VALUES (?, 'run-ord-01', 'abc1234', 0.7, 'keep', 'test', ?)",
        )
        .bind(eid)
        .bind(ts)
        .execute(&state.pool)
        .await
        .unwrap();
    }

    let res = server
        .get("/api/autoresearch/runs/run-ord-01/experiments")
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body: Value = res.json();
    let exps = body["experiments"].as_array().unwrap();
    assert_eq!(exps.len(), 2);
    // ASC order: exp-a (00:01) before exp-b (00:02).
    assert_eq!(exps[0]["experiment_id"], "exp-a");
    assert_eq!(exps[1]["experiment_id"], "exp-b");
}

// ── GET /api/autoresearch/runs/:id/stream ─────────────────────────────────────

#[tokio::test]
async fn stream_route_exists_and_returns_sse_content_type() {
    let (server, _tmp, state) = boot().await;

    // Insert a run so the handler doesn't 404.
    sqlx::query(
        "INSERT INTO autoresearch_runs
         (run_id, run_tag, label_strategy, label_config, git_branch,
          worktree_path, status, started_at)
         VALUES ('run-sse-01', 'jun01b', 'price_forward', '{}',
                 'autoresearch/jun01b', '.worktrees/autoresearch-jun01b',
                 'running', '2026-06-14T00:00:00Z')",
    )
    .execute(&state.pool)
    .await
    .unwrap();

    // Use oneshot so we read only the response head (an SSE body is an infinite
    // stream — TestServer would try to buffer it and hang).
    let app = build_router(state);
    let request = Request::builder()
        .method("GET")
        .uri("/api/autoresearch/runs/run-sse-01/stream")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let ct = response
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");
    assert!(
        ct.contains("text/event-stream"),
        "expected SSE content-type, got {ct}"
    );
}
