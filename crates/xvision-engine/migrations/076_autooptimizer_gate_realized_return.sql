-- 076: Add realized-return ratio columns to autooptimizer_gate_records
-- for the realized-return gate dimension (prevents "open and hope" strategies).
ALTER TABLE autooptimizer_gate_records ADD COLUMN parent_realized_return_ratio REAL;
ALTER TABLE autooptimizer_gate_records ADD COLUMN child_realized_return_ratio REAL;
ALTER TABLE autooptimizer_gate_records ADD COLUMN gate_min_realized_return_ratio REAL;
