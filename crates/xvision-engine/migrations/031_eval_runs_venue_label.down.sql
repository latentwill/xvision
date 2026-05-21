-- Rollback 030: SQLite does not support DROP COLUMN in older versions.
-- Dropping and recreating eval_runs without venue_label would be destructive.
-- For SQLite compatibility, this rollback is a no-op — the column stays but
-- the application code ignores it when operating on an older schema version.
-- A proper rollback requires exporting + reimporting the table.
SELECT 1; -- no-op
