-- Agent Run Observability — canonical schema (Phase A leaf 1).
--
-- See `docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md`
-- for the data model rationale and the IPC emission boundary that this
-- schema is designed to feed once Phase B lands.
--
-- The schema follows the rule: SQLite is the canonical local execution
-- ledger; OpenTelemetry export is a derived optional sink. Spans are a
-- shared skeleton; specialized detail tables (`model_calls`, `tool_calls`,
-- …) hold domain data so the recorder can answer first-class queries like
-- "total cost by provider for run X" without parsing JSON blobs.

CREATE TABLE IF NOT EXISTS agent_runs (
    id                   TEXT PRIMARY KEY,
    objective            TEXT NOT NULL,
    strategy_id          TEXT,
    eval_run_id          TEXT,
    source_cli_job_id    TEXT,
    -- queued|running|completed|failed|cancelled|interrupted|agent_failure
    status               TEXT NOT NULL,
    started_at           TEXT NOT NULL,
    finished_at          TEXT,
    -- hash_only|redacted|full_debug
    retention_mode       TEXT NOT NULL,
    -- Sidecar fingerprint from the IPC handshake. Populated by
    -- xvision-agent-client when Phase B emission lands. Null on rows
    -- written by the in-memory NoopRecorder / pre-sidecar smoke tests.
    sidecar_version      TEXT,
    cline_sdk_version    TEXT,
    protocol_version     TEXT,
    -- Run-time snapshots so exports survive Cline migration step 9's MCP
    -- config decision (per-strategy DB vs xvision-agentd config).
    skills_json          TEXT,
    mcp_servers_json     TEXT,
    -- Optional OTel trace id (when the `otel` cargo feature is on).
    otel_trace_id        TEXT,
    final_artifact_id    TEXT,
    error                TEXT,
    FOREIGN KEY (eval_run_id)       REFERENCES eval_runs(id),
    FOREIGN KEY (source_cli_job_id) REFERENCES cli_jobs(job_id)
);

CREATE INDEX IF NOT EXISTS agent_runs_started_idx ON agent_runs(started_at);
CREATE INDEX IF NOT EXISTS agent_runs_eval_idx    ON agent_runs(eval_run_id);

-- Shared span skeleton. Hierarchy + minimum-viable attribution. Specialized
-- detail tables FK their `span_id` to this table.
CREATE TABLE IF NOT EXISTS spans (
    id                TEXT PRIMARY KEY,
    run_id            TEXT NOT NULL,
    parent_span_id    TEXT,
    otel_trace_id     TEXT,
    otel_span_id      TEXT,
    -- agent.run|agent.plan|model.call|tool.call|approval.request|
    -- approval.response|sandbox.exec|supervisor.review|financial.eval|
    -- artifact.write|ipc.notification|skill.invoke
    kind              TEXT NOT NULL,
    name              TEXT NOT NULL,
    -- ok|error|cancelled|interrupted (interrupted = sidecar crash mid-span)
    status            TEXT NOT NULL,
    started_at        TEXT NOT NULL,
    ended_at          TEXT,
    duration_ms       INTEGER,
    -- Small attribute bag (NOT the full payload). Recorder API refuses
    -- payload-string attributes by construction.
    attributes_json   TEXT,
    error_json        TEXT,
    FOREIGN KEY (run_id)         REFERENCES agent_runs(id),
    FOREIGN KEY (parent_span_id) REFERENCES spans(id)
);

CREATE INDEX IF NOT EXISTS spans_run_id_idx ON spans(run_id);
CREATE INDEX IF NOT EXISTS spans_parent_idx ON spans(parent_span_id);
CREATE INDEX IF NOT EXISTS spans_kind_idx   ON spans(kind);

-- Canonical Rust-owned checkpoints. Replay reconstructs runs from these
-- rows, NOT from Cline's internal snapshots (which are diagnostic only).
CREATE TABLE IF NOT EXISTS checkpoints (
    id                  TEXT PRIMARY KEY,
    run_id              TEXT NOT NULL,
    span_id             TEXT NOT NULL,
    sequence            INTEGER NOT NULL,
    -- model_step|tool_step
    kind                TEXT NOT NULL,
    input_hash          TEXT NOT NULL,
    output_hash         TEXT,
    -- Refs into the content-addressed blob store. Populated only when
    -- retention_mode != hash_only.
    input_payload_ref   TEXT,
    output_payload_ref  TEXT,
    created_at          TEXT NOT NULL,
    FOREIGN KEY (run_id)  REFERENCES agent_runs(id),
    FOREIGN KEY (span_id) REFERENCES spans(id)
);

