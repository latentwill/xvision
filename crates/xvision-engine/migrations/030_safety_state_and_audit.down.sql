-- Rollback 029: drop safety tables.
DROP INDEX IF EXISTS idx_safety_audit_result;
DROP INDEX IF EXISTS idx_safety_audit_action_kind;
DROP INDEX IF EXISTS idx_safety_audit_timestamp;
DROP TABLE IF EXISTS safety_audit;
DROP TABLE IF EXISTS safety_state;
