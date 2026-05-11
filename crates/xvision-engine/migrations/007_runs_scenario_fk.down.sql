-- 007_runs_scenario_fk.down.sql
-- Reverse 007_runs_scenario_fk.sql: drop the triggers + index. The
-- pre-existing idx_eval_runs_scenario (migration 002) remains.

DROP TRIGGER IF EXISTS runs_scenario_id_fk_update;
DROP TRIGGER IF EXISTS runs_scenario_id_fk_insert;
DROP INDEX IF EXISTS runs_by_scenario;
