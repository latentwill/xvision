-- v1 schema. Compile-time checks disabled for now — runtime sqlx::query.
-- Migrations are applied in lexical order by xianvec_core::store::Store::migrate.

CREATE TABLE IF NOT EXISTS setups (
    setup_id    TEXT PRIMARY KEY,
    asset       TEXT NOT NULL,
    horizon_h   INTEGER NOT NULL,
    market_state_json TEXT NOT NULL,
    created_at  TEXT NOT NULL
);

-- Tier 1 fix #1: briefings are keyed by setup_id only — every arm reads the
-- SAME briefing. Cache key in xianvec-intern includes (provider, model) so
-- changing the Intern backend invalidates rows.
CREATE TABLE IF NOT EXISTS briefings (
    setup_id      TEXT PRIMARY KEY,
    provider      TEXT NOT NULL,
    model         TEXT NOT NULL,
    briefing_json TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    FOREIGN KEY (setup_id) REFERENCES setups(setup_id)
);

-- Decisions are keyed by (setup_id, arm_name) so multiple strategy arms
-- (trader_arm + baselines) persist independently against the same setup.
CREATE TABLE IF NOT EXISTS decisions (
    setup_id            TEXT NOT NULL,
    arm_name            TEXT NOT NULL,
    decision_json       TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    PRIMARY KEY (setup_id, arm_name),
    FOREIGN KEY (setup_id) REFERENCES setups(setup_id)
);

CREATE TABLE IF NOT EXISTS risk_outcomes (
    setup_id            TEXT NOT NULL,
    arm_name            TEXT NOT NULL,
    risk_decision_json  TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    PRIMARY KEY (setup_id, arm_name),
    FOREIGN KEY (setup_id) REFERENCES setups(setup_id)
);

CREATE TABLE IF NOT EXISTS executions (
    execution_id        TEXT PRIMARY KEY,
    setup_id            TEXT NOT NULL,
    arm_name            TEXT NOT NULL,
    venue               TEXT NOT NULL,    -- alpaca | orderly | backtest
    receipt_json        TEXT NOT NULL,
    realized_pnl        REAL,
    created_at          TEXT NOT NULL,
    closed_at           TEXT,
    FOREIGN KEY (setup_id) REFERENCES setups(setup_id)
);

-- Flight recorder. One row per `tracing` span we care to persist for offline
-- replay. Mirrors a subset of OTel attribute keys so the v2 telemetry crate can
-- read this back into spans without remapping.
CREATE TABLE IF NOT EXISTS traces (
    trace_id    TEXT NOT NULL,
    span_id     TEXT NOT NULL,
    parent_id   TEXT,
    run_id      TEXT NOT NULL,
    setup_id    TEXT,
    stage       TEXT NOT NULL,            -- intern | trader | risk | execution
    name        TEXT NOT NULL,
    attrs_json  TEXT NOT NULL,
    started_at  TEXT NOT NULL,
    ended_at    TEXT NOT NULL,
    PRIMARY KEY (trace_id, span_id)
);

CREATE INDEX IF NOT EXISTS idx_decisions_setup    ON decisions(setup_id);
CREATE INDEX IF NOT EXISTS idx_executions_setup   ON executions(setup_id);
CREATE INDEX IF NOT EXISTS idx_traces_run_setup   ON traces(run_id, setup_id);
CREATE INDEX IF NOT EXISTS idx_traces_stage       ON traces(stage);
