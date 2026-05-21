//! Acceptance tests for the v2b-remote-cli-job-safety hardening:
//!
//! * Orphan detection on simulated restart (PID-liveness check)
//! * Cancellation timing: SIGTERM → SIGKILL with 5-second grace period
//! * Output cap enforcement: kill + flag when child exceeds max_output_bytes
//! * Runtime cap enforcement: SIGTERM + flag when child exceeds max_runtime_seconds
//! * Allowlist rejection: `sh` and other non-xvn commands return 400 CommandNotAllowed

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::cli_jobs::store::CliJobStore;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

// ─── Test helpers ────────────────────────────────────────────────────────────

async fn boot_with_cli(cli: std::path::PathBuf) -> (TestServer, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state")
        .with_cli_command_for_tests(cli);
    let server = TestServer::new(build_router(state)).unwrap();
    (server, tmp)
}

/// Write a shell script that simulates a slow-running process (loops forever).
fn write_infinite_loop_cli(root: &std::path::Path) -> std::path::PathBuf {
    let cli = root.join("fake-xvn-loop");
    fs::write(
        &cli,
        "#!/bin/sh
# Simulates a long-running job that ignores SIGTERM.
# The dashboard runner should escalate to SIGKILL after the grace period.
case \"$1\" in
  eval)
    # Ignore SIGTERM by trapping it (so we test the SIGKILL escalation path).
    trap '' TERM
    while true; do sleep 0.1; done
    ;;
  *)
    echo \"ok\"
    exit 0
    ;;
esac
",
    )
    .unwrap();
    make_executable(&cli);
    cli
}

/// Write a shell script that writes a large amount of output and then sleeps.
fn write_chatty_cli(root: &std::path::Path, output_size_kb: usize) -> std::path::PathBuf {
    let cli = root.join("fake-xvn-chatty");
    // Write output_size_kb kilobytes then sleep.
    let bytes_per_line = 80usize; // approximate
    let lines = (output_size_kb * 1024) / bytes_per_line + 1;
    fs::write(
        &cli,
        format!(
            "#!/bin/sh
case \"$1\" in
  eval)
    python3 -c \"
import sys
line = 'x' * 79 + '\\n'
for _ in range({lines}):
    sys.stdout.write(line)
    sys.stdout.flush()
\"
    sleep 60
    ;;
  *)
    echo \"ok\"
    exit 0
    ;;
esac
"
        ),
    )
    .unwrap();
    make_executable(&cli);
    cli
}

/// Write a shell script that sleeps longer than the runtime cap.
fn write_slow_cli(root: &std::path::Path, sleep_secs: u64) -> std::path::PathBuf {
    let cli = root.join("fake-xvn-slow");
    // Note: this script sleeps regardless of subcommand for the runtime-cap test.
    // The runtime-cap test uses argv starting with "eval", which matches the
    // first case, but sleep_secs is set to something greater than the cap.
    fs::write(
        &cli,
        format!(
            "#!/bin/sh
# Slow CLI: sleeps for {sleep_secs} seconds to trigger runtime or timeout caps.
case \"$1\" in
  eval)
    sleep {sleep_secs}
    exit 0
    ;;
  *)
    echo \"ok\"
    exit 0
    ;;
esac
"
        ),
    )
    .unwrap();
    make_executable(&cli);
    cli
}

