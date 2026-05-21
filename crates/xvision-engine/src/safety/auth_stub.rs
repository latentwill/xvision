//! Local AuthContext placeholder.
//!
//! `v2b-dashboard-auth-boundary` is landing the canonical
//! `xvision_dashboard::auth::AuthContext` in parallel. This stub carries the
//! same shape so `safety_audit.user` + pause-toggle audit compile today.
//! When auth-boundary merges, a small follow-up PR deletes this file and
//! swaps the import — same pattern PR #447 used in `cli_jobs/auth_stub.rs`.

/// Minimal authentication context used by the safety subsystem until
/// `v2b-dashboard-auth-boundary` lands the canonical
/// `xvision_dashboard::auth::AuthContext`.
#[derive(Debug, Clone, Default)]
pub struct AuthContext {
    /// Username / identity — free text for now; the auth-boundary track
    /// will populate this from the session token.
    pub user: String,
    /// Source surface: `"api"`, `"cli"`, `"mcp"`, or `"system"`.
    pub source: String,
}

impl AuthContext {
    pub fn system() -> Self {
        Self {
            user: "system".into(),
            source: "system".into(),
        }
    }

    pub fn api_anonymous() -> Self {
        Self {
            user: "anonymous".into(),
            source: "api".into(),
        }
    }
}
