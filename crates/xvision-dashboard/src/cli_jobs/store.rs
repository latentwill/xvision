use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use sqlx::{Row, SqlitePool};

use super::auth_stub::AuthContext;
use super::model::{
    CliJob, CliJobOutput, CliJobStatus, DEFAULT_MAX_OUTPUT_BYTES, DEFAULT_MAX_RUNTIME_SECONDS,
};

/// Hard per-stream persistence limit: we only keep the first 256 KB of each
/// stream (stdout / stderr) in the `cli_job_output_chunks` table. The job's
/// `stdout_bytes` / `stderr_bytes` counters always reflect the true byte count
/// of what the process produced, regardless of truncation.
const MAX_PERSISTED_STREAM_BYTES: u64 = 256 * 1024;

#[derive(Clone)]
pub struct CliJobStore {
    pool: SqlitePool,
}

/// Result of the startup orphan-recovery sweep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliJobRecovery {
    /// Jobs that were `Queued` at restart time and should be re-spawned.
    pub restarted_queued: Vec<CliJob>,
    /// Number of `Running` jobs whose recorded PID was not alive; these have
    /// been transitioned to `Orphaned`.
    pub orphaned_running: u64,
}

/// Parameters for creating a new job row.
pub struct CreateJobParams<'a> {
    pub argv: Vec<String>,
    pub timeout_secs: u64,
    pub auth: &'a AuthContext,
    /// Override for the supervisor runtime cap. `0` = use `DEFAULT_MAX_RUNTIME_SECONDS`.
    pub max_runtime_seconds: u64,
    /// Override for the supervisor output cap. `0` = use `DEFAULT_MAX_OUTPUT_BYTES`.
    pub max_output_bytes: u64,
}

