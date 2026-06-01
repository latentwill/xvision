-- 049_autooptimizer_diversity.down.sql
--
-- Reverses 049_autooptimizer_diversity.sql.
-- Requires SQLite 3.35+ for ALTER TABLE ... DROP COLUMN.

DROP INDEX IF EXISTS idx_lineage_embeddings_bundle;
ALTER TABLE lineage_nodes DROP COLUMN diversity_score;
DROP TABLE IF EXISTS lineage_embeddings;
