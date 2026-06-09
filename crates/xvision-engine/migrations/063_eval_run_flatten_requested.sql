-- Migration 062: one-shot "flatten positions" request flag on eval_runs.
--
-- `flatten_requested` is an ADDITIVE, per-run, ONE-SHOT request honored by
-- the live executor ALONGSIDE the A1 `paused` flag: when set, the next live
-- loop cycle closes ALL open broker positions at market (the same close path
-- A2 uses on cancel) and then CLEARS the flag — the run is NOT terminated and
-- keeps iterating (it typically stays paused). It is the cockpit's
-- [Flatten positions] action (spec §2.7): flatten now, keep the run alive.
--
-- Existing rows default to not-requested, which matches every run created
-- before this migration. Mirrors 061's per-run pause flag in shape and the
-- partial-apply-safe runtime migrator (`migrate_eval_run_flatten_requested`).

ALTER TABLE eval_runs ADD COLUMN flatten_requested BOOLEAN NOT NULL DEFAULT 0;
