CREATE TABLE IF NOT EXISTS pattern_optimizations (
    optimization_id TEXT NOT NULL,
    pattern_id      TEXT NOT NULL,
    role            TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    PRIMARY KEY (optimization_id, pattern_id, role),
    FOREIGN KEY (optimization_id) REFERENCES agent_slot_optimizations(optimization_id)
);

CREATE INDEX IF NOT EXISTS idx_pattern_optimizations_pattern
    ON pattern_optimizations(pattern_id);
