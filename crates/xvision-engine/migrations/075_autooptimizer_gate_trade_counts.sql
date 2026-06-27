-- 075: Add trade-count fields to autooptimizer_gate_records for the
-- min-trade-retention gate dimension (fix 0-trade degenerate strategies).
ALTER TABLE autooptimizer_gate_records ADD COLUMN parent_n_trades INTEGER;
ALTER TABLE autooptimizer_gate_records ADD COLUMN child_n_trades INTEGER;
ALTER TABLE autooptimizer_gate_records ADD COLUMN min_trade_retention_ratio REAL;
