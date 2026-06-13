-- Migration 065 (CT5 live-deployment foundation, Epic s78 Wave 3): two
-- ADDITIVE columns on `eval_runs` that back the `LiveDeploymentSummary`
-- read contract (docs/superpowers/specs/2026-06-13-ct5-live-deployment-contract.md).
--
--   * `source` — who queued the run. `'human'` for the operator queue path
--     (POST /api/eval/runs), `'optimizer'` for the autooptimizer eval adapter.
--     Drives `awm`'s Cancel-gate (only human-sourced runs may be cancelled
--     from the strip). Defaults to `'human'`, which matches every run created
--     before this migration AND keeps backtests behaviorally unchanged.
--
--   * `unrealized_pnl_usd` — per-run mark-to-market unrealized PnL, written by
--     the live loop's buffered equity flush (§6.3, option A). NULLABLE on
--     purpose: HONESTY MANDATE (§8.1) — an unsourceable / pre-first-fill value
--     surfaces as NULL ("—" in the UI), NEVER a fabricated 0. Backtests leave
--     it NULL with no behavior change.
--
-- Both are additive single-column adds (SQLite ALTER TABLE ... ADD COLUMN).
-- The runtime path applies the same DDL guarded on column existence via
-- `migrate_eval_run_source_and_unrealized_pnl` in `api/mod.rs`, mirroring
-- `migrate_eval_run_paused` / `migrate_eval_run_flatten_requested`, so a
-- partial apply or a re-open converges to both columns present.

ALTER TABLE eval_runs ADD COLUMN source TEXT NOT NULL DEFAULT 'human';
ALTER TABLE eval_runs ADD COLUMN unrealized_pnl_usd REAL;
