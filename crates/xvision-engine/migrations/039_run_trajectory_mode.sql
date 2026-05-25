-- 039_run_trajectory_mode.sql
--
-- Stage 1 of the Cline runtime unification (operational-visibility
-- contract, umbrella "Subplan inheritance contract" item 3). Adds a
-- `trajectory_mode` column to `agent_runs` so live runs surface their
-- runtime mode in the CLI / dashboard / structured run artifacts.
--
-- At Stage 1 the only value produced is `'live'` (the Cline sidecar live
-- path). The sibling fields below are DECLARED now and POPULATED in
-- Stages 2-3 (record/replay), so the schema is stable before replay lands:
--   * replay_hit_ratio  — fraction of model frames served from a recorded
--                          trajectory on a replay run (NULL on live runs).
--   * dropped_events     — count of observability events dropped under
--                          backpressure for this run (Stage 4 piping).
--   * recovery_reason    — why a partial-cycle / divergence recovery fired
--                          (Stage 3 live-vs-replay divergence handling).
--
-- SQLite ALTER TABLE ADD COLUMN is non-rewriting and safe on the existing
-- agent_runs table (migration 018).

ALTER TABLE agent_runs ADD COLUMN trajectory_mode TEXT NOT NULL DEFAULT 'live';
ALTER TABLE agent_runs ADD COLUMN replay_hit_ratio REAL;
ALTER TABLE agent_runs ADD COLUMN dropped_events INTEGER NOT NULL DEFAULT 0;
ALTER TABLE agent_runs ADD COLUMN recovery_reason TEXT;
