-- 035_eval_bakeoffs.down.sql
DROP INDEX IF EXISTS idx_eval_bakeoff_runs_bakeoff;
DROP INDEX IF EXISTS idx_eval_bakeoff_runs_run_id;
DROP TABLE IF EXISTS eval_bakeoff_runs;
DROP INDEX IF EXISTS idx_eval_bakeoffs_status;
DROP TABLE IF EXISTS eval_bakeoffs;
