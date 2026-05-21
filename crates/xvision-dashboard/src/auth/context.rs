//! Canonical `AuthContext` — per-request caller identity.
//!
//! This is the single, authoritative definition consumed by every route that
//! needs to know who is making the request. The `cli_jobs` stub
//! (`crates/xvision-dashboard/src/cli_jobs/auth_stub.rs`) mirrors this shape
//! and will be replaced by an import of this type in a small follow-up PR
//! (noted in the v2b-dashboard-auth-boundary PR body).
//!
//! # Shape contract
//!
//! At minimum `{ user: String, source: String }` — chosen so the stub swap
//! in `v2b-remote-cli-job-safety` and `v2b-broker-wallet-kill-switch` is a
//! one-line import change without breaking call sites.

/// Caller-identity snapshot captured at the point of authentication.
///
/// For session-token requests this is populated from the `dashboard_sessions`
/// row. For loopback requests (always allowed) this is populated from the
/// request metadata. For Tailscale CLI requests this may carry the Tailscale
/// node name (see `README.md` for the exemption rationale).
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthContext {
    /// Human-readable user identifier.
    ///
    /// Examples:
    /// - `"session:<session_id>"` — authenticated via a dashboard session token
    /// - `"tailscale:<node>"` — Tailscale node name from `X-Tailscale-Node`
    /// - `"localhost"` — loopback request from the same host
    /// - `"unknown"` — fallback when identity cannot be determined
    pub user: String,

    /// Source descriptor indicating *how* the caller was identified.
    ///
    /// Examples: `"session"`, `"tailscale:<node>"`, `"localhost"`, `"unknown"`.
    pub source: String,
}

impl AuthContext {
    /// Build an `AuthContext` from a verified session token.
    pub fn from_session(session_id: &str) -> Self {
        Self {
            user: format!("session:{session_id}"),
            source: "session".into(),
        }
    }

    /// Build an `AuthContext` for a loopback (localhost) request.
    pub fn from_loopback() -> Self {
        Self {
            user: "localhost".into(),
            source: "localhost".into(),
        }
    }

    /// Build an `AuthContext` for a Tailscale-authenticated request.
    ///
    /// Used when the Tailscale sidecar injects a `X-Tailscale-Node` header.
    pub fn from_tailscale(node: &str) -> Self {
        Self {
            user: format!("tailscale:{node}"),
            source: format!("tailscale:{node}"),
        }
    }

    /// Fallback identity when the source cannot be determined.
    pub fn unknown() -> Self {
        Self {
            user: "unknown".into(),
            source: "unknown".into(),
        }
    }
}
