-- Migration: 0001_dashboard_sessions
-- Track: v2b-dashboard-auth-boundary
-- Creates the session token table and the auth audit log.
--
-- dashboard_sessions: one row per live session.
-- auth_audit: append-only audit log for every mutating API call.

CREATE TABLE IF NOT EXISTS dashboard_sessions (
    -- Short hex ID derived from the first 16 chars of the token_hash.
    session_id       TEXT NOT NULL PRIMARY KEY,
    -- SHA-256 hex digest of the raw session token. The raw token is never stored.
    token_hash       TEXT NOT NULL UNIQUE,
    -- RFC 3339 timestamps.
    created_at       TEXT NOT NULL,
    expires_at       TEXT NOT NULL,
    -- Peer IP at session-creation time (for audit purposes).
    source_ip        TEXT,
    -- Optional human label ("browser", "cli", etc.). Not used for auth.
    label            TEXT
);

CREATE INDEX IF NOT EXISTS idx_dashboard_sessions_token_hash
    ON dashboard_sessions (token_hash);

CREATE INDEX IF NOT EXISTS idx_dashboard_sessions_expires_at
    ON dashboard_sessions (expires_at);

-- auth_audit: append-only log of every mutating route call.
-- One row per request that passed through require_auth_middleware.
CREATE TABLE IF NOT EXISTS auth_audit (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp           TEXT NOT NULL,
    route               TEXT NOT NULL,
    method              TEXT NOT NULL,
    -- First 16 hex chars of the session token hash (or "localhost" / "no-token").
    session_token_hash  TEXT NOT NULL,
    source_ip           TEXT NOT NULL,
    response_status     INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_auth_audit_timestamp
    ON auth_audit (timestamp);

CREATE INDEX IF NOT EXISTS idx_auth_audit_session_token_hash
    ON auth_audit (session_token_hash);
