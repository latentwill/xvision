use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

async fn boot() -> (TestServer, TempDir) {
    let tmp = TempDir::new().unwrap();
    let cli = write_fake_cli(tmp.path());
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state")
        .with_cli_command_for_tests(cli);
    let server = TestServer::new(build_router(state)).unwrap();
    (server, tmp)
}

async fn boot_http() -> (String, TempDir, tokio::task::JoinHandle<()>) {
    let tmp = TempDir::new().unwrap();
    let cli = write_fake_cli(tmp.path());
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state")
        .with_cli_command_for_tests(cli);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = build_router(state);
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), tmp, handle)
}

fn write_fake_cli(root: &std::path::Path) -> std::path::PathBuf {
    let cli = root.join("fake-xvn");
    fs::write(
        &cli,
        "#!/bin/sh
case \"$1\" in
  help)
    echo 'Usage: xvn <COMMAND>'
    exit 0
    ;;
  slow)
    sleep 2
    echo 'slow done'
    exit 0
    ;;
  fail)
    echo 'failure from fake xvn' >&2
    exit 2
    ;;
  *)
    echo \"unknown:$1\" >&2
    exit 2
    ;;
esac
",
    )
    .unwrap();
    let mut perms = fs::metadata(&cli).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&cli, perms).unwrap();
    cli
}

async fn wait_for_terminal_status(server: &TestServer, job_id: &str) -> serde_json::Value {
    for _ in 0..160 {
        let path = format!("/api/cli/jobs/{job_id}");
        let meta = server.get(&path).await;
        meta.assert_status_ok();
        let body: serde_json::Value = meta.json();
        let status = body["status"].as_str().unwrap_or("");
        if matches!(status, "succeeded" | "failed" | "timed_out" | "cancelled") {
            return body;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    panic!("job {job_id} did not reach terminal status");
}

#[tokio::test]
async fn create_job_persists_queued_row() {
    let (server, _tmp) = boot().await;

    let response = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({
            "argv": ["help"],
            "timeout_secs": 30
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let job_id = body["job_id"].as_str().expect("job_id");

    let path = format!("/api/cli/jobs/{job_id}");
    let get = server.get(&path).await;
    get.assert_status_ok();
    let meta: serde_json::Value = get.json();
    assert_eq!(meta["argv"], serde_json::json!(["help"]));
}

#[tokio::test]
async fn create_job_rejects_empty_argv() {
    let (server, _tmp) = boot().await;
    let response = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": [] }))
        .await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

#[tokio::test]
async fn create_job_rejects_dashboard_subcommand() {
    let (server, _tmp) = boot().await;
    let response = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["dashboard", "serve"] }))
        .await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

#[tokio::test]
async fn create_job_rejects_zero_timeout() {
    let (server, _tmp) = boot().await;
    let response = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["help"], "timeout_secs": 0 }))
        .await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

#[tokio::test]
async fn create_job_runs_xvn_and_captures_output() {
    let (server, _tmp) = boot().await;
    let create = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["help"], "timeout_secs": 30 }))
        .await;
    create.assert_status_ok();
    let body: serde_json::Value = create.json();
    let job_id = body["job_id"].as_str().unwrap();

    let meta = wait_for_terminal_status(&server, job_id).await;
    assert_eq!(meta["status"], "succeeded");

    let output_path = format!("/api/cli/jobs/{job_id}/output");
    let out = server.get(&output_path).await;
    out.assert_status_ok();
    let payload: serde_json::Value = out.json();
    assert!(payload["stdout"].as_str().unwrap_or("").contains("Usage: xvn"));
}

#[tokio::test]
async fn job_timeout_marks_timed_out_status() {
    let (server, _tmp) = boot().await;
    let create = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({
            "argv": ["slow"],
            "timeout_secs": 1
        }))
        .await;
    create.assert_status_ok();
    let body: serde_json::Value = create.json();
    let job_id = body["job_id"].as_str().unwrap();

    let meta = wait_for_terminal_status(&server, job_id).await;
    assert_eq!(meta["status"], "timed_out");
    assert_eq!(meta["timed_out"], true);
}

#[tokio::test]
async fn cancel_job_marks_cancelled_status() {
    let (server, _tmp) = boot().await;
    let create = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({
            "argv": ["slow"],
            "timeout_secs": 30
        }))
        .await;
    create.assert_status_ok();
    let body: serde_json::Value = create.json();
    let job_id = body["job_id"].as_str().unwrap();

    let cancel_path = format!("/api/cli/jobs/{job_id}/cancel");
    let cancel = server.post(&cancel_path).await;
    cancel.assert_status_ok();

    let meta = wait_for_terminal_status(&server, job_id).await;
    assert_eq!(meta["status"], "cancelled");
    assert_eq!(meta["cancel_requested"], true);
}

#[tokio::test]
async fn sse_stream_emits_job_started_and_job_finished_events() {
    let (base_url, _tmp, handle) = boot_http().await;
    let client = reqwest::Client::new();

    let create = client
        .post(format!("{base_url}/api/cli/jobs"))
        .json(&serde_json::json!({
            "argv": ["help"],
            "timeout_secs": 30
        }))
        .send()
        .await
        .unwrap();
    assert!(create.status().is_success());
    let body: serde_json::Value = create.json().await.unwrap();
    let job_id = body["job_id"].as_str().unwrap();

    let response = client
        .get(format!("{base_url}/api/cli/jobs/{job_id}/events"))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let text = response.text().await.unwrap();

    assert!(text.contains("event: job_started"));
    assert!(text.contains("event: job_finished"));

    handle.abort();
}
