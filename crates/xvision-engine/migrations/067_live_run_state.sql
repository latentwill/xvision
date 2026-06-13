-- Migration 067: per-run live-deployment capital-risk snapshot.
--
-- One upserted row per live (mode='live') run, written by run_inner_live each
-- bar. Per-run PortfolioBook-computed capital-risk + denormalized strategy name
-- + monotonic risk-veto counter, so GET /api/live/deployments is a single join.
CREATE TABLE live_run_state (
    run_id                   TEXT PRIMARY KEY REFERENCES eval_runs(id) ON DELETE CASCADE,
    strategy_id              TEXT,
    strategy_name            TEXT,
    deployed_capital_usd     REAL NOT NULL,
    equity_usd               REAL,
    unrealized_pnl_usd       REAL,
    realized_pnl_usd         REAL,
    realized_today_usd       REAL,
    daily_loss_remaining_usd REAL,
    drawdown_pct             REAL,
    peak_equity_usd          REAL,
    risk_veto_count          INTEGER NOT NULL DEFAULT 0,
    last_decision_at         TEXT,
    updated_at               TEXT NOT NULL
);
