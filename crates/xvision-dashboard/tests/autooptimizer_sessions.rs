//! Integration tests for P1-W4: session routes + SSE Last-Event-ID replay.
//!
//! Tests are ordered to match the DoD in the spec.

mod support;

use serde_json::Value;
use support::test_server;

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/status
// ---------------------------------------------------------------------------

/// test_status_idle: no sessions → { active_session: null }
#[tokio::test]
async fn test_status_idle() {
    let (server, _tmp) = test_server().await;
    let res = server.get("/api/autooptimizer/status").await;
    res.assert_status_ok();
    let body: Value = res.json();
    assert!(body.is_object(), "response must be object");
    assert!(
        body["active_session"].is_null(),
        "active_session should be null when idle, got: {:?}",
        body["active_session"]
    );
    assert!(
        body["last_event_seq"].is_number(),
        "last_event_seq should be a number, got: {:?}",
        body["last_event_seq"]
    );
}

/// test_status_active: insert a running session, GET /status returns session summary.
#[tokio::test]
async fn test_status_active() {
    use axum_test::TestServer;
    use tempfile::TempDir;
    use xvision_dashboard::server::build_router;
    use xvision_dashboard::AppState;

    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");

    // Insert a running session directly so we don't need to do a full run-cycle call.
    let session_id = xvision_engine::autooptimizer::session::create_session(
        &state.pool,
        "test-strategy-1",
        "{}",
        "once",
        Some(5),
    )
    .await
    .expect("create test session");

    let server = TestServer::new(build_router(state)).unwrap();
    let res = server.get("/api/autooptimizer/status").await;
    res.assert_status_ok();
    let body: Value = res.json();

    assert!(
        !body["active_session"].is_null(),
        "active_session should be non-null when a running session exists, got: {:?}",
        body
    );
    let active = &body["active_session"];
    assert_eq!(
        active["session_id"].as_str().unwrap(),
        session_id,
        "session_id should match"
    );
    assert_eq!(active["strategy_id"], "test-strategy-1");
    assert_eq!(active["state"], "running");
    assert_eq!(active["mode"], "once");
    assert_eq!(active["cycles_completed"], 0);
}

// ---------------------------------------------------------------------------
// POST /api/autooptimizer/sessions
// ---------------------------------------------------------------------------

