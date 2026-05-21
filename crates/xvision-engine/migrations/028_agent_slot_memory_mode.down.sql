-- Revert 028_agent_slot_memory_mode.sql.

ALTER TABLE agent_slots DROP COLUMN memory_mode;
