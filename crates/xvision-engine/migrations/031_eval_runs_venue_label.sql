-- Migration 030: add venue_label column to eval_runs.
--
-- venue_label stores the coarse venue classification for each run:
--   "paper" (default), "testnet", "live".
-- Existing rows default to "paper" which matches all current scenarios.
--
-- The field is also stored on Scenario.venue_label (in body_json); this
-- column provides a fast filter surface for the UI without parsing JSON.

ALTER TABLE eval_runs ADD COLUMN venue_label TEXT NOT NULL DEFAULT 'paper';
CREATE INDEX IF NOT EXISTS idx_eval_runs_venue_label ON eval_runs(venue_label);
