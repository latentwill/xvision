//! Bridge: translate `eval_run_<ulid>` identifiers into synthetic `CliJob` /
//! `CliJobOutput` shapes so `get_cli_job` and `get_cli_job_output` work for
//! eval-run job ids the LLM receives from `run_eval`.
//!
//! ## Why this exists
//!
//! `run_eval` returns a plain `run_id` (ULID). The LLM wizard, following the
//! convention that MCP job ids are prefixed by domain, calls back with
//! `get_cli_job("eval_run_<ulid>")`.  The cli-jobs SQLite table has no such
//! row (eval runs are tracked in `eval_runs`), so the store's `get` falls
//! through with "not found".
//!
//! This module intercepts the `eval_run_` prefix, strips it, calls
//! `RunStore::get`, and maps the `Run` row onto `CliJob` / `CliJobOutput`
//! shapes that are identical to what the rest of the wizard loop already
//! handles.  The bridge is read-only; it never writes to `cli_jobs`.
//!
//! ## Status mapping
//!
//! | `RunStatus`      | `CliJobStatus`         |
//! |------------------|------------------------|
//! | `Queued`         | `Queued`               |
//! | `Running`        | `Running`              |
//! | `Completed`      | `Succeeded`            |
//! | `Failed`         | `Failed`               |
//! | `Cancelled`      | `Cancelled`            |

pub const EVAL_RUN_PREFIX: &str = "eval_run_";

use anyhow::Result;

use xvision_engine::eval::run::RunStatus;
use xvision_engine::eval::store::RunStore;

use super::model::{CliJob, CliJobOutput, CliJobStatus};

/// If `job_id` starts with `eval_run_`, look up the underlying eval run and
/// return a synthetic `CliJob`. Returns `Ok(None)` if the prefix matches but
/// the run does not exist.
pub async fn get_synthetic_job(pool: &sqlx::SqlitePool, job_id: &str) -> Result<Option<CliJob>> {
    let run_id = match job_id.strip_prefix(EVAL_RUN_PREFIX) {
        Some(id) => id,
        None => return Ok(None),
    };

    let store = RunStore::new(pool.clone());
    let run = match store.get(run_id).await {
        Ok(r) => r,
        Err(e) if e.to_string().contains("run not found") => return Ok(None),
        Err(e) => return Err(e),
    };

    let status = map_run_status(run.status);
    let started_at = Some(run.started_at.to_rfc3339());
    let finished_at = run.completed_at.map(|dt| dt.to_rfc3339());

    Ok(Some(CliJob {
        job_id: job_id.to_string(),
        // Synthetic argv: mirrors the subcommand the agent conceptually ran.
        argv: vec![
            "eval".to_string(),
            "run".to_string(),
            "--agent".to_string(),
            run.agent_id.clone(),
            "--scenario".to_string(),
            run.scenario_id.clone(),
        ],
        status,
        created_at: run.started_at.to_rfc3339(),
        started_at,
        finished_at,
        exit_code: None,
        // eval runs have no concept of timeout_secs; zero is the sentinel.
        timeout_secs: 0,
        timed_out: false,
        cancel_requested: run.status == RunStatus::Cancelled,
        stdout_bytes: 0,
        stderr_bytes: 0,
        stdout_truncated: false,
        stderr_truncated: false,
        error_message: run.error.clone(),
        // Synthetic jobs have no backing cli_jobs row — audit fields are not
        // applicable. The orphan-recovery sweep skips eval_run_* prefixed IDs
        // because they have no entry in the cli_jobs table.
        pid: None,
        job_user: None,
        job_source: None,
        command_class: Some("eval".to_string()),
        cancelled_at: None,
        cancel_signal: None,
        recovered_at: None,
        recovery_reason: None,
        max_runtime_seconds: 0,
        max_output_bytes: 0,
        output_cap_exceeded: false,
        runtime_cap_exceeded: false,
    }))
}

/// If `job_id` starts with `eval_run_`, look up the underlying eval run and
/// return a synthetic `CliJobOutput` where `stdout` carries the JSON eval
/// summary. Returns `Ok(None)` if the run does not exist.
pub async fn get_synthetic_output(pool: &sqlx::SqlitePool, job_id: &str) -> Result<Option<CliJobOutput>> {
    let run_id = match job_id.strip_prefix(EVAL_RUN_PREFIX) {
        Some(id) => id,
        None => return Ok(None),
    };

    let store = RunStore::new(pool.clone());
    let run = match store.get(run_id).await {
        Ok(r) => r,
        Err(e) if e.to_string().contains("run not found") => return Ok(None),
        Err(e) => return Err(e),
    };

    let status = map_run_status(run.status);

    // Build the eval summary JSON that the agent can read back.
    let summary = serde_json::json!({
        "run_id": run.id,
        "status": run.status.as_str(),
        "mode": run.mode.as_str(),
        "agent_id": run.agent_id,
        "scenario_id": run.scenario_id,
        "started_at": run.started_at.to_rfc3339(),
        "completed_at": run.completed_at.map(|dt| dt.to_rfc3339()),
        "metrics": run.metrics,
        "error": run.error,
        "detail_url": format!("/eval-runs/{}", run.id),
    });

    let stdout = serde_json::to_string_pretty(&summary).unwrap_or_default();
    let stdout_bytes = stdout.len() as u64;

    // Non-terminal runs have no output yet; still return the summary so the
    // agent can see status + detail_url and knows where to poll.
    let stderr = run
        .error
        .as_deref()
        .map(|e| format!("eval run error: {e}"))
        .unwrap_or_default();
    let stderr_bytes = stderr.len() as u64;

    Ok(Some(CliJobOutput {
        job_id: job_id.to_string(),
        status,
        exit_code: None,
        stdout,
        stderr,
        stdout_bytes,
        stderr_bytes,
        stdout_truncated: false,
        stderr_truncated: false,
    }))
}

fn map_run_status(status: RunStatus) -> CliJobStatus {
    match status {
        RunStatus::Queued => CliJobStatus::Queued,
        RunStatus::Running => CliJobStatus::Running,
        RunStatus::Completed => CliJobStatus::Succeeded,
        RunStatus::Failed => CliJobStatus::Failed,
        RunStatus::Cancelled => CliJobStatus::Cancelled,
    }
}
