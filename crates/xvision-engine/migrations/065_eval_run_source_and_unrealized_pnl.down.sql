-- Rollback 065: SQLite does not support DROP COLUMN in older versions and
-- dropping/recreating eval_runs would be destructive. For SQLite
-- compatibility this rollback is a no-op — the columns stay but application
-- code on an older schema version ignores them. A proper rollback requires
-- exporting + reimporting the table. Mirrors the 061/062/063 down-migrations.
SELECT 1; -- no-op
