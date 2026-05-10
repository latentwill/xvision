//! Integration tests for the chat rail REST surface:
//!   POST   /api/chat-rail/sessions
//!   GET    /api/chat-rail/sessions/:id/history
//!   POST   /api/chat-rail/sessions/:id/scope
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
async fn create_session_returns_session_id() {
    let (server, _tmp, _state) = boot().await;
    let resp = server
        .post("/api/chat-rail/sessions")
        .json(&json!({"scope": {"scope": "workspace"}}))
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    let id = body["session_id"].as_str().expect("session_id present");
    assert!(!id.is_empty());
}

#[tokio::test]
async fn create_session_persists_scope() {
    let (server, _tmp, state) = boot().await;
    let resp = server
        .post("/api/chat-rail/sessions")
        .json(&json!({"scope": {"scope": "run", "run_id": "01HABC"}}))
        .await;
    resp.assert_status_ok();
    let id = resp.json::<Value>()["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let scope = ChatSessionStore::load_scope(&state.pool, &id).await.unwrap();
    assert_eq!(
        scope,
        ContextScope::Run {
            run_id: "01HABC".into(),
        }
    );
}

#[tokio::test]
async fn history_returns_empty_array_for_fresh_session() {
    let (server, _tmp, state) = boot().await;
    let id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    let resp = server
        .get(&format!("/api/chat-rail/sessions/{id}/history"))
        .await;
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
    ChatSessionStore::append(
        &state.pool,
        &id,
        "user",
        &[json!({"type":"text","text":"hi"})],
    )
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

    let resp = server
        .get(&format!("/api/chat-rail/sessions/{id}/history"))
        .await;
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
async fn update_scope_changes_persisted_scope() {
    let (server, _tmp, state) = boot().await;
    let id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();

    let resp = server
        .post(&format!("/api/chat-rail/sessions/{id}/scope"))
        .json(&json!({"scope": "strategy", "draft_id": "btc-mom"}))
        .await;
    assert_eq!(resp.status_code(), StatusCode::NO_CONTENT);

    let scope = ChatSessionStore::load_scope(&state.pool, &id).await.unwrap();
    assert_eq!(
        scope,
        ContextScope::Strategy {
            draft_id: "btc-mom".into(),
        }
    );
}

#[tokio::test]
async fn update_scope_404s_for_unknown_session() {
    let (server, _tmp, _state) = boot().await;
    let resp = server
        .post("/api/chat-rail/sessions/does-not-exist/scope")
        .json(&json!({"scope": "workspace"}))
        .await;
    assert_eq!(resp.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_session_clears_history() {
    let (server, _tmp, state) = boot().await;
    let id = ChatSessionStore::create_session(&state.pool, &ContextScope::Workspace)
        .await
        .unwrap();
    ChatSessionStore::append(
        &state.pool,
        &id,
        "user",
        &[json!({"type":"text","text":"hi"})],
    )
    .await
    .unwrap();

    let resp = server
        .delete(&format!("/api/chat-rail/sessions/{id}"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::NO_CONTENT);

    // Deleted: load_scope errors, history is empty (cascade dropped rows).
    let load = ChatSessionStore::load_scope(&state.pool, &id).await;
    assert!(load.is_err(), "session should be gone");
    let history = ChatSessionStore::load_history(&state.pool, &id)
        .await
        .unwrap();
    assert_eq!(history.len(), 0);
}
