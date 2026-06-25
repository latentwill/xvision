//! Flywheel API surface.
//!
//! Thin dashboard wrappers around the engine's offline memory flywheel
//! APIs. Mutating handlers are registered in `server::mutating_router`;
//! read-only status/inspect handlers are registered in
//! `server::readonly_router`.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use xvision_engine::api::{
    autooptimizer::{
        self, AutoOptimizerGateRequest, AutoOptimizerRunDto, AutoOptimizerRunListRequest,
        AutoOptimizerRunListResponse, AutoOptimizerRunRequest,
    },
    flywheel::{
        self, FlywheelLineageDto, FlywheelLineageRequest, FlywheelStatusDto, FlywheelStatusRequest,
        FlywheelVelocityDto, FlywheelVelocityRequest,
    },
    optimize::{
        self, MemoryDemoOptimizeDto, MemoryDemoOptimizeRequest, OptimizationGateDto, OptimizationGateRequest,
    },
};

use crate::error::DashboardError;
use crate::routes::memory as memory_route;
use crate::state::AppState;

#[derive(Deserialize, Default)]
pub struct FlywheelStatusQuery {
    pub namespace: Option<String>,
    pub agent: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct FlywheelVelocityQuery {
    pub namespace: Option<String>,
    pub agent: Option<String>,
    pub days: Option<i64>,
}

#[derive(Deserialize, Default)]
pub struct FlywheelLineageQuery {
    pub namespace: Option<String>,
    pub agent: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Deserialize, Default)]
