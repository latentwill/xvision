-- Revert 020_eval_batches.sql.
--
-- SQLite ALTER TABLE DROP COLUMN requires SQLite >= 3.35 (2021-03-12).
-- The workspace ships with sqlx 0.8 which bundles libsqlite3 >= 3.39, so
-- the simple DROP COLUMN form is safe here. This matches the pattern used
-- by 019_agent_slot_prompt_version.down.sql (bare DROP COLUMN).

DROP INDEX IF EXISTS idx_eval_runs_batch;
ALTER TABLE eval_runs DROP COLUMN batch_id;
DROP TABLE IF EXISTS eval_batches;
