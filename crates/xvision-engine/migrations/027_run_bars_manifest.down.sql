-- 026_run_bars_manifest.down.sql — roll back bars manifest columns.
--
-- SQLite does not support DROP COLUMN before version 3.35 (2021-03-12).
-- The workspace MSRV is Rust 1.95 which ships with a SQLite version that
-- supports DROP COLUMN. However, the safest approach for a rollback is to
-- recreate the table without the added columns.
--
-- Note: sqlx-migrate does not invoke down-migrations automatically; this
-- file exists for manual rollback scenarios during development.

DROP INDEX IF EXISTS idx_eval_runs_manifest_canonical;

-- SQLite: use table reconstruction to drop columns (no direct DROP COLUMN
-- for multiple columns in one statement on older SQLite versions).
-- We use the ADD + SELECT approach since SQLite ≥ 3.35 supports single
-- DROP COLUMN. For robustness we drop each column individually.
ALTER TABLE eval_runs DROP COLUMN IF EXISTS bars_manifest;
ALTER TABLE eval_runs DROP COLUMN IF EXISTS manifest_canonical;
ALTER TABLE eval_runs DROP COLUMN IF EXISTS bars_content_hash;
