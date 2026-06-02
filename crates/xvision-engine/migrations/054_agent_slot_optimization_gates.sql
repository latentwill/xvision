ALTER TABLE agent_slot_optimizations ADD COLUMN dev_metric TEXT;
ALTER TABLE agent_slot_optimizations ADD COLUMN holdout_metric TEXT;
ALTER TABLE agent_slot_optimizations ADD COLUMN parent_dev_score REAL;
ALTER TABLE agent_slot_optimizations ADD COLUMN child_dev_score REAL;
ALTER TABLE agent_slot_optimizations ADD COLUMN parent_holdout_score REAL;
ALTER TABLE agent_slot_optimizations ADD COLUMN child_holdout_score REAL;
ALTER TABLE agent_slot_optimizations ADD COLUMN gate_epsilon REAL;
ALTER TABLE agent_slot_optimizations ADD COLUMN delta_dev REAL;
ALTER TABLE agent_slot_optimizations ADD COLUMN delta_holdout REAL;
ALTER TABLE agent_slot_optimizations ADD COLUMN gate_verdict TEXT;
ALTER TABLE agent_slot_optimizations ADD COLUMN gate_reason TEXT;
ALTER TABLE agent_slot_optimizations ADD COLUMN gated_at TEXT;

CREATE INDEX IF NOT EXISTS idx_agent_slot_optimizations_gate_verdict
    ON agent_slot_optimizations(gate_verdict);
