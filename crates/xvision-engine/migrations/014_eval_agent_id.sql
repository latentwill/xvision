-- Rename eval run strategy references from the legacy bundle term to agent_id.
-- The value is the pre-mint strategy agent id used by `xvn strategy`.

ALTER TABLE eval_runs RENAME COLUMN strategy_bundle_hash TO agent_id;
ALTER TABLE eval_attestations RENAME COLUMN strategy_bundle_hash TO agent_id;

DROP INDEX IF EXISTS idx_eval_runs_strategy;
CREATE INDEX IF NOT EXISTS idx_eval_runs_agent
    ON eval_runs(agent_id);
