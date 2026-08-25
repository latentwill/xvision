-- Migration 078: re-derivation provenance for autooptimizer lineage nodes.
-- Additive nullable columns preserve existing lineage rows while recording the
-- exact mutation, seed, evaluated windows, and gate objective for new nodes.
ALTER TABLE lineage_nodes ADD COLUMN mutation_diff_json TEXT;
ALTER TABLE lineage_nodes ADD COLUMN seed INTEGER;
ALTER TABLE lineage_nodes ADD COLUMN data_window_json TEXT;
ALTER TABLE lineage_nodes ADD COLUMN objective TEXT;