CREATE UNIQUE INDEX IF NOT EXISTS checkpoints_run_seq_idx
    ON checkpoints(run_id, sequence);

CREATE TABLE IF NOT EXISTS model_calls (
    span_id              TEXT PRIMARY KEY,
    provider             TEXT NOT NULL,
    model                TEXT NOT NULL,
    input_token_count    INTEGER,
    output_token_count   INTEGER,
    cost_usd             REAL,
    prompt_hash          TEXT NOT NULL,
    response_hash        TEXT,
    prompt_payload_ref   TEXT,
    response_payload_ref TEXT,
    -- JSON array of tool_call ids the model requested in this turn.
    tool_calls_requested TEXT,
    -- tool_choice|response_format|schema_injection|structured_output|
    -- streaming_tool_calls
    capability_path      TEXT,
    FOREIGN KEY (span_id) REFERENCES spans(id)
);

CREATE TABLE IF NOT EXISTS tool_calls (
    span_id              TEXT PRIMARY KEY,
    tool_name            TEXT NOT NULL,
    -- native|mcp:<server>|cline_builtin
    origin               TEXT NOT NULL DEFAULT 'native',
    tool_version         TEXT,
    tool_hash            TEXT,
    input_hash           TEXT NOT NULL,
    output_hash          TEXT,
    input_payload_ref    TEXT,
    output_payload_ref   TEXT,
    -- pure|read_only|external_read|external_write
    side_effect_level    TEXT NOT NULL,
    -- safe_read|expensive_compute|file_write|network_call|
    -- strategy_mutation|real_trade_blocked
    risk_level           TEXT NOT NULL,
    requires_approval    INTEGER NOT NULL DEFAULT 0,
    approval_id          TEXT,
    exit_code            INTEGER,
    -- 1 if this tool call ended the run (today: `submit_decision`).
    is_run_terminator    INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (span_id) REFERENCES spans(id)
);

CREATE TABLE IF NOT EXISTS approvals (
    id            TEXT PRIMARY KEY,
    span_id       TEXT NOT NULL,
    tool_call_id  TEXT NOT NULL,
    reason        TEXT NOT NULL,
    risk_level    TEXT NOT NULL,
    requested_at  TEXT NOT NULL,
    decided_at    TEXT,
    -- pending|granted|denied|expired
    decision      TEXT,
    decided_by    TEXT,
    FOREIGN KEY (span_id)      REFERENCES spans(id),
    FOREIGN KEY (tool_call_id) REFERENCES tool_calls(span_id)
);

CREATE TABLE IF NOT EXISTS sandbox_results (
    span_id      TEXT PRIMARY KEY,
    command      TEXT NOT NULL,
    cwd          TEXT,
    stdout_ref   TEXT,
    stderr_ref   TEXT,
    exit_code    INTEGER NOT NULL,
    duration_ms  INTEGER,
    FOREIGN KEY (span_id) REFERENCES spans(id)
);

CREATE TABLE IF NOT EXISTS supervisor_notes (
    id          TEXT PRIMARY KEY,
    run_id      TEXT NOT NULL,
    -- planner|reviewer|guard|system
    role        TEXT NOT NULL,
    content     TEXT NOT NULL,
    -- info|warn|error
    severity    TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id)
);

CREATE INDEX IF NOT EXISTS supervisor_notes_run_idx ON supervisor_notes(run_id);

CREATE TABLE IF NOT EXISTS artifacts (
    id                    TEXT PRIMARY KEY,
    run_id                TEXT NOT NULL,
    -- final|intermediate
    kind                  TEXT NOT NULL,
    title                 TEXT,
    summary               TEXT,
    hypothesis            TEXT,
    recommendation        TEXT,
    -- JSON list of { label, value, source_span_id }
    evidence_json         TEXT,
    next_experiments_json TEXT,
    created_at            TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES agent_runs(id)
);

CREATE TABLE IF NOT EXISTS events (
    id           TEXT PRIMARY KEY,
    run_id       TEXT NOT NULL,
    span_id      TEXT,
    kind         TEXT NOT NULL,
    payload_json TEXT,
    created_at   TEXT NOT NULL,
    FOREIGN KEY (run_id)  REFERENCES agent_runs(id),
    FOREIGN KEY (span_id) REFERENCES spans(id)
);

CREATE INDEX IF NOT EXISTS events_run_idx ON events(run_id);