pub struct AutoOptimizerRunListQuery {
    pub namespace: Option<String>,
    pub agent: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl From<AutoOptimizerRunListQuery> for AutoOptimizerRunListRequest {
    fn from(q: AutoOptimizerRunListQuery) -> Self {
        Self {
            namespace: q.namespace,
            agent: q.agent,
            limit: q.limit,
            offset: q.offset,
        }
    }
}

impl From<FlywheelStatusQuery> for FlywheelStatusRequest {
    fn from(q: FlywheelStatusQuery) -> Self {
        Self {
            namespace: q.namespace,
            agent: q.agent,
        }
    }
}

impl From<FlywheelVelocityQuery> for FlywheelVelocityRequest {
    fn from(q: FlywheelVelocityQuery) -> Self {
        Self {
            namespace: q.namespace,
            agent: q.agent,
            days: q.days,
        }
    }
}

impl From<FlywheelLineageQuery> for FlywheelLineageRequest {
    fn from(q: FlywheelLineageQuery) -> Self {
        Self {
            namespace: q.namespace,
            agent: q.agent,
            limit: q.limit,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoOptimizerRunHttpRequest {
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub scenario_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    pub pattern_text: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub min_observations: Option<usize>,
    /// Embedding vector for the candidate Pattern. Dashboard-side live
    /// embedding is intentionally not hidden in this route; callers must
    /// supply the vector until the provider-backed embedder UX lands.
    pub embedding: Vec<f32>,
    #[serde(default = "default_dashboard_embedder_id")]
    pub embedder_id: String,
}

fn default_dashboard_embedder_id() -> String {
    "dashboard:provided".to_string()
}

impl From<AutoOptimizerRunHttpRequest> for AutoOptimizerRunRequest {
    fn from(req: AutoOptimizerRunHttpRequest) -> Self {
        Self {
            namespace: req.namespace,
            agent: req.agent,
            scenario_id: req.scenario_id,
            run_id: req.run_id,
            pattern_text: req.pattern_text,
            active: req.active,
            limit: req.limit,
            min_observations: req.min_observations,
        }
    }
}

pub async fn status(
    State(_state): State<AppState>,
    Query(q): Query<FlywheelStatusQuery>,
) -> Result<Json<FlywheelStatusDto>, DashboardError> {
    let store = memory_route::resolve_store().await?;
    let resp = flywheel::status(&store, q.into()).await?;
    Ok(Json(resp))
}

pub async fn velocity(
    State(state): State<AppState>,
    Query(q): Query<FlywheelVelocityQuery>,
) -> Result<Json<FlywheelVelocityDto>, DashboardError> {
    let store = memory_route::resolve_store().await?;
    let ctx = state.api_context();
    let resp = flywheel::velocity(&ctx, &store, q.into()).await?;
    Ok(Json(resp))
}

pub async fn lineage(
    State(state): State<AppState>,
    Query(q): Query<FlywheelLineageQuery>,
) -> Result<Json<FlywheelLineageDto>, DashboardError> {
    let ctx = state.api_context();
    let resp = flywheel::lineage(&ctx, q.into()).await?;
    Ok(Json(resp))
}

pub async fn autooptimizer_run(
    State(_state): State<AppState>,
    Json(body): Json<AutoOptimizerRunHttpRequest>,
) -> Result<Json<AutoOptimizerRunDto>, DashboardError> {
    let store = memory_route::resolve_store().await?;
    let embedder_id = body.embedder_id.clone();
    let embedding = body.embedding.clone();
    let req = body.into();
    let resp = autooptimizer::run_memory_distillation(&store, &embedder_id, embedding, req).await?;
    Ok(Json(resp))
}

pub async fn autooptimizer_get(
    Path(id): Path<String>,
    State(_state): State<AppState>,
) -> Result<Json<AutoOptimizerRunDto>, DashboardError> {
    let store = memory_route::resolve_store().await?;
    let resp = autooptimizer::inspect_run(&store, &id).await?;
    Ok(Json(resp))
}

pub async fn autooptimizer_list(
    Query(q): Query<AutoOptimizerRunListQuery>,
    State(_state): State<AppState>,
) -> Result<Json<AutoOptimizerRunListResponse>, DashboardError> {
    let store = memory_route::resolve_store().await?;
    let resp = autooptimizer::list_runs(&store, q.into()).await?;
    Ok(Json(resp))
}

pub async fn autooptimizer_promote(
    Path(id): Path<String>,
    State(_state): State<AppState>,
) -> Result<Json<AutoOptimizerRunDto>, DashboardError> {
    let store = memory_route::resolve_store().await?;
    let resp = autooptimizer::promote_run(&store, &id).await?;
    Ok(Json(resp))
}

pub async fn autooptimizer_gate(
    Path(id): Path<String>,
    State(_state): State<AppState>,
    Json(body): Json<AutoOptimizerGateRequest>,
) -> Result<Json<AutoOptimizerRunDto>, DashboardError> {
    let store = memory_route::resolve_store().await?;
    let resp = autooptimizer::gate_run(&store, &id, body).await?;
    Ok(Json(resp))
}

pub async fn autooptimizer_demote(
    Path(id): Path<String>,
    State(_state): State<AppState>,
) -> Result<Json<AutoOptimizerRunDto>, DashboardError> {
    let store = memory_route::resolve_store().await?;
    let resp = autooptimizer::demote_run(&store, &id).await?;
    Ok(Json(resp))
}

pub async fn optimize_memory_demos(
    State(state): State<AppState>,
    Json(body): Json<MemoryDemoOptimizeRequest>,
) -> Result<Json<MemoryDemoOptimizeDto>, DashboardError> {
    let store = memory_route::resolve_store().await?;
    let ctx = state.api_context();
    let resp = optimize::compile_memory_demos(&ctx, &store, body).await?;
    Ok(Json(resp))
}

pub async fn optimize_memory_demos_gate(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<OptimizationGateRequest>,
) -> Result<Json<OptimizationGateDto>, DashboardError> {
    let ctx = state.api_context();
    let resp = optimize::gate_memory_demo_optimization(&ctx, &id, body).await?;
    Ok(Json(resp))
}

// ── Convenience wrapper: POST /api/optimize/run ───────────────────────────
// Agents driving xvn expect a single-call optimization endpoint that accepts
// an agent_id and triggers the flywheel memory-distillation pipeline. This
// wrapper synthesizes sensible defaults so the agent doesn't need to supply
// pattern_text, embedding, namespace, or other flywheel internals.

/// Minimal request body for the agent-facing convenience endpoint.
#[derive(Debug, Deserialize)]
pub struct OptimizeRunSimpleRequest {
    /// Agent ID whose flywheel namespace will be optimized.
    pub agent_id: String,
    /// Optional pattern text. Defaults to "Auto-optimized pattern for <agent_id>".
    #[serde(default)]
    pub pattern_text: Option<String>,
    /// Whether the resulting Pattern is active immediately. Default true.
    #[serde(default = "default_active")]
    pub active: bool,
    /// Maximum Observation rows to distill. Default 50.
    #[serde(default = "default_limit")]
    pub limit: Option<i64>,
    /// Minimum Observation count to produce a Pattern. Default 2.
    #[serde(default = "default_min_observations")]
    pub min_observations: Option<usize>,
}

const fn default_active() -> bool { true }
const fn default_limit() -> Option<i64> { Some(50) }
const fn default_min_observations() -> Option<usize> { Some(2) }

/// POST /api/optimize/run — convenience wrapper for autooptimizer::run_memory_distillation.
///
/// Accepts `{"agent_id":"..."}` with optional `pattern_text`, `active`,
/// `limit`, `min_observations`. Synthesizes defaults for embedding and
/// namespace so agents don't need flywheel internals.
pub async fn optimize_run_simple(
    State(state): State<AppState>,
    Json(body): Json<OptimizeRunSimpleRequest>,
) -> Result<Json<AutoOptimizerRunDto>, DashboardError> {
    let store = memory_route::resolve_store().await?;
    let namespace = format!("agent:{}", body.agent_id);
    let pattern_text = body.pattern_text.unwrap_or_else(|| {
        format!("Auto-optimized pattern for agent {}", body.agent_id)
    });
    let req = AutoOptimizerRunRequest {
        namespace: Some(namespace),
        agent: Some(body.agent_id.clone()),
        scenario_id: None,
        run_id: None,
        pattern_text,
        active: body.active,
        limit: body.limit,
        min_observations: body.min_observations,
    };
    // A simple unit embedding — the flywheel's observation recall works
    // against this to find semantically similar observations. A unit
    // vector matches all patterns equally, which is a reasonable default
    // for an agent that hasn't supplied a target embedding.
    let embedding: Vec<f32> = vec![1.0_f32; 384];
    let resp = autooptimizer::run_memory_distillation(&store, "auto", embedding, req).await
        .map_err(|e| DashboardError::Validation {
            field: "optimize/run".into(),
            msg: format!("autooptimizer run failed: {e}"),
        })?;
    Ok(Json(resp))
}
