-- 0003_cycles_trace_indices.sql — V2E autoresearcher query columns + indices.
--
-- The autoresearcher's primary query shape is "all decisions made by model X
-- under regime Y". This migration adds the LLM-provenance columns to the
-- `cycles` table and indices to make those queries fast.
--
-- Column semantics:
--   model_id             — LLM model identifier used for this cycle
--                          (e.g. 'claude-opus-4-7'). Nullable: pre-V2E cycles
--                          do not have this populated.
--   prompt_template_hash — SHA-256 hex of the prompt template. Nullable for
--                          the same reason.
--   regime_tag           — Regime label at the time of this cycle. Nullable;
--                          populated when the scenario carries regime metadata
--                          (from `eval-pinned-fixtures-and-manifest` /
--                          024_scenario_regime_labels on the engine side).
--
-- Existing rows: all three columns default to NULL. No backfill pass is
-- required; cycle writers updated to V2E fill the columns going forward.
--
-- The indices are the essential autoresearcher query support:
--   idx_cycles_model_id  — filter by model (primary query axis)
--   idx_cycles_prompt    — filter by prompt template (secondary axis)
--   idx_cycles_regime    — filter by regime tag (tertiary axis)
--   idx_cycles_model_regime — composite for the common "model X + regime Y"
--                             pattern; the EXPLAIN QUERY PLAN acceptance test
--                             verifies this index is used.
--
-- See: team/contracts/eval-trace-surface-foundation.md (acceptance item 6)
--      docs/superpowers/research/2026-05-19-eval-data-and-execution-accuracy.md §5

ALTER TABLE cycles ADD COLUMN model_id             TEXT;
ALTER TABLE cycles ADD COLUMN prompt_template_hash TEXT;
ALTER TABLE cycles ADD COLUMN regime_tag           TEXT;

CREATE INDEX IF NOT EXISTS idx_cycles_model_id
    ON cycles(model_id);

CREATE INDEX IF NOT EXISTS idx_cycles_prompt
    ON cycles(prompt_template_hash);

CREATE INDEX IF NOT EXISTS idx_cycles_regime
    ON cycles(regime_tag);

-- Composite index for the primary autoresearcher query pattern:
-- "all cycles where model_id = ? AND regime_tag = ?".
-- SQLite can use this for either prefix alone or the full composite.
CREATE INDEX IF NOT EXISTS idx_cycles_model_regime
    ON cycles(model_id, regime_tag);
