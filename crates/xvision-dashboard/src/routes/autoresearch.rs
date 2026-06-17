//! Autoresearch run routes.
//!
//! - `POST /api/autoresearch/runs`                — start run (mutating+auth+gate)
//! - `POST /api/autoresearch/runs/:run_id/stop`   — stop run  (mutating+auth)
//! - `GET  /api/autoresearch/runs`                — list runs (readonly)
//! - `GET  /api/autoresearch/runs/:run_id`        — run detail (readonly)
//! - `GET  /api/autoresearch/runs/:run_id/stream` — SSE stdout feed (readonly)
//! - `GET  /api/autoresearch/runs/:run_id/experiments` — experiment log, ASC (readonly)

use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use axum::response::sse::Sse;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;
use ulid::Ulid;

use xvision_engine::autoresearch::run_config::{LabelConfig, LabelStrategy, RunConfig};
use xvision_engine::autoresearch::training_gate::require_training_enabled;
use xvision_engine::autoresearch::worktree::WorktreeHandle;

use crate::error::DashboardError;
use crate::sse::autoresearch_sse::autoresearch_sse;
use crate::state::AppState;

// ─── Request / response types ─────────────────────────────────────────────────

/// Request body for `POST /api/autoresearch/runs`.
#[derive(Debug, Deserialize)]
pub struct StartRunBody {
    pub run_tag: String,
    pub source_strategy_id: Option<String>,
    pub label_strategy: String,
    pub label_config: serde_json::Value,
    pub min_cycle_count: Option<u32>,
    pub train_wall_clock_sec: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct RunSummary {
    pub run_id: String,
    pub run_tag: String,
    pub source_strategy_id: Option<String>,
    pub label_strategy: String,
    pub status: String,
    pub started_at: String,
    pub stopped_at: Option<String>,
    pub experiments: i64,
    pub best_acc: Option<f64>,
    pub best_model_id: Option<String>,
    pub git_branch: String,
    pub worktree_path: String,
}

#[derive(Debug, Serialize)]
pub struct ListRunsResponse {
    pub runs: Vec<RunSummary>,
}

#[derive(Debug, Serialize)]
pub struct ExperimentRow {
    pub experiment_id: String,
    pub run_id: String,
    pub git_commit: String,
    pub val_acc: Option<f64>,
    pub val_loss: Option<f64>,
    pub peak_vram_mb: Option<f64>,
    pub training_seconds: Option<f64>,
    pub status: String,
    pub description: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ListExperimentsResponse {
    pub experiments: Vec<ExperimentRow>,
}

#[derive(Debug, Serialize)]
pub struct StartRunResponse {
    pub run_id: String,
    pub run_tag: String,
    pub git_branch: String,
    pub worktree_path: String,
}

#[derive(Debug, Serialize)]
pub struct StopRunResponse {
    pub run_id: String,
    pub status: String,
}

// ─── POST /api/autoresearch/runs ─────────────────────────────────────────────

pub async fn start_run(
    State(state): State<AppState>,
    Json(body): Json<StartRunBody>,
) -> Result<(StatusCode, Json<StartRunResponse>), DashboardError> {
    // 1. Deploy-host gate. Returns 403 if XVN_ENABLE_LOCAL_TRAINING is not set.
    require_training_enabled().map_err(|e| DashboardError::Forbidden(e.to_string()))?;

    // 2. Validate run_tag — delegate to the pure-Rust fn from Phase 2 Task 2.1.
    //    `validate_run_tag` enforces ^[a-z0-9][a-z0-9-]{0,31}$ via pure std char
    //    checks (no regex dependency in xvision-dashboard).
    xvision_engine::nanochat::validate::validate_run_tag(&body.run_tag)
        .map_err(|msg| DashboardError::Validation {
            field: "run_tag".into(),
            msg,
        })?;

    // 3. Parse and validate label_strategy.
    let label_strategy: LabelStrategy = match body.label_strategy.as_str() {
        "price_forward" => LabelStrategy::PriceForward,
        "outcome_imitation" => LabelStrategy::OutcomeImitation,
        other => {
            return Err(DashboardError::Validation {
                field: "label_strategy".into(),
                msg: format!(
                    "unknown label_strategy {other:?}; expected price_forward or outcome_imitation"
                ),
            })
        }
    };

    // 4. Parse label_config from the request's JSON value.
    let label_config: LabelConfig = serde_json::from_value(body.label_config.clone())
        .map_err(|e| DashboardError::Validation {
            field: "label_config".into(),
            msg: format!("invalid label_config: {e}"),
        })?;

    // 5. Build and validate RunConfig (also enforces min_cycle_count > 0 etc).
    let min_cycle_count = body.min_cycle_count.unwrap_or(500);
    let train_wall_clock_sec = body.train_wall_clock_sec.unwrap_or(300);

    let run_config = RunConfig {
        source_strategy_id: body.source_strategy_id.clone().unwrap_or_default(),
        label_strategy,
        label_config,
        min_cycle_count,
        train_wall_clock_sec,
        db_path: state.xvn_home.join("xvn.db").to_string_lossy().to_string(),
        output_dir: state
            .xvn_home
            .join("nanochat-models")
            .to_string_lossy()
            .to_string(),
        promotion_epsilon: 0.01,
        promotion_acc_floor: 0.52,
        promotion_min_holdout: 200,
    };
    run_config.validate().map_err(|e| DashboardError::Validation {
        field: "request".into(),
        msg: e.to_string(),
    })?;

    // 6. Pre-check concurrency (the DB unique index is the hard guard; this
    //    gives a clear 409 before worktree creation).
    let running: Option<String> = sqlx::query_scalar(
        "SELECT run_id FROM autoresearch_runs WHERE status = 'running' LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!("concurrency check: {e}")))?;

    if let Some(existing_id) = running {
        return Err(DashboardError::Conflict(format!(
            "autoresearch run {existing_id} is already running; stop it before starting a new one"
        )));
    }

    // 7. Create the git worktree.
    let repo_root = std::env::current_dir()
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("current_dir: {e}")))?;
    let wt = WorktreeHandle::create(&repo_root, &body.run_tag)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("worktree create: {e}")))?;

    let worktree_path = wt.path().to_string_lossy().to_string();
    let git_branch = wt.branch().to_string();

    // 8. Write run_config.json into the worktree.
    let config_path = wt.path().join("run_config.json");
    run_config
        .write_to(&config_path)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("write run_config: {e}")))?;

    // 9. Insert the run row.
    let run_id = Ulid::new().to_string();
    let started_at = Utc::now().to_rfc3339();
    let label_config_json =
        serde_json::to_string(&body.label_config).unwrap_or_else(|_| "{}".to_string());

    sqlx::query(
        "INSERT INTO autoresearch_runs
            (run_id, run_tag, source_strategy_id, label_strategy, label_config,
             git_branch, worktree_path, status, started_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'running', ?)",
    )
    .bind(&run_id)
    .bind(&body.run_tag)
    .bind(&body.source_strategy_id)
    .bind(body.label_strategy.as_str())
    .bind(&label_config_json)
    .bind(&git_branch)
    .bind(&worktree_path)
    .bind(&started_at)
    .execute(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!("insert autoresearch_runs: {e}")))?;

    // Spawn the training executor detached. The HTTP 201 is returned immediately
    // and the run lifecycle (completed/failed) is managed inside the spawned task.
    tokio::spawn(crate::autoresearch_runner::execute_training_run(
        state.pool.clone(),
        state.autoresearch_stdout_tx.clone(),
        run_id.clone(),
        wt.path().to_path_buf(),
        config_path.clone(),
        Duration::from_secs(run_config.train_wall_clock_sec),
    ));

    Ok((
        StatusCode::CREATED,
        Json(StartRunResponse {
            run_id,
            run_tag: body.run_tag,
            git_branch,
            worktree_path,
        }),
    ))
}

