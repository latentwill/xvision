-- Migration 061: per-run (per-run) pause flag on eval_runs.
--
-- `paused` is an ADDITIVE, per-run skip honored by the live executor
-- ALONGSIDE the global SafetyManager pause: a paused run keeps iterating
-- but does NOT submit broker orders for the affected cycles. It never
-- terminates the run — resume clears the flag and submits resume.
--
-- `paused_at` records the RFC3339 timestamp of the most recent pause (NULL
-- when never paused / after resume). Existing rows default to not-paused,
-- which matches every run created before this migration.

ALTER TABLE eval_runs ADD COLUMN paused BOOLEAN NOT NULL DEFAULT 0;
ALTER TABLE eval_runs ADD COLUMN paused_at TEXT;