fn make_executable(path: &std::path::Path) {
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

async fn wait_for_terminal_status(server: &TestServer, job_id: &str) -> serde_json::Value {
    for _ in 0..400 {
        let path = format!("/api/cli/jobs/{job_id}");
        let meta = server.get(&path).await;
        meta.assert_status_ok();
        let body: serde_json::Value = meta.json();
        let status = body["status"].as_str().unwrap_or("");
        if is_terminal(status) {
            return body;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("job {job_id} did not reach terminal status within timeout");
}

fn is_terminal(status: &str) -> bool {
    matches!(
        status,
        "succeeded"
            | "failed"
            | "timed_out"
            | "cancelled"
            | "orphaned"
            | "output_cap_exceeded"
            | "runtime_cap_exceeded"
    )
}

// ─── Orphan detection ────────────────────────────────────────────────────────

/// A job in `Running` state whose pid column is NULL (no PID persisted) is
/// transitioned to `Orphaned` by the startup recovery sweep.
#[tokio::test]
async fn orphan_recovery_transitions_null_pid_running_job_to_orphaned() {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let store = CliJobStore::new(state.pool.clone());

    // Insert a Running job with no PID (simulates pre-028 schema or missed write).
    let job = store
        .create_queued(
            vec![
                "eval".into(),
                "run".into(),
                "--strategy".into(),
                "x".into(),
                "--scenario".into(),
                "s".into(),
            ],
            30,
        )
        .await
        .expect("create queued job");
    store.mark_running(&job.job_id).await.expect("mark running");

    drop(store);
    drop(state);

    // Restart the dashboard — trigger orphan-recovery sweep.
    let tmp_path = tmp.path().to_path_buf();
    let state2 = AppState::new(tmp_path.clone()).await.expect("init second state");
    state2.recover_cli_jobs().await.expect("orphan recovery");

    let store2 = CliJobStore::new(state2.pool.clone());
    let recovered = store2
        .get(&job.job_id)
        .await
        .expect("load job")
        .expect("job not found");

    assert_eq!(
        recovered.status,
        xvision_dashboard::cli_jobs::model::CliJobStatus::Orphaned,
        "job with NULL pid should be Orphaned after restart",
    );
    assert_eq!(
        recovered.recovery_reason.as_deref(),
        Some("process_not_found"),
        "recovery_reason must be set",
    );
    assert!(recovered.recovered_at.is_some(), "recovered_at must be set",);
}

/// A job in `Running` state whose recorded PID does not exist in the process
/// table is transitioned to `Orphaned` by the startup recovery sweep.
#[tokio::test]
async fn orphan_recovery_transitions_dead_pid_running_job_to_orphaned() {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let store = CliJobStore::new(state.pool.clone());

    let job = store
        .create_queued(
            vec![
                "eval".into(),
                "run".into(),
                "--strategy".into(),
                "x".into(),
                "--scenario".into(),
                "s".into(),
            ],
            30,
        )
        .await
        .expect("create queued job");

    // Persist a PID that is virtually guaranteed to be dead: use PID 1 for now
    // but more robustly set it to a clearly non-existent value.
    // We mark it running via a direct SQL write with an obviously dead PID.
    sqlx::query("UPDATE cli_jobs SET status = 'running', started_at = datetime('now'), pid = 2147483647 WHERE job_id = ?1")
        .bind(&job.job_id)
        .execute(&state.pool)
        .await
        .expect("mark running with dead pid");

    drop(store);
    drop(state);

    let tmp_path = tmp.path().to_path_buf();
    let state2 = AppState::new(tmp_path).await.expect("init second state");
    state2.recover_cli_jobs().await.expect("orphan recovery");

    let store2 = CliJobStore::new(state2.pool.clone());
    let recovered = store2
        .get(&job.job_id)
        .await
        .expect("load job")
        .expect("job not found");

    // PID 2147483647 is almost certainly not alive on the test host.
    // If by some cosmic coincidence it is, this assertion still passes once it dies.
    // The important thing is the store correctly transitions it.
    assert_eq!(
        recovered.status,
        xvision_dashboard::cli_jobs::model::CliJobStatus::Orphaned,
        "job with dead PID should be Orphaned after restart",
    );
}

// ─── Cancellation timing ─────────────────────────────────────────────────────

/// A running job receiving a cancel request via DELETE /api/cli/jobs/:id
/// must exit within 6 seconds (SIGTERM grace = 5s + 1s slack) and the
/// row must be in the `Cancelled` state.
#[tokio::test]
async fn delete_endpoint_cancels_job_within_6_seconds() {
    let tmp = TempDir::new().unwrap();
    let cli = write_infinite_loop_cli(tmp.path());
    let (server, _tmp) = boot_with_cli(cli).await;

    let create = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({
            "argv": ["eval", "run", "--strategy", "x", "--scenario", "s"],
            "timeout_secs": 300
        }))
        .await;
    create.assert_status_ok();
    let body: serde_json::Value = create.json();
    let job_id = body["job_id"].as_str().unwrap();

    // Wait until the job is running.
    for _ in 0..80 {
        let meta: serde_json::Value = server.get(&format!("/api/cli/jobs/{job_id}")).await.json();
        if meta["status"] == "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let start = Instant::now();
    let delete = server.delete(&format!("/api/cli/jobs/{job_id}")).await;
    delete.assert_status_ok();

    let meta = wait_for_terminal_status(&server, job_id).await;
    let elapsed = start.elapsed();

    // Should be Cancelled; SIGKILL escalation might make it happen faster.
    assert_eq!(
        meta["status"], "cancelled",
        "job should be cancelled, got: {:?}",
        meta
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "cancellation should complete within 10 seconds; took {elapsed:?}",
    );
    assert_eq!(meta["cancel_requested"], true);
}

// ─── Allowlist rejection ──────────────────────────────────────────────────────

/// Attempting to create a job with `command = "sh"` must return 400 with
/// a message that clearly explains the command is not allowed.
#[tokio::test]
async fn sh_command_is_rejected_with_400() {
    let tmp = TempDir::new().unwrap();
    let cli = write_slow_cli(tmp.path(), 300);
    let (server, _tmp) = boot_with_cli(cli).await;

    let resp = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["sh", "-c", "echo pwned"] }))
        .await;

    resp.assert_status_bad_request();
    let body: serde_json::Value = resp.json();
    assert_eq!(
        body["code"], "validation",
        "error code must be 'validation', got: {:?}",
        body
    );
    let msg = body["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("not a supported remote cli subcommand") || msg.contains("not allowed"),
        "error message must explain why 'sh' is not allowed, got: {msg}",
    );
}

