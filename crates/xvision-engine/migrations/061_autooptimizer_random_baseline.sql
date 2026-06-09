-- Random-baseline edge metric (informational, never gating).
-- edge_over_random = child_day_score - random_baseline_score
-- parent_edge      = parent_day_score - random_baseline_score
-- edge_delta       = edge_over_random - parent_edge
-- Computed once per training window from a fixed-seed random LONG/SHORT/FLAT
-- agent (direction-restricted) run through the same backtest engine.
ALTER TABLE autooptimizer_gate_records ADD COLUMN edge_over_random REAL;
ALTER TABLE autooptimizer_gate_records ADD COLUMN parent_edge REAL;
ALTER TABLE autooptimizer_gate_records ADD COLUMN edge_delta REAL;
