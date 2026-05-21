//! Integration tests for `GET /api/agent-runs/:id/blobs/:ref`.
//!
//! Boots a real `axum::serve` on an ephemeral port, writes a blob to
//! the `xvn_home/agent_runs/blobs/` directory, seeds the corresponding
//! `model_calls` / `agent_runs` rows, then exercises the route's four
//! response codes (200 / 400 / 403 / 404).

mod support;

use support::live_server;
use xvision_dashboard::AppState;
use xvision_observability::BlobStore;

async fn seed_run(state: &AppState, run_id: &str, retention: &str) {
    sqlx::query(
        "INSERT INTO agent_runs (id, objective, status, started_at, retention_mode) \
         VALUES (?1, 'blob route test', 'completed', '2026-05-17T16:00:00Z', ?2)",
    )
    .bind(run_id)
    .bind(retention)
    .execute(&state.pool)
    .await
    .expect("seed agent_runs row");
}

async fn seed_span(state: &AppState, span_id: &str, run_id: &str) {
    sqlx::query(
        "INSERT INTO spans (id, run_id, kind, name, status, started_at) \
         VALUES (?1, ?2, 'model.call', 'm', 'ok', '2026-05-17T16:00:01Z')",
    )
    .bind(span_id)
    .bind(run_id)
    .execute(&state.pool)
    .await
    .expect("seed span");
}

async fn seed_model_call_with_prompt_ref(state: &AppState, span_id: &str, prompt_ref: &str) {
    sqlx::query(
        "INSERT INTO model_calls (span_id, provider, model, prompt_hash, prompt_payload_ref) \
         VALUES (?1, 'anthropic', 'claude', 'sha256:abc', ?2)",
    )
    .bind(span_id)
    .bind(prompt_ref)
    .execute(&state.pool)
    .await
    .expect("seed model_call");
}

async fn seed_model_call_with_response_ref(state: &AppState, span_id: &str, response_ref: &str) {
    sqlx::query(
        "INSERT INTO model_calls (span_id, provider, model, prompt_hash, response_hash, response_payload_ref) \
         VALUES (?1, 'anthropic', 'claude', 'sha256:abc', 'sha256:def', ?2)",
    )
    .bind(span_id)
    .bind(response_ref)
    .execute(&state.pool)
    .await
    .expect("seed model_call response ref");
}

async fn seed_tool_call_with_output_ref(state: &AppState, span_id: &str, output_ref: &str) {
    sqlx::query(
        "INSERT INTO tool_calls \
         (span_id, tool_name, input_hash, output_hash, output_payload_ref, side_effect_level, risk_level) \
         VALUES (?1, 'xvision_health_ping', 'sha256:abc', 'sha256:def', ?2, 'pure', 'safe_read')",
    )
    .bind(span_id)
    .bind(output_ref)
    .execute(&state.pool)
    .await
    .expect("seed tool_call output ref");
}

