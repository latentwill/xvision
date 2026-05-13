DROP INDEX IF EXISTS idx_eval_runs_agent;

ALTER TABLE eval_runs RENAME COLUMN agent_id TO strategy_bundle_hash;
ALTER TABLE eval_attestations RENAME COLUMN agent_id TO strategy_bundle_hash;

CREATE INDEX IF NOT EXISTS idx_eval_runs_strategy
    ON eval_runs(strategy_bundle_hash);
