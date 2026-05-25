//! Integration tests for the focus-chain REST surface (Phase 2.4):
//!   GET /api/chat-rail/focus?scope_kind=&scope_id=
//!   PUT /api/chat-rail/focus
//!
//! Locks: absent focus → found:false; save→load round-trips content + a
//! stable hash; an edit changes the hash; unsafe scope components are
//! rejected (no traversal out of `$XVN_HOME/scopes/`); a save with a
//! `session_id` appends a `FocusEdited` event to the session log.

use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::{json, Value};
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::chat_session::{ChatSessionStore, ContextScope, SessionEventLog};

async fn boot() -> (TestServer, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, tmp, state)
}

#[tokio::test]
async fn get_absent_focus_returns_found_false() {
    let (server, _tmp, _state) = boot().await;
    let resp = server
        .get("/api/chat-rail/focus")
        .add_query_param("scope_kind", "strategy")
        .add_query_param("scope_id", "btc-momentum")
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert_eq!(body["found"], json!(false));
    assert!(body["doc"].is_null());
}

#[tokio::test]
async fn put_then_get_round_trips_with_stable_hash() {
    let (server, _tmp, _state) = boot().await;
    let content = "# Focus\n\nKeep sizing conservative.\n";

    let put = server
        .put("/api/chat-rail/focus")
        .json(&json!({
            "scope_kind": "strategy",
            "scope_id": "btc-momentum",
            "content": content,
        }))
        .await;
    put.assert_status_ok();
    let saved: Value = put.json();
    assert_eq!(saved["content"], json!(content));
    let saved_hash = saved["content_hash"].as_str().unwrap().to_string();
    assert!(!saved_hash.is_empty());

    let get = server
        .get("/api/chat-rail/focus")
        .add_query_param("scope_kind", "strategy")
        .add_query_param("scope_id", "btc-momentum")
        .await;
    get.assert_status_ok();
    let body: Value = get.json();
    assert_eq!(body["found"], json!(true));
    assert_eq!(body["doc"]["content"], json!(content));
    assert_eq!(
        body["doc"]["content_hash"].as_str().unwrap(),
        saved_hash,
        "hash stable across save→load"
    );
}

#[tokio::test]
async fn edit_changes_the_hash() {
    let (server, _tmp, _state) = boot().await;

    let v1 = server
        .put("/api/chat-rail/focus")
        .json(&json!({"scope_kind": "run", "scope_id": "r1", "content": "first"}))
        .await;
    v1.assert_status_ok();
    let h1 = v1.json::<Value>()["content_hash"].as_str().unwrap().to_string();

    let v2 = server
        .put("/api/chat-rail/focus")
        .json(&json!({"scope_kind": "run", "scope_id": "r1", "content": "second"}))
        .await;
    v2.assert_status_ok();
    let h2 = v2.json::<Value>()["content_hash"].as_str().unwrap().to_string();

    assert_ne!(h1, h2, "edit must change the hash");
}

#[tokio::test]
async fn workspace_scope_without_id_round_trips() {
    let (server, _tmp, _state) = boot().await;
    let put = server
        .put("/api/chat-rail/focus")
        .json(&json!({"scope_kind": "workspace", "content": "ws focus"}))
        .await;
    put.assert_status_ok();

    let get = server
        .get("/api/chat-rail/focus")
        .add_query_param("scope_kind", "workspace")
        .await;
    get.assert_status_ok();
    let body: Value = get.json();
    assert_eq!(body["found"], json!(true));
    assert_eq!(body["doc"]["content"], json!("ws focus"));
}

#[tokio::test]
async fn put_rejects_path_traversal() {
    let (server, _tmp, _state) = boot().await;
    let resp = server
        .put("/api/chat-rail/focus")
        .json(&json!({
            "scope_kind": "strategy",
            "scope_id": "../../escape",
            "content": "x",
        }))
        .await;
    assert_eq!(
        resp.status_code(),
        StatusCode::BAD_REQUEST,
        "traversal must be rejected with 400"
    );
}

#[tokio::test]
async fn get_rejects_separator_in_scope_kind() {
    let (server, _tmp, _state) = boot().await;
    let resp = server
        .get("/api/chat-rail/focus")
        .add_query_param("scope_kind", "a/b")
        .await;
    assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn save_with_session_id_emits_focus_edited_event() {
    let (server, _tmp, state) = boot().await;
    let scope = ContextScope::Strategy {
        draft_id: "s1".into(),
    };
    let session_id = ChatSessionStore::create_session(&state.pool, &scope)
        .await
        .unwrap();

    let resp = server
        .put("/api/chat-rail/focus")
        .json(&json!({
            "scope_kind": "strategy",
            "scope_id": "s1",
            "content": "watch the funding rate",
            "session_id": session_id,
        }))
        .await;
    resp.assert_status_ok();

    // A FocusEdited event must now be in the session log.
    let events = SessionEventLog::load_after(&state.pool, &session_id, -1)
        .await
        .unwrap();
    assert_eq!(events.len(), 1, "exactly one focus_edited event appended");
    assert_eq!(events[0].event_name(), "focus_edited");
}
