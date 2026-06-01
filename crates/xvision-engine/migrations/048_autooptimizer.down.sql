-- 048_autooptimizer.down.sql
--
-- Reverse of 048: drop autooptimizer tables in reverse-dependency order.
-- cycle_seals references cycle_id (logical); lineage_nodes has a self-ref
-- on parent_hash; session_commitments has no inbound references.

DROP TABLE IF EXISTS cycle_seals;
DROP TABLE IF EXISTS lineage_nodes;
DROP TABLE IF EXISTS session_commitments;
