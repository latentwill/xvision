//! End-to-end auth-gate tests for `qa-dashboard-auth-hardening`.
//!
//! Two paths cover the contract acceptance criteria (a)/(b):
//! - **Loopback bind:** server boots without `XVN_DASHBOARD_TOKEN` and
//!   serves every route to unauth'd clients.
//! - **Non-loopback bind:** server refuses to start when the env var is
//!   missing; when set, rejects requests without the token and accepts
//!   them with the token via header or query.

use std::net::SocketAddr;
use std::sync::{Mutex, MutexGuard};

use axum::{
    body::Body,
    extract::connect_info::ConnectInfo,
    http::{Request, StatusCode},
    Router,
};
use tempfile::TempDir;
use tower::ServiceExt;
use xvision_dashboard::{
    auth::{AuthState, AUTH_TOKEN_ENV, AUTH_TOKEN_HEADER},
    server::{build_router, wrap_with_auth},
    AppState,
};

/// Serialize env-mutating tests. Cargo runs integration tests in
/// parallel; without this lock they race on the same env var.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    _lock: Option<MutexGuard<'static, ()>>,
    prev: Option<String>,
}

impl EnvGuard {
    fn remove() -> Self {
        let guard = Self::capture_locked();
        std::env::remove_var(AUTH_TOKEN_ENV);
        guard
    }

    fn set(value: &str) -> Self {
        let guard = Self::capture_locked();
        std::env::set_var(AUTH_TOKEN_ENV, value);
        guard
    }

    fn capture_locked() -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        Self {
            _lock: Some(lock),
            prev: std::env::var(AUTH_TOKEN_ENV).ok(),
        }
    }

    fn set_without_lock(value: &str) -> Self {
        let guard = Self {
            _lock: None,
            prev: std::env::var(AUTH_TOKEN_ENV).ok(),
        };
        std::env::set_var(AUTH_TOKEN_ENV, value);
        guard
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(AUTH_TOKEN_ENV, v),
            None => std::env::remove_var(AUTH_TOKEN_ENV),
        }
    }
}

async fn boot_auth_router(auth: AuthState) -> (Router, TempDir) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    (wrap_with_auth(build_router(state), auth), tmp)
}

async fn request_status(
    app: Router,
    path: &str,
    client_addr: &str,
    header_token: Option<&str>,
) -> StatusCode {
    let mut request = Request::builder().uri(path);
    if let Some(token) = header_token {
        request = request.header(AUTH_TOKEN_HEADER, token);
    }
    let mut request = request.body(Body::empty()).unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::new(client_addr.parse().unwrap(), 49152)));
    app.oneshot(request).await.unwrap().status()
}

#[test]
fn from_env_loopback_no_token_required() {
    let _env = EnvGuard::remove();
    let addr: SocketAddr = "127.0.0.1:8788".parse().unwrap();
    let state = AuthState::from_env(&addr).expect("loopback bind needs no token");
    assert!(!state.is_gated());
}

#[test]
fn from_env_non_loopback_without_token_refuses() {
    let _env = EnvGuard::remove();
    let addr: SocketAddr = "203.0.113.5:8788".parse().unwrap();
    let err = AuthState::from_env(&addr).expect_err("non-loopback bind without token must refuse to start");
    let msg = err.to_string();
    assert!(
        msg.contains(AUTH_TOKEN_ENV) && msg.contains("non-loopback"),
        "startup error must mention {AUTH_TOKEN_ENV} + non-loopback, got: {msg}"
    );
}

#[test]
fn from_env_non_loopback_with_token_is_gated() {
    let _env = EnvGuard::set("hunter2");
    let addr: SocketAddr = "203.0.113.5:8788".parse().unwrap();
    let state = AuthState::from_env(&addr).unwrap();
    assert!(state.is_gated());
}

#[test]
fn unspecified_bind_treated_as_non_loopback() {
    // 0.0.0.0 binds to every interface (including the public one), so
    // it must require a token rather than slipping through as
    // loopback-equivalent.
    let _env = EnvGuard::remove();
    let addr: SocketAddr = "0.0.0.0:8788".parse().unwrap();
    AuthState::from_env(&addr).expect_err("0.0.0.0 bind must require a configured token");
}

#[test]
fn env_guard_restores_token_after_panic() {
    let _env = EnvGuard::set("original");

    let result = std::panic::catch_unwind(|| {
        let _temporary = EnvGuard::set_without_lock("temporary");
        assert_eq!(std::env::var(AUTH_TOKEN_ENV).as_deref(), Ok("temporary"));
        panic!("exercise env restoration during unwinding");
    });

    assert!(result.is_err());
    assert_eq!(std::env::var(AUTH_TOKEN_ENV).as_deref(), Ok("original"));
}

#[tokio::test]
async fn request_gate_rejects_missing_token_and_accepts_header_or_query() {
    let (loopback_app, _loopback_tmp) = boot_auth_router(AuthState::loopback_only()).await;
    assert_eq!(
        request_status(loopback_app, "/api/health", "203.0.113.5", None).await,
        StatusCode::OK,
        "loopback-only auth state should not gate dashboard routes"
    );

    let (gated_app, _gated_tmp) = boot_auth_router(AuthState::with_required_token("hunter2".into())).await;

    assert_eq!(
        request_status(gated_app.clone(), "/api/health", "203.0.113.5", None).await,
        StatusCode::UNAUTHORIZED,
        "non-loopback client without token should be rejected"
    );
    assert_eq!(
        request_status(gated_app.clone(), "/api/health", "203.0.113.5", Some("hunter2"),).await,
        StatusCode::OK,
        "non-loopback client with header token should be accepted"
    );
    assert_eq!(
        request_status(gated_app, "/api/health?token=hunter2", "203.0.113.5", None).await,
        StatusCode::OK,
        "non-loopback client with query token should be accepted"
    );
}
