//! `/api/memory` — axum handlers around `engine::api::memory::*`.
//!
//! Operator-facing surface for V2D Observations + Patterns (v1.1
//! follow-up to PR #404). The engine module is the source of truth for
//! request/response shapes and business logic; this file is a thin
//! axum-shaped wrapper:
//!
//! - `GET    /api/memory`                  → `engine::api::memory::list`
//! - `GET    /api/memory/:id`              → `engine::api::memory::get`
//! - `POST   /api/memory/patterns`         → `engine::api::memory::create_pattern`
//! - `DELETE /api/memory/:id`              → `engine::api::memory::delete_one`
//! - `DELETE /api/memory`                  → `engine::api::memory::forget`
//! - `POST   /api/memory/undo-forget`      → `engine::api::memory::undo_forget`
//!
//! ## Memory store sourcing
//!
//! The dashboard's per-request `ApiContext` (`AppState::api_context()`)
//! is constructed via `ApiContext::new` and does NOT carry a
//! `MemoryRecorder`, so these handlers can't read the store off the
//! context. Instead we open it lazily via
//! `engine::api::memory::open_default_store` and cache the resulting
//! `Arc<MemoryStore>` in a process-wide `OnceCell` — pool open is
//! sub-millisecond, but doing it once per HTTP request would still be
//! wasteful in steady state.
//!
//! ## Authentication
//!
//! Whatever auth posture `crate::auth::auth_middleware` enforces at
//! the router boundary applies here unchanged. The v2b dashboard auth
//! boundary track (in flight) will retrofit per-route RBAC; until then
//! these mutate routes inherit the existing loopback-or-shared-secret
//! gate, matching every other dashboard mutate route (agents,
//! strategies, scenarios, eval).

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;

use xvision_engine::api::memory::{
    self, ForgetResponse, ListMemoryRequest, MemoryItemDto, MemoryListResponse, PatternCreateRequest,
    UndoForgetRequest, UndoForgetResponse,
};
use xvision_memory::store::MemoryStore;

use crate::error::DashboardError;
use crate::state::AppState;

/// Process-wide memory store handle. Resolved on first request and
/// reused afterwards — the file path comes from `$XVN_MEMORY_DB` (or
/// `~/.xvn/memory.db`) so we can't bake it into `AppState` without
/// touching state.rs, which is outside this contract's
/// `allowed_paths`. `OnceCell` keeps the resolution lazy + thread-safe.
static MEMORY_STORE: OnceCell<Arc<MemoryStore>> = OnceCell::const_new();

async fn resolve_store() -> Result<Arc<MemoryStore>, DashboardError> {
    let store = MEMORY_STORE.get_or_try_init(memory::open_default_store).await?;
    Ok(store.clone())
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    pub tier: Option<String>,
    pub namespace: Option<String>,
    pub agent: Option<String>,
    pub scenario_id: Option<String>,
    pub run_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// When `Some(true)` the response includes soft-deleted rows. Default
    /// hides them.
    pub include_forgotten: Option<bool>,
}

impl From<ListQuery> for ListMemoryRequest {
    fn from(q: ListQuery) -> Self {
        ListMemoryRequest {
            tier: q.tier,
            namespace: q.namespace,
            agent: q.agent,
            scenario_id: q.scenario_id,
            run_id: q.run_id,
            limit: q.limit,
            offset: q.offset,
            include_forgotten: q.include_forgotten,
        }
    }
}

#[derive(Deserialize, Default)]
pub struct ForgetQuery {
    /// Exact namespace to clear. Mutually exclusive with `agent`.
    pub namespace: Option<String>,
    /// Convenience: clear `agent:<id>`.
    pub agent: Option<String>,
}

#[derive(Serialize)]
pub struct EmptyResponse {}

pub async fn list(
    State(_state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<MemoryListResponse>, DashboardError> {
    let store = resolve_store().await?;
    let resp = memory::list(&store, q.into()).await?;
    Ok(Json(resp))
}

pub async fn get(
    Path(id): Path<String>,
    State(_state): State<AppState>,
) -> Result<Json<MemoryItemDto>, DashboardError> {
    let store = resolve_store().await?;
    let item = memory::get(&store, &id).await?;
    Ok(Json(item))
}

pub async fn create_pattern(
    State(_state): State<AppState>,
    Json(body): Json<PatternCreateRequest>,
) -> Result<(StatusCode, Json<MemoryItemDto>), DashboardError> {
    let store = resolve_store().await?;
    // No embedder wired in v1.1 — operator-seeded Patterns are stored
    // with an empty embedding vector, marked with the synthetic
    // "operator-seed" embedder id so audit traces distinguish them
    // from auto-recorded items. The dispatcher's recall path can't
    // match unembedded Patterns; the CLI + UI both warn about this at
    // seed time. Re-embedding is a follow-up (intake Decision 7
    // delegates embedder UX to follow-ups).
    let embedder_id = "operator-seed";
    let item = memory::create_pattern(&store, embedder_id, Vec::new(), body).await?;
    Ok((StatusCode::CREATED, Json(item)))
}

pub async fn delete_one(
    Path(id): Path<String>,
    State(_state): State<AppState>,
) -> Result<StatusCode, DashboardError> {
    let store = resolve_store().await?;
    memory::delete_one(&store, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn forget(
    State(_state): State<AppState>,
    Query(q): Query<ForgetQuery>,
) -> Result<Json<ForgetResponse>, DashboardError> {
    let namespace = match (q.namespace, q.agent) {
        (Some(_), Some(_)) => {
            return Err(DashboardError::Validation {
                field: "namespace".into(),
                msg: "set either `namespace` or `agent`, not both".into(),
            });
        }
        (Some(ns), None) => ns,
        (None, Some(agent)) => memory::agent_namespace(&agent),
        (None, None) => {
            return Err(DashboardError::Validation {
                field: "namespace".into(),
                msg: "one of `namespace` or `agent` is required".into(),
            });
        }
    };
    let store = resolve_store().await?;
    let resp = memory::forget(&store, &namespace).await?;
    Ok(Json(resp))
}

/// `POST /api/memory/undo-forget` — restore rows soft-deleted by a
/// recent `DELETE /api/memory` call. Rows whose `forgotten_at` is older
/// than the grace window are not restored (the janitor sweep is about
/// to or already has hard-deleted them).
pub async fn undo_forget(
    State(_state): State<AppState>,
    Json(body): Json<UndoForgetRequest>,
) -> Result<Json<UndoForgetResponse>, DashboardError> {
    let store = resolve_store().await?;
    let resp = memory::undo_forget(&store, body).await?;
    Ok(Json(resp))
}
