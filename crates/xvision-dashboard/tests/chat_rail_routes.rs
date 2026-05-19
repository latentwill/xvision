//! Integration tests for the chat rail REST surface:
//!   POST   /api/chat-rail/sessions
//!   GET    /api/chat-rail/sessions
//!   POST   /api/chat-rail/sessions/resolve
//!   GET    /api/chat-rail/sessions/:id/history
//!   DELETE /api/chat-rail/sessions/:id
//!
//! The SSE `POST /api/chat-rail/chat` route requires `ANTHROPIC_API_KEY`
//! and a real LLM call, so it's covered indirectly by the WizardLoop unit
//! tests (which exercise the same code path with `MockDispatch`). Here we
//! lock the session-lifecycle endpoints that the React rail will call.

use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::{json, Value};
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::chat_session::{ChatSessionStore, ContextScope};

async fn boot() -> (TestServer, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, tmp, state)
}

#[tokio::test]
async fn resolve_creates_session_on_first_call() {
    let (server, _tmp, _state) = boot().await;
    let resp = server
        .post("/api/chat-rail/sessions/resolve")
        .json(&json!({"scope": {"scope": "workspace"}}))
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    let id = body["session_id"].as_str().expect("session_id present");
    assert!(!id.is_empty());
    assert_eq!(
        body["history"].as_array().expect("history array").len(),
        0,
        "fresh resolve has empty history"
    );
}

#[tokio::test]
async fn create_session_always_starts_new_history_without_deleting_old_session() {
    let (server, _tmp, state) = boot().await;
    let scope = ContextScope::Route {
        route: "/strategies".into(),
    };
    let old_id = ChatSessionStore::create_session(&state.pool, &scope)
        .await
        .unwrap();
    ChatSessionStore::append(
        &state.pool,
        &old_id,
        "user",
        &[json!({"type":"text","text":"previous question"})],
    )
    .await
    .unwrap();

    let resp = server
        .post("/api/chat-rail/sessions")
        .json(&json!({"scope": {"scope": "route", "route": "/strategies"}}))
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    let new_id = body["session_id"].as_str().expect("new id present");
    assert_ne!(new_id, old_id, "new chat creates a distinct session");
    assert_eq!(body["history"].as_array().unwrap().len(), 0);

    let old_history = ChatSessionStore::load_history(&state.pool, &old_id)
        .await
        .unwrap();
    assert_eq!(old_history.len(), 1, "old conversation stays available");
}

#[tokio::test]
async fn list_sessions_returns_summaries_newest_first() {
    let (server, _tmp, state) = boot().await;
    let route_scope = ContextScope::Route {
        route: "/strategies".into(),
    };
    let run_scope = ContextScope::Run {
        run_id: "01HABC".into(),
    };
    let older_id = ChatSessionStore::create_session(&state.pool, &route_scope)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let newer_id = ChatSessionStore::create_session(&state.pool, &run_scope)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    ChatSessionStore::append(
        &state.pool,
        &older_id,
        "user",
        &[json!({"type":"text","text":"bring this session forward"})],
    )
    .await
    .unwrap();

    let resp = server.get("/api/chat-rail/sessions").await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    let sessions = body.as_array().expect("sessions array");
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0]["id"], older_id);
    assert_eq!(
        sessions[0]["scope"],
        json!({"scope": "route", "route": "/strategies"})
    );
    assert!(sessions[0]["started_at"].as_str().is_some());
    assert!(sessions[0]["last_activity_at"].as_str().is_some());
    assert_eq!(sessions[1]["id"], newer_id);
    assert_eq!(sessions[1]["scope"], json!({"scope": "run", "run_id": "01HABC"}));
}

#[tokio::test]
async fn resolve_returns_same_session_for_same_scope() {
    let (server, _tmp, _state) = boot().await;
    let scope = json!({"scope": "run", "run_id": "01HABC"});

    let first = server
        .post("/api/chat-rail/sessions/resolve")
        .json(&json!({"scope": scope}))
        .await
        .json::<Value>();
    let first_id = first["session_id"].as_str().unwrap().to_string();

    let second = server
        .post("/api/chat-rail/sessions/resolve")
        .json(&json!({"scope": scope}))
        .await
        .json::<Value>();
    let second_id = second["session_id"].as_str().unwrap().to_string();
    assert_eq!(first_id, second_id, "same scope → same session");
}

