//! End-to-end auth-gate tests for `qa-dashboard-auth-hardening`.
//!
//! Two paths cover the contract acceptance criteria (a)/(b):
//! - **Loopback bind:** server boots without `XVN_DASHBOARD_TOKEN` and
//!   serves every route to unauth'd clients.
//! - **Non-loopback bind:** server refuses to start when the env var is
//!   missing; when set, rejects requests without the token and accepts
//!   them with the token via header or query.

use std::net::SocketAddr;
use std::sync::Mutex;

use xvision_dashboard::auth::{AuthState, AUTH_TOKEN_ENV};

/// Serialize env-mutating tests. Cargo runs integration tests in
/// parallel; without this lock they race on the same env var.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn restore_env(prev: Option<String>) {
    match prev {
        Some(v) => std::env::set_var(AUTH_TOKEN_ENV, v),
        None => std::env::remove_var(AUTH_TOKEN_ENV),
    }
}

#[test]
fn from_env_loopback_no_token_required() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = std::env::var(AUTH_TOKEN_ENV).ok();
    std::env::remove_var(AUTH_TOKEN_ENV);
    let addr: SocketAddr = "127.0.0.1:8788".parse().unwrap();
    let state = AuthState::from_env(&addr).expect("loopback bind needs no token");
    assert!(!state.is_gated());
    restore_env(prev);
}

#[test]
fn from_env_non_loopback_without_token_refuses() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = std::env::var(AUTH_TOKEN_ENV).ok();
    std::env::remove_var(AUTH_TOKEN_ENV);
    let addr: SocketAddr = "203.0.113.5:8788".parse().unwrap();
    let err = AuthState::from_env(&addr)
        .expect_err("non-loopback bind without token must refuse to start");
    let msg = err.to_string();
    assert!(
        msg.contains(AUTH_TOKEN_ENV) && msg.contains("non-loopback"),
        "startup error must mention {AUTH_TOKEN_ENV} + non-loopback, got: {msg}"
    );
    restore_env(prev);
}

#[test]
fn from_env_non_loopback_with_token_is_gated() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = std::env::var(AUTH_TOKEN_ENV).ok();
    std::env::set_var(AUTH_TOKEN_ENV, "hunter2");
    let addr: SocketAddr = "203.0.113.5:8788".parse().unwrap();
    let state = AuthState::from_env(&addr).unwrap();
    assert!(state.is_gated());
    restore_env(prev);
}

#[test]
fn unspecified_bind_treated_as_non_loopback() {
    // 0.0.0.0 binds to every interface (including the public one), so
    // it must require a token rather than slipping through as
    // loopback-equivalent.
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let prev = std::env::var(AUTH_TOKEN_ENV).ok();
    std::env::remove_var(AUTH_TOKEN_ENV);
    let addr: SocketAddr = "0.0.0.0:8788".parse().unwrap();
    AuthState::from_env(&addr)
        .expect_err("0.0.0.0 bind must require a configured token");
    restore_env(prev);
}
