-- SQLite does not support DROP COLUMN in versions prior to 3.35.
-- Rolling back migration 028 requires recreating the table without the
-- added columns.

CREATE TABLE cli_jobs_rollback_028 AS
    SELECT
        job_id, argv_json, status, created_at, started_at, finished_at,
        exit_code, timeout_secs, timed_out, cancel_requested,
        stdout_bytes, stderr_bytes, stdout_truncated, stderr_truncated,
        error_message
    FROM cli_jobs;

DROP TABLE cli_jobs;

ALTER TABLE cli_jobs_rollback_028 RENAME TO cli_jobs;
