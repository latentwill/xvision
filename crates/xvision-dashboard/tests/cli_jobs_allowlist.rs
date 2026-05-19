//! End-to-end tests for `qa-dashboard-auth-hardening` allowlist behavior
//! over `POST /api/cli/jobs`. Default mode (no devmode env) must reject
//! anything outside the allowlisted templates. Devmode must allow the
//! permissive path.

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
async fn unknown_subcommand_is_rejected_with_400() {
    let _guard = devmode_off();
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["eval", "run", "--strategy", "x"] }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body = resp.text();
    assert!(
        body.contains("allowlisted"),
        "validation error must mention the allowlist, got: {body}"
    );
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
async fn dashboard_subcommand_is_rejected_even_in_devmode() {
    let _guard = devmode_on();
    let (server, _tmp) = boot().await;
    let resp = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["dashboard", "serve"] }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
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
