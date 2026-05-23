-- Revert 036_agents_scope_strategy_id.sql.

DROP INDEX IF EXISTS idx_agents_scope_strategy_id;
ALTER TABLE agents DROP COLUMN scope_strategy_id;
