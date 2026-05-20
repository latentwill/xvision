use std::time::Duration;

use axum::extract::{Json, Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::{Deserialize, Serialize};

use crate::cli_jobs::allowlist::{check_argv, AllowlistDecision};
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
    let store = CliJobStore::new(state.pool.clone());
    let Some(job) = store.get(&job_id).await.map_err(DashboardError::Internal)? else {
        return Err(DashboardError::NotFound(format!("cli job '{job_id}'")));
    };

    Ok(Json(serde_json::json!({
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
        "error_message": job.error_message
    })))
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

pub async fn cancel(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    let store = CliJobStore::new(state.pool.clone());
    let Some(job) = store
        .request_cancel(&job_id)
        .await
        .map_err(DashboardError::Internal)?
    else {
        return Err(DashboardError::NotFound(format!("cli job '{job_id}'")));
    };

    state.cli_runner().cancel(&job_id).await;

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
    // operator/eval commands must not require a dev-mode bypass on live nodes.
    if let AllowlistDecision::Reject(msg) = check_argv(&body.argv) {
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