impl CliJobStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_queued(&self, argv: Vec<String>, timeout_secs: u64) -> Result<CliJob> {
        let auth = AuthContext::unknown();
        self.create_queued_with_auth(CreateJobParams {
            argv,
            timeout_secs,
            auth: &auth,
            max_runtime_seconds: 0,
            max_output_bytes: 0,
        })
        .await
    }

    pub async fn create_queued_with_auth(&self, params: CreateJobParams<'_>) -> Result<CliJob> {
        let job_id = format!("job_{}", ulid::Ulid::new());
        let created_at = Utc::now().to_rfc3339();
        let argv_json = serde_json::to_string(&params.argv).context("serialize argv")?;
        let command_class = params.argv.first().cloned();
        let max_runtime_seconds = if params.max_runtime_seconds == 0 {
            DEFAULT_MAX_RUNTIME_SECONDS
        } else {
            params.max_runtime_seconds
        };
        let max_output_bytes = if params.max_output_bytes == 0 {
            DEFAULT_MAX_OUTPUT_BYTES
        } else {
            params.max_output_bytes
        };

        sqlx::query(
            "INSERT INTO cli_jobs (
                job_id, argv_json, status, created_at, timeout_secs,
                timed_out, cancel_requested, stdout_bytes, stderr_bytes,
                stdout_truncated, stderr_truncated,
                job_user, job_source, command_class,
                max_runtime_seconds, max_output_bytes,
                output_cap_exceeded, runtime_cap_exceeded
             ) VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 0, 0, 0, 0, ?6, ?7, ?8, ?9, ?10, 0, 0)",
        )
        .bind(&job_id)
        .bind(&argv_json)
        .bind(CliJobStatus::Queued.as_str())
        .bind(&created_at)
        .bind(i64::try_from(params.timeout_secs).context("timeout_secs overflow")?)
        .bind(&params.auth.user)
        .bind(&params.auth.source)
        .bind(command_class.as_deref())
        .bind(i64::try_from(max_runtime_seconds).context("max_runtime_seconds overflow")?)
        .bind(i64::try_from(max_output_bytes).context("max_output_bytes overflow")?)
        .execute(&self.pool)
        .await
        .context("insert cli_jobs row")?;

        Ok(CliJob {
            job_id,
            argv: params.argv,
            status: CliJobStatus::Queued,
            created_at,
            started_at: None,
            finished_at: None,
            exit_code: None,
            timeout_secs: params.timeout_secs,
            timed_out: false,
            cancel_requested: false,
            stdout_bytes: 0,
            stderr_bytes: 0,
            stdout_truncated: false,
            stderr_truncated: false,
            error_message: None,
            pid: None,
            job_user: Some(params.auth.user.clone()),
            job_source: Some(params.auth.source.clone()),
            command_class,
            cancelled_at: None,
            cancel_signal: None,
            recovered_at: None,
            recovery_reason: None,
            max_runtime_seconds,
            max_output_bytes,
            output_cap_exceeded: false,
            runtime_cap_exceeded: false,
        })
    }

    pub async fn get(&self, job_id: &str) -> Result<Option<CliJob>> {
        let row = sqlx::query(
            "SELECT
                job_id, argv_json, status, created_at, started_at, finished_at,
                exit_code, timeout_secs, timed_out, cancel_requested,
                stdout_bytes, stderr_bytes, stdout_truncated, stderr_truncated,
                error_message,
                pid, job_user, job_source, command_class,
                cancelled_at, cancel_signal,
                recovered_at, recovery_reason,
                max_runtime_seconds, max_output_bytes,
                output_cap_exceeded, runtime_cap_exceeded
             FROM cli_jobs
             WHERE job_id = ?1",
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await
        .context("load cli_jobs row")?;

        row.map(row_to_job).transpose()
    }

    pub async fn output(&self, job_id: &str) -> Result<Option<CliJobOutput>> {
        let Some(job) = self.get(job_id).await? else {
            return Ok(None);
        };

        let rows = sqlx::query(
            "SELECT stream, payload
             FROM cli_job_output_chunks
             WHERE job_id = ?1
             ORDER BY stream ASC, chunk_index ASC",
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await
        .context("load cli_job_output_chunks rows")?;

        let mut stdout = String::new();
        let mut stderr = String::new();
        for row in rows {
            match row.try_get::<String, _>("stream")?.as_str() {
                "stdout" => stdout.push_str(&row.try_get::<String, _>("payload")?),
                "stderr" => stderr.push_str(&row.try_get::<String, _>("payload")?),
                _ => {}
            }
        }

        Ok(Some(CliJobOutput {
            job_id: job.job_id,
            status: job.status,
            exit_code: job.exit_code,
            stdout,
            stderr,
            stdout_bytes: job.stdout_bytes,
            stderr_bytes: job.stderr_bytes,
            stdout_truncated: job.stdout_truncated,
            stderr_truncated: job.stderr_truncated,
        }))
    }

    pub async fn mark_running(&self, job_id: &str) -> Result<()> {
        self.mark_running_with_pid(job_id, None).await
    }

    pub async fn mark_running_with_pid(&self, job_id: &str, pid: Option<u32>) -> Result<()> {
        sqlx::query(
            "UPDATE cli_jobs
             SET status = ?2, started_at = ?3, pid = ?4
             WHERE job_id = ?1",
        )
        .bind(job_id)
        .bind(CliJobStatus::Running.as_str())
        .bind(Utc::now().to_rfc3339())
        .bind(pid.map(i64::from))
        .execute(&self.pool)
        .await
        .context("mark cli job running")?;
        Ok(())
    }

    pub async fn append_stdout(&self, job_id: &str, payload: &str) -> Result<()> {
        self.append_chunk(job_id, "stdout", payload).await
    }

    pub async fn append_stderr(&self, job_id: &str, payload: &str) -> Result<()> {
        self.append_chunk(job_id, "stderr", payload).await
    }

    pub async fn finish(
        &self,
        job_id: &str,
        status: CliJobStatus,
        exit_code: Option<i64>,
        error_message: Option<String>,
    ) -> Result<()> {
        self.finish_detailed(FinishParams {
            job_id,
            status,
            exit_code,
            error_message,
            cancelled_at: None,
            cancel_signal: None,
            output_cap_exceeded: false,
            runtime_cap_exceeded: false,
        })
        .await
    }

    pub async fn finish_detailed(&self, p: FinishParams<'_>) -> Result<()> {
        let timed_out = matches!(
            p.status,
            CliJobStatus::TimedOut | CliJobStatus::RuntimeCapExceeded
        );
        let cancelled = matches!(p.status, CliJobStatus::Cancelled);
        let output_cap = if p.output_cap_exceeded { 1 } else { 0 };
        let runtime_cap = if p.runtime_cap_exceeded { 1 } else { 0 };

        sqlx::query(
            "UPDATE cli_jobs
             SET status = ?2,
                 finished_at = ?3,
                 exit_code = ?4,
                 timed_out = ?5,
                 cancel_requested = CASE
                     WHEN ?6 = 1 THEN 1
                     ELSE cancel_requested
                 END,
                 error_message = ?7,
                 cancelled_at = ?8,
                 cancel_signal = ?9,
                 output_cap_exceeded = ?10,
                 runtime_cap_exceeded = ?11
             WHERE job_id = ?1",
        )
        .bind(p.job_id)
        .bind(p.status.as_str())
        .bind(Utc::now().to_rfc3339())
        .bind(p.exit_code)
        .bind(if timed_out { 1 } else { 0 })
        .bind(if cancelled { 1 } else { 0 })
        .bind(p.error_message)
        .bind(p.cancelled_at)
        .bind(p.cancel_signal)
        .bind(output_cap)
        .bind(runtime_cap)
        .execute(&self.pool)
        .await
        .context("finish cli job")?;

        Ok(())
    }

    pub async fn request_cancel(&self, job_id: &str) -> Result<Option<CliJob>> {
        let result = sqlx::query(
            "UPDATE cli_jobs
             SET cancel_requested = 1
             WHERE job_id = ?1",
        )
        .bind(job_id)
        .execute(&self.pool)
        .await
        .context("request cli job cancellation")?;

        if result.rows_affected() == 0 {
            return Ok(None);
        }

        self.get(job_id).await
    }

    /// Startup orphan-recovery sweep.
    ///
    /// Scans `cli_jobs` for rows in `Running` state. For each, checks whether
    /// the recorded PID is alive using `sysinfo`. Jobs whose PID is not alive
    /// (or whose PID column is NULL) are transitioned to `Orphaned`.
    ///
    /// Jobs in `Queued` state are returned for re-spawning by the runner.
    ///
    /// Synthetic `eval_run_<ulid>` IDs are never stored in `cli_jobs`, so
    /// the bridge path is unaffected.
    pub async fn recover_after_restart(&self) -> Result<CliJobRecovery> {
        use sysinfo::System;

        // Snapshot live PIDs once up front.
        let mut sys = System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        // Load all Running rows.
        let running_rows = sqlx::query(
            "SELECT
                job_id, argv_json, status, created_at, started_at, finished_at,
                exit_code, timeout_secs, timed_out, cancel_requested,
                stdout_bytes, stderr_bytes, stdout_truncated, stderr_truncated,
                error_message,
                pid, job_user, job_source, command_class,
                cancelled_at, cancel_signal,
                recovered_at, recovery_reason,
                max_runtime_seconds, max_output_bytes,
                output_cap_exceeded, runtime_cap_exceeded
             FROM cli_jobs
             WHERE status = ?1",
        )
        .bind(CliJobStatus::Running.as_str())
        .fetch_all(&self.pool)
        .await
        .context("load running cli jobs for orphan recovery")?;

        let now = Utc::now().to_rfc3339();
        let mut orphaned_count = 0u64;

        for row in running_rows {
            let job_id: String = row.try_get("job_id")?;
            let pid: Option<i64> = row.try_get("pid")?;

            // A job is a confirmed orphan if:
            // 1. Its PID column is NULL (started before this migration), or
            // 2. Its recorded PID is not alive in the current process table.
            let is_orphan = match pid {
                None => true,
                Some(pid_val) => {
                    let pid_u32 = u32::try_from(pid_val).unwrap_or(u32::MAX);
                    sys.process(sysinfo::Pid::from_u32(pid_u32)).is_none()
                }
            };

            if is_orphan {
                sqlx::query(
                    "UPDATE cli_jobs
                     SET status = ?2,
                         finished_at = ?3,
                         error_message = ?4,
                         recovered_at = ?5,
                         recovery_reason = ?6
                     WHERE job_id = ?1",
                )
                .bind(&job_id)
                .bind(CliJobStatus::Orphaned.as_str())
                .bind(&now)
                .bind("cli job orphaned by dashboard restart")
                .bind(&now)
                .bind("process_not_found")
                .execute(&self.pool)
                .await
                .context("mark cli job orphaned")?;

                orphaned_count += 1;
            }
            // If the PID is still alive, the job might have survived a partial
            // restart (e.g. hot-reload in dev). Leave it Running; the runner
            // will not re-attach to the live process, but the row is still
            // queryable and the operator can cancel it via DELETE.
        }

        // Load Queued rows for re-spawning.
        let queued_rows = sqlx::query(
            "SELECT
                job_id, argv_json, status, created_at, started_at, finished_at,
                exit_code, timeout_secs, timed_out, cancel_requested,
                stdout_bytes, stderr_bytes, stdout_truncated, stderr_truncated,
                error_message,
                pid, job_user, job_source, command_class,
                cancelled_at, cancel_signal,
                recovered_at, recovery_reason,
                max_runtime_seconds, max_output_bytes,
                output_cap_exceeded, runtime_cap_exceeded
             FROM cli_jobs
             WHERE status = ?1
             ORDER BY created_at ASC",
        )
        .bind(CliJobStatus::Queued.as_str())
        .fetch_all(&self.pool)
        .await
        .context("load queued cli jobs for restart recovery")?;

        let restarted_queued = queued_rows
            .into_iter()
            .map(row_to_job)
            .collect::<Result<Vec<_>>>()?;

        Ok(CliJobRecovery {
            restarted_queued,
            orphaned_running: orphaned_count,
        })
    }

    /// Check whether the combined stdout+stderr byte count for `job_id` has
    /// exceeded `cap_bytes`. Returns `true` when the cap is breached.
    pub async fn output_bytes_exceed_cap(&self, job_id: &str, cap_bytes: u64) -> Result<bool> {
        let row = sqlx::query("SELECT stdout_bytes, stderr_bytes FROM cli_jobs WHERE job_id = ?1")
            .bind(job_id)
            .fetch_optional(&self.pool)
            .await
            .context("check output byte cap")?;

        let Some(row) = row else {
            return Ok(false);
        };
        let stdout: u64 = u64::try_from(row.try_get::<i64, _>("stdout_bytes")?).unwrap_or(0);
        let stderr: u64 = u64::try_from(row.try_get::<i64, _>("stderr_bytes")?).unwrap_or(0);
        Ok(stdout.saturating_add(stderr) > cap_bytes)
    }

    async fn append_chunk(&self, job_id: &str, stream: &str, payload: &str) -> Result<()> {
        if payload.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await.context("begin cli chunk tx")?;
        let row = sqlx::query(
            "SELECT stdout_bytes, stderr_bytes, stdout_truncated, stderr_truncated
             FROM cli_jobs
             WHERE job_id = ?1",
        )
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await
        .context("load cli job byte counters")?
        .ok_or_else(|| anyhow!("cli job '{job_id}' not found"))?;

        let (bytes_col, truncated_col) = match stream {
            "stdout" => ("stdout_bytes", "stdout_truncated"),
            "stderr" => ("stderr_bytes", "stderr_truncated"),
            other => return Err(anyhow!("unknown cli job stream '{other}'")),
        };

        let current_bytes: u64 =
            u64::try_from(row.try_get::<i64, _>(bytes_col)?).context("negative stream byte count")?;
        let current_truncated = row.try_get::<i64, _>(truncated_col)? != 0;
        let payload_bytes = payload.len() as u64;
        let next_bytes = current_bytes.saturating_add(payload_bytes);
        let retain_bytes = MAX_PERSISTED_STREAM_BYTES.saturating_sub(current_bytes);
        let retained = if current_truncated || retain_bytes == 0 {
            String::new()
        } else {
            truncate_on_char_boundary(payload, retain_bytes as usize).to_string()
        };
        let truncated_now = current_truncated || next_bytes > MAX_PERSISTED_STREAM_BYTES;

        if !retained.is_empty() {
            let next_index: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(chunk_index), -1) + 1
                 FROM cli_job_output_chunks
                 WHERE job_id = ?1 AND stream = ?2",
            )
            .bind(job_id)
            .bind(stream)
            .fetch_one(&mut *tx)
            .await
            .context("load next cli output chunk index")?;

            sqlx::query(
                "INSERT INTO cli_job_output_chunks (
                    job_id, stream, chunk_index, byte_offset, payload, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(job_id)
            .bind(stream)
            .bind(next_index)
            .bind(i64::try_from(current_bytes).context("byte offset overflow")?)
            .bind(retained)
            .bind(Utc::now().to_rfc3339())
            .execute(&mut *tx)
            .await
            .context("insert cli output chunk")?;
        }

        let update_sql = match stream {
            "stdout" => {
                "UPDATE cli_jobs
                 SET stdout_bytes = ?2,
                     stdout_truncated = ?3
                 WHERE job_id = ?1"
            }
            "stderr" => {
                "UPDATE cli_jobs
                 SET stderr_bytes = ?2,
                     stderr_truncated = ?3
                 WHERE job_id = ?1"
            }
            _ => unreachable!(),
        };

        sqlx::query(update_sql)
            .bind(job_id)
            .bind(i64::try_from(next_bytes).context("stream byte count overflow")?)
            .bind(if truncated_now { 1 } else { 0 })
            .execute(&mut *tx)
            .await
            .context("update cli output counters")?;

        tx.commit().await.context("commit cli chunk tx")?;
        Ok(())
    }
}

