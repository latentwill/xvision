//! Integration tests for persisted memory flywheel event projections.

mod support;

use axum_test::TestServer;
use support::state_with_dashboard_migrations;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

async fn server_with_state() -> (TestServer, tempfile::TempDir, AppState) {
    let (state, tmp) = state_with_dashboard_migrations().await;
    let server = TestServer::new(build_router(state.clone())).expect("test server");
    (server, tmp, state)
}

async fn seed_run(state: &AppState, run_id: &str) {
    // Some migrated test DBs still have an `agent_runs.eval_run_id`
    // foreign-key reference pointing at the transient live-migration
    // table name. These route tests do not use eval_run_id; create the
    // empty table so SQLite can validate the FK metadata while we seed
    // a minimal run row.
    sqlx::query("CREATE TABLE IF NOT EXISTS eval_runs_old_live_migration (id TEXT PRIMARY KEY)")
        .execute(&state.pool)
        .await
        .expect("compat eval_runs_old_live_migration table");
    sqlx::query(
        "INSERT INTO agent_runs (id, objective, status, started_at, retention_mode) \
         VALUES (?1, 'memory events route test', 'completed', '2026-05-25T00:00:00Z', 'hash_only')",
    )
    .bind(run_id)
    .execute(&state.pool)
    .await
    .expect("seed agent_runs row");
}

async fn seed_event(state: &AppState, id: &str, run_id: &str, kind: &str, payload: &str, created_at: &str) {
    sqlx::query(
        "INSERT INTO events (id, run_id, span_id, kind, payload_json, created_at) \
         VALUES (?1, ?2, NULL, ?3, ?4, ?5)",
    )
    .bind(id)
    .bind(run_id)
    .bind(kind)
    .bind(payload)
    .bind(created_at)
    .execute(&state.pool)
    .await
    .expect("seed event row");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn memory_events_route_returns_recall_and_write_events() {
    let (server, _tmp, state) = server_with_state().await;
    let run_id = "run_memory_events_route";
    seed_run(&state, run_id).await;
    seed_event(
        &state,
        "evt_write",
        run_id,
        "memory_write",
        r#"{"run_id":"run_memory_events_route","flywheel_cycle_id":"run_memory_events_route:2","decision_id":2,"namespace":"agent:A","memory_item_id":"obs_2","text_preview":"remembered"}"#,
        "2026-05-25T00:00:02Z",
    )
    .await;
    seed_event(
        &state,
        "evt_recall",
        run_id,
        "memory_recall",
        r#"{"run_id":"run_memory_events_route","flywheel_cycle_id":"run_memory_events_route:1","decision_id":1,"namespace":"agent:A","items":[{"id":"pat_1","score":0.9,"text_preview":"case law"}]}"#,
        "2026-05-25T00:00:01Z",
    )
    .await;
    seed_event(
        &state,
        "evt_other",
        run_id,
        "decision_started",
        r#"{"decision_id":0}"#,
        "2026-05-25T00:00:00Z",
    )
    .await;

    let resp = server
        .get(&format!("/api/agent-runs/{run_id}/memory-events"))
        .await;
    assert_eq!(resp.status_code(), 200);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["run_id"], run_id);
    let events = body["events"].as_array().expect("events array");
    assert_eq!(events.len(), 2, "non-memory events must be excluded: {body}");
    assert_eq!(events[0]["kind"], "memory_recall");
    assert_eq!(
        events[0]["payload"]["flywheel_cycle_id"],
        "run_memory_events_route:1"
    );
    assert_eq!(events[0]["payload"]["items"][0]["id"], "pat_1");
    assert_eq!(events[1]["kind"], "memory_write");
    assert_eq!(events[1]["payload"]["memory_item_id"], "obs_2");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn memory_recalls_route_is_wired() {
    let (server, _tmp, state) = server_with_state().await;
    let run_id = "run_memory_recalls_route";
    seed_run(&state, run_id).await;
    seed_event(
        &state,
        "evt_recall_only",
        run_id,
        "memory_recall",
        r#"{"run_id":"run_memory_recalls_route","flywheel_cycle_id":"run_memory_recalls_route:7","decision_id":7,"namespace":"global","items":[]}"#,
        "2026-05-25T00:00:07Z",
    )
    .await;

    let resp = server
        .get(&format!("/api/agent-runs/{run_id}/memory-recalls"))
        .await;
    assert_eq!(resp.status_code(), 200);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["run_id"], run_id);
    assert_eq!(body["recalls"][0]["decision_id"], 7);
    assert_eq!(
        body["recalls"][0]["flywheel_cycle_id"],
        "run_memory_recalls_route:7"
    );
}