// ─── POST /api/autoresearch/runs/:run_id/stop ────────────────────────────────

pub async fn stop_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<StopRunResponse>, DashboardError> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT status FROM autoresearch_runs WHERE run_id = ?")
            .bind(&run_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| {
                DashboardError::Internal(anyhow::anyhow!("stop_run status check: {e}"))
            })?;

    let current_status = existing.ok_or_else(|| {
        DashboardError::NotFound(format!("autoresearch run {run_id} not found"))
    })?;

    if current_status != "running" {
        // Idempotent: already stopped/completed — return current status.
        return Ok(Json(StopRunResponse {
            run_id,
            status: current_status,
        }));
    }

    let stopped_at = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE autoresearch_runs SET status = 'stopped', stopped_at = ? WHERE run_id = ?",
    )
    .bind(&stopped_at)
    .bind(&run_id)
    .execute(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!("stop_run update: {e}")))?;

    Ok(Json(StopRunResponse {
        run_id,
        status: "stopped".to_string(),
    }))
}

// ─── GET /api/autoresearch/runs ───────────────────────────────────────────────

pub async fn list_runs(
    State(state): State<AppState>,
) -> Result<Json<ListRunsResponse>, DashboardError> {
    let rows = sqlx::query_as::<_, (
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        Option<String>,
        i64,
        Option<f64>,
        Option<String>,
        String,
        String,
    )>(
        "SELECT run_id, run_tag, source_strategy_id, label_strategy, status,
                started_at, stopped_at, experiments, best_acc, best_model_id,
                git_branch, worktree_path
         FROM autoresearch_runs
         ORDER BY started_at DESC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!("list_runs: {e}")))?;

    let runs = rows
        .into_iter()
        .map(
            |(
                run_id,
                run_tag,
                source_strategy_id,
                label_strategy,
                status,
                started_at,
                stopped_at,
                experiments,
                best_acc,
                best_model_id,
                git_branch,
                worktree_path,
            )| RunSummary {
                run_id,
                run_tag,
                source_strategy_id,
                label_strategy,
                status,
                started_at,
                stopped_at,
                experiments,
                best_acc,
                best_model_id,
                git_branch,
                worktree_path,
            },
        )
        .collect();

    Ok(Json(ListRunsResponse { runs }))
}

