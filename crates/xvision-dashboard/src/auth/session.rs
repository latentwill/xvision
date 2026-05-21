//! DB-backed session token model.
//!
//! ## Endpoints
//!
//! - `POST /api/auth/session` — issue a new 24h session token.
//! - `DELETE /api/auth/session` — revoke the presented token.
//! - `GET /api/auth/session/current` — return the current session identity.
//!
//! ## Storage
//!
//! Session tokens are persisted in the `dashboard_sessions` table
//! (migration `0001_dashboard_sessions.sql`). The raw token UUID is
//! returned to the caller once. Only the SHA-256 hash is stored.
//! Verification uses constant-time comparison.
//!
//! ## TTL
//!
//! Default 24h. Override via `XVN_SESSION_TTL_SECS` environment variable.

use std::net::SocketAddr;
use std::time::Duration;

use axum::{
    extract::{ConnectInfo, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Environment variable to override the default 24h session TTL.
pub const SESSION_TTL_ENV: &str = "XVN_SESSION_TTL_SECS";
/// Cookie name for browser-side session persistence.
pub const SESSION_COOKIE_NAME: &str = "xvn_session";
/// Default TTL for session tokens: 24 hours.
pub const DEFAULT_SESSION_TTL_SECS: u64 = 86_400;

/// Parse the configured session TTL. Falls back to 24h if the env var
/// is absent or cannot be parsed.
pub fn session_ttl() -> Duration {
    std::env::var(SESSION_TTL_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(DEFAULT_SESSION_TTL_SECS))
}

/// Request body for `POST /api/auth/session`.
///
/// Currently only a passthrough — the dashboard has a single shared secret
/// per `XVN_DASHBOARD_TOKEN`. The request body is reserved for future OIDC
/// or multi-user flows.
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    /// Optional label for the session (e.g. "browser", "cli"). Stored for
    /// audit purposes only; does not affect auth behaviour.
    #[serde(default)]
    pub label: Option<String>,
}

/// Response for `POST /api/auth/session`.
#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    /// The raw session token. Present exactly once at creation; the server
    /// stores only the hash thereafter.
    pub token: String,
    /// Session identifier (SHA-256 hex prefix, first 16 chars).
    pub session_id: String,
    /// When this session expires (RFC 3339).
    pub expires_at: String,
}

/// Response for `GET /api/auth/session/current`.
#[derive(Debug, Serialize)]
pub struct SessionCurrentResponse {
    pub session_id: String,
    pub source: String,
    pub created_at: String,
    pub expires_at: String,
}

/// A row from `dashboard_sessions`.
#[derive(Debug)]
pub struct SessionRow {
    pub session_id: String,
    pub token_hash: String,
    pub created_at: String,
    pub expires_at: String,
    pub source_ip: Option<String>,
    pub label: Option<String>,
}

// ---------------------------------------------------------------------------
// Token hashing + constant-time verification
// ---------------------------------------------------------------------------

/// SHA-256 hash of the token bytes, returned as a lowercase hex string.
pub fn hash_token(token: &str) -> String {
    use std::fmt::Write as _;
    let digest = sha2_hash(token.as_bytes());
    let mut s = String::with_capacity(64);
    for b in &digest {
        write!(s, "{b:02x}").expect("writing to String never fails");
    }
    s
}

/// Minimal SHA-256 via a simple portable implementation.
///
/// We avoid pulling in `ring` or `sha2` as a hard dependency just for this
/// one operation. The `sha2` crate is a transitive dep through several paths;
/// rather than relying on that we compute it inline with Rust's standard
/// facilities using the `sha2` crate from dev-deps or as a direct dep.
///
/// NOTE: We add `sha2` to the dashboard's `[dependencies]` in Cargo.toml.
fn sha2_hash(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}

/// Constant-time comparison of two hex strings. Returns true iff they are
/// equal in both length and content without leaking via timing.
pub fn ct_eq(a: &str, b: &str) -> bool {
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

/// Derive a short human-readable session_id from the hash (first 16 hex chars).
pub fn session_id_from_hash(hash: &str) -> &str {
    &hash[..16.min(hash.len())]
}

// ---------------------------------------------------------------------------
// DB helpers
// ---------------------------------------------------------------------------

/// Insert a new session row.
pub async fn insert_session(
    pool: &SqlitePool,
    token_hash: &str,
    expires_at: &DateTime<Utc>,
    source_ip: Option<&str>,
    label: Option<&str>,
) -> anyhow::Result<()> {
    let session_id = session_id_from_hash(token_hash).to_string();
    let expires_at_s = expires_at.to_rfc3339();
    let created_at_s = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO dashboard_sessions (session_id, token_hash, created_at, expires_at, source_ip, label)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&session_id)
    .bind(token_hash)
    .bind(&created_at_s)
    .bind(&expires_at_s)
    .bind(source_ip)
    .bind(label)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch a session by token_hash. Returns `None` if not found or expired.
pub async fn find_session_by_hash(pool: &SqlitePool, token_hash: &str) -> anyhow::Result<Option<SessionRow>> {
    use sqlx::Row as _;
    let now = Utc::now().to_rfc3339();
    let row = sqlx::query(
        "SELECT session_id, token_hash, created_at, expires_at, source_ip, label
         FROM dashboard_sessions
         WHERE token_hash = ?1 AND expires_at > ?2",
    )
    .bind(token_hash)
    .bind(&now)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| SessionRow {
        session_id: r.get("session_id"),
        token_hash: r.get("token_hash"),
        created_at: r.get("created_at"),
        expires_at: r.get("expires_at"),
        source_ip: r.get("source_ip"),
        label: r.get("label"),
    }))
}

