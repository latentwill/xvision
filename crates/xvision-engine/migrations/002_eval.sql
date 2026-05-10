-- Eval engine module — Phase 3.A foundation tables.
-- See docs/superpowers/plans/2026-05-08-eval-engine-plan.md Task 1.
-- Migration registry: v1-shipping-plan.md §"Migration reservations".

-- Per-run metadata. Status transitions: queued → running → (completed | failed | cancelled).
CREATE TABLE IF NOT EXISTS eval_runs (
    id                       TEXT PRIMARY KEY,        -- ULID
    strategy_bundle_hash     TEXT NOT NULL,           -- bundle artifact hash (forward-link to xvision-engine/bundle)
    scenario_id              TEXT NOT NULL,           -- references eval_scenarios.id (logical, no FK to keep it loose)
    params_override_json     TEXT,                    -- optional: per-run overrides to bundle params
    mode                     TEXT NOT NULL,           -- 'backtest' | 'paper'
    status                   TEXT NOT NULL,           -- 'queued' | 'running' | 'completed' | 'failed' | 'cancelled'
    started_at               TEXT NOT NULL,           -- RFC3339 UTC
    completed_at             TEXT,                    -- RFC3339 UTC; null while in flight
    metrics_json             TEXT,                    -- MetricsSummary serialized; null until finalize()
    error                    TEXT,                    -- error message when status = 'failed'
    estimated_total_tokens   INTEGER,                 -- pre-run estimate (Plan #1 token estimator)
    actual_input_tokens      INTEGER,                 -- post-run actuals
    actual_output_tokens     INTEGER
);

CREATE INDEX IF NOT EXISTS idx_eval_runs_strategy
    ON eval_runs(strategy_bundle_hash);
CREATE INDEX IF NOT EXISTS idx_eval_runs_scenario
    ON eval_runs(scenario_id);
CREATE INDEX IF NOT EXISTS idx_eval_runs_status
    ON eval_runs(status);

-- One row per trader decision in a run. PK is composite so decision_index
-- is monotonic within a run (chronological replay order).
CREATE TABLE IF NOT EXISTS eval_decisions (
    run_id            TEXT NOT NULL,
    decision_index    INTEGER NOT NULL,
    timestamp         TEXT NOT NULL,                  -- RFC3339 UTC of the decision
    asset             TEXT NOT NULL,                  -- e.g. 'BTC/USD'
    action            TEXT NOT NULL,                  -- 'long_open' | 'short_open' | 'flat' | 'hold'
    conviction        REAL,                           -- trader's reported conviction [0, 1]
    justification     TEXT,                           -- trader's audit-trail summary
    order_size        REAL,                           -- base-asset units; 0 for non-actionable
    fill_price        REAL,                           -- avg fill price; null for non-actionable / paper-only
    fill_size         REAL,                           -- actual filled qty; null for non-actionable
    fee               REAL,                           -- venue fee in USD; null when not modeled
    pnl_realized      REAL,                           -- realized P&L since prior decision; null on entry
    PRIMARY KEY (run_id, decision_index)
);

CREATE INDEX IF NOT EXISTS idx_decisions_run
    ON eval_decisions(run_id);

-- Per-timestamp equity samples for the equity curve render. For backtest
-- mode this is per-decision (so the curve has the same resolution as
-- decisions); for paper mode it can be sparser (e.g. per N seconds).
CREATE TABLE IF NOT EXISTS eval_equity_samples (
    run_id        TEXT NOT NULL,
    timestamp     TEXT NOT NULL,                      -- RFC3339 UTC
    equity_usd    REAL NOT NULL,
    PRIMARY KEY (run_id, timestamp)
);

-- LLM-extracted findings about a completed run (Phase 3.C — defined here so
-- the migration is single-shot per the v1 migration registry).
CREATE TABLE IF NOT EXISTS eval_findings (
    id                TEXT PRIMARY KEY,               -- ULID
    run_id            TEXT NOT NULL,
    kind              TEXT NOT NULL,                  -- e.g. 'overconfidence' | 'leverage_spike'
    severity          TEXT NOT NULL,                  -- 'info' | 'warning' | 'critical'
    summary           TEXT NOT NULL,
    evidence_json     TEXT NOT NULL,                  -- structured pointers into eval_decisions
    extracted_at      TEXT NOT NULL,                  -- RFC3339 UTC
    schema_version    TEXT NOT NULL DEFAULT '1'
);

CREATE INDEX IF NOT EXISTS idx_findings_run
    ON eval_findings(run_id);
CREATE INDEX IF NOT EXISTS idx_findings_kind
    ON eval_findings(kind);

-- Persisted Scenario configs (canonical scenarios live in
-- data/probes/scenarios/*.json and can be installed into this table by
-- a one-shot xvn command — Phase 3.D).
CREATE TABLE IF NOT EXISTS eval_scenarios (
    id                       TEXT PRIMARY KEY,        -- 'crypto-bull-q1-2025' etc.
    display_name             TEXT NOT NULL,
    description              TEXT,
    config_json              TEXT NOT NULL,           -- full Scenario struct
    created_at               TEXT NOT NULL            -- RFC3339 UTC
);

-- Signed Ed25519 attestations for marketplace publishing (Phase 3.C —
-- defined here so the migration is single-shot). On-chain push lands
-- in Plan 5 (blockchain).
CREATE TABLE IF NOT EXISTS eval_attestations (
    id                       TEXT PRIMARY KEY,
    run_id                   TEXT NOT NULL,
    strategy_bundle_hash     TEXT NOT NULL,
    scenario_id              TEXT NOT NULL,
    signed_metrics_json      TEXT NOT NULL,
    signature_hex            TEXT NOT NULL,
    signing_pubkey_hex       TEXT NOT NULL,
    signed_at                TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES eval_runs(id)
);
