//! Per-route auth middleware for mutating dashboard routes.
//!
//! ## Purpose
//!
//! `require_auth` is an axum `from_fn`-compatible middleware that:
//!
//! 1. Reads the operator-chosen dashboard password hash from the
//!    `dashboard_auth` table.
//! 2. If no password is set (hash is NULL), all requests pass through.
//! 3. If a password IS set, the request must present it via
//!    `Authorization: Bearer <password>`, the `x-xvision-token` header,
//!    or the `xvn_dashboard_password` cookie.
//! 4. On failure returns `{"error": "unauthenticated"}` with HTTP 401.
//!
//! ## Loopback exemption
//!
//! Requests from a loopback IP (127.0.0.1, ::1) are allowed through
//! without a password. This preserves frictionless local dev.
//!
//! ## Usage in `server.rs`
//!
//! Attach `require_auth` as a `.route_layer` on mutating route groups:
//!
//! ```rust,ignore
//! .route("/api/agents", get(agents::list).post(agents::create))
//! .route_layer(axum::middleware::from_fn_with_state(
//!     pool.clone(),
//!     require_auth_middleware,
//! ))
//! ```
//!
//! Read-only GET routes should NOT have this layer; they remain open.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sqlx::SqlitePool;

use super::gate::{AUTH_TOKEN_HEADER, AUTH_TOKEN_QUERY_PARAM};
use super::session::hash_token;

/// Cookie name for the dashboard password (set after a successful header/query
/// bootstrap so the browser doesn't need to re-send the password on every
/// request). Named to avoid collision with the legacy session cookie.
const PASSWORD_COOKIE_NAME: &str = "xvn_dashboard_password";

// ---------------------------------------------------------------------------
// Password hash helpers
// ---------------------------------------------------------------------------

/// Hash a plaintext password with SHA-256 (hex-encoded).
/// Uses the same `hash_token` digest as sessions.
fn hash_password(plaintext: &str) -> String {
    hash_token(plaintext)
}

/// Verify a plaintext candidate against the stored hash.
fn verify_password(plaintext: &str, hash: &str) -> bool {
    let candidate = hash_password(plaintext);
    // Simple comparison — the hash values are hex strings, and the
    // candidate is produced locally. For a local dashboard, timing
    // attacks on the password are not a practical concern.
    candidate == hash
}
async fn get_password_hash(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT password_hash FROM dashboard_auth WHERE id = 1")
        .fetch_one(pool)
        .await
        .ok()
        .flatten()
}

pub async fn verify_configured_dashboard_password(
    pool: &SqlitePool,
    headers: &HeaderMap,
    query: Option<&str>,
) -> bool {
    let Some(stored_hash) = get_password_hash(pool).await else {
        return false;
    };
    let candidate = extract_bearer_token(headers)
        .or_else(|| extract_header_token(headers, AUTH_TOKEN_HEADER))
        .or_else(|| extract_cookie_token(headers, PASSWORD_COOKIE_NAME))
        .or_else(|| extract_query_token(query, AUTH_TOKEN_QUERY_PARAM));
    matches!(candidate, Some(token) if verify_password(&token, &stored_hash))
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/// Axum middleware that enforces dashboard password auth on mutating routes.
///
/// Attach via `.route_layer(axum::middleware::from_fn_with_state(pool, require_auth_middleware))`.
pub async fn require_auth_middleware(
    State(pool): State<SqlitePool>,
    peer_opt: Option<ConnectInfo<SocketAddr>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().to_string();

    // Always exempt loopback.
    if let Some(ConnectInfo(peer)) = &peer_opt {
        if peer.ip().is_loopback() {
            return next.run(request).await;
        }
    }

    // Read the stored password hash. If none is set, dashboard is open.
    let stored_hash = match get_password_hash(&pool).await {
        Some(h) => h,
        // No password set → dashboard is open, allow all.
        None => return next.run(request).await,
    };

    let headers = request.headers();

    // Try each channel: Bearer, custom header, cookie, query.
    let candidate = extract_bearer_token(headers)
        .or_else(|| extract_header_token(headers, AUTH_TOKEN_HEADER))
        .or_else(|| extract_cookie_token(headers, PASSWORD_COOKIE_NAME))
        .or_else(|| extract_query_token(request.uri().query(), AUTH_TOKEN_QUERY_PARAM));

    match candidate {
        Some(token) if verify_password(&token, &stored_hash) => {
            // Set the password cookie on success so the browser can use
            // it for subsequent requests without re-sending the header.
            let mut resp = next.run(request).await;
            set_password_cookie(&mut resp, &token);
            resp
        }
        _ => {
            // Log the rejection.
            tracing::warn!(
                path = %path,
                method = %method,
                peer = ?peer_opt.map(|ci| ci.0.to_string()),
                "require_auth: rejected (password mismatch or missing)"
            );
            unauthenticated()
        }
    }
}

// ---------------------------------------------------------------------------
// Token extraction (shared with gate.rs patterns)
// ---------------------------------------------------------------------------

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ").map(|t| t.to_string())
}

fn extract_header_token(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get(name)?.to_str().ok().map(|s| s.to_string())
}

fn extract_cookie_token(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let kv = part.trim();
        if let Some(value) = kv.strip_prefix(&format!("{name}=")) {
            return Some(value.to_string());
        }
    }
    None
}

fn extract_query_token(query: Option<&str>, param: &str) -> Option<String> {
    let query = query?;
    for part in query.split('&') {
        let kv = part.trim();
        if let Some(value) = kv.strip_prefix(&format!("{param}=")) {
            let decoded = urlencoding::decode(value).ok()?;
            return Some(decoded.into_owned());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn set_password_cookie(response: &mut Response, token: &str) {
    let encoded = urlencoding::encode(token);
    let cookie_value =
        format!("{PASSWORD_COOKIE_NAME}={encoded}; Path=/; HttpOnly; SameSite=Lax; Max-Age=86400");
    if let Ok(value) = cookie_value.parse() {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
}

fn unauthenticated() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({"error": "unauthenticated"})),
    )
        .into_response()
}
