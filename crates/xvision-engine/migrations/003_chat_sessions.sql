-- Chat-rail persistence — Plan #11 Task 1.
-- See v1-shipping-plan.md §"Migration reservations" — owner: Plan #11.
-- Sibling migration 004_search_index.sql (Plan #12 Command Palette) ships
-- the FTS5 search index against the same xvn.db.
--
-- One row per active chat session. The Wizard's conversation persists
-- across route changes for the duration of a session, so navigating away
-- and back doesn't lose context.
--
-- `context_scope_json` carries the serialized `ContextScope` enum value
-- (Workspace / Route / Run / Strategy / Deployment / Compare /
-- JournalFilter / Selection / Seed). Keeping it as a JSON blob means the
-- enum can grow new variants without a migration; the engine deserializes
-- on read.

CREATE TABLE IF NOT EXISTS chat_sessions (
    id                    TEXT PRIMARY KEY,           -- ULID
    started_at            TEXT NOT NULL,              -- RFC3339 UTC
    last_activity_at      TEXT NOT NULL,              -- RFC3339 UTC
    context_scope_json    TEXT NOT NULL DEFAULT '{}'  -- ContextScope serialized
);

-- Append-only message log. `seq` is monotonic within a session
-- (chronological replay order). Keyed by (session_id, seq) on the index
-- so `load_history` is a clean ordered scan.
CREATE TABLE IF NOT EXISTS chat_messages (
    id                    TEXT PRIMARY KEY,           -- ULID
    session_id            TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    seq                   INTEGER NOT NULL,
    role                  TEXT NOT NULL,              -- 'user' | 'assistant'
    content_blocks_json   TEXT NOT NULL,              -- serde_json::Value array
    ts                    TEXT NOT NULL               -- RFC3339 UTC
);

CREATE INDEX IF NOT EXISTS idx_chat_messages_session
    ON chat_messages(session_id, seq);
