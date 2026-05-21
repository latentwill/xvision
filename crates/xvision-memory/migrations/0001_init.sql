CREATE TABLE memory_items (
    id                    TEXT PRIMARY KEY,
    namespace             TEXT NOT NULL,
    tier                  TEXT NOT NULL,
    text                  TEXT NOT NULL,
    embedding             BLOB NOT NULL,
    embedding_dim         INTEGER NOT NULL,
    embedder_id           TEXT NOT NULL,
    created_at            TEXT NOT NULL,
    -- Observation provenance; NULL on Patterns.
    run_id                TEXT,
    scenario_id           TEXT,
    cycle_idx             INTEGER,
    -- Pattern temporal-safety field; NULL on Observations.
    training_window_end   TEXT
);

CREATE INDEX idx_memory_items_tier_namespace ON memory_items(tier, namespace);
CREATE INDEX idx_memory_items_training_window ON memory_items(training_window_end);
CREATE INDEX idx_memory_items_created ON memory_items(created_at);
