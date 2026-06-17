//! `xvision-dashboard` — axum HTTP server hosting the xvision web SPA.
//!
//! Phase A scope: route scaffolding (`/api/health`) plus an embedded SPA loader
//! so `xvn dashboard serve` boots end-to-end. Phase B adds typed API routes
//! that wrap `xvision_engine::api::*`.

pub mod auth;
pub mod autoresearch_runner;
pub mod chain_config;
pub mod ratelimit;
pub mod chat_unified;
pub mod cli_jobs;
pub mod embed;
pub mod error;
pub mod hooks;
pub mod ipc;
pub mod llm_dispatch;
pub mod marketplace_index;
pub mod marketplace_nonce;
pub mod redact;
pub mod routes;
pub mod server;
pub mod session_bus;
pub mod sse;
pub mod state;
pub mod wizard_loop;

pub use server::serve;
pub use state::AppState;
