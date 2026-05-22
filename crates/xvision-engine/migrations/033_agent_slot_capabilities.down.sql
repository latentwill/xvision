-- Revert 033_agent_slot_capabilities.sql.

ALTER TABLE agent_slots DROP COLUMN capabilities;
