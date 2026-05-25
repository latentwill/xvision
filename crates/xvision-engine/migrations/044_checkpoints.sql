-- 044_checkpoints.sql
--
-- Phase 2.5 of the chat-rail / DSPy / strategy-agents wave
-- (docs/superpowers/specs/2026-05-24-chat-rail-and-strategy-agents-evaluation.md).
--
-- NOTE on the table name: migration 018 already owns a `checkpoints` table for
-- agent-run replay (run_id/sequence/span_id). This is a DIFFERENT concept — the
-- chat-rail authoring snapshot — so it lives in `chat_checkpoints` to avoid the
-- name collision.
--
-- Checkpoint + restore for the chat rail. A checkpoint is an immutable
-- snapshot of a session's mutable authoring artifacts taken *before* a
-- mutating tool runs, so an operator can rewind a bad edit. The snapshot is
-- content-addressed: the individual artifact payloads (Strategy JSON, agent
-- slot rows, tool policy, focus file) are written to the blob store and a
-- single `captured_json` manifest records the artifact-kind → blob-hash map
-- plus the metadata needed to write each one back (e.g. the strategy id, the
-- agent id, the focus path). Restore reads those blobs and rewinds each
-- artifact verbatim.
--
--   * checkpoint_id — globally-unique ULID. PK. Referenced by
--                     CheckpointRestored events and chat_sessions.checkpoint_head.
--   * session_id    — owning chat session. CASCADE-deletes with the session so
--                     a deleted conversation leaves no orphaned checkpoints.
--   * created_at    — RFC3339 UTC timestamp the snapshot was taken.
--   * kind          — why the checkpoint was taken (e.g. `pre_tool`, `manual`).
--                     Free text; the rail renders it, the engine does not branch on it.
--   * content_hash  — sha256 (hex) of the canonical `captured_json` manifest.
--                     Same artifacts at the same hashes → same content_hash, so a
--                     no-op checkpoint is recognizable and cheap to dedupe on.
--   * captured_json — the manifest: artifact kinds + their blob hashes + the
--                     metadata required to restore each (strategy id, agent id,
--                     focus path). The blob payloads live in the blob store.
--   * label         — optional operator-facing label for the checkpoint.
--
-- Additive; no existing table is touched. CREATE TABLE / CREATE INDEX with
-- IF NOT EXISTS so a re-run is idempotent (matches the 042/043 convention).
--
-- Wired at runtime via `migrate_checkpoints` in `ApiContext::open` (the
-- hand-maintained registry; this repo does NOT apply migrations through
-- `sqlx::migrate!`). Without that wiring the table never exists at runtime.
CREATE TABLE IF NOT EXISTS chat_checkpoints (
    checkpoint_id TEXT PRIMARY KEY,           -- ULID
    session_id    TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    created_at    TEXT NOT NULL,              -- RFC3339 UTC
    kind          TEXT NOT NULL,              -- pre_tool | manual | …
    content_hash  TEXT NOT NULL,              -- sha256 hex of captured_json
    captured_json TEXT NOT NULL,              -- artifact-kind → blob-hash manifest
    label         TEXT                        -- optional operator label
);

CREATE INDEX IF NOT EXISTS idx_chat_checkpoints_session ON chat_checkpoints(session_id, created_at);