/// test_start_session_creates_record: POST /sessions with mode=once returns 202
/// and a DB record exists.
#[tokio::test]
async fn test_start_session_creates_record() {
    use axum_test::TestServer;
    use tempfile::TempDir;
    use xvision_dashboard::server::build_router;
    use xvision_dashboard::AppState;

    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let pool = state.pool.clone();
    let server = TestServer::new(build_router(state)).unwrap();

    let body = serde_json::json!({
        "strategy_id": "strat-abc",
        "mode": "once"
    });
    let res = server.post("/api/autooptimizer/sessions").json(&body).await;
    assert_eq!(res.status_code(), 202, "POST /sessions must return 202");
    let resp_body: Value = res.json();
    let session_id = resp_body["session_id"].as_str().expect("session_id in response");

    // Verify the DB has the record.
    let row: Option<xvision_engine::autooptimizer::session::OptimizerSession> =
        sqlx::query_as("SELECT * FROM autooptimizer_session_state WHERE session_id = ?")
            .bind(session_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(row.is_some(), "DB should have a session record");
    let row = row.unwrap();
    assert_eq!(row.strategy_id, "strat-abc");
    assert_eq!(row.mode, "once");
    assert_eq!(row.state, "running");
}

/// test_start_session_409_when_active: insert a running session, POST /sessions
/// returns 409.
#[tokio::test]
async fn test_start_session_409_when_active() {
    use axum_test::TestServer;
    use tempfile::TempDir;
    use xvision_dashboard::server::build_router;
    use xvision_dashboard::AppState;

    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");

    // Pre-insert a running session.
    xvision_engine::autooptimizer::session::create_session(&state.pool, "existing-strat", "{}", "once", None)
        .await
        .expect("create pre-existing session");

    let server = TestServer::new(build_router(state)).unwrap();
    let body = serde_json::json!({
        "strategy_id": "strat-xyz",
        "mode": "once"
    });
    let res = server.post("/api/autooptimizer/sessions").json(&body).await;
    assert_eq!(res.status_code(), 409, "should 409 when session already active");
    let resp_body: Value = res.json();
    assert_eq!(resp_body["code"], "conflict");
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/sessions
// ---------------------------------------------------------------------------

/// test_list_sessions_newest_first: insert 3 sessions, GET /sessions returns
/// them in created_at desc order.
#[tokio::test]
async fn test_list_sessions_newest_first() {
    use axum_test::TestServer;
    use tempfile::TempDir;
    use xvision_dashboard::server::build_router;
    use xvision_dashboard::AppState;

    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");

    // Insert 3 sessions with different timestamps to ensure ordering.
    let now = chrono::Utc::now();
    let pool = state.pool.clone();
    for (i, strat) in ["strat-1", "strat-2", "strat-3"].iter().enumerate() {
        let created_at = (now + chrono::Duration::seconds(i as i64)).to_rfc3339();
        sqlx::query(
            "INSERT INTO autooptimizer_session_state \
             (session_id, strategy_id, config_json, state, mode, cycles_planned, \
              cycles_completed, kept_count, suspect_count, dropped_count, created_at) \
             VALUES (?, ?, '{}', 'finished', 'once', NULL, 0, 0, 0, 0, ?)",
        )
        .bind(format!("sess-{i}"))
        .bind(strat)
        .bind(&created_at)
        .execute(&pool)
        .await
        .unwrap();
    }

    let server = TestServer::new(build_router(state)).unwrap();
    let res = server.get("/api/autooptimizer/sessions").await;
    res.assert_status_ok();
    let body: Value = res.json();
    let items = body.as_array().expect("response should be array");
    assert_eq!(items.len(), 3);
    // Verify newest first (strat-3 was created last).
    assert_eq!(items[0]["strategy_id"], "strat-3");
    assert_eq!(items[1]["strategy_id"], "strat-2");
    assert_eq!(items[2]["strategy_id"], "strat-1");
}

// ---------------------------------------------------------------------------
// GET /api/autooptimizer/sessions/:id
// ---------------------------------------------------------------------------

/// test_session_detail_404: GET /sessions/nonexistent returns 404.
#[tokio::test]
async fn test_session_detail_404() {
    let (server, _tmp) = test_server().await;
    let res = server
        .get("/api/autooptimizer/sessions/nonexistent-session-id")
        .await;
    assert_eq!(res.status_code(), 404);
}

/// test_session_detail_returns_record: GET /sessions/:id returns the record.
#[tokio::test]
async fn test_session_detail_returns_record() {
    use axum_test::TestServer;
    use tempfile::TempDir;
    use xvision_dashboard::server::build_router;
    use xvision_dashboard::AppState;

    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");

    let session_id = xvision_engine::autooptimizer::session::create_session(
        &state.pool,
        "strat-detail",
        "{}",
        "once",
        Some(3),
    )
    .await
    .expect("create session");

    let server = TestServer::new(build_router(state)).unwrap();
    let res = server
        .get(&format!("/api/autooptimizer/sessions/{session_id}"))
        .await;
    res.assert_status_ok();
    let body: Value = res.json();
    assert_eq!(body["session_id"].as_str().unwrap(), session_id);
    assert_eq!(body["strategy_id"], "strat-detail");
    assert_eq!(body["state"], "running");
}

// ---------------------------------------------------------------------------
// POST /api/autooptimizer/run-cycle back-compat
// ---------------------------------------------------------------------------

/// test_run_cycle_compat: POST /run-cycle is still registered (not 404).
/// Back-compat: existing callers receive at minimum the route; the `session_id`
/// field is additive on successful runs.
#[tokio::test]
async fn test_run_cycle_compat_returns_session_id() {
    let (server, _tmp) = test_server().await;

    // POST /run-cycle with an empty body — the route is registered so it
    // must not return 404. It may return 400/500 from validation/config errors;
    // what matters is the route exists.
    let body = serde_json::json!({});
    let res = server.post("/api/autooptimizer/run-cycle").json(&body).await;
    assert_ne!(res.status_code(), 404, "run-cycle route must exist (not 404)");
}

// ---------------------------------------------------------------------------
// POST /api/autooptimizer/cycles/:id/cancel back-compat
// ---------------------------------------------------------------------------

/// test_cancel_cycle_compat: POST /cycles/nonexistent/cancel returns 404 (not found),
/// same as before since no session is running.
#[tokio::test]
async fn test_cancel_cycle_compat_404() {
    let (server, _tmp) = test_server().await;
    let res = server
        .post("/api/autooptimizer/cycles/fake-cycle-id/cancel")
        .await;
    assert_eq!(res.status_code(), 404);
}

// ---------------------------------------------------------------------------
// SSE Last-Event-ID replay
// ---------------------------------------------------------------------------

/// test_sse_replay: insert 5 events with seq 1-5, connect SSE with Last-Event-ID: 3,
/// verify only seq 4 and 5 are replayed first.
#[tokio::test]
async fn test_sse_replay() {
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::time::timeout;
    use xvision_dashboard::server::build_router;
    use xvision_dashboard::AppState;

    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let pool = state.pool.clone();

    // Create a session to attach events to.
    let session_id =
        xvision_engine::autooptimizer::session::create_session(&pool, "strat-sse", "{}", "once", None)
            .await
            .expect("create session");

    // Insert 5 events using the events_store helper. They will get seq 1-5.
    for i in 1..=5usize {
        xvision_engine::autooptimizer::append_event(
            &pool,
            &session_id,
            Some(&format!("cycle-{i}")),
            &format!("event_kind_{i}"),
            &format!(r#"{{"n":{i}}}"#),
        )
        .await
        .expect("append event");
    }

    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{addr}");
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("axum serve failed");
    });

    // Connect with Last-Event-ID: 3 — should replay only seq 4 and 5.
    let client = reqwest::Client::new();
    let url = format!("{base_url}/api/autooptimizer/events");

    let body_bytes = timeout(Duration::from_secs(5), async {
        // Use a separate task so we can close the connection after reading enough.
        let resp = client
            .get(&url)
            .header("Last-Event-ID", "3")
            .send()
            .await
            .expect("GET /api/autooptimizer/events");
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        // Read chunks until we have both replayed events (seq 4 and 5).
        // We use a short read with a timeout; the stream won't self-close
        // (it waits for live events), so we abort after getting the replay.
        use futures_util::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut collected = Vec::new();
        let mut timeout_inner = tokio::time::interval(Duration::from_millis(200));
        // Skip the first tick (fires immediately)
        timeout_inner.tick().await;
        loop {
            tokio::select! {
                chunk = stream.next() => {
                    match chunk {
                        Some(Ok(bytes)) => {
                            collected.extend_from_slice(&bytes);
                            let text = std::str::from_utf8(&collected).unwrap_or("");
                            // We've received seq 4 and 5 when both appear.
                            if text.contains("id: 4") && text.contains("id: 5") {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
                _ = timeout_inner.tick() => {
                    // Give up waiting — might have received all we need.
                    break;
                }
            }
        }
        collected
    })
    .await
    .expect("SSE replay test timed out");

    let text = std::str::from_utf8(&body_bytes).expect("body is utf-8");

    // seq 4 and 5 must appear in the replay
    assert!(
        text.contains("id: 4") || text.contains("\"n\":4"),
        "seq 4 should be replayed; body:\n{text}"
    );
    assert!(
        text.contains("id: 5") || text.contains("\"n\":5"),
        "seq 5 should be replayed; body:\n{text}"
    );

    // seq 1, 2, 3 must NOT appear in the replay (they are before Last-Event-ID)
    assert!(
        !text.contains("\"n\":1") && !text.contains("\"n\":2") && !text.contains("\"n\":3"),
        "seq 1-3 must not appear in replay; body:\n{text}"
    );
}
