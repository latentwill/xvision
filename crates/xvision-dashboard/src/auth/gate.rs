//! Dashboard HTTP auth gate (qa-dashboard-auth-hardening, 2026-05-17).
//!
//! ## Threat model
//!
//! Before this gate, every `/api/*` route was implicitly trusted because the
//! default bind was `127.0.0.1:8788`. The operator could re-bind to a public
//! address (e.g. for tailnet access) and unwittingly expose
//! `reset_workspace`, `factory_reset`, `cli/jobs`, and every
//! provider/secret mutation surface
//! to anyone who could reach the socket — no authentication required.
//!
//! ## What this layer does
//!
//! - **Loopback** binds (`127.0.0.1`, `::1`) pass through unchanged. Local
//!   dev stays frictionless.
//! - **Non-loopback** binds require a shared secret configured via the
//!   `XVN_DASHBOARD_TOKEN` env var. Requests must present the secret via
//!   `Authorization: Bearer <token>`, the dedicated `X-Xvision-Token`
//!   header, a scoped auth cookie, or `?token=<token>` query parameter.
//!   A valid header/query token sets the cookie so browser loads can fetch
//!   static assets and same-origin API calls without appending the token to
//!   every URL.
//! - **Missing secret on a non-loopback bind is a startup error.** The
//!   server refuses to start in that configuration rather than silently
//!   accepting unauthenticated traffic. The runbook documents how to set
//!   the secret.
//! - **No-op for tests.** The unit-test path constructs an `AuthState`
//!   directly (see `AuthState::loopback_only` and
//!   `AuthState::with_required_token`); integration tests rely on
//!   `start_test_server` choosing loopback binds.
//!
//! Out of scope for this layer: TLS termination, per-route RBAC, session
//! cookies. A reverse proxy in front of the dashboard handles TLS; richer
//! auth lives in V2B+.

use std::net::{IpAddr, SocketAddr};

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Env var that supplies the shared secret on non-loopback binds.
pub const AUTH_TOKEN_ENV: &str = "XVN_DASHBOARD_TOKEN";

/// Header alias for the shared secret. Either this or
/// `Authorization: Bearer <token>` is accepted.
pub const AUTH_TOKEN_HEADER: &str = "x-xvision-token";

/// Query parameter alias for the shared secret. Useful for SSE / `<a>`
/// downloads where the browser can't easily attach a header.
pub const AUTH_TOKEN_QUERY_PARAM: &str = "token";

/// Cookie name used after a successful header/query-token bootstrap.
const AUTH_COOKIE_NAME: &str = "xvn_dashboard_token";

/// Auth posture for the running server. Cheap to clone because the
/// inner token is wrapped in `Arc`.
#[derive(Clone, Debug)]
pub struct AuthState {
    inner: std::sync::Arc<AuthStateInner>,
}

#[derive(Debug)]
struct AuthStateInner {
    /// `Some(token)` when bound to a non-loopback address; `None` when
    /// bound loopback-only and no token gate applies.
    required_token: Option<String>,
}

impl AuthState {
    /// Auth posture for a loopback-only bind: no token required.
    pub fn loopback_only() -> Self {
        Self {
            inner: std::sync::Arc::new(AuthStateInner { required_token: None }),
        }
    }

    /// Auth posture for a non-loopback bind: every request must present
    /// the configured shared secret.
    pub fn with_required_token(token: String) -> Self {
        Self {
            inner: std::sync::Arc::new(AuthStateInner {
                required_token: Some(token),
            }),
        }
    }

    /// Decide the auth posture for a given bind address by consulting
    /// the `XVN_DASHBOARD_TOKEN` env var. Loopback binds may run open for local
    /// development. Non-loopback binds must configure a non-empty token because
    /// they are reachable from Docker/Tailscale/LAN interfaces.
    pub fn from_env(addr: &SocketAddr) -> anyhow::Result<Self> {
        if is_loopback(addr) {
            return Ok(Self::loopback_only());
        }
        let token = std::env::var(AUTH_TOKEN_ENV).unwrap_or_default();
        if token.is_empty() {
            anyhow::bail!("{AUTH_TOKEN_ENV} must be set for non-loopback dashboard bind {addr}");
        }
        Ok(Self::with_required_token(token))
    }

