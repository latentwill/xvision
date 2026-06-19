//! Dashboard auth module — session tokens, bearer gate, AuthContext.
//!
//! # Structure
//!
//! * `gate` — shared-secret bearer/cookie/query middleware (non-loopback gate
//!   introduced in qa-dashboard-auth-hardening). Re-exports `AuthState`,
//!   `auth_middleware`, and the `AUTH_TOKEN_*` constants.
//! * `session` — DB-backed session tokens. `POST /api/auth/session` issues a
//!   new session, `DELETE /api/auth/session` revokes it, and
//!   `GET /api/auth/session/current` returns the current session identity.
//! * `require_auth` — per-route middleware that validates a session token from
//!   the `Authorization: Bearer` header (or the `xvn_session` cookie) and
//!   writes an `auth_audit` row for every mutating call.
//! * `context` — canonical `AuthContext` type consumed by every route that
//!   needs to know who is making the request.
//!
//! # Two-layer model
//!
//! The outer `AuthState` / `auth_middleware` (from `gate`) handles the
//! coarse non-loopback gate. The inner `require_auth` middleware handles
//! per-request session validation for mutating routes. Read-only routes are
//! exempt from `require_auth` but still pass through the outer gate.
//!
//! # Tailscale exemption
//!
//! See `crates/xvision-dashboard/src/auth/README.md`.

pub mod context;
pub mod gate;
pub mod login;
pub mod require_auth;
pub mod session;

// Re-export the coarse bearer gate surface so callers can keep using the
// flat `crate::auth::*` import path.
pub use gate::{auth_middleware, AuthState, AUTH_TOKEN_ENV, AUTH_TOKEN_HEADER, AUTH_TOKEN_QUERY_PARAM};

// Re-export the canonical AuthContext.
pub use context::AuthContext;
