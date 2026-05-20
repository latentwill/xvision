CREATE TABLE memory_items (
    id            TEXT PRIMARY KEY,
    namespace     TEXT NOT NULL,
    text          TEXT NOT NULL,
    embedding     BLOB NOT NULL,
    embedding_dim INTEGER NOT NULL,
    embedder_id   TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    source_run_id TEXT,
    source_cycle_id TEXT
);

CREATE INDEX idx_memory_items_namespace ON memory_items(namespace);
CREATE INDEX idx_memory_items_created   ON memory_items(created_at);
