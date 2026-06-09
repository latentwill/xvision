-- Rollback 061: SQLite does not support DROP COLUMN in older versions.
-- Dropping and recreating eval_runs without paused/paused_at would be
-- destructive. For SQLite compatibility, this rollback is a no-op — the
-- columns stay but the application code ignores them when operating on an
-- older schema version. A proper rollback requires exporting + reimporting
-- the table.
SELECT 1; -- no-op
