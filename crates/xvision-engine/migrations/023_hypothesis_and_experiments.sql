-- 023_hypothesis_and_experiments.sql — strategy hypothesis + experiment ledger.
--
-- Part A: strategy hypothesis (intake #7)
--   The intake spec proposed ALTER TABLE strategies ADD COLUMN hypothesis_json.
--   However, strategies in this codebase are stored as JSON files on the
--   filesystem via FilesystemStore, not in SQLite. There is no `strategies`
--   table in xvn.db. Therefore, the hypothesis is stored as an optional
--   `hypothesis` field directly in the Strategy struct / JSON file.
--   This migration contains NO DDL for Part A; the Rust struct change
--   carries it. Existing strategy JSON files deserialise cleanly because
--   the field is `#[serde(default, skip_serializing_if = "Option::is_none")]`.
--   Design decision documented here (open-spec question #1 from the intake):
--     - Simpler: one JSON blob per strategy file, zero FK joins.
--     - Consistent: matches `mechanical_params` which is also a JSON blob
--       on the struct.
--     - Reversible: dropping the field is a no-op deserialisation change.
--
-- Part B: experiment ledger (intake #8)
--   New `experiments` table — organises one research question across a set
--   of strategies + scenarios. Optionally bound to an `eval_batches` row
--   when the experiment has been run.

-- Part B: experiment ledger.
CREATE TABLE IF NOT EXISTS experiments (
    experiment_id       TEXT PRIMARY KEY,             -- exp_<ULID>
    name                TEXT NOT NULL,
    question            TEXT,                          -- 1-2 sentence research question
    strategy_ids        TEXT NOT NULL,                 -- JSON array of strategy ids
    scenario_ids        TEXT NOT NULL,                 -- JSON array of scenario ids
    batch_id            TEXT REFERENCES eval_batches(batch_id),  -- nullable; bound when run
    decision_budget     INTEGER,
    result_json         TEXT,                          -- JSON result summary, populated when batch finishes
    conclusion          TEXT,                          -- operator-written summary
    next_recommendation TEXT,                          -- operator-written recommendation
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_experiments_created ON experiments(created_at);
CREATE INDEX IF NOT EXISTS idx_experiments_batch ON experiments(batch_id);
