-- 049_autooptimizer_diversity.sql
--
-- AR-2 Task 7: diversity-decay metric tables.
-- Stores per-bundle embeddings (via blob store) and adds a lazily-computed
-- diversity_score column to lineage_nodes.
--
-- lineage_embeddings.embedding_blob_hash is the blob_store SHA-256 key for
-- the JSON-encoded f32[] vector produced from the bundle's program-view text.
-- diversity_score in lineage_nodes is computed by compute_diversity_score()
-- and cached lazily; NULL until that function is called for the node.

CREATE TABLE IF NOT EXISTS lineage_embeddings (
    bundle_hash         TEXT PRIMARY KEY REFERENCES lineage_nodes(bundle_hash),
    embedding_blob_hash TEXT NOT NULL,
    embedding_dim       INTEGER NOT NULL,
    embedded_at         TEXT NOT NULL
);

ALTER TABLE lineage_nodes ADD COLUMN diversity_score REAL;

CREATE INDEX IF NOT EXISTS idx_lineage_embeddings_bundle ON lineage_embeddings(bundle_hash);