    /// True when the auth state will gate every request (non-loopback
    /// bind with a token configured).
    pub fn is_gated(&self) -> bool {
        self.inner.required_token.is_some()
    }

    /// Authenticate a request. `client_ip` is the socket peer address;
    /// requests from loopback are exempt even when the server is bound
    /// to a public interface (this preserves localhost dev access via
    /// SSH tunnel etc).
    fn authenticate(&self, client_ip: IpAddr, headers: &HeaderMap, query: Option<&str>) -> AuthDecision {
        let Some(expected) = self.inner.required_token.as_deref() else {
            return AuthDecision::Allow;
        };
        if client_ip.is_loopback() {
            return AuthDecision::Allow;
        }

        if let Some(presented) = read_bearer(headers) {
            if constant_time_eq(presented, expected) {
                return AuthDecision::AllowAndPersistCookie;
            }
        }
        if let Some(presented) = headers.get(AUTH_TOKEN_HEADER).and_then(|v| v.to_str().ok()) {
            if constant_time_eq(presented, expected) {
                return AuthDecision::AllowAndPersistCookie;
            }
        }
        if let Some(presented) = read_cookie_token(headers) {
            if constant_time_eq(&presented, expected) {
                return AuthDecision::Allow;
            }
        }
        if let Some(presented) = read_query_token(query) {
            if constant_time_eq(&presented, expected) {
                return AuthDecision::AllowAndPersistCookie;
            }
        }
        AuthDecision::Reject
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthDecision {
    Allow,
    AllowAndPersistCookie,
    Reject,
}

/// Axum middleware factory. Returns a `from_fn_with_state`-compatible
/// closure when wired into a router.
pub async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<AuthState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    let headers = request.headers().clone();
    let query = request.uri().query().map(|q| q.to_string());
    match state.authenticate(addr.ip(), &headers, query.as_deref()) {
        AuthDecision::Allow => Ok(next.run(request).await),
        AuthDecision::AllowAndPersistCookie => {
            let mut response = next.run(request).await;
            response
                .headers_mut()
                .insert(header::SET_COOKIE, auth_cookie_header(&state));
            Ok(response)
        }
        AuthDecision::Reject => Err(unauthorized_response()),
    }
}

fn unauthorized_response() -> Response {
    let body = serde_json::json!({
        "code": "unauthorized",
        "message": "missing or invalid dashboard auth token",
    });
    let mut resp = Response::new(Body::from(body.to_string()));
    *resp.status_mut() = StatusCode::UNAUTHORIZED;
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        "application/json".parse().expect("valid header"),
    );
    resp.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        "Bearer realm=\"xvision-dashboard\""
            .parse()
            .expect("valid header"),
    );
    resp.into_response()
}

fn read_bearer(headers: &HeaderMap) -> Option<&str> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    raw.strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
}

fn read_query_token(query: Option<&str>) -> Option<String> {
    let q = query?;
    for pair in q.split('&') {
        let mut it = pair.splitn(2, '=');
        let key = it.next()?;
        if key == AUTH_TOKEN_QUERY_PARAM {
            let value = it.next().unwrap_or("");
            return Some(percent_decode(value));
        }
    }
    None
}

fn read_cookie_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in raw.split(';') {
        let trimmed = pair.trim();
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        if name == AUTH_COOKIE_NAME {
            return Some(percent_decode(value));
        }
    }
    None
}

fn auth_cookie_header(state: &AuthState) -> axum::http::HeaderValue {
    let token = state.inner.required_token.as_deref().unwrap_or("");
    let value = format!(
        "{AUTH_COOKIE_NAME}={}; Path=/; HttpOnly; SameSite=Lax",
        percent_encode_cookie_value(token),
    );
    axum::http::HeaderValue::from_str(&value).expect("auth cookie value must be valid header")
}