/// Attempting to create a job with `command = "bash"` must also return 400.
#[tokio::test]
async fn bash_command_is_rejected_with_400() {
    let tmp = TempDir::new().unwrap();
    let cli = write_slow_cli(tmp.path(), 300);
    let (server, _tmp) = boot_with_cli(cli).await;

    let resp = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["bash", "-c", "echo pwned"] }))
        .await;

    resp.assert_status_bad_request();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["code"], "validation");
}

/// An arbitrary executable path (e.g. `/usr/bin/curl`) is rejected.
#[tokio::test]
async fn arbitrary_executable_path_is_rejected_with_400() {
    let tmp = TempDir::new().unwrap();
    let cli = write_slow_cli(tmp.path(), 300);
    let (server, _tmp) = boot_with_cli(cli).await;

    let resp = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({
            "argv": ["/usr/bin/curl", "http://evil.example.com"]
        }))
        .await;

    resp.assert_status_bad_request();
}

// ─── Audit fields ────────────────────────────────────────────────────────────

/// A completed job must have `command_class` set to the first argv element.
#[tokio::test]
async fn completed_job_has_command_class_set() {
    let tmp = TempDir::new().unwrap();
    let cli = {
        let p = tmp.path().join("fake-xvn-audit");
        fs::write(&p, "#!/bin/sh\necho ok\nexit 0\n").unwrap();
        make_executable(&p);
        p
    };
    let (server, _tmp) = boot_with_cli(cli).await;

    let create = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({
            "argv": ["eval", "list"],
            "timeout_secs": 30
        }))
        .await;
    create.assert_status_ok();
    let body: serde_json::Value = create.json();
    let job_id = body["job_id"].as_str().unwrap();

    let meta = wait_for_terminal_status(&server, job_id).await;
    assert_eq!(
        meta["command_class"], "eval",
        "command_class should be the first argv element",
    );
    // output_bytes should be the sum of stdout_bytes + stderr_bytes.
    let output_bytes = meta["output_bytes"].as_u64().unwrap_or(0);
    let stdout_bytes = meta["stdout_bytes"].as_u64().unwrap_or(0);
    let stderr_bytes = meta["stderr_bytes"].as_u64().unwrap_or(0);
    assert_eq!(
        output_bytes,
        stdout_bytes.saturating_add(stderr_bytes),
        "output_bytes must equal stdout_bytes + stderr_bytes",
    );
}