/// Write a payload through the production `BlobStore` so the test
/// hits the same hash algorithm + on-disk layout the dashboard reads.
fn write_blob(state: &AppState, payload: &[u8]) -> String {
    let blob_root = state.xvn_home.join("agent_runs").join("blobs");
    let store = BlobStore::new(blob_root);
    store.write(payload).expect("write blob").0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returns_200_with_blob_bytes_when_owned_by_run_and_retention_allows() {
    let (base_url, _tmp, state) = live_server().await;
    let run_id = "run_blob_200";
    let payload = b"hello world prompt body";

    seed_run(&state, run_id, "full_debug").await;
    seed_span(&state, "span_200", run_id).await;
    let blob_hex = write_blob(&state, payload);
    seed_model_call_with_prompt_ref(&state, "span_200", &blob_hex).await;

    let url = format!("{base_url}/api/agent-runs/{run_id}/blobs/{blob_hex}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap_or("")),
        Some("application/octet-stream"),
    );
    assert_eq!(
        resp.headers()
            .get(reqwest::header::CACHE_CONTROL)
            .map(|v| v.to_str().unwrap_or("")),
        Some("private, no-store"),
    );
    let body = resp.bytes().await.unwrap();
    assert_eq!(body.as_ref(), payload);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returns_200_when_owned_by_model_response_ref() {
    let (base_url, _tmp, state) = live_server().await;
    let run_id = "run_blob_model_response";
    let payload = b"hello world response body";

    seed_run(&state, run_id, "full_debug").await;
    seed_span(&state, "span_model_response", run_id).await;
    let blob_hex = write_blob(&state, payload);
    seed_model_call_with_response_ref(&state, "span_model_response", &blob_hex).await;

    let url = format!("{base_url}/api/agent-runs/{run_id}/blobs/{blob_hex}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), payload);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returns_200_when_owned_by_tool_output_ref() {
    let (base_url, _tmp, state) = live_server().await;
    let run_id = "run_blob_tool_output";
    let payload = b"hello world tool output";

    seed_run(&state, run_id, "full_debug").await;
    seed_span(&state, "span_tool_output", run_id).await;
    let blob_hex = write_blob(&state, payload);
    seed_tool_call_with_output_ref(&state, "span_tool_output", &blob_hex).await;

    let url = format!("{base_url}/api/agent-runs/{run_id}/blobs/{blob_hex}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), payload);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returns_400_for_non_hex_ref() {
    let (base_url, _tmp, _state) = live_server().await;
    // 64 chars but contains a `/` (URL-encoded) — most server routers
    // would split the path; we test with an obviously-wrong ref that
    // still routes (uppercase rejected; non-hex letter included).
    let bad_ref = "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ";
    let url = format!("{base_url}/api/agent-runs/run_any/blobs/{bad_ref}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returns_404_when_ref_not_owned_by_run() {
    let (base_url, _tmp, state) = live_server().await;
    let run_id = "run_blob_404a";
    let payload = b"orphan payload";

    seed_run(&state, run_id, "full_debug").await;
    // Write a blob but don't link it to any model_call/tool_call row
    // for `run_id`. The DB ownership check must refuse it.
    let blob_hex = write_blob(&state, payload);

    let url = format!("{base_url}/api/agent-runs/{run_id}/blobs/{blob_hex}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returns_404_when_blob_missing_on_disk_but_referenced_in_db() {
    let (base_url, _tmp, state) = live_server().await;
    let run_id = "run_blob_404b";

    seed_run(&state, run_id, "full_debug").await;
    seed_span(&state, "span_404b", run_id).await;
    // Reference a 64-hex ref that has no corresponding file on disk.
    let dangling = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    seed_model_call_with_prompt_ref(&state, "span_404b", dangling).await;

    let url = format!("{base_url}/api/agent-runs/{run_id}/blobs/{dangling}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returns_403_when_retention_is_hash_only() {
    let (base_url, _tmp, state) = live_server().await;
    let run_id = "run_blob_403";
    let payload = b"this should not be served";

    seed_run(&state, run_id, "hash_only").await;
    seed_span(&state, "span_403", run_id).await;
    let blob_hex = write_blob(&state, payload);
    // A misconfigured producer would not normally insert a payload ref
    // under hash_only retention, but the schema permits it. The route
    // must refuse to serve the body regardless.
    seed_model_call_with_prompt_ref(&state, "span_403", &blob_hex).await;

    let url = format!("{base_url}/api/agent-runs/{run_id}/blobs/{blob_hex}");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 403);
    let body_text = resp.text().await.unwrap();
    assert!(
        body_text.contains("hash_only"),
        "expected 403 body to mention hash_only retention, got {body_text}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returns_404_for_unknown_run_id() {
    let (base_url, _tmp, _state) = live_server().await;
    let blob_hex = "1111111111111111111111111111111111111111111111111111111111111111";
    let url = format!("{base_url}/api/agent-runs/run_does_not_exist/blobs/{blob_hex}");
    let resp = reqwest::get(&url).await.unwrap();
    // Unknown run + valid-shape ref → 404 (the ownership query returns
    // None for both "no such run" and "ref not owned").
    assert_eq!(resp.status(), 404);
}
