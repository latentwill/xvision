-- Revert 025_agent_slot_cache_and_window.sql.

ALTER TABLE agent_slots DROP COLUMN bar_history_limit;
