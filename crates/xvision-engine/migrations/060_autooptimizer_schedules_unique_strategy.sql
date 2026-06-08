-- P5-W2: enforce one schedule per strategy (UNIQUE index on strategy_id).
-- ON CONFLICT DO UPDATE upserts require a unique constraint on the target column.
CREATE UNIQUE INDEX IF NOT EXISTS idx_aosched_strategy_id
  ON autooptimizer_schedules(strategy_id);
