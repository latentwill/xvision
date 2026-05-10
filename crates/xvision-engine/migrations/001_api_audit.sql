-- Append-only log of every engine::api::* invocation.
-- Written by api::audit::record(); never updated, never deleted.

CREATE TABLE IF NOT EXISTS api_audit (
    id              TEXT PRIMARY KEY,           -- ULID
    occurred_at     TEXT NOT NULL,              -- RFC3339 UTC
    actor           TEXT NOT NULL,              -- 'cli' | 'mcp' | 'agent_runner' | 'scheduler'
    actor_id        TEXT,                       -- caller-specific id (cli user, mcp session, run id, schedule id)
    domain          TEXT NOT NULL,              -- 'strategy' | 'eval' | 'settings' | 'risk' | ...
    operation       TEXT NOT NULL,              -- function name (e.g., 'create', 'list', 'add_provider')
    target          TEXT,                       -- subject id (strategy id, run id, etc.)
    args_json       TEXT,                       -- redacted input args
    outcome         TEXT NOT NULL,              -- 'ok' | 'error'
    error           TEXT,                       -- error message when outcome = 'error'
    duration_ms     INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_api_audit_occurred ON api_audit(occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_api_audit_domain_op ON api_audit(domain, operation);
CREATE INDEX IF NOT EXISTS idx_api_audit_target ON api_audit(target);
