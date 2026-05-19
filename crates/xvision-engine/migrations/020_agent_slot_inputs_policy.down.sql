-- Revert 020_agent_slot_inputs_policy.sql.

ALTER TABLE agent_slots DROP COLUMN inputs_policy;
