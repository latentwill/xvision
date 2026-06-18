//! Nanochat checkpoint routes.
//!
//! - `GET  /api/nanochat/checkpoints`                  — list promoted checkpoints.
//! - `GET  /api/nanochat/checkpoints/:model_id`        — checkpoint detail + input_spec.
//! - `POST /api/nanochat/checkpoints/:model_id/approve`— set live_approved = 1 (idempotent).
//!
//! Auth: GET routes are in `readonly_router` (no auth). POST approve is in
//! `mutating_router` behind `require_auth_middleware`.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::error::DashboardError;
use crate::state::AppState;

// ─── Response types ──────────────────────────────────────────────────────────

/// Slim summary for the list endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckpointSummary {
    pub model_id: String,
    pub display_name: String,
    pub run_tag: String,
    pub label_strategy: String,
    pub best_acc: Option<f64>,
    pub best_loss: Option<f64>,
    pub holdout_samples: Option<i64>,
    pub promoted: bool,
    pub live_approved: bool,
    pub source_strategy_id: Option<String>,
    pub source_strategy_name: Option<String>,
    pub autoresearch_run_id: Option<String>,
    pub created_at: String,
}

/// Full detail row including input_spec JSON string.
#[derive(Debug, Serialize, Deserialize)]
pub struct CheckpointDetail {
    pub model_id: String,
    pub display_name: String,
    pub run_tag: String,
    pub checkpoint_path: String,
    pub weights_format: String,
    pub weights_sha256: String,
    /// Raw JSON string of the input spec (window_bars, indicators, normalization).
    pub input_spec: String,
    pub base_model: String,
    pub label_strategy: String,
    pub label_config: String,
    pub best_acc: Option<f64>,
    pub best_loss: Option<f64>,
    pub holdout_samples: Option<i64>,
    pub promoted: bool,
    pub live_approved: bool,
    pub source_strategy_id: Option<String>,
    pub source_strategy_name: Option<String>,
    pub autoresearch_run_id: Option<String>,
    pub created_at: String,
}

// ─── GET /api/nanochat/checkpoints ───────────────────────────────────────────

pub async fn list_checkpoints(
    State(state): State<AppState>,
) -> Result<Json<Vec<CheckpointSummary>>, DashboardError> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            Option<f64>,
            Option<f64>,
            Option<i64>,
            i64,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
        ),
    >(
        "SELECT model_id, display_name, run_tag, label_strategy,
                best_acc, best_loss, holdout_samples,
                promoted, live_approved,
                source_strategy_id, source_strategy_name, autoresearch_run_id,
                created_at
         FROM trained_models
         WHERE promoted = 1
         ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!("list_checkpoints: {e}")))?;

    let checkpoints = rows
        .into_iter()
        .map(
            |(
                model_id,
                display_name,
                run_tag,
                label_strategy,
                best_acc,
                best_loss,
                holdout_samples,
                promoted,
                live_approved,
                source_strategy_id,
                source_strategy_name,
                autoresearch_run_id,
                created_at,
            )| CheckpointSummary {
                model_id,
                display_name,
                run_tag,
                label_strategy,
                best_acc,
                best_loss,
                holdout_samples,
                promoted: promoted == 1,
                live_approved: live_approved == 1,
                source_strategy_id,
                source_strategy_name,
                autoresearch_run_id,
                created_at,
            },
        )
        .collect();

    Ok(Json(checkpoints))
}

// ─── GET /api/nanochat/checkpoints/:model_id ─────────────────────────────────

pub async fn get_checkpoint(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> Result<Json<CheckpointDetail>, DashboardError> {
    // NOTE: a 19-element tuple exceeds sqlx's FromRow tuple-arity limit (16),
    // so we extract columns by name via `sqlx::Row::get` (mirrors
    // NanochatStore::row_to_trained_model). INTEGER 0/1 → bool explicitly.
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT model_id, display_name, run_tag, checkpoint_path, weights_format,
                weights_sha256, input_spec, base_model, label_strategy, label_config,
                best_acc, best_loss, holdout_samples, promoted, live_approved,
                source_strategy_id, source_strategy_name, autoresearch_run_id, created_at
         FROM trained_models
         WHERE model_id = ?",
    )
    .bind(&model_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!("get_checkpoint: {e}")))?
    .ok_or_else(|| DashboardError::NotFound(format!("checkpoint {model_id} not found")))?;

    Ok(Json(CheckpointDetail {
        model_id: row.get("model_id"),
        display_name: row.get("display_name"),
        run_tag: row.get("run_tag"),
        checkpoint_path: row.get("checkpoint_path"),
        weights_format: row.get("weights_format"),
        weights_sha256: row.get("weights_sha256"),
        input_spec: row.get("input_spec"),
        base_model: row.get("base_model"),
        label_strategy: row.get("label_strategy"),
        label_config: row.get("label_config"),
        best_acc: row.get("best_acc"),
        best_loss: row.get("best_loss"),
        holdout_samples: row.get("holdout_samples"),
        promoted: row.get::<i64, _>("promoted") == 1,
        live_approved: row.get::<i64, _>("live_approved") == 1,
        source_strategy_id: row.get("source_strategy_id"),
        source_strategy_name: row.get("source_strategy_name"),
        autoresearch_run_id: row.get("autoresearch_run_id"),
        created_at: row.get("created_at"),
    }))
}

// ─── POST /api/nanochat/checkpoints/:model_id/approve ────────────────────────

#[derive(Debug, Serialize)]
pub struct ApproveResponse {
    pub model_id: String,
    pub live_approved: bool,
}

/// Set `live_approved = 1`. Idempotent: calling on an already-approved
/// checkpoint is a 200 no-op, not an error.
pub async fn approve_checkpoint(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> Result<Json<ApproveResponse>, DashboardError> {
    // Verify the checkpoint exists first.
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM trained_models WHERE model_id = ?")
        .bind(&model_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("approve_checkpoint exists check: {e}")))?;

    if exists.is_none() {
        return Err(DashboardError::NotFound(format!(
            "checkpoint {model_id} not found"
        )));
    }

    // Set live_approved = 1. Idempotent: no-op if already 1.
    sqlx::query("UPDATE trained_models SET live_approved = 1 WHERE model_id = ?")
        .bind(&model_id)
        .execute(&state.pool)
        .await
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("approve_checkpoint update: {e}")))?;

    Ok(Json(ApproveResponse {
        model_id,
        live_approved: true,
    }))
}
