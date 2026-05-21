//! Engine-side `AuthContext` — caller identity captured at the safety boundary.
//!
//! This is the canonical engine-crate identity type used by the safety
//! subsystem (pause/resume audit, broker-submit gating, wallet writes).
//! The dashboard crate has its own
//! [`xvision_dashboard::auth::AuthContext`] (richer: session ids, Tailscale
//! node info). The dashboard converts its richer type to this engine-side
//! shape at the engine API boundary — see
//! `xvision_dashboard::routes::safety` for the conversion site.
//!
//! Kept in `xvision-engine` (not `xvision-dashboard`) because the engine's
//! safety APIs cannot depend on dashboard types: `xvision-dashboard` depends
//! on `xvision-engine`, not the other way around.

/// Minimal authentication context used by the safety subsystem.
///
/// Shape: `{ user, source }`. The dashboard's richer
/// `xvision_dashboard::auth::AuthContext` collapses to this shape at the
/// engine call boundary.
#[derive(Debug, Clone, Default)]
pub struct AuthContext {
    /// Username / identity (e.g. `"session:<id>"`, `"tailscale:<node>"`,
    /// `"localhost"`, `"system"`, `"anonymous"`).
    pub user: String,
    /// Source surface: `"session"`, `"tailscale:<node>"`, `"localhost"`,
    /// `"api"`, `"cli"`, `"mcp"`, or `"system"`.
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
