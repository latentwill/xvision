//! Integration test for `GET /api/agent-runs/:id/stream`.
//!
//! Boots a real `axum::serve` on an ephemeral port, publishes a synthetic
//! run on the observability bus, then connects an SSE client over
//! `reqwest` streaming and asserts the snapshot + at least one
//! incremental event arrive before the stream closes on `RunFinished`.

use std::time::Duration;

use chrono::Utc;
use futures_util::StreamExt;
use tempfile::TempDir;
use tokio::time::timeout;
use xvision_dashboard::{server::build_router, AppState};
use xvision_observability::types::{RunStatus, SpanKind};
use xvision_observability::{
    ModelCallFinishedEvent, RunEvent, RunFinishedEvent, RunStartedEvent, SpanStartedEvent,
};

async fn boot_server() -> (String, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let router = build_router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{addr}");

    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("axum serve failed");
    });

    (base_url, tmp, state)
}

/// Insert a minimal `agent_runs` row so `build_export` can produce a
/// snapshot for the stream's first event. We bypass the bus on this
/// initial write to keep the test focused on the SSE path.
async fn seed_run_row(state: &AppState, run_id: &str) {
    let started_at = Utc::now();
    sqlx::query(
        "INSERT INTO agent_runs (id, objective, status, started_at, retention_mode, sidecar_version, cline_sdk_version, protocol_version)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(run_id)
    .bind("integration test run")
    .bind("running")
    .bind(started_at.to_rfc3339())
    .bind("summary")
    .bind("test-sidecar")
    .bind("test-cline")
    .bind("xvision/1")
    .execute(&state.pool)
    .await
    .expect("seed agent_runs row");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sse_stream_emits_snapshot_then_live_event_then_closes() {
    let (base_url, _tmp, state) = boot_server().await;

    let run_id = "run_sse_int_01";
    seed_run_row(&state, run_id).await;

    // Subscribe BEFORE we open the HTTP request would race; instead,
    // open the request first and emit events from a spawned task once
    // the server has had a moment to subscribe.
    let bus = state.obs_event_bus.clone();
    let run_id_emit = run_id.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        bus.publish(RunEvent::RunStarted(RunStartedEvent {
            run_id: run_id_emit.clone(),
            objective: "integration test run".into(),
            strategy_id: None,
            eval_run_id: None,
            source_cli_job_id: None,
            started_at: Utc::now(),
            retention_mode: "summary".into(),
            sidecar_version: Some("test-sidecar".into()),
            cline_sdk_version: Some("test-cline".into()),
            protocol_version: Some("xvision/1".into()),
            skills_json: None,
            mcp_servers_json: None,
        }))
        .await;
        bus.publish(RunEvent::SpanStarted(SpanStartedEvent {
            span_id: "span_run".into(),
            run_id: run_id_emit.clone(),
            parent_span_id: None,
            kind: SpanKind::AgentRun,
            name: "agent.run".into(),
            started_at: Utc::now(),
            otel_trace_id: None,
            otel_span_id: None,
            attributes_json: None,
        }))
        .await;
        bus.publish(RunEvent::ModelCallFinished(ModelCallFinishedEvent {
            span_id: "span_run".into(),
            provider: "test-provider".into(),
            model: "test-model".into(),
            input_token_count: Some(5),
            output_token_count: Some(3),
            cost_usd: None,
            prompt_hash: "sha256:prompt".into(),
            response_hash: Some("sha256:response".into()),
            prompt_payload_ref: None,
            response_payload_ref: None,
            tool_calls_requested: None,
            capability_path: None,
        }))
        .await;
        // Closing lifecycle event so the SSE handler ends gracefully.
        bus.publish(RunEvent::RunFinished(RunFinishedEvent {
            run_id: run_id_emit.clone(),
            finished_at: Utc::now(),
            status: RunStatus::Completed,
            final_artifact_id: None,
            error: None,
        }))
        .await;
    });

    let url = format!("{base_url}/api/agent-runs/{run_id}/stream");
    let client = reqwest::Client::new();

    let body_text = timeout(Duration::from_secs(8), async {
        let resp = client
            .get(&url)
            .send()
            .await
            .expect("GET /api/agent-runs/.../stream");
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            content_type.contains("text/event-stream"),
            "expected text/event-stream content-type, got: {content_type}"
        );
        // Stream the body chunk-by-chunk, breaking once we have both
        // the snapshot and a terminal `run_finished` event. We can't
        // use `resp.bytes().await` here because keep-alive comments
        // would extend the body indefinitely if the handler didn't
        // close — but it does close on `RunFinished`, so collecting
        // until end-of-stream is also fine and simpler. Use the
        // streaming API to avoid waiting on a 15s keep-alive.
        let mut stream = resp.bytes_stream();
        let mut acc: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.expect("read chunk");
            acc.extend_from_slice(&bytes);
            let text = std::str::from_utf8(&acc).unwrap_or("");
            if text.contains("event: snapshot") && text.contains("event: run_finished") {
                break;
            }
        }
        String::from_utf8(acc).expect("body utf-8")
    })
    .await
    .expect("SSE stream did not deliver expected events within 8 s");

    assert!(
        body_text.contains("event: snapshot"),
        "expected 'event: snapshot' in body; got:\n{body_text}"
    );
    assert!(
        body_text.contains("event: run_started") || body_text.contains("event: span_started"),
        "expected at least one live event (run_started or span_started) in body; got:\n{body_text}"
    );
    assert!(
        body_text.contains("event: model_call_finished"),
        "expected span-scoped model_call_finished in body; got:\n{body_text}"
    );
    assert!(
        body_text.contains("event: run_finished"),
        "expected 'event: run_finished' terminator in body; got:\n{body_text}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sse_stream_returns_404_for_unknown_run() {
    let (base_url, _tmp, _state) = boot_server().await;
    let url = format!("{base_url}/api/agent-runs/run_does_not_exist/stream");
    let client = reqwest::Client::new();
    let resp = timeout(Duration::from_secs(5), client.get(&url).send())
        .await
        .expect("request did not return")
        .expect("GET /api/agent-runs/.../stream");
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
}