/// Minimal percent-decode: handles `+` → space and `%XX` byte escapes.
/// We don't bring `urlencoding` in just for one call site; the auth
/// token doesn't need full WHATWG URL parsing.
fn percent_decode(input: &str) -> String {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(h), Some(l)) = (hi, lo) {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn percent_encode_cookie_value(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn is_loopback(addr: &SocketAddr) -> bool {
    // Strict loopback check. `0.0.0.0` / `::` bind to every interface
    // (including the public one), so they're conservatively treated as
    // non-loopback even though they accept loopback connections too.
    // Operators who genuinely want loopback-only should bind
    // `127.0.0.1:<port>` explicitly.
    addr.ip().is_loopback()
}

/// Constant-time string compare. Avoids leaking secret length / prefix
/// via timing for the secret-comparison path. Falls back to length
/// inequality returning false without comparing further.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    /// Serialize env-mutating tests. Without it, cargo runs lib tests
    /// in parallel and races on `AUTH_TOKEN_ENV`.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn headers_with(name: &str, value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            name.parse::<axum::http::HeaderName>().unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
        h
    }

    fn allowed(decision: AuthDecision) -> bool {
        matches!(
            decision,
            AuthDecision::Allow | AuthDecision::AllowAndPersistCookie,
        )
    }

    #[test]
    fn loopback_only_lets_everything_through() {
        let state = AuthState::loopback_only();
        assert!(allowed(state.authenticate(
            ip("127.0.0.1"),
            &HeaderMap::new(),
            None,
        )));
        assert!(allowed(state.authenticate(
            ip("203.0.113.5"),
            &HeaderMap::new(),
            None,
        )));
        assert!(!state.is_gated());
    }

    #[test]
    fn non_loopback_rejects_when_token_missing() {
        let state = AuthState::with_required_token("s3cr3t".into());
        assert_eq!(
            state.authenticate(ip("203.0.113.5"), &HeaderMap::new(), None),
            AuthDecision::Reject,
        );
        assert!(state.is_gated());
    }

    #[test]
    fn non_loopback_accepts_with_bearer_header() {
        let state = AuthState::with_required_token("s3cr3t".into());
        let headers = headers_with("authorization", "Bearer s3cr3t");
        assert_eq!(
            state.authenticate(ip("203.0.113.5"), &headers, None),
            AuthDecision::AllowAndPersistCookie,
        );
    }

    #[test]
    fn non_loopback_accepts_with_xvision_token_header() {
        let state = AuthState::with_required_token("s3cr3t".into());
        let headers = headers_with(AUTH_TOKEN_HEADER, "s3cr3t");
        assert_eq!(
            state.authenticate(ip("203.0.113.5"), &headers, None),
            AuthDecision::AllowAndPersistCookie,
        );
    }

    #[test]
    fn non_loopback_accepts_with_auth_cookie_without_resetting_cookie() {
        let state = AuthState::with_required_token("s3cr3t".into());
        let headers = headers_with("cookie", "theme=dark; xvn_dashboard_token=s3cr3t; other=1");
        assert_eq!(
            state.authenticate(ip("203.0.113.5"), &headers, None),
            AuthDecision::Allow,
        );
    }

    #[test]
    fn non_loopback_accepts_with_query_token() {
        let state = AuthState::with_required_token("s3cr3t".into());
        assert_eq!(
            state.authenticate(ip("203.0.113.5"), &HeaderMap::new(), Some("token=s3cr3t&foo=bar"),),
            AuthDecision::AllowAndPersistCookie,
        );
    }

    #[test]
    fn query_token_must_url_decode() {
        let state = AuthState::with_required_token("a b".into());
        assert_eq!(
            state.authenticate(ip("203.0.113.5"), &HeaderMap::new(), Some("token=a%20b")),
            AuthDecision::AllowAndPersistCookie,
        );
    }

    #[test]
    fn loopback_clients_pass_even_when_server_is_gated() {
        let state = AuthState::with_required_token("s3cr3t".into());
        // Even though the server bound to 0.0.0.0, a request from 127.0.0.1
        // is treated as trusted (loopback is the operator's own machine).
        assert_eq!(
            state.authenticate(ip("127.0.0.1"), &HeaderMap::new(), None),
            AuthDecision::Allow,
        );
        assert_eq!(
            state.authenticate(ip("::1"), &HeaderMap::new(), None),
            AuthDecision::Allow,
        );
    }

    #[test]
    fn wrong_token_is_rejected_via_every_channel() {
        let state = AuthState::with_required_token("right".into());
        let header_wrong = headers_with("authorization", "Bearer wrong");
        assert_eq!(
            state.authenticate(ip("203.0.113.5"), &header_wrong, None),
            AuthDecision::Reject,
        );
        let h2 = headers_with(AUTH_TOKEN_HEADER, "wrong");
        assert_eq!(
            state.authenticate(ip("203.0.113.5"), &h2, None),
            AuthDecision::Reject,
        );
        assert_eq!(
            state.authenticate(ip("203.0.113.5"), &HeaderMap::new(), Some("token=wrong")),
            AuthDecision::Reject,
        );
    }

    #[test]
    fn auth_cookie_percent_encodes_token_for_browser_bootstrap() {
        let state = AuthState::with_required_token("a b;c".into());
        let header = auth_cookie_header(&state);
        let value = header.to_str().unwrap();
        assert!(value.contains("xvn_dashboard_token=a%20b%3Bc"));
        assert!(value.contains("Path=/"));
        assert!(value.contains("HttpOnly"));
    }

    #[test]
    fn from_env_loopback_bind_is_loopback_only_regardless_of_env() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let addr: SocketAddr = "127.0.0.1:8788".parse().unwrap();
        let prev = std::env::var(AUTH_TOKEN_ENV).ok();
        std::env::remove_var(AUTH_TOKEN_ENV);
        let state = AuthState::from_env(&addr).unwrap();
        assert!(!state.is_gated());
        match prev {
            Some(v) => std::env::set_var(AUTH_TOKEN_ENV, v),
            None => std::env::remove_var(AUTH_TOKEN_ENV),
        }
    }

    #[test]
    fn from_env_non_loopback_without_token_is_open() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let addr: SocketAddr = "203.0.113.5:8788".parse().unwrap();
        let prev = std::env::var(AUTH_TOKEN_ENV).ok();
        std::env::remove_var(AUTH_TOKEN_ENV);
        // No longer an error — the server starts open and the operator
        // can set a password later via Settings UI.
        let state = AuthState::from_env(&addr).unwrap();
        assert!(!state.is_gated());
        match prev {
            Some(v) => std::env::set_var(AUTH_TOKEN_ENV, v),
            None => std::env::remove_var(AUTH_TOKEN_ENV),
        }
    }

    #[test]
    fn from_env_non_loopback_with_token_gates() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let addr: SocketAddr = "203.0.113.5:8788".parse().unwrap();
        let prev = std::env::var(AUTH_TOKEN_ENV).ok();
        std::env::set_var(AUTH_TOKEN_ENV, "s3cr3t");
        let state = AuthState::from_env(&addr).unwrap();
        assert!(state.is_gated());
        match prev {
            Some(v) => std::env::set_var(AUTH_TOKEN_ENV, v),
            None => std::env::remove_var(AUTH_TOKEN_ENV),
        }
    }

    #[test]
    fn constant_time_eq_handles_unequal_lengths_without_panic() {
        assert!(!constant_time_eq("abc", "abcd"));
        assert!(!constant_time_eq("", "x"));
        assert!(constant_time_eq("", ""));
        assert!(constant_time_eq("hello", "hello"));
    }
}
