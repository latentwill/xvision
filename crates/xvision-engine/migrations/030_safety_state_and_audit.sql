-- Migration 029: safety_state (single-row pause gate) + safety_audit (event log).
--
-- safety_state: one row (id=1) holds the global pause/resume state.
--   paused = 1 when the system is paused (broker submits refused).
--   paused_at, paused_by, reason carry the last toggle metadata.
--
-- safety_audit: append-only event log. One row per:
--   pause toggle, broker submit, wallet write, marketplace action, contract write.
--   Fields: timestamp (RFC3339), user, source, action_kind, params_json,
--           result (allowed|denied_*|errored), pause_state_at_time.
--
-- A follow-on janitor pass can add TTL-based pruning against this table;
-- the observability janitor at crates/xvision-engine/src/eval/ is the model.

CREATE TABLE IF NOT EXISTS safety_state (
    id       INTEGER PRIMARY KEY CHECK (id = 1),  -- enforces single-row
    paused   INTEGER NOT NULL DEFAULT 0,           -- SQLite bool: 0=false, 1=true
    paused_at TEXT,                                -- RFC3339 timestamp of last toggle
    paused_by TEXT,                                -- user who toggled
    reason   TEXT                                  -- optional human reason
);

CREATE TABLE IF NOT EXISTS safety_audit (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp           TEXT NOT NULL,             -- RFC3339
    user                TEXT NOT NULL DEFAULT '',
    source              TEXT NOT NULL DEFAULT '',  -- "api" | "cli" | "mcp" | "system"
    action_kind         TEXT NOT NULL,             -- "pause_toggle" | "broker_submit" | ...
    params_json         TEXT NOT NULL DEFAULT '{}',-- action-specific details
    result              TEXT NOT NULL,             -- "allowed" | "denied_safety_paused" | ...
    pause_state_at_time INTEGER NOT NULL DEFAULT 0 -- snapshot of paused flag at write time
);

CREATE INDEX IF NOT EXISTS idx_safety_audit_timestamp ON safety_audit(timestamp);
CREATE INDEX IF NOT EXISTS idx_safety_audit_action_kind ON safety_audit(action_kind);
CREATE INDEX IF NOT EXISTS idx_safety_audit_result ON safety_audit(result);