// ─── GET /api/autoresearch/runs/:run_id ──────────────────────────────────────

pub async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<RunSummary>, DashboardError> {
    let row = sqlx::query_as::<_, (
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        Option<String>,
        i64,
        Option<f64>,
        Option<String>,
        String,
        String,
    )>(
        "SELECT run_id, run_tag, source_strategy_id, label_strategy, status,
                started_at, stopped_at, experiments, best_acc, best_model_id,
                git_branch, worktree_path
         FROM autoresearch_runs
         WHERE run_id = ?",
    )
    .bind(&run_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!("get_run: {e}")))?
    .ok_or_else(|| DashboardError::NotFound(format!("autoresearch run {run_id} not found")))?;

    let (
        run_id,
        run_tag,
        source_strategy_id,
        label_strategy,
        status,
        started_at,
        stopped_at,
        experiments,
        best_acc,
        best_model_id,
        git_branch,
        worktree_path,
    ) = row;

    Ok(Json(RunSummary {
        run_id,
        run_tag,
        source_strategy_id,
        label_strategy,
        status,
        started_at,
        stopped_at,
        experiments,
        best_acc,
        best_model_id,
        git_branch,
        worktree_path,
    }))
}

// ─── GET /api/autoresearch/runs/:run_id/stream ───────────────────────────────

pub async fn stream_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<
    Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>>,
    DashboardError,
> {
    // Verify run exists.
    let exists: Option<String> =
        sqlx::query_scalar("SELECT run_id FROM autoresearch_runs WHERE run_id = ?")
            .bind(&run_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| DashboardError::Internal(anyhow::anyhow!("stream_run: {e}")))?;

    if exists.is_none() {
        return Err(DashboardError::NotFound(format!(
            "autoresearch run {run_id} not found"
        )));
    }

    let rx = state.autoresearch_stdout_tx.subscribe();
    Ok(autoresearch_sse(&run_id, rx))
}

// ─── GET /api/autoresearch/runs/:run_id/experiments ──────────────────────────

pub async fn list_experiments(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<Json<ListExperimentsResponse>, DashboardError> {
    let rows = sqlx::query_as::<_, (
        String,
        String,
        String,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        String,
        String,
        String,
    )>(
        "SELECT experiment_id, run_id, git_commit,
                val_acc, val_loss, peak_vram_mb, training_seconds,
                status, description, created_at
         FROM autoresearch_experiments
         WHERE run_id = ?
         ORDER BY created_at ASC",
    )
    .bind(&run_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(anyhow::anyhow!("list_experiments: {e}")))?;

    let experiments = rows
        .into_iter()
        .map(
            |(
                experiment_id,
                run_id,
                git_commit,
                val_acc,
                val_loss,
                peak_vram_mb,
                training_seconds,
                status,
                description,
                created_at,
            )| ExperimentRow {
                experiment_id,
                run_id,
                git_commit,
                val_acc,
                val_loss,
                peak_vram_mb,
                training_seconds,
                status,
                description,
                created_at,
            },
        )
        .collect();

    Ok(Json(ListExperimentsResponse { experiments }))
}