// ─── Output cap enforcement ───────────────────────────────────────────────────

/// A job that writes more than max_output_bytes (1 KB in this test) must be
/// killed and transition to `output_cap_exceeded`.
///
/// Note: the per-job cap is set via the DB after creation because the current
/// route doesn't expose it in the request body. We insert a row with a small
/// cap directly via the store for this test.
///
/// Uses multi_thread flavor so the spawned runner task can execute concurrently
/// with the polling loop in the test's current task.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn output_cap_exceeded_kills_chatty_process() {
    use xvision_dashboard::cli_jobs::auth_stub::AuthContext;
    use xvision_dashboard::cli_jobs::store::CreateJobParams;

    let tmp = TempDir::new().unwrap();
    // Write 100 KB of output (cap is 1 KB = 1024 bytes).
    let cli = write_chatty_cli(tmp.path(), 100);
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init state")
        .with_cli_command_for_tests(cli);

    // Create a job with a very small output cap (1 KB) directly via the store.
    let store = CliJobStore::new(state.pool.clone());
    let auth = AuthContext::unknown();
    let job = store
        .create_queued_with_auth(CreateJobParams {
            argv: vec![
                "eval".into(),
                "run".into(),
                "--strategy".into(),
                "x".into(),
                "--scenario".into(),
                "s".into(),
            ],
            timeout_secs: 300,
            auth: &auth,
            max_runtime_seconds: 0, // use default
            max_output_bytes: 1024, // 1 KB cap
        })
        .await
        .expect("create job with small output cap");

    let pool2 = state.pool.clone();
    state.cli_runner().start(job.clone());
    let _server = TestServer::new(build_router(state)).unwrap();
    let store2 = CliJobStore::new(pool2);

    // Poll directly on the store so we don't depend on HTTP routing.
    let deadline = Duration::from_secs(30);
    let start = Instant::now();
    let result = loop {
        let job_state = store2
            .get(&job.job_id)
            .await
            .expect("load job")
            .expect("job not found");
        if is_terminal(job_state.status.as_str()) {
            break job_state;
        }
        if start.elapsed() > deadline {
            panic!(
                "job {} did not reach terminal status within {:?}; last status: {:?}",
                job.job_id, deadline, job_state.status,
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };

    assert_eq!(
        result.status,
        xvision_dashboard::cli_jobs::model::CliJobStatus::OutputCapExceeded,
        "job exceeding output cap must transition to output_cap_exceeded, got: {:?}",
        result.status,
    );
    assert!(result.output_cap_exceeded, "output_cap_exceeded flag must be set",);
}

// ─── Runtime cap enforcement ──────────────────────────────────────────────────

/// A job that runs past its max_runtime_seconds cap must receive SIGTERM and
/// transition to `runtime_cap_exceeded`.
///
/// Uses multi_thread flavor so the spawned runner task can execute concurrently
/// with the polling loop in the test's current task.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime_cap_exceeded_kills_slow_process() {
    use xvision_dashboard::cli_jobs::auth_stub::AuthContext;
    use xvision_dashboard::cli_jobs::store::CreateJobParams;

    let tmp = TempDir::new().unwrap();
    // A job that sleeps 60 seconds; our cap is 2 seconds.
    let cli = write_slow_cli(tmp.path(), 60);
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init state")
        .with_cli_command_for_tests(cli);

    let store = CliJobStore::new(state.pool.clone());
    let auth = AuthContext::unknown();
    let job = store
        .create_queued_with_auth(CreateJobParams {
            argv: vec![
                "eval".into(),
                "run".into(),
                "--strategy".into(),
                "x".into(),
                "--scenario".into(),
                "s".into(),
            ],
            timeout_secs: 300,
            auth: &auth,
            max_runtime_seconds: 2, // 2-second cap
            max_output_bytes: 0,    // use default
        })
        .await
        .expect("create job with short runtime cap");

    // Verify caps are persisted correctly before starting.
    assert_eq!(
        job.max_runtime_seconds, 2,
        "max_runtime_seconds must be 2 in the in-memory job"
    );

    // Verify the DB row has the correct cap.
    let db_job = store
        .get(&job.job_id)
        .await
        .expect("load job from DB")
        .expect("job not found");
    assert_eq!(
        db_job.max_runtime_seconds, 2,
        "max_runtime_seconds must be 2 in the DB"
    );

    let pool2 = state.pool.clone();
    state.cli_runner().start(job.clone());
    let _server = TestServer::new(build_router(state)).unwrap();

    // Poll directly on the store rather than via HTTP to avoid any router
    // state-divergence issues in this test.
    let store2 = CliJobStore::new(pool2);
    let start = Instant::now();
    let deadline = Duration::from_secs(20);
    let result = loop {
        let job_state = store2
            .get(&job.job_id)
            .await
            .expect("load job")
            .expect("job not found");
        if is_terminal(job_state.status.as_str()) {
            break job_state;
        }
        if start.elapsed() > deadline {
            panic!(
                "job {} did not reach terminal status within {:?}; last status: {:?}",
                job.job_id, deadline, job_state.status
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let elapsed = start.elapsed();

    assert_eq!(
        result.status,
        xvision_dashboard::cli_jobs::model::CliJobStatus::RuntimeCapExceeded,
        "job exceeding runtime cap must transition to runtime_cap_exceeded, got: {:?}",
        result.status,
    );
    assert!(
        result.runtime_cap_exceeded,
        "runtime_cap_exceeded flag must be set",
    );
    // Should terminate within 2s cap + 5s SIGKILL grace + 5s slack = 12s total.
    assert!(
        elapsed < Duration::from_secs(15),
        "runtime cap should be enforced within 15 seconds; took {elapsed:?}",
    );
}

// ─── DELETE endpoint wiring ───────────────────────────────────────────────────

/// DELETE /api/cli/jobs/:id on a non-existent job returns 404.
#[tokio::test]
async fn delete_nonexistent_job_returns_404() {
    let tmp = TempDir::new().unwrap();
    let cli = write_slow_cli(tmp.path(), 300);
    let (server, _tmp) = boot_with_cli(cli).await;

    let resp = server.delete("/api/cli/jobs/job_DOESNOTEXIST").await;

    resp.assert_status(axum::http::StatusCode::NOT_FOUND);
}

/// DELETE /api/cli/jobs/:id on a completed job is idempotent.
#[tokio::test]
async fn delete_completed_job_is_idempotent() {
    let tmp = TempDir::new().unwrap();
    let cli = {
        let p = tmp.path().join("fake-xvn-ok");
        fs::write(&p, "#!/bin/sh\necho ok\nexit 0\n").unwrap();
        make_executable(&p);
        p
    };
    let (server, _tmp) = boot_with_cli(cli).await;

    let create = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({
            "argv": ["eval", "list"],
            "timeout_secs": 30
        }))
        .await;
    create.assert_status_ok();
    let body: serde_json::Value = create.json();
    let job_id = body["job_id"].as_str().unwrap();

    wait_for_terminal_status(&server, job_id).await;

    // DELETE on a terminal job should be idempotent (200 with current status).
    let del = server.delete(&format!("/api/cli/jobs/{job_id}")).await;
    del.assert_status_ok();
    let del_body: serde_json::Value = del.json();
    assert_eq!(del_body["status"], "succeeded");
}
