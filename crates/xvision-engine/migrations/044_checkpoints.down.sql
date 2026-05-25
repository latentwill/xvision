-- Down: drop the chat_checkpoints table + its index (Phase 2.5).
DROP INDEX IF EXISTS idx_chat_checkpoints_session;
DROP TABLE IF EXISTS chat_checkpoints;
