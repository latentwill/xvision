-- 042_session_events.sql
--
-- Phase 1.2 of the chat-rail / DSPy / strategy-agents wave
-- (docs/superpowers/specs/2026-05-24-chat-rail-and-strategy-agents-evaluation.md).
--
-- Persisted unified-event log. Every chat-rail row (and, post-migration, every
-- trace-dock row) is a projection of a `UnifiedEvent`. The legacy WizardEvent
-- SSE stream stays as a deprecated compatibility shim; the unified session
-- stream replays from this table on reconnect and then tails live events from
-- the per-session broadcast bus.
--
--   * event_id     — globally-unique ULID; stable across reconnects so the
--                    per-row reducer is idempotent.
--   * session_id   — owning chat session. CASCADE-deletes with the session so
--                    a deleted conversation leaves no orphaned events.
--   * seq          — monotonic per-session sequence number. Resume replays
--                    events with seq > the client's cursor; gaps signal a drop.
--   * ts           — RFC3339 UTC timestamp of the event.
--   * source       — EventSource snake_case (chat_rail | agent_run | engine |
--                    optimizer | hook). Provenance for the dual-path migration.
--   * kind         — UnifiedPayload kind (the adjacently-tagged discriminant),
--                    duplicated out of payload_json for cheap filtering.
--   * payload_json — the full UnifiedEvent JSON (envelope + payload), the
--                    source of truth replayed verbatim over SSE.
--
-- Additive; no existing table is touched. CREATE TABLE / CREATE INDEX with
-- IF NOT EXISTS so a re-run is idempotent (matches the 003 chat_sessions
-- convention).

CREATE TABLE IF NOT EXISTS session_events (
    event_id    TEXT PRIMARY KEY,            -- ULID
    session_id  TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    seq         INTEGER NOT NULL,            -- monotonic per session
    ts          TEXT NOT NULL,               -- RFC3339 UTC
    source      TEXT NOT NULL,               -- EventSource snake_case
    kind        TEXT NOT NULL,               -- UnifiedPayload kind
    payload_json TEXT NOT NULL               -- full UnifiedEvent JSON
);

CREATE INDEX IF NOT EXISTS idx_session_events_seq ON session_events(session_id, seq);
