-- Revert 041_chat_session_rail_state.sql.
--
-- Drops the rail-state columns added to chat_sessions. Independent column
-- drops; the up migration re-adds them with their defaults. (sqlx does not
-- run down migrations in production; this exists for parity with the
-- sibling reversible migrations and local rollback testing.)

ALTER TABLE chat_sessions DROP COLUMN participants_json;
ALTER TABLE chat_sessions DROP COLUMN checkpoint_head;
ALTER TABLE chat_sessions DROP COLUMN tool_policy_json;
ALTER TABLE chat_sessions DROP COLUMN mode;
ALTER TABLE chat_sessions DROP COLUMN focus_path;
ALTER TABLE chat_sessions DROP COLUMN event_cursor;
