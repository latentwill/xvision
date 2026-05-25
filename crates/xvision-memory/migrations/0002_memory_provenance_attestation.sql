ALTER TABLE memory_items ADD COLUMN source_window_start TEXT;
ALTER TABLE memory_items ADD COLUMN source_window_end TEXT;
ALTER TABLE memory_items ADD COLUMN promotion_state TEXT;
ALTER TABLE memory_items ADD COLUMN attestation_id TEXT;

CREATE INDEX IF NOT EXISTS idx_memory_items_source_window_end ON memory_items(source_window_end);
CREATE INDEX IF NOT EXISTS idx_memory_items_promotion_state ON memory_items(promotion_state);
CREATE INDEX IF NOT EXISTS idx_memory_items_attestation_id ON memory_items(attestation_id);

CREATE TABLE IF NOT EXISTS operator_attestations (
    id                         TEXT PRIMARY KEY,
    operator_initials          TEXT NOT NULL,
    surface                    TEXT NOT NULL,
    warning_text_hash          TEXT NOT NULL,
    created_at                 TEXT NOT NULL,
    signature                  TEXT
);
