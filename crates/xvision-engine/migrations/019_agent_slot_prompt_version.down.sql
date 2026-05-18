-- Revert 019_agent_slot_prompt_version.sql.

ALTER TABLE agent_slots DROP COLUMN prompt_version;
