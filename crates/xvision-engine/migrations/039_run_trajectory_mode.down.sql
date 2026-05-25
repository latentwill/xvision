-- Revert 039_run_trajectory_mode.sql.
--
-- Drops the trajectory-mode observability columns added to agent_runs.
-- Column drops are independent and reversible (the up migration re-adds
-- them with their defaults).

ALTER TABLE agent_runs DROP COLUMN recovery_reason;
ALTER TABLE agent_runs DROP COLUMN dropped_events;
ALTER TABLE agent_runs DROP COLUMN replay_hit_ratio;
ALTER TABLE agent_runs DROP COLUMN trajectory_mode;
