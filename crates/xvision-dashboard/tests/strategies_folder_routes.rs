//! Integration tests for `/api/strategies-folder/*` routes.
//!
//! Coverage (smoke + key edge cases):
//! - `GET /api/strategies-folder/list` returns `{ items: [] }` on an
//!   uninitialised workspace.
//! - `POST /api/strategies-folder/import` accepts a multipart `.md`
//!   upload and the entry lands under `notes/`.
//! - `POST /api/strategies-folder/import` rejects a `.exe` upload with
//!   400 + `type_not_allowed`.

use axum_test::multipart::{MultipartForm, Part};
use axum_test::TestServer;
use serde_json::Value;
use tempfile::TempDir;

use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

async fn boot() -> (TestServer, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state)).unwrap();
    (server, tmp)
}

#[tokio::test]
async fn list_returns_empty_on_fresh_workspace() {
    let (server, _tmp) = boot().await;
    let res = server.get("/api/strategies-folder/list").await;
    res.assert_status_ok();
    let body: Value = res.json();
    assert!(body["items"].is_array());
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn import_markdown_lands_in_notes() {
    let (server, _tmp) = boot().await;
    let part = Part::bytes(b"# hello\n\nbody\n".to_vec())
        .file_name("hello.md")
        .mime_type("text/markdown");
    let form = MultipartForm::new().add_part("file", part);
    let res = server.post("/api/strategies-folder/import").multipart(form).await;
    res.assert_status_ok();
    let body: Value = res.json();
    assert_eq!(body["entry"]["rel_path"], "notes/hello.md");
    assert_eq!(body["entry"]["kind"], "markdown");
    assert!(body["summary"].is_null(), "md should not have a sidecar");
}

#[tokio::test]
async fn import_rejects_disallowed_type() {
    let (server, _tmp) = boot().await;
    let part = Part::bytes(b"MZ\x90\x00".to_vec())
        .file_name("nope.exe")
        .mime_type("application/octet-stream");
    let form = MultipartForm::new().add_part("file", part);
    let res = server.post("/api/strategies-folder/import").multipart(form).await;
    assert_eq!(res.status_code(), 400);
    let body: Value = res.json();
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .contains("type_not_allowed"),
        "expected type_not_allowed, got: {body}"
    );
}

#[tokio::test]
async fn import_then_list_reflects_uploaded_file() {
    let (server, _tmp) = boot().await;
    let part = Part::bytes(b"some plain text".to_vec())
        .file_name("scratch.txt")
        .mime_type("text/plain");
    let form = MultipartForm::new().add_part("file", part);
    server
        .post("/api/strategies-folder/import")
        .multipart(form)
        .await
        .assert_status_ok();

    let res = server.get("/api/strategies-folder/list").await;
    res.assert_status_ok();
    let body: Value = res.json();
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["rel_path"], "notes/scratch.txt");
}

#[tokio::test]
async fn import_csv_generates_sidecar() {
    let (server, _tmp) = boot().await;
    let part = Part::bytes(b"a,b\n1,2\n3,4\n".to_vec())
        .file_name("rows.csv")
        .mime_type("text/csv");
    let form = MultipartForm::new().add_part("file", part);
    let res = server.post("/api/strategies-folder/import").multipart(form).await;
    res.assert_status_ok();
    let body: Value = res.json();
    assert_eq!(body["entry"]["rel_path"], "docs/rows.csv");
    assert_eq!(body["summary"]["rel_path"], "docs/rows.summary.md");
}

#[tokio::test]
async fn import_respects_explicit_to_subfolder() {
    let (server, _tmp) = boot().await;
    let part = Part::bytes(b"# note\n".to_vec())
        .file_name("note.md")
        .mime_type("text/markdown");
    let form = MultipartForm::new().add_part("file", part).add_text("to", "docs");
    let res = server.post("/api/strategies-folder/import").multipart(form).await;
    res.assert_status_ok();
    let body: Value = res.json();
    assert_eq!(body["entry"]["rel_path"], "docs/note.md");
}

#[tokio::test]
async fn import_missing_file_part_returns_400() {
    let (server, _tmp) = boot().await;
    let form = MultipartForm::new().add_text("to", "notes");
    let res = server.post("/api/strategies-folder/import").multipart(form).await;
    assert_eq!(res.status_code(), 400);
}
