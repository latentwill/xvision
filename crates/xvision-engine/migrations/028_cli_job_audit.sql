-- Migration 028: CLI job audit fields, PID tracking, and supervisor caps.
--
-- Adds columns to cli_jobs that support:
--   * PID-liveness orphan recovery (pid column)
--   * Audit trail: user, source, command_class, cancelled_at, cancel_signal
--   * Orphan recovery timestamps: recovered_at, recovery_reason
--   * Dashboard-layer supervisor caps (distinct from engine token/decision caps):
--       max_runtime_seconds  — dashboard kills the child process after N seconds
--       max_output_bytes     — dashboard truncates + kills after N bytes total
--   * Cap-breach status flags: output_cap_exceeded, runtime_cap_exceeded
--
-- All columns are nullable / have defaults so existing rows remain valid.

ALTER TABLE cli_jobs ADD COLUMN pid INTEGER;

-- Audit fields: who submitted the job and via which surface.
ALTER TABLE cli_jobs ADD COLUMN job_user TEXT;
ALTER TABLE cli_jobs ADD COLUMN job_source TEXT;
ALTER TABLE cli_jobs ADD COLUMN command_class TEXT;

-- Cancellation detail: when was SIGTERM sent, which signal killed it.
ALTER TABLE cli_jobs ADD COLUMN cancelled_at TEXT;
ALTER TABLE cli_jobs ADD COLUMN cancel_signal TEXT;

-- Orphan-recovery metadata.
ALTER TABLE cli_jobs ADD COLUMN recovered_at TEXT;
ALTER TABLE cli_jobs ADD COLUMN recovery_reason TEXT;

-- Dashboard process-supervisor caps.
-- max_runtime_seconds = 0 means "use server default" (3600s).
-- max_output_bytes    = 0 means "use server default" (10 MB).
ALTER TABLE cli_jobs ADD COLUMN max_runtime_seconds INTEGER NOT NULL DEFAULT 0;
ALTER TABLE cli_jobs ADD COLUMN max_output_bytes INTEGER NOT NULL DEFAULT 0;

-- Cap-breach flags.
ALTER TABLE cli_jobs ADD COLUMN output_cap_exceeded INTEGER NOT NULL DEFAULT 0;
ALTER TABLE cli_jobs ADD COLUMN runtime_cap_exceeded INTEGER NOT NULL DEFAULT 0;
