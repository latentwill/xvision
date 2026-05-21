use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::cli_jobs::store::CliJobStore;
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

struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn boot_existing_home(root: &std::path::Path) -> TestServer {
    let cli = write_fake_cli(root);
    let state = AppState::new(root.to_path_buf())
        .await
        .expect("init dashboard state")
        .with_cli_command_for_tests(cli);
    state.recover_cli_jobs().await.expect("recover cli jobs");
    TestServer::new(build_router(state)).unwrap()
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
  eval)
    case \"$2\" in
      watch)
        sleep 2
        echo 'slow done'
        exit 0
        ;;
      *)
        echo 'eval ok'
        exit 0
        ;;
    esac
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
            "argv": ["eval", "watch", "slow-run"],
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
            "argv": ["eval", "watch", "slow-run"],
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
    let _server = AbortOnDrop(handle);
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

    let response = tokio::time::timeout(
        Duration::from_secs(5),
        client
            .get(format!("{base_url}/api/cli/jobs/{job_id}/events"))
            .send(),
    )
    .await
    .expect("SSE request timed out")
    .unwrap();
    assert!(response.status().is_success());
    let text = tokio::time::timeout(Duration::from_secs(5), response.text())
        .await
        .expect("SSE stream did not close after expected events")
        .unwrap();

    assert!(text.contains("event: job_started"));
    assert!(text.contains("event: job_finished"));
}

#[tokio::test]
async fn startup_recovers_queued_jobs() {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let store = CliJobStore::new(state.pool.clone());
    let job = store
        .create_queued(vec!["help".into()], 30)
        .await
        .expect("create queued job");

    drop(store);
    drop(state);

    let server = boot_existing_home(tmp.path()).await;
    let meta = wait_for_terminal_status(&server, &job.job_id).await;
    assert_eq!(meta["status"], "succeeded");

    let output_path = format!("/api/cli/jobs/{}/output", job.job_id);
    let out = server.get(&output_path).await;
    out.assert_status_ok();
    let payload: serde_json::Value = out.json();
    assert!(payload["stdout"].as_str().unwrap_or("").contains("Usage: xvn"));
}

#[tokio::test]
async fn startup_orphans_running_jobs_whose_pid_is_not_alive() {
    // Insert a job in Running state with no PID recorded (simulates a job
    // started before migration 028, or a job whose PID was not written).
    // The orphan-recovery sweep should transition it to `orphaned`.
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let store = CliJobStore::new(state.pool.clone());
    let job = store
        .create_queued(vec!["eval".into(), "watch".into(), "slow-run".into()], 30)
        .await
        .expect("create queued job");
    // mark_running with no PID simulates a pre-028 row (pid IS NULL).
    store.mark_running(&job.job_id).await.expect("mark running");

    drop(store);
    drop(state);

    let server = boot_existing_home(tmp.path()).await;
    let path = format!("/api/cli/jobs/{}", job.job_id);
    let response = server.get(&path).await;
    response.assert_status_ok();
    let meta: serde_json::Value = response.json();
    // NULL pid → confirmed orphan; status is now "orphaned" not "failed"
    // so the operator can distinguish "lost due to restart" from "ran but failed".
    assert_eq!(meta["status"], "orphaned");
    assert_eq!(meta["recovery_reason"], "process_not_found");
    assert!(meta["error_message"]
        .as_str()
        .unwrap_or("")
        .contains("orphaned by dashboard restart"));
}
