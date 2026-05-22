-- 035_eval_bakeoffs.sql
--
-- Persisted bakeoff record for `xvn model bakeoff` (Wave B #6,
-- contract `cli-model-bakeoff`). A bakeoff is one operator invocation
-- of the (strategy × model) matrix verb. The row in `eval_bakeoffs`
-- captures the parameters, status, and roll-up summary; each arm
-- launched is one row in `eval_bakeoff_runs` joined to the underlying
-- `eval_runs` row.
--
-- Status lifecycle:
--   "running"   — at least one arm still pending/active
--   "completed" — every arm landed in a non-failed terminal state
--   "partial"   — every arm terminal, but at least one failed/cancelled
--   "failed"    — every arm failed/cancelled (no completed arms)
--
-- The summary_json column holds the rolled-up result (per-arm
-- (provider, model, return_pct, status) tuples) so the `xvn model
-- bakeoff status` read path does not need to re-query each
-- contributing run.

CREATE TABLE IF NOT EXISTS eval_bakeoffs (
    bakeoff_id TEXT PRIMARY KEY,
    name TEXT,
    status TEXT NOT NULL,
    params_json TEXT NOT NULL,
    summary_json TEXT,
    started_at TEXT NOT NULL,
    completed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_eval_bakeoffs_status
    ON eval_bakeoffs(status);

CREATE TABLE IF NOT EXISTS eval_bakeoff_runs (
    bakeoff_id TEXT NOT NULL,
    arm_index INTEGER NOT NULL,
    run_id TEXT,
    arm_strategy_id TEXT NOT NULL,
    arm_provider TEXT NOT NULL,
    arm_model TEXT NOT NULL,
    status TEXT NOT NULL,
    error TEXT,
    PRIMARY KEY (bakeoff_id, arm_index),
    FOREIGN KEY (bakeoff_id) REFERENCES eval_bakeoffs(bakeoff_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_eval_bakeoff_runs_run_id
    ON eval_bakeoff_runs(run_id);
CREATE INDEX IF NOT EXISTS idx_eval_bakeoff_runs_bakeoff
    ON eval_bakeoff_runs(bakeoff_id);
