ALTER TABLE lineage_nodes ADD COLUMN diff_hash TEXT;
ALTER TABLE lineage_nodes ADD COLUMN metrics_day_hash TEXT;
ALTER TABLE lineage_nodes ADD COLUMN metrics_untouched_hash TEXT;

CREATE TABLE IF NOT EXISTS cycle_seals (
    seal_id            TEXT PRIMARY KEY,
    cycle_id           TEXT NOT NULL,
    merkle_root        TEXT NOT NULL,
    operator_signature TEXT NOT NULL,
    sealed_at          TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS session_commitments (
    session_id                   TEXT PRIMARY KEY,
    config_hash                  TEXT NOT NULL,
    parent_strategy_hashes_json  TEXT NOT NULL,
    signature                    TEXT NOT NULL,
    created_at                   TEXT NOT NULL
);
