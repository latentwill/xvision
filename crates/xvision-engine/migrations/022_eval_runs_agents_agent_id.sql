-- 022_eval_runs_agents_agent_id.sql — F-11 eval-bundle-agent-id-map.
--
-- Adds a sibling column `agents_agent_id` to `eval_runs` carrying the
-- long-lived `agents.agent_id` ULID of the agent that drove the run.
--
-- The pre-existing `eval_runs.agent_id` column is the bundle / strategy
-- artifact hash (see migration 014's "rename strategy_bundle_hash to
-- agent_id" — the value is the *strategy* agent id, not the workspace
-- agent record id). That overload makes it impossible to navigate from
-- an eval run back to the calling agent in the agents library without a
-- fragile regex / heuristic lookup. F-11 splits the two concerns:
--
--   - `agent_id` — strategy bundle hash (unchanged).
--   - `agents_agent_id` — workspace `agents.agent_id` ULID (new, nullable).
--
-- Nullable + no FK on purpose. Pre-existing rows are not backfilled —
-- there is no reliable mapping from a bundle hash to the originating
-- agent record for historical runs. New rows populate it at run start
-- in `crates/xvision-engine/src/api/eval.rs`.
--
-- An index supports the reverse lookup ("list eval_runs for agent X")
-- the agents-page UI surfaces.

ALTER TABLE eval_runs ADD COLUMN agents_agent_id TEXT;

CREATE INDEX IF NOT EXISTS idx_eval_runs_agents_agent_id
    ON eval_runs(agents_agent_id);
