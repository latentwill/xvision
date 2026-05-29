-- 050_mutator_attribution.sql
--
-- Mutator attribution: records which (provider, model, prompt_version) tuple
-- proposed each mutation bundle. compute_ladder joins this with lineage_nodes
-- to build the experiment-writer ladder.
--
-- delta_sharpe is nullable; populated post-gate by record_outcome when the
-- gate emits the Δ-Sharpe metric for a passing bundle.
--
-- bundle_hash references lineage_nodes logically; record_proposal is called at
-- proposal time (before lineage insert), so the FK cannot be enforced eagerly.
-- Callers must ensure the lineage_node is inserted before querying the ladder.

CREATE TABLE IF NOT EXISTS mutator_attribution (
    bundle_hash    TEXT PRIMARY KEY,
    provider       TEXT NOT NULL,
    model          TEXT NOT NULL,
    prompt_version TEXT NOT NULL,
    proposed_at    TEXT NOT NULL,
    delta_sharpe   REAL
);

CREATE INDEX IF NOT EXISTS idx_attr_provider_model
    ON mutator_attribution(provider, model);
