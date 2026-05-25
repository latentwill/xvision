-- Down: drop the checkpoints table + its index (Phase 2.5).
DROP INDEX IF EXISTS idx_checkpoints_session;
DROP TABLE IF EXISTS checkpoints;