#[tokio::test]
async fn resolve_setup_route_session_survives_reload() {
    let (server, _tmp, _state) = boot().await;
    let scope = json!({"scope": "route", "route": "/setup"});

    let first = server
        .post("/api/chat-rail/sessions/resolve")
        .json(&json!({"scope": scope}))
        .await
        .json::<Value>();
    let first_id = first["session_id"].as_str().unwrap().to_string();

    let second = server
        .post("/api/chat-rail/sessions/resolve")
        .json(&json!({"scope": scope}))
        .await
        .json::<Value>();
    let second_id = second["session_id"].as_str().unwrap().to_string();

    assert_eq!(first_id, second_id, "/setup should resolve a stable session");
}

#[tokio::test]
async fn resolve_persists_scope_and_returns_history() {
    let (server, _tmp, state) = boot().await;
    let resp = server
        .post("/api/chat-rail/sessions/resolve")
        .json(&json!({"scope": {"scope": "run", "run_id": "01HABC"}}))
        .await;
    resp.assert_status_ok();
    let id = resp.json::<Value>()["session_id"].as_str().unwrap().to_string();
    let scope = ChatSessionStore::load_scope(&state.pool, &id).await.unwrap();
    assert_eq!(
        scope,
        ContextScope::Run {
            run_id: "01HABC".into(),
        }
    );

    // Append a message and re-resolve — the response now includes it.
    ChatSessionStore::append(&state.pool, &id, "user", &[json!({"type":"text","text":"hi"})])
        .await
        .unwrap();
    let after = server
        .post("/api/chat-rail/sessions/resolve")
        .json(&json!({"scope": {"scope": "run", "run_id": "01HABC"}}))
        .await
        .json::<Value>();
    assert_eq!(after["session_id"].as_str().unwrap(), id);
    let history = after["history"].as_array().unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["role"], "user");
}

#[tokio::test]
async fn history_returns_empty_array_for_fresh_session() {
    let (server, _tmp, state) = boot().await;
    let id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    let resp = server.get(&format!("/api/chat-rail/sessions/{id}/history")).await;
    resp.assert_status_ok();
    let history: Value = resp.json();
    assert_eq!(history.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn history_returns_persisted_messages_in_seq_order() {
    let (server, _tmp, state) = boot().await;
    let id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    ChatSessionStore::append(&state.pool, &id, "user", &[json!({"type":"text","text":"hi"})])
        .await
        .unwrap();
    ChatSessionStore::append(
        &state.pool,
        &id,
        "assistant",
        &[json!({"type":"text","text":"hey"})],
    )
    .await
    .unwrap();

    let resp = server.get(&format!("/api/chat-rail/sessions/{id}/history")).await;
    resp.assert_status_ok();
    let history: Value = resp.json();
    let arr = history.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["role"], "user");
    assert_eq!(arr[0]["seq"], 0);
    assert_eq!(arr[1]["role"], "assistant");
    assert_eq!(arr[1]["seq"], 1);
}

#[tokio::test]
async fn delete_session_clears_history() {
    let (server, _tmp, state) = boot().await;
    let id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    ChatSessionStore::append(&state.pool, &id, "user", &[json!({"type":"text","text":"hi"})])
        .await
        .unwrap();

    let resp = server.delete(&format!("/api/chat-rail/sessions/{id}")).await;
    assert_eq!(resp.status_code(), StatusCode::NO_CONTENT);

    // Deleted: load_scope errors, history is empty (cascade dropped rows).
    let load = ChatSessionStore::load_scope(&state.pool, &id).await;
    assert!(load.is_err(), "session should be gone");
    let history = ChatSessionStore::load_history(&state.pool, &id).await.unwrap();
    assert_eq!(history.len(), 0);
}
