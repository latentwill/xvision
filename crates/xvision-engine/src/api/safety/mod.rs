//! `GET /api/safety/state`
//! `POST /api/safety/pause`
//! `POST /api/safety/resume`
//! `GET /api/safety/audit`
//!
//! All write endpoints are auth-required. In this track we use the
//! `AuthContext` stub; when `v2b-dashboard-auth-boundary` merges the import
//! swaps to `xvision_dashboard::auth::AuthContext`.

pub mod routes;

pub use routes::{get_audit, get_state, pause, resume, PauseRequest};
