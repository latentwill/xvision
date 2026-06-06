-- 055_autooptimizer_regime_results.sql
--
-- Phase 2 regime-matrix: per-regime evaluation results for an optimizer
-- candidate. Stores one row per (bundle_hash, regime_label) pair so a
-- completed cycle's detail can display the full regime breakdown.
--
-- bundle_hash references lineage_nodes logically; the FK is declared but
-- NOT enforced at write time (lineage_nodes must exist before querying).
-- INSERT OR REPLACE is safe for idempotent re-runs.

CREATE TABLE IF NOT EXISTS autooptimizer_regime_results (
    bundle_hash            TEXT NOT NULL,
    regime_label           TEXT NOT NULL,
    side                   TEXT NOT NULL,
    metrics_day_json       TEXT NOT NULL,
    metrics_untouched_json TEXT NOT NULL,
    delta_sharpe           REAL NOT NULL,
    verdict                TEXT NOT NULL,
    created_at             TEXT NOT NULL,
    PRIMARY KEY (bundle_hash, regime_label),
    FOREIGN KEY (bundle_hash) REFERENCES lineage_nodes(bundle_hash)
);
CREATE INDEX IF NOT EXISTS idx_regime_results_label ON autooptimizer_regime_results(regime_label);
