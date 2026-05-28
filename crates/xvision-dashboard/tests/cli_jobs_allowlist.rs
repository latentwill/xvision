//! End-to-end tests for remote CLI policy over `POST /api/cli/jobs`.
//! The remote surface supports normal operator/eval commands by default while
//! rejecting categorically dangerous subcommands. The opt-in
//! `XVN_DASHBOARD_CLI_DEVMODE` flag (off by default) turns the policy into a
//! FULL bypass for trusted dev nodes — every argv is accepted, including
//! live-trade and host-admin verbs. These tests pin both modes.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::Mutex;

use axum_test::TestServer;
use tempfile::TempDir;
use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;

const DEVMODE_ENV: &str = "XVN_DASHBOARD_CLI_DEVMODE";

/// Process-wide lock so devmode-mutating tests don't race each other.
/// The static `Mutex` is the smallest tool that fixes this; pulling in
/// `serial_test` for one suite is overkill.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct DevmodeGuard {
    prev: Option<String>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl Drop for DevmodeGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(DEVMODE_ENV, v),
            None => std::env::remove_var(DEVMODE_ENV),
        }
    }
}

fn devmode_off() -> DevmodeGuard {
    let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = std::env::var(DEVMODE_ENV).ok();
    std::env::remove_var(DEVMODE_ENV);
    DevmodeGuard { prev, _lock: lock }
}

fn devmode_on() -> DevmodeGuard {
    let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = std::env::var(DEVMODE_ENV).ok();
    std::env::set_var(DEVMODE_ENV, "1");
    DevmodeGuard { prev, _lock: lock }
}

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

fn write_fake_cli(root: &std::path::Path) -> std::path::PathBuf {
    let cli = root.join("fake-xvn");
    fs::write(&cli, "#!/bin/sh\nexit 0\n").unwrap();
    let mut perms = fs::metadata(&cli).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&cli, perms).unwrap();
    cli
}

#[tokio::test]
async fn bars_fetch_allowed_argv_creates_job() {
    let _guard = devmode_off();
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({
            "argv": [
                "bars", "fetch",
                "--asset", "BTC/USD",
                "--granularity", "1h",
                "--from", "2025-01-01",
                "--to", "2025-02-01",
            ],
        }))
        .await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn eval_run_is_allowed_without_devmode() {
    let _guard = devmode_off();
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({
            "argv": ["eval", "run", "--strategy", "x", "--scenario", "s", "--mode", "backtest"]
        }))
        .await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn bars_fetch_with_unknown_flag_is_rejected() {
    let _guard = devmode_off();
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({
            "argv": ["bars", "fetch", "--asset", "BTC/USD", "--force", "true"],
        }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_subcommand_is_rejected_with_400() {
    let _guard = devmode_off();
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["not-a-command"] }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body = resp.text();
    assert!(
        body.contains("not a supported remote cli subcommand"),
        "validation error must mention unsupported command, got: {body}"
    );
}

#[tokio::test]
async fn devmode_bypass_allows_otherwise_unsupported_argv() {
    let _guard = devmode_on();
    let (server, _tmp) = boot().await;
    // Unsupported heads are rejected by default; the full devmode bypass lets
    // them through (the job still shells out to the real `xvn`, which validates).
    let resp = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["not-a-command"] }))
        .await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn devmode_bypass_allows_dashboard_subcommand() {
    let _guard = devmode_on();
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["dashboard", "serve"] }))
        .await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn devmode_bypass_allows_mutating_and_admin_nested_subcommands() {
    let _guard = devmode_on();
    let (server, _tmp) = boot().await;
    for argv in [
        serde_json::json!(["strategy", "new", "--name", "remote-test"]),
        serde_json::json!(["agent", "create", "--name", "remote-agent"]),
        serde_json::json!(["scenario", "create", "--name", "remote-test"]),
        serde_json::json!(["store", "migrate"]),
        serde_json::json!(["migrate", "--dry-run"]),
    ] {
        let resp = server
            .post("/api/cli/jobs")
            .json(&serde_json::json!({ "argv": argv }))
            .await;
        resp.assert_status_ok();
    }
}

#[tokio::test]
async fn fire_trade_is_rejected_in_default_mode() {
    let _guard = devmode_off();
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["fire-trade", "--whatever"] }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body = resp.text();
    assert!(body.contains("not allowed over remote cli") || body.contains("fire-trade"));
}

#[tokio::test]
async fn mutating_nested_subcommands_are_rejected() {
    let _guard = devmode_off();
    let (server, _tmp) = boot().await;

    for argv in [
        serde_json::json!(["scenario", "rm", "sc_1"]),
        serde_json::json!(["strategy", "remove-agent", "st_1", "--role", "trader"]),
        serde_json::json!(["obs", "retention", "set", "--mode", "full-debug"]),
        serde_json::json!(["store", "migrate"]),
    ] {
        let resp = server
            .post("/api/cli/jobs")
            .json(&serde_json::json!({ "argv": argv }))
            .await;
        resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
        let body = resp.text();
        assert!(
            body.contains("not allowed over remote cli"),
            "validation error must mention remote cli policy, got: {body}"
        );
    }
}

#[tokio::test]
async fn fire_trade_is_allowed_in_devmode() {
    let _guard = devmode_on();
    let (server, _tmp) = boot().await;
    // The whole point of full devmode: even live-trade verbs are reachable on
    // a trusted dev node. NEVER set the env on a node with live broker creds.
    let resp = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["fire-trade", "--whatever"] }))
        .await;
    resp.assert_status_ok();
}
