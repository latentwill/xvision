//! `xvision-dashboard` — axum HTTP server hosting the xvision web SPA.
//!
//! Phase A scope: route scaffolding (`/api/health`) plus an embedded SPA loader
//! so `xvn dashboard serve` boots end-to-end. Phase B adds typed API routes
//! that wrap `xvision_engine::api::*`.

pub mod embed;
pub mod error;
pub mod routes;
pub mod server;

pub use server::serve;
