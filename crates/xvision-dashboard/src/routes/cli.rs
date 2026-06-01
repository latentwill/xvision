use std::time::Duration;

use axum::extract::{Json, Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::{Deserialize, Serialize};

use crate::cli_jobs::allowlist::{check_argv_with_env, AllowlistDecision};
use crate::cli_jobs::runner::{CliJobEvent, DEFAULT_TIMEOUT_SECS, MAX_TIMEOUT_SECS};
use crate::cli_jobs::store::CliJobStore;
use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateCliJobReq {
    pub argv: Vec<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

#[derive(Debug, Serialize)]
pub struct CreateCliJobResp {
    pub job_id: String,
    pub status: String,
}

fn default_timeout_secs() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateCliJobReq>,
) -> Result<Json<CreateCliJobResp>, DashboardError> {
    validate_create_body(&body)?;

    let store = CliJobStore::new(state.pool.clone());
    let job = store
        .create_queued(body.argv, body.timeout_secs)
        .await
        .map_err(DashboardError::Internal)?;
    state.cli_runner().start(job.clone());

    Ok(Json(CreateCliJobResp {
        job_id: job.job_id,
        status: job.status.as_str().to_string(),
    }))
}

pub async fn get(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    // Check for synthetic eval-run bridge IDs first.
    if job_id.starts_with(crate::cli_jobs::eval_run_bridge::EVAL_RUN_PREFIX) {
        if let Some(job) = crate::cli_jobs::eval_run_bridge::get_synthetic_job(&state.pool, &job_id)
            .await
            .map_err(DashboardError::Internal)?
        {
            return Ok(Json(job_to_json(&job)));
        }
        return Err(DashboardError::NotFound(format!("cli job '{job_id}'")));
    }

    let store = CliJobStore::new(state.pool.clone());
    let Some(job) = store.get(&job_id).await.map_err(DashboardError::Internal)? else {
        return Err(DashboardError::NotFound(format!("cli job '{job_id}'")));
    };

    Ok(Json(job_to_json(&job)))
}

pub async fn output(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    let store = CliJobStore::new(state.pool.clone());
    let Some(output) = store.output(&job_id).await.map_err(DashboardError::Internal)? else {
        return Err(DashboardError::NotFound(format!("cli job '{job_id}'")));
    };

    Ok(Json(serde_json::json!({
        "job_id": output.job_id,
        "status": output.status.as_str(),
        "exit_code": output.exit_code,
        "stdout": output.stdout,
        "stderr": output.stderr,
        "stdout_bytes": output.stdout_bytes,
        "stderr_bytes": output.stderr_bytes,
        "stdout_truncated": output.stdout_truncated,
        "stderr_truncated": output.stderr_truncated
    })))
}

/// `POST /api/cli/jobs/:id/cancel` — request cancellation (existing endpoint,
/// kept for backwards compatibility with callers using POST).
///
/// Marks the DB row as cancel-requested and notifies the runner to send
/// SIGTERM → SIGKILL (with 5-second grace period).
pub async fn cancel(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    cancel_job(&state, &job_id).await
}

/// `DELETE /api/cli/jobs/:id` — cancel a running job and kill the backing
/// process with SIGTERM → SIGKILL (5-second grace period).
///
/// This is the preferred cancellation surface for `xvn eval cancel` and
/// other verbs that call through to the dashboard. The `xvn eval cancel`
/// verb marks the eval-run row (PR #425); this endpoint closes the gap by
/// also killing the backing dashboard child process.
///
/// Idempotent: calling it on a job that is already terminal returns the
/// current status without erroring.
pub async fn delete(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    cancel_job(&state, &job_id).await
}

/// Shared implementation for `cancel` and `delete`.
async fn cancel_job(state: &AppState, job_id: &str) -> Result<Json<serde_json::Value>, DashboardError> {
    let store = CliJobStore::new(state.pool.clone());

    // If the job is already terminal, return its current state — idempotent.
    let existing = store
        .get(job_id)
        .await
        .map_err(DashboardError::Internal)?
        .ok_or_else(|| DashboardError::NotFound(format!("cli job '{job_id}'")))?;

    if existing.status.is_terminal() {
        return Ok(Json(serde_json::json!({
            "job_id": existing.job_id,
            "status": existing.status.as_str(),
            "cancel_requested": existing.cancel_requested,
        })));
    }

    // Mark cancel-requested in the DB.
    let Some(job) = store
        .request_cancel(job_id)
        .await
        .map_err(DashboardError::Internal)?
    else {
        return Err(DashboardError::NotFound(format!("cli job '{job_id}'")));
    };

    // Signal the runner: SIGTERM → (5-second grace) → SIGKILL.
    state.cli_runner().cancel(job_id).await;

    Ok(Json(serde_json::json!({
        "job_id": job.job_id,
        "status": job.status.as_str(),
        "cancel_requested": true
    })))
}

pub async fn events(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>, DashboardError> {
    let store = CliJobStore::new(state.pool.clone());
    let Some(job) = store.get(&job_id).await.map_err(DashboardError::Internal)? else {
        return Err(DashboardError::NotFound(format!("cli job '{job_id}'")));
    };

    let mut rx = if job.status.is_terminal() {
        None
    } else {
        Some(state.cli_runner().subscribe(&job_id).await)
    };

    let sse_stream = async_stream::stream! {
        if job.started_at.is_some() {
            if let Some(event) = to_sse_event(CliJobEvent::JobStarted {
                job_id: job.job_id.clone(),
                argv: job.argv.clone(),
            }) {
                yield Ok(event);
            }
        }

        if job.status.is_terminal() {
            if let Some(event) = to_sse_event(CliJobEvent::JobFinished {
                job_id: job.job_id.clone(),
                status: job.status.as_str().to_string(),
                exit_code: job.exit_code,
                timed_out: job.timed_out,
                cancelled: matches!(job.status, crate::cli_jobs::model::CliJobStatus::Cancelled),
                error_message: job.error_message.clone(),
            }) {
                yield Ok(event);
            }
            return;
        }

        if let Some(ref mut rx) = rx {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let Some(sse_event) = to_sse_event(event) {
                            yield Ok(sse_event);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    };

    Ok(Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

fn validate_create_body(body: &CreateCliJobReq) -> Result<(), DashboardError> {
    if body.argv.is_empty() {
        return Err(DashboardError::Validation {
            field: "argv".into(),
            msg: "must contain at least one argument".into(),
        });
    }

    // Remote CLI policy: typed argv only, an explicit supported-command list,
    // and a small hard denylist for server/live-trading commands. Normal
    // operator/eval commands work without any dev-mode flag. Setting
    // XVN_DASHBOARD_CLI_DEVMODE turns this into a full bypass (trusted dev
    // nodes only — see allowlist::check_argv_with_env).
    if let AllowlistDecision::Reject(msg) = check_argv_with_env(&body.argv) {
        return Err(DashboardError::Validation {
            field: "argv".into(),
            msg,
        });
    }

    if body.timeout_secs == 0 {
        return Err(DashboardError::Validation {
            field: "timeout_secs".into(),
            msg: "must be greater than zero".into(),
        });
    }

    if body.timeout_secs > MAX_TIMEOUT_SECS {
        return Err(DashboardError::Validation {
            field: "timeout_secs".into(),
            msg: format!("must be at most {MAX_TIMEOUT_SECS} seconds"),
        });
    }

    Ok(())
}

fn to_sse_event(event: CliJobEvent) -> Option<Event> {
    let name = event.name();
    serde_json::to_string(&event)
        .ok()
        .map(|payload| Event::default().event(name).data(payload))
}

/// Serialize a `CliJob` to the JSON shape returned by `GET /api/cli/jobs/:id`.
/// Includes all audit fields added in migration 028.
fn job_to_json(job: &crate::cli_jobs::model::CliJob) -> serde_json::Value {
    serde_json::json!({
        "job_id": job.job_id,
        "argv": job.argv,
        "status": job.status.as_str(),
        "created_at": job.created_at,
        "started_at": job.started_at,
        "finished_at": job.finished_at,
        "exit_code": job.exit_code,
        "timed_out": job.timed_out,
        "cancel_requested": job.cancel_requested,
        "stdout_bytes": job.stdout_bytes,
        "stderr_bytes": job.stderr_bytes,
        "stdout_truncated": job.stdout_truncated,
        "stderr_truncated": job.stderr_truncated,
        "error_message": job.error_message,
        // --- Audit fields (migration 028) ---
        "pid": job.pid,
        "user": job.job_user,
        "source": job.job_source,
        "command_class": job.command_class,
        "cancelled_at": job.cancelled_at,
        "cancel_signal": job.cancel_signal,
        "recovered_at": job.recovered_at,
        "recovery_reason": job.recovery_reason,
        "max_runtime_seconds": job.max_runtime_seconds,
        "max_output_bytes": job.max_output_bytes,
        "output_cap_exceeded": job.output_cap_exceeded,
        "runtime_cap_exceeded": job.runtime_cap_exceeded,
        "output_bytes": job.stdout_bytes.saturating_add(job.stderr_bytes),
    })
}
