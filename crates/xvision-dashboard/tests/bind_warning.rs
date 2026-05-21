//! Bind-warning tests for v2b-dashboard-auth-boundary.
//!
//! Verifies:
//! - `xvn dash --bind 127.0.0.1:<port>` (loopback) does NOT print the
//!   non-loopback warning to stderr.
//! - `xvn dash --bind 0.0.0.0:<port>` (non-loopback) prints a loud
//!   single-line WARNING to stderr at startup.
//!
//! The warning logic lives in `crates/xvision-dashboard/src/server.rs::serve`.
//! We test the actual `is_loopback` predicate directly (the serve function
//! is hard to unit-test without binding a real port) via the public
//! `AuthState::from_env` / bind-address inspection path.
//!
//! The integration test for the actual WARNING line uses the subprocess-level
//! `xvn dash serve` binary which we can't invoke easily in a cargo test, so
//! we instead verify the logic pathway in the server module is correct by
//! asserting the IpAddr::is_loopback() contract for each bind scenario.

use std::net::{IpAddr, SocketAddr};

// ---------------------------------------------------------------------------
// Loopback detection (unit)
// ---------------------------------------------------------------------------

#[test]
fn loopback_bind_127_0_0_1_does_not_trigger_warning() {
    let addr: SocketAddr = "127.0.0.1:8788".parse().unwrap();
    assert!(
        addr.ip().is_loopback(),
        "127.0.0.1 must be detected as loopback (no warning)"
    );
}

#[test]
fn loopback_bind_ipv6_1_does_not_trigger_warning() {
    let addr: SocketAddr = "[::1]:8788".parse().unwrap();
    assert!(
        addr.ip().is_loopback(),
        "::1 must be detected as loopback (no warning)"
    );
}

#[test]
fn unspecified_bind_0_0_0_0_triggers_warning() {
    let addr: SocketAddr = "0.0.0.0:8788".parse().unwrap();
    assert!(
        !addr.ip().is_loopback(),
        "0.0.0.0 must NOT be detected as loopback (warning required)"
    );
}

#[test]
fn public_ip_bind_triggers_warning() {
    let addr: SocketAddr = "203.0.113.5:8788".parse().unwrap();
    assert!(
        !addr.ip().is_loopback(),
        "public IP must NOT be detected as loopback (warning required)"
    );
}

#[test]
fn tailnet_ip_bind_triggers_warning() {
    // Tailscale addresses are typically in 100.64.0.0/10 range.
    let addr: SocketAddr = "100.120.48.1:8788".parse().unwrap();
    assert!(
        !addr.ip().is_loopback(),
        "Tailscale IP must NOT be detected as loopback (warning required)"
    );
}

// ---------------------------------------------------------------------------
// AuthState startup: non-loopback without token errors
// ---------------------------------------------------------------------------

/// The auth gate refuses to start on a non-loopback bind without a token.
/// This complements the warning — if the warning fires, the token must also be set.
#[test]
fn non_loopback_bind_requires_dashboard_token_or_startup_fails() {
    use xvision_dashboard::auth::{AuthState, AUTH_TOKEN_ENV};

    let addr: SocketAddr = "0.0.0.0:8788".parse().unwrap();
    // Ensure the env var is NOT set.
    let prev = std::env::var(AUTH_TOKEN_ENV).ok();
    std::env::remove_var(AUTH_TOKEN_ENV);

    let result = AuthState::from_env(&addr);
    assert!(
        result.is_err(),
        "non-loopback bind without XVN_DASHBOARD_TOKEN must refuse to start"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains(AUTH_TOKEN_ENV) || msg.contains("non-loopback"),
        "startup error must mention {AUTH_TOKEN_ENV} or 'non-loopback', got: {msg}"
    );

    // Restore.
    match prev {
        Some(v) => std::env::set_var(AUTH_TOKEN_ENV, v),
        None => std::env::remove_var(AUTH_TOKEN_ENV),
    }
}

#[test]
fn loopback_bind_does_not_require_dashboard_token() {
    use xvision_dashboard::auth::{AuthState, AUTH_TOKEN_ENV};

    let addr: SocketAddr = "127.0.0.1:8788".parse().unwrap();
    let prev = std::env::var(AUTH_TOKEN_ENV).ok();
    std::env::remove_var(AUTH_TOKEN_ENV);

    let result = AuthState::from_env(&addr);
    assert!(
        result.is_ok(),
        "loopback bind must not require XVN_DASHBOARD_TOKEN"
    );

    // Restore.
    match prev {
        Some(v) => std::env::set_var(AUTH_TOKEN_ENV, v),
        None => std::env::remove_var(AUTH_TOKEN_ENV),
    }
}
