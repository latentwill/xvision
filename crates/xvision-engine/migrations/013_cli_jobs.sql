CREATE TABLE IF NOT EXISTS cli_jobs (
    job_id TEXT PRIMARY KEY,
    argv_json TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    exit_code INTEGER,
    timeout_secs INTEGER NOT NULL,
    timed_out INTEGER NOT NULL DEFAULT 0,
    cancel_requested INTEGER NOT NULL DEFAULT 0,
    stdout_bytes INTEGER NOT NULL DEFAULT 0,
    stderr_bytes INTEGER NOT NULL DEFAULT 0,
    stdout_truncated INTEGER NOT NULL DEFAULT 0,
    stderr_truncated INTEGER NOT NULL DEFAULT 0,
    error_message TEXT
);

CREATE TABLE IF NOT EXISTS cli_job_output_chunks (
    job_id TEXT NOT NULL,
    stream TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    byte_offset INTEGER NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (job_id, stream, chunk_index),
    FOREIGN KEY (job_id) REFERENCES cli_jobs(job_id) ON DELETE CASCADE
);
