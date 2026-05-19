-- Revert 023_hypothesis_and_experiments.sql.
--
-- Part A (hypothesis) stored no DDL in the forward migration, so nothing
-- to revert here. Strategy JSON files will simply ignore the `hypothesis`
-- field once the Rust struct field is removed (unknown fields are skipped
-- by serde; callers that still have hypothesis JSON will ignore it).
--
-- Part B: drop the experiments table and its indexes.
-- SQLite DROP INDEX IF EXISTS + DROP TABLE IF EXISTS are safe and idempotent.

DROP INDEX IF EXISTS idx_experiments_batch;
DROP INDEX IF EXISTS idx_experiments_created;
DROP TABLE IF EXISTS experiments;
