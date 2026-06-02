-- Drop provenance tables
DROP TABLE IF EXISTS cycle_seals;
DROP TABLE IF EXISTS session_commitments;

-- Rebuild lineage_nodes without the three hash columns.
-- SQLite < 3.35 has no DROP COLUMN; table-rebuild is the portable approach.
CREATE TABLE lineage_nodes_new (
    bundle_hash       TEXT PRIMARY KEY,
    parent_hash       TEXT REFERENCES lineage_nodes_new(bundle_hash),
    gate_verdict      TEXT NOT NULL,
    status            TEXT NOT NULL,
    cycle_id          TEXT,
    created_at        TEXT NOT NULL,
    diversity_score   REAL
);
INSERT INTO lineage_nodes_new
    SELECT bundle_hash, parent_hash, gate_verdict, status,
           cycle_id, created_at, diversity_score
    FROM lineage_nodes;
DROP TABLE lineage_nodes;
ALTER TABLE lineage_nodes_new RENAME TO lineage_nodes;
