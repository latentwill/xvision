-- 022_scenario_regime_labels.sql — first-class regime metadata on scenarios.
--
-- Adds four optional regime-classification columns to the `scenarios` table
-- so agents and operators can filter by market regime without parsing tags.
--
-- Column semantics:
--   regime_label      — broad market character: "trend" | "chop" | "crash" |
--                       "expansion" | "recovery" | null
--   volatility_label  — per-bar vol bucket: "low" | "normal" | "high" |
--                       "extreme" | null
--   trend_direction   — net slope over the window: "up" | "down" |
--                       "sideways" | null
--   regime_derived    — false = operator-set, true = auto-derived by
--                       `xvn scenario classify`.  Determines whether a
--                       subsequent classify run may overwrite values.
--
-- Values are TEXT (not enums) so the constraint lives in the API layer;
-- out-of-set values produce ApiError::Validation at write time.
--
-- Existing rows: all four columns default to NULL / FALSE.  Operators can
-- backfill via `xvn scenario classify --all`; nothing is auto-backfilled
-- here so the migration is a pure schema change with no data risk.

ALTER TABLE scenarios ADD COLUMN regime_label     TEXT;
ALTER TABLE scenarios ADD COLUMN volatility_label TEXT;
ALTER TABLE scenarios ADD COLUMN trend_direction  TEXT;
ALTER TABLE scenarios ADD COLUMN regime_derived   BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_scenarios_regime ON scenarios(regime_label);
