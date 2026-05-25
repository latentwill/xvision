-- Revert 042_session_events.sql.
--
-- Drops the unified-event log table and its seq index. (sqlx does not run
-- down migrations in production; this exists for parity with the sibling
-- reversible migrations and local rollback testing.)

DROP INDEX IF EXISTS idx_session_events_seq;
DROP TABLE IF EXISTS session_events;
