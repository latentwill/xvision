-- 041_chat_session_rail_state.sql
--
-- Phase 1.3 of the chat-rail / DSPy / strategy-agents wave
-- (docs/superpowers/specs/2026-05-24-chat-rail-and-strategy-agents-evaluation.md).
--
-- Extends `chat_sessions` (migration 003) with the durable rail state the
-- unified event surface needs for resume + safety:
--
--   * event_cursor    — last unified-event seq the client has consumed, so
--                        navigating away and back resumes from the right row.
--   * focus_path       — pinned focus-chain file for the session's scope
--                        (Phase 2.4). NULL until a focus file is attached.
--   * mode             — Research / Act mode (Phase 2.2). Defaults to the
--                        read-only `research` mode; server-side enforcement
--                        lives in the chat route, not here.
--   * tool_policy_json — snapshot of the three-state tool policy in force
--                        for the session (Phase 2.3). NULL = inherit the
--                        user/global default.
--   * checkpoint_head  — id of the most recent checkpoint written for this
--                        session (Phase 2.5). NULL until the first mutating
--                        tool runs.
--   * participants_json — optional participant list (multi-actor sessions).
--
-- All columns are additive with defaults, so the prior binary keeps working
-- against a migrated DB (it simply ignores the new columns). SQLite
-- ALTER TABLE ADD COLUMN is non-rewriting and safe on the existing table.

ALTER TABLE chat_sessions ADD COLUMN event_cursor      INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chat_sessions ADD COLUMN focus_path        TEXT;
ALTER TABLE chat_sessions ADD COLUMN mode              TEXT NOT NULL DEFAULT 'research';
ALTER TABLE chat_sessions ADD COLUMN tool_policy_json  TEXT;
ALTER TABLE chat_sessions ADD COLUMN checkpoint_head   TEXT;
ALTER TABLE chat_sessions ADD COLUMN participants_json TEXT;
