CREATE TABLE IF NOT EXISTS autooptimizer_runs (
    id                   TEXT PRIMARY KEY,
    namespace            TEXT NOT NULL,
    observation_ids_json TEXT NOT NULL,
    pattern_id           TEXT NOT NULL,
    pattern_text         TEXT NOT NULL,
    promotion_state      TEXT NOT NULL,
    min_observations     INTEGER NOT NULL,
    created_at           TEXT NOT NULL,
    status               TEXT NOT NULL,
    error                TEXT
);

CREATE INDEX IF NOT EXISTS idx_autooptimizer_runs_namespace_created
    ON autooptimizer_runs(namespace, created_at);
CREATE INDEX IF NOT EXISTS idx_autooptimizer_runs_pattern_id
    ON autooptimizer_runs(pattern_id);
