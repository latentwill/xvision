CREATE TABLE IF NOT EXISTS autooptimizer_pattern_snapshots (
    id               TEXT PRIMARY KEY,
    namespace        TEXT NOT NULL,
    instruction      TEXT NOT NULL,
    demos_json       TEXT NOT NULL,
    signature_hash   TEXT NOT NULL,
    metric_name      TEXT NOT NULL,
    optimizer_name   TEXT NOT NULL,
    optimizer_version TEXT NOT NULL,
    provenance_json  TEXT NOT NULL,
    rng_seed         INTEGER NOT NULL DEFAULT 0,
    parent_id        TEXT,
    created_at       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pattern_snapshots_namespace
    ON autooptimizer_pattern_snapshots(namespace, created_at DESC);
