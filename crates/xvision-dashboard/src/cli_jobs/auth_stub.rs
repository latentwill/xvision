//! Temporary AuthContext placeholder for the remote CLI job audit trail.
//!
//! ## Why this exists
//!
//! `v2b-dashboard-auth-boundary` will land `xvision_dashboard::auth::AuthContext`
//! with a richer shape (per-user identity, Tailscale node info, RBAC). That
//! track is not yet merged as of 2026-05-21.
//!
//! This stub matches the planned shape (`user`, `source`) and populates it from
//! whatever identity information the existing request handlers already have:
//! the `x-xvision-token` shared-secret auth layer knows whether the request
//! came over the loopback or from a Tailscale node, but does not yet carry
//! per-user identity.
//!
//! ## Migration path
//!
//! When `v2b-dashboard-auth-boundary` is merged:
//! 1. Delete this file.
//! 2. Change every `use crate::cli_jobs::auth_stub::AuthContext` import to
//!    `use crate::auth::AuthContext` (or wherever the real type lands).
//! 3. Remove this module from `crates/xvision-dashboard/src/cli_jobs/mod.rs`.
//!
//! **Note to the operator:** this stub needs a small follow-up PR after
//! `v2b-dashboard-auth-boundary` lands. The PR that introduces this stub
//! (v2b-remote-cli-job-safety) explicitly notes this coordination item.

/// Lightweight caller-identity snapshot captured at job creation time.
///
/// This is a placeholder matching the shape planned by
/// `v2b-dashboard-auth-boundary`. When that track merges, this struct is
/// deleted and replaced by `xvision_dashboard::auth::AuthContext`.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// Human-readable user identifier. Examples:
    /// - `"tailscale:hostname"` — Tailscale node that submitted the job
    /// - `"localhost"` — loopback request from the same host
    /// - `"unknown:dashboard"` — fallback when identity cannot be determined
    pub user: String,
    /// Source descriptor. Examples: `"tailscale:<node>"`, `"localhost"`, `"unknown"`.
    pub source: String,
}

impl AuthContext {
    /// Build an `AuthContext` from what the existing dashboard auth layer knows.
    ///
    /// The current dashboard auth uses a shared-secret bearer token but does
    /// not carry per-user identity. We derive `user` and `source` from whether
    /// the request came via loopback or not, plus an optional Tailscale node
    /// name pulled from the `X-Tailscale-Node` header (if the Tailscale
    /// sidecar injects it in future).
    ///
    /// `is_loopback`: true when the client IP is 127.0.0.1 / ::1.
    /// `tailscale_node`: optional value from a `X-Tailscale-Node`-style header.
    pub fn from_request(is_loopback: bool, tailscale_node: Option<&str>) -> Self {
        if is_loopback {
            return Self {
                user: "localhost".into(),
                source: "localhost".into(),
            };
        }
        match tailscale_node {
            Some(node) if !node.is_empty() => Self {
                user: format!("tailscale:{node}"),
                source: format!("tailscale:{node}"),
            },
            _ => Self {
                user: "unknown:dashboard".into(),
                source: "unknown".into(),
            },
        }
    }

    /// Convenience: always returns `"unknown:dashboard"` / `"unknown"`.
    /// Used in code paths that don't have access to the request context
    /// (background tasks, startup recovery).
    pub fn unknown() -> Self {
        Self {
            user: "unknown:dashboard".into(),
            source: "unknown".into(),
        }
    }
}
