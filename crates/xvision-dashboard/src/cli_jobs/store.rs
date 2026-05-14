use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use sqlx::{Row, SqlitePool};

use super::model::{CliJob, CliJobOutput, CliJobStatus};

const MAX_PERSISTED_STREAM_BYTES: u64 = 256 * 1024;

#[derive(Clone)]
pub struct CliJobStore {
    pool: SqlitePool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliJobRecovery {
    pub restarted_queued: Vec<CliJob>,
    pub failed_running: u64,
}

impl CliJobStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_queued(&self, argv: Vec<String>, timeout_secs: u64) -> Result<CliJob> {
        let job_id = format!("job_{}", ulid::Ulid::new());
        let created_at = Utc::now().to_rfc3339();
        let argv_json = serde_json::to_string(&argv).context("serialize argv")?;

        sqlx::query(
            "INSERT INTO cli_jobs (
                job_id, argv_json, status, created_at, timeout_secs,
                timed_out, cancel_requested, stdout_bytes, stderr_bytes,
                stdout_truncated, stderr_truncated
             ) VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 0, 0, 0, 0)",
        )
        .bind(&job_id)
        .bind(argv_json)
        .bind(CliJobStatus::Queued.as_str())
        .bind(&created_at)
        .bind(i64::try_from(timeout_secs).context("timeout_secs overflow")?)
        .execute(&self.pool)
        .await
        .context("insert cli_jobs row")?;

        Ok(CliJob {
            job_id,
            argv,
            status: CliJobStatus::Queued,
            created_at,
            started_at: None,
            finished_at: None,
            exit_code: None,
            timeout_secs,
            timed_out: false,
            cancel_requested: false,
            stdout_bytes: 0,
            stderr_bytes: 0,
            stdout_truncated: false,
            stderr_truncated: false,
            error_message: None,
        })
    }

    pub async fn get(&self, job_id: &str) -> Result<Option<CliJob>> {
        let row = sqlx::query(
            "SELECT
                job_id, argv_json, status, created_at, started_at, finished_at,
                exit_code, timeout_secs, timed_out, cancel_requested,
                stdout_bytes, stderr_bytes, stdout_truncated, stderr_truncated,
                error_message
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
        sqlx::query(
            "UPDATE cli_jobs
             SET status = ?2, started_at = ?3
             WHERE job_id = ?1",
        )
        .bind(job_id)
        .bind(CliJobStatus::Running.as_str())
        .bind(Utc::now().to_rfc3339())
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
        let timed_out = matches!(status, CliJobStatus::TimedOut);
        let cancelled = matches!(status, CliJobStatus::Cancelled);

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
                 error_message = ?7
             WHERE job_id = ?1",
        )
        .bind(job_id)
        .bind(status.as_str())
        .bind(Utc::now().to_rfc3339())
        .bind(exit_code)
        .bind(if timed_out { 1 } else { 0 })
        .bind(if cancelled { 1 } else { 0 })
        .bind(error_message)
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

    pub async fn recover_after_restart(&self) -> Result<CliJobRecovery> {
        let queued_rows = sqlx::query(
            "SELECT
                job_id, argv_json, status, created_at, started_at, finished_at,
                exit_code, timeout_secs, timed_out, cancel_requested,
                stdout_bytes, stderr_bytes, stdout_truncated, stderr_truncated,
                error_message
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

        let failed = sqlx::query(
            "UPDATE cli_jobs
             SET status = ?1,
                 finished_at = ?2,
                 error_message = ?3
             WHERE status = ?4",
        )
        .bind(CliJobStatus::Failed.as_str())
        .bind(Utc::now().to_rfc3339())
        .bind("cli job orphaned by dashboard restart")
        .bind(CliJobStatus::Running.as_str())
        .execute(&self.pool)
        .await
        .context("fail orphaned running cli jobs after restart")?
        .rows_affected();

        Ok(CliJobRecovery {
            restarted_queued,
            failed_running: failed,
        })
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

        let current_bytes: u64 = u64::try_from(row.try_get::<i64, _>(bytes_col)?)
            .context("negative stream byte count")?;
        let current_truncated = row.try_get::<i64, _>(truncated_col)? != 0;
        let payload_bytes = payload.as_bytes().len() as u64;
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

fn row_to_job(row: sqlx::sqlite::SqliteRow) -> Result<CliJob> {
    let status = row.try_get::<String, _>("status")?;

    Ok(CliJob {
        job_id: row.try_get("job_id")?,
        argv: serde_json::from_str(&row.try_get::<String, _>("argv_json")?)
            .context("deserialize argv_json")?,
        status: CliJobStatus::from_db(&status)
            .ok_or_else(|| anyhow!("unknown cli job status '{status}'"))?,
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
