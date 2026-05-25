//! Integration tests for the Phase 1.2 unified session stream:
//!   GET /api/chat-rail/sessions/:id/stream?after_seq=<n>
//!
//! Proves reconnect/resume by session_id end-to-end:
//!   1. A finished session's UnifiedEvents are appended directly via
//!      `SessionEventLog` (simulating a completed chat turn).
//!   2. `SessionEventLog::load_after(session, 2)` — the resume primitive —
//!      returns only events with seq > 2.
//!   3. The live SSE endpoint, opened with `?after_seq=2`, replays exactly
//!      those events (named by payload kind), emits `replay_complete` with the
//!      last replayed seq, then tails a live event published on the session
//!      bus and terminates on the terminal event.
//!
//! Uses `AppState::new` against a freshly-migrated tempdir DB, so migration
//! `042_session_events.sql` is exercised through `sqlx::migrate!` (not inline
//! DDL) here.

use std::time::Duration;

use chrono::Utc;
use tempfile::TempDir;
use tokio::time::timeout;
use xvision_dashboard::{server::build_router, AppState};
use xvision_engine::chat_session::{ChatSessionStore, ContextScope, SessionEventLog};
use xvision_observability::{
    Actor, EventScope, EventSource, UnifiedEvent, UnifiedPayload,
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

fn token_event(session_id: &str, seq: u64, text: &str) -> UnifiedEvent {
    UnifiedEvent {
        event_id: format!("ev_{seq}"),
        session_id: Some(session_id.into()),
        run_id: None,
        span_id: None,
        parent_event_id: None,
        seq,
        ts: Utc::now(),
        scope: EventScope::workspace(),
        actor: Actor::Agent,
        source: EventSource::ChatRail,
        blob_hash: None,
        payload: UnifiedPayload::AssistantTokenDelta { text: text.into() },
    }
}

fn completed_event(session_id: &str, seq: u64) -> UnifiedEvent {
    UnifiedEvent {
        event_id: format!("ev_{seq}"),
        session_id: Some(session_id.into()),
        run_id: None,
        span_id: None,
        parent_event_id: None,
        seq,
        ts: Utc::now(),
        scope: EventScope::workspace(),
        actor: Actor::System,
        source: EventSource::ChatRail,
        blob_hash: None,
        payload: UnifiedPayload::SessionCompleted,
    }
}

/// The resume primitive in isolation, against the real migrated DB:
/// load_after(session, 2) returns only seq > 2, ascending.
#[tokio::test]
async fn load_after_resumes_from_cursor() {
    let (_url, _tmp, state) = boot_server().await;
    let sid = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    for (i, t) in ["a", "b", "c", "d", "e"].iter().enumerate() {
        SessionEventLog::append(&state.pool, &token_event(&sid, i as u64, t))
            .await
            .unwrap();
    }

    let resumed = SessionEventLog::load_after(&state.pool, &sid, 2).await.unwrap();
    let seqs: Vec<u64> = resumed.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![3, 4], "resume from cursor 2 yields only seq > 2");

    // Whole-log replay for a fresh consumer.
    let all = SessionEventLog::load_after(&state.pool, &sid, -1).await.unwrap();
    assert_eq!(all.len(), 5);
}

/// Full reconnect path over the live SSE endpoint: replay seg (filtered by
/// after_seq) → replay_complete → live tail → terminal close.
#[tokio::test]
async fn stream_replays_after_cursor_then_tails_live() {
    let (base_url, _tmp, state) = boot_server().await;
    let sid = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();

    // Persist a finished turn: seq 0..=4.
    for (i, t) in ["a", "b", "c", "d", "e"].iter().enumerate() {
        SessionEventLog::append(&state.pool, &token_event(&sid, i as u64, t))
            .await
            .unwrap();
    }

    let url = format!("{base_url}/api/chat-rail/sessions/{sid}/stream?after_seq=2");
    let client = reqwest::Client::new();

    let bus = state.session_event_bus.clone();
    let publish_sid = sid.clone();
    // Once the handler has subscribed, publish a live token (seq 5) and then a
    // terminal SessionCompleted (seq 6) so the stream tail closes.
    let publisher = tokio::spawn(async move {
        timeout(Duration::from_secs(2), async {
            loop {
                if bus.subscriber_count(&publish_sid).await > 0 {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("stream handler did not subscribe");
        bus.publish(&token_event(&publish_sid, 5, "live")).await;
        bus.publish(&completed_event(&publish_sid, 6)).await;
    });

    let body = timeout(Duration::from_secs(5), async move {
        let resp = client.get(&url).send().await.expect("GET stream");
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.contains("text/event-stream"), "got content-type: {ct}");
        resp.text().await.expect("read stream body")
    })
    .await
    .expect("stream did not close within timeout");

    publisher.await.unwrap();

    // Parse the SSE frames into (event, data) pairs.
    let frames = parse_sse(&body);
    let names: Vec<&str> = frames.iter().map(|(e, _)| e.as_str()).collect();

    // Replay segment: only seq 3 and 4 (after_seq=2), each named by payload
    // kind, then replay_complete, then the live token, then session_completed.
    assert_eq!(
        names,
        vec![
            "assistant_token_delta", // seq 3
            "assistant_token_delta", // seq 4
            "replay_complete",
            "assistant_token_delta", // seq 5 (live)
            "session_completed",     // seq 6 (terminal, closes stream)
        ],
        "frame order; full body was:\n{body}"
    );

    // replay_complete carries the last replayed seq (4).
    let (_, complete_data) = frames
        .iter()
        .find(|(e, _)| e == "replay_complete")
        .expect("replay_complete frame present");
    let v: serde_json::Value = serde_json::from_str(complete_data).unwrap();
    assert_eq!(v["last_seq"].as_i64().unwrap(), 4);

    // The two replayed event frames are the persisted seq 3 and 4 in order.
    let replayed_seqs: Vec<u64> = frames
        .iter()
        .take(2)
        .map(|(_, d)| {
            serde_json::from_str::<serde_json::Value>(d).unwrap()["seq"]
                .as_u64()
                .unwrap()
        })
        .collect();
    assert_eq!(replayed_seqs, vec![3, 4]);
}

/// Deleting the session cascades its persisted events (migration 042 FK).
#[tokio::test]
async fn delete_session_cascades_persisted_events() {
    let (_url, _tmp, state) = boot_server().await;
    let sid = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    SessionEventLog::append(&state.pool, &token_event(&sid, 0, "x"))
        .await
        .unwrap();

    ChatSessionStore::delete_session(&state.pool, &sid).await.unwrap();

    let remaining = SessionEventLog::load_after(&state.pool, &sid, -1).await.unwrap();
    assert!(remaining.is_empty(), "events cascade-deleted with the session");
}

/// Minimal SSE frame parser: splits on blank lines, reads `event:` + `data:`
/// lines. Ignores keep-alive comment lines (`: keep-alive`).
fn parse_sse(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for block in body.split("\n\n") {
        let mut event = None;
        let mut data = None;
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("event:") {
                event = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("data:") {
                data = Some(rest.trim().to_string());
            }
        }
        if let (Some(e), Some(d)) = (event, data) {
            out.push((e, d));
        }
    }
    out
}