/// Parameters for `finish_detailed`.
pub struct FinishParams<'a> {
    pub job_id: &'a str,
    pub status: CliJobStatus,
    pub exit_code: Option<i64>,
    pub error_message: Option<String>,
    pub cancelled_at: Option<String>,
    pub cancel_signal: Option<String>,
    pub output_cap_exceeded: bool,
    pub runtime_cap_exceeded: bool,
}

fn row_to_job(row: sqlx::sqlite::SqliteRow) -> Result<CliJob> {
    let status = row.try_get::<String, _>("status")?;

    let max_runtime_seconds: u64 = u64::try_from(
        row.try_get::<i64, _>("max_runtime_seconds")
            .unwrap_or(DEFAULT_MAX_RUNTIME_SECONDS as i64),
    )
    .unwrap_or(DEFAULT_MAX_RUNTIME_SECONDS);

    let max_output_bytes: u64 = u64::try_from(
        row.try_get::<i64, _>("max_output_bytes")
            .unwrap_or(DEFAULT_MAX_OUTPUT_BYTES as i64),
    )
    .unwrap_or(DEFAULT_MAX_OUTPUT_BYTES);

    Ok(CliJob {
        job_id: row.try_get("job_id")?,
        argv: serde_json::from_str(&row.try_get::<String, _>("argv_json")?)
            .context("deserialize argv_json")?,
        status: CliJobStatus::from_db(&status).ok_or_else(|| anyhow!("unknown cli job status '{status}'"))?,
        created_at: row.try_get("created_at")?,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
        exit_code: row.try_get("exit_code")?,
        timeout_secs: u64::try_from(row.try_get::<i64, _>("timeout_secs")?)
            .context("negative timeout_secs")?,
        timed_out: row.try_get::<i64, _>("timed_out")? != 0,
        cancel_requested: row.try_get::<i64, _>("cancel_requested")? != 0,
        stdout_bytes: u64::try_from(row.try_get::<i64, _>("stdout_bytes")?)
            .context("negative stdout_bytes")?,
        stderr_bytes: u64::try_from(row.try_get::<i64, _>("stderr_bytes")?)
            .context("negative stderr_bytes")?,
        stdout_truncated: row.try_get::<i64, _>("stdout_truncated")? != 0,
        stderr_truncated: row.try_get::<i64, _>("stderr_truncated")? != 0,
        error_message: row.try_get("error_message")?,
        pid: row.try_get("pid").unwrap_or(None),
        job_user: row.try_get("job_user").unwrap_or(None),
        job_source: row.try_get("job_source").unwrap_or(None),
        command_class: row.try_get("command_class").unwrap_or(None),
        cancelled_at: row.try_get("cancelled_at").unwrap_or(None),
        cancel_signal: row.try_get("cancel_signal").unwrap_or(None),
        recovered_at: row.try_get("recovered_at").unwrap_or(None),
        recovery_reason: row.try_get("recovery_reason").unwrap_or(None),
        max_runtime_seconds,
        max_output_bytes,
        output_cap_exceeded: row.try_get::<i64, _>("output_cap_exceeded").unwrap_or(0) != 0,
        runtime_cap_exceeded: row.try_get::<i64, _>("runtime_cap_exceeded").unwrap_or(0) != 0,
    })
}

fn truncate_on_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}
