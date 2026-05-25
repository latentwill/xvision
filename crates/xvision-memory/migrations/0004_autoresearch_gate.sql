ALTER TABLE autoresearch_runs ADD COLUMN gate_metric TEXT;
ALTER TABLE autoresearch_runs ADD COLUMN baseline_score REAL;
ALTER TABLE autoresearch_runs ADD COLUMN candidate_score REAL;
ALTER TABLE autoresearch_runs ADD COLUMN gate_threshold REAL;
ALTER TABLE autoresearch_runs ADD COLUMN gate_passed INTEGER;
ALTER TABLE autoresearch_runs ADD COLUMN gated_at TEXT;
ALTER TABLE autoresearch_runs ADD COLUMN finding_text TEXT;
ALTER TABLE autoresearch_runs ADD COLUMN finding_model TEXT;
ALTER TABLE autoresearch_runs ADD COLUMN finding_blind INTEGER;

CREATE INDEX IF NOT EXISTS idx_autoresearch_runs_gate_passed
    ON autoresearch_runs(gate_passed);