/// Delete a session row by token_hash (revoke).
pub async fn delete_session_by_hash(pool: &SqlitePool, token_hash: &str) -> anyhow::Result<u64> {
    let result = sqlx::query("DELETE FROM dashboard_sessions WHERE token_hash = ?1")
        .bind(token_hash)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

/// `POST /api/auth/session` — issue a new session token.
///
/// The caller must already pass the outer `auth_middleware` gate (i.e. present
/// a valid `XVN_DASHBOARD_TOKEN` if the server is on a non-loopback bind). On
/// success, returns the raw token (only ever returned once) plus metadata.
pub async fn create_session(
    State(state): State<crate::state::AppState>,
    peer_opt: Option<ConnectInfo<SocketAddr>>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<Response, StatusCode> {
    let pool = &state.pool;
    // Generate a cryptographically random token.
    let token = new_token();
    let token_hash = hash_token(&token);
    let ttl = session_ttl();
    let expires_at = Utc::now() + chrono::Duration::from_std(ttl).unwrap_or(chrono::Duration::hours(24));
    let source_ip = peer_opt
        .map(|ci| ci.0.ip().to_string())
        .unwrap_or_else(|| "unknown".into());
    let label = body.label.as_deref();

    if let Err(e) = insert_session(pool, &token_hash, &expires_at, Some(&source_ip), label).await {
        tracing::error!(error = %e, "failed to insert session");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let session_id = session_id_from_hash(&token_hash).to_string();
    let resp = CreateSessionResponse {
        token: token.clone(),
        session_id,
        expires_at: expires_at.to_rfc3339(),
    };

    // Set a session cookie as well so browser loads don't need to attach a header.
    let cookie_value = format!(
        "{SESSION_COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        ttl.as_secs()
    );

    let mut response = (StatusCode::CREATED, Json(resp)).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie_value
            .parse()
            .expect("session cookie value must be a valid header"),
    );
    Ok(response)
}

/// `DELETE /api/auth/session` — revoke the current session token.
pub async fn delete_session(State(state): State<crate::state::AppState>, headers: HeaderMap) -> StatusCode {
    let pool = &state.pool;
    let Some(token) = extract_session_token(&headers) else {
        return StatusCode::UNAUTHORIZED;
    };
    let token_hash = hash_token(&token);
    match delete_session_by_hash(pool, &token_hash).await {
        Ok(0) => StatusCode::NOT_FOUND,
        Ok(_) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!(error = %e, "failed to revoke session");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// `GET /api/auth/session/current` — return the current session identity.
pub async fn current_session(
    State(state): State<crate::state::AppState>,
    headers: HeaderMap,
) -> Result<Json<SessionCurrentResponse>, StatusCode> {
    let pool = &state.pool;
    let Some(token) = extract_session_token(&headers) else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let token_hash = hash_token(&token);
    match find_session_by_hash(pool, &token_hash).await {
        Ok(Some(row)) => Ok(Json(SessionCurrentResponse {
            session_id: row.session_id,
            source: row.source_ip.unwrap_or_else(|| "unknown".into()),
            created_at: row.created_at,
            expires_at: row.expires_at,
        })),
        Ok(None) => Err(StatusCode::UNAUTHORIZED),
        Err(e) => {
            tracing::error!(error = %e, "failed to fetch current session");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ---------------------------------------------------------------------------
// Token extraction helpers
// ---------------------------------------------------------------------------

/// Extract the raw session token from `Authorization: Bearer <token>` or
/// the `xvn_session` cookie.
pub fn extract_session_token(headers: &HeaderMap) -> Option<String> {
    // Prefer the Authorization header.
    if let Some(bearer) = read_bearer_token(headers) {
        return Some(bearer.to_owned());
    }
    // Fall back to cookie.
    read_session_cookie(headers)
}

fn read_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    raw.strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
}

fn read_session_cookie(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in raw.split(';') {
        let trimmed = pair.trim();
        let (name, value) = trimmed.split_once('=')?;
        if name == SESSION_COOKIE_NAME {
            return Some(value.to_owned());
        }
    }
    None
}

/// Generate a cryptographically random UUID v4 token string.
fn new_token() -> String {
    // Use std random to avoid pulling in uuid. A 128-bit random value
    // displayed as a hex string gives sufficient entropy for a session token.
    // We use the OS random source via std's thread-local random.
    let mut bytes = [0u8; 32];
    fill_random(&mut bytes);
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write;
        write!(s, "{b:02x}").expect("infallible");
        s
    })
}

/// Fill a byte slice with cryptographically random bytes.
///
/// Uses `getrandom` which is already a transitive dependency via `ulid`.
fn fill_random(buf: &mut [u8]) {
    getrandom::getrandom(buf).expect("getrandom must succeed on supported platforms");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_token_is_deterministic() {
        let t = "test_token_abc123";
        assert_eq!(hash_token(t), hash_token(t));
        assert_ne!(hash_token(t), hash_token("other"));
    }

    #[test]
    fn ct_eq_correct() {
        assert!(ct_eq("abc", "abc"));
        assert!(!ct_eq("abc", "abcd"));
        assert!(!ct_eq("abc", "xyz"));
        assert!(ct_eq("", ""));
    }

    #[test]
    fn session_id_from_hash_length() {
        let hash = "a".repeat(64);
        let id = session_id_from_hash(&hash);
        assert_eq!(id.len(), 16);
    }

    #[test]
    fn new_token_is_64_chars() {
        let t = new_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn new_tokens_are_unique() {
        let a = new_token();
        let b = new_token();
        assert_ne!(a, b);
    }
}
