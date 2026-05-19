-- 021_eval_batches.sql — eval-batch-persistence track.
--
-- Persists batch metadata so `xvn eval batch status <batch_id>` has a
-- source of truth and `xvn eval compare --batch <id>` can resolve runs
-- without a separate `--runs` list.
--
-- Status lifecycle: pending → running → (completed | partial | failed)
--   completed : all runs completed successfully
--   partial   : some runs completed, at least one failed
--   failed    : all runs failed (or none completed)
--
-- review_with is the agent profile id passed via --review-with; null when
-- the batch was launched without per-run review.

CREATE TABLE IF NOT EXISTS eval_batches (
    batch_id     TEXT PRIMARY KEY,
    strategy_id  TEXT NOT NULL,
    review_with  TEXT,
    created_at   TEXT NOT NULL,
    completed_at TEXT,
    status       TEXT NOT NULL DEFAULT 'pending'
);

ALTER TABLE eval_runs ADD COLUMN batch_id TEXT REFERENCES eval_batches(batch_id);
CREATE INDEX IF NOT EXISTS idx_eval_runs_batch ON eval_runs(batch_id);
