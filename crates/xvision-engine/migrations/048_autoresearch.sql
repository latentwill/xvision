-- 048_autoresearch.sql
--
-- AR-1 autoresearch tables: lineage graph, cycle seals, session commitments.
-- Operator-surface status values: 'active' | 'rejected' (terminology lock
-- 2026-05-27: LineageStatus::Ghost -> "Rejected").
--
-- cycle_id columns reference xvision-core's cycles table logically; SQLite
-- cannot enforce cross-file foreign keys, so no FK clause is declared here.

CREATE TABLE IF NOT EXISTS lineage_nodes (
    bundle_hash              TEXT PRIMARY KEY,
    parent_hash              TEXT REFERENCES lineage_nodes(bundle_hash),
    diff_hash                TEXT,
    metrics_day_hash         TEXT,
    metrics_untouched_hash   TEXT,
    gate_verdict             TEXT NOT NULL,
    status                   TEXT NOT NULL,  -- 'active' | 'rejected'
    cycle_id                 TEXT,           -- logical ref to xvision-core cycles table; not a SQL FK (separate DB)
    created_at               TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cycle_seals (
    seal_id            TEXT PRIMARY KEY,
    cycle_id           TEXT NOT NULL,        -- logical ref to xvision-core cycles table; not a SQL FK (separate DB)
    merkle_root        TEXT NOT NULL,
    operator_signature TEXT NOT NULL,
    sealed_at          TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS session_commitments (
    session_id                  TEXT PRIMARY KEY,
    config_hash                 TEXT NOT NULL,
    parent_strategy_hashes_json TEXT NOT NULL,
    signature                   TEXT NOT NULL,
    created_at                  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_lineage_parent ON lineage_nodes(parent_hash);
CREATE INDEX IF NOT EXISTS idx_lineage_status  ON lineage_nodes(status);
CREATE INDEX IF NOT EXISTS idx_cycle_seals_cycle ON cycle_seals(cycle_id);
