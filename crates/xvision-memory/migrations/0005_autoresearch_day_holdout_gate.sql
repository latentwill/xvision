ALTER TABLE autoresearch_runs ADD COLUMN parent_day_score REAL;
ALTER TABLE autoresearch_runs ADD COLUMN child_day_score REAL;
ALTER TABLE autoresearch_runs ADD COLUMN parent_holdout_score REAL;
ALTER TABLE autoresearch_runs ADD COLUMN child_holdout_score REAL;
ALTER TABLE autoresearch_runs ADD COLUMN gate_epsilon REAL;
ALTER TABLE autoresearch_runs ADD COLUMN delta_day REAL;
ALTER TABLE autoresearch_runs ADD COLUMN delta_holdout REAL;
ALTER TABLE autoresearch_runs ADD COLUMN gate_verdict TEXT;
ALTER TABLE autoresearch_runs ADD COLUMN gate_reason TEXT;
ALTER TABLE autoresearch_runs ADD COLUMN qualitative_finding_json TEXT;
ALTER TABLE autoresearch_runs ADD COLUMN finding_blinded_metrics INTEGER;
ALTER TABLE autoresearch_runs ADD COLUMN judge_model TEXT;
ALTER TABLE autoresearch_runs ADD COLUMN judge_token_cost INTEGER;

CREATE INDEX IF NOT EXISTS idx_autoresearch_runs_gate_verdict
    ON autoresearch_runs(gate_verdict);
