-- 046_holdout.sql
--
-- Phase 4.4 (metrics & holdout discipline) of the chat-rail / DSPy /
-- strategy-agents wave
-- (docs/superpowers/specs/2026-05-24-chat-rail-and-strategy-agents-evaluation.md).
--
-- The HOLDOUT DISCIPLINE table: the persisted out-of-sample (holdout) result for
-- an optimization snapshot, plus the overfit-detection bookkeeping that gates
-- both `accept` (promote a candidate into a child agent) and marketplace mint.
--
-- WHY a separate table rather than overloading `optimization_candidates.split`:
-- the candidate `split` column (train|val|test) describes WHICH split each
-- search-time candidate was *scored* on during the optimizer's inner loop. The
-- discipline we enforce here is a DIFFERENT axis — for the snapshot the operator
-- wants to ACCEPT, we require a paired (train metric, holdout metric) measured on
-- a held-out corpus the optimizer never saw, plus an overfit verdict. Keeping it
-- in its own table avoids mangling the search-time semantics and lets accept /
-- mint join one row per snapshot.
--
-- HARD INVARIANT (inherited from migration 045): xvision-engine must NOT depend
-- on xvision-dspy. These are plain SQLite rows; the metric VALUES are produced by
-- the eval harness (CLI side) and persisted here as scalars. Nothing here imports
-- the optimizer or the eval crates.
--
-- The gate the engine enforces over this table (see `mint::holdout`):
--   * A snapshot CANNOT be accepted without a holdout_result row UNLESS a
--     non-empty `override_reason` is recorded on the accept call.
--   * If train_metric_value >> holdout_metric_value beyond the configured
--     relative threshold, `overfit_warning` is set; an unwaived overfit warning
--     BLOCKS marketplace mint. A recorded `overfit_waiver_reason` lifts the block.
--
-- Additive; no existing table is touched. Every statement uses IF NOT EXISTS so a
-- re-run is idempotent (matches the 042/043/044/045 convention).
--
-- Wired at runtime via `migrate_holdout` in `ApiContext::open` (the
-- hand-maintained registry; this repo does NOT apply migrations through
-- `sqlx::migrate!`). Without that wiring the table never exists at runtime.

-- One paired train/holdout result for an accepted-or-acceptable snapshot. Keyed
-- by snapshot id (one holdout verdict per snapshot). The run id is denormalized
-- so the mint gate can join from the run without a second lookup.
CREATE TABLE IF NOT EXISTS optimization_holdout_results (
    snapshot_id          TEXT PRIMARY KEY REFERENCES optimization_snapshots(id) ON DELETE CASCADE,
    run_id               TEXT NOT NULL REFERENCES optimization_runs(id) ON DELETE CASCADE,
    metric               TEXT NOT NULL,             -- metric name measured (mirrors run.metric)
    train_metric_value   REAL NOT NULL,             -- value on the training corpus
    holdout_metric_value REAL NOT NULL,             -- value on the held-out corpus the optimizer never saw
    overfit_warning      INTEGER NOT NULL DEFAULT 0,-- 1 ⇒ train >> holdout beyond threshold
    overfit_ratio        REAL,                      -- (train - holdout) / |train|, the detection statistic
    overfit_waiver_reason TEXT,                     -- non-NULL ⇒ overfit block waived (recorded rationale)
    created_at           TEXT NOT NULL              -- RFC3339 UTC
);

CREATE INDEX IF NOT EXISTS idx_holdout_results_run
    ON optimization_holdout_results(run_id);
