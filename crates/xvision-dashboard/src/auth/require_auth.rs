//! Per-route auth middleware for mutating dashboard routes.
//!
//! ## Purpose
//!
//! `require_auth` is an axum `from_fn`-compatible middleware that:
//!
//! 1. Extracts the session token from the `Authorization: Bearer` header or
//!    the `xvn_session` cookie.
//! 2. Verifies it against the `dashboard_sessions` table via a constant-time
//!    hash comparison.
//! 3. Inserts an `auth_audit` row recording the outcome.
//! 4. On failure returns `{"error": "unauthenticated"}` with HTTP 401.
//! 5. On success sets an `x-session-id` response extension so downstream
//!    handlers can read the verified session identity without a second DB hit.
//!
//! ## Loopback exemption
//!
//! Requests from a loopback IP (127.0.0.1, ::1) are allowed through
//! without a session token. The audit row is still written with
//! `source = "localhost"`. This preserves frictionless local dev while
//! ensuring every mutating operation is recorded.
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
//! Read-only GET routes should NOT have this layer; they remain open on
//! loopback binds (see the read/write split comment in each route file).

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use sqlx::SqlitePool;

use super::context::AuthContext;
use super::session::{extract_session_token, find_session_by_hash, hash_token};

// ---------------------------------------------------------------------------
// Audit log
// ---------------------------------------------------------------------------

/// Write one row to `auth_audit`.
async fn write_audit_row(
    pool: &SqlitePool,
    route: &str,
    method: &str,
    session_token_hash: &str,
    source_ip: &str,
    response_status: u16,
) {
    let timestamp = Utc::now().to_rfc3339();
    let status_i64 = i64::from(response_status);
    if let Err(e) = sqlx::query(
        "INSERT INTO auth_audit (timestamp, route, method, session_token_hash, source_ip, response_status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&timestamp)
    .bind(route)
    .bind(method)
    .bind(session_token_hash)
    .bind(source_ip)
    .bind(status_i64)
    .execute(pool)
    .await
    {
        // Audit failures are not fatal but always logged.
        tracing::warn!(error = %e, "failed to write auth_audit row");
    }
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/// Axum middleware that enforces session-token auth on mutating routes.
///
/// Attach via `.route_layer(axum::middleware::from_fn_with_state(pool, require_auth_middleware))`.
pub async fn require_auth_middleware(
    State(pool): State<SqlitePool>,
    peer_opt: Option<ConnectInfo<SocketAddr>>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().as_str().to_uppercase();
    let path = request.uri().path().to_string();
    // Default to localhost when ConnectInfo is not injected (e.g. in unit tests).
    let peer = peer_opt
        .map(|ci| ci.0)
        .unwrap_or_else(|| "127.0.0.1:0".parse().unwrap());
    let source_ip = peer.ip().to_string();
    let is_loopback = peer.ip().is_loopback();

    // Loopback clients are always allowed through; write audit row with
    // the "localhost" identity and proceed.
    if is_loopback {
        let ctx = AuthContext::from_loopback();
        let mut req = request;
        req.extensions_mut().insert(ctx);
        let response = next.run(req).await;
        let status = response.status().as_u16();
        write_audit_row(&pool, &path, &method, "localhost", "localhost", status).await;
        return response;
    }

    let headers = request.headers().clone();
    let token = match extract_session_token(&headers) {
        Some(t) => t,
        None => {
            let status: u16 = 401;
            write_audit_row(&pool, &path, &method, "no-token", &source_ip, status).await;
            return unauthenticated();
        }
    };

    let token_hash = hash_token(&token);
    match find_session_by_hash(&pool, &token_hash).await {
        Ok(Some(session)) => {
            let ctx = AuthContext::from_session(&session.session_id);
            let mut req = request;
            req.extensions_mut().insert(ctx);
            let response = next.run(req).await;
            let status = response.status().as_u16();
            write_audit_row(&pool, &path, &method, &token_hash[..16], &source_ip, status).await;
            response
        }
        Ok(None) => {
            let status: u16 = 401;
            write_audit_row(
                &pool,
                &path,
                &method,
                &token_hash[..16.min(token_hash.len())],
                &source_ip,
                status,
            )
            .await;
            unauthenticated()
        }
        Err(e) => {
            tracing::error!(error = %e, "session lookup failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"error": "internal error"})),
            )
                .into_response()
        }
    }
}

fn unauthenticated() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({"error": "unauthenticated"})),
    )
        .into_response()
}
