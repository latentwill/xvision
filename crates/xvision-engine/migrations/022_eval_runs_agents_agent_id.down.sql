DROP INDEX IF EXISTS idx_eval_runs_agents_agent_id;

ALTER TABLE eval_runs DROP COLUMN agents_agent_id;
