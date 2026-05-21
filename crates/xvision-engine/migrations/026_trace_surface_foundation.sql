-- 026_trace_surface_foundation.sql — V2E eval trace-surface foundation.
--
-- Adds:
--   1. `determinism_receipts` table — one receipt per run, proving that a
--      given (strategy, scenario, bars_content, seed, engine_version) tuple
--      was evaluated under a known schema version.
--   2. `evidence_cycle_ids_json` and `produced_by_check` columns on
--      `eval_findings` — V2E backref fields. Existing rows backfill with
--      empty evidence list and produced_by_check = 'legacy'.
--
-- Note on the `cycles` table: `cycles` lives in the xvision-core DB
-- (a separate SQLite file managed by xvision-core's migration runner).
-- Core migration 0003 handles the model_id / prompt_template_hash /
-- regime_tag columns and their indices. Engine migrations must not touch
-- the core DB.
--
-- See: team/contracts/eval-trace-surface-foundation.md
--      docs/superpowers/research/2026-05-19-eval-data-and-execution-accuracy.md §5
--      team/MANIFEST.md (migration 026 reserved for this track)
--
-- Down: 026_trace_surface_foundation.down.sql

-- 1. Determinism receipts table.
--
--    One row per completed run. `receipt_hash` = sha256(
--        strategy_hash || "\0" || scenario_id || "\0" ||
--        bars_content_hash || "\0" || seed || "\0" || engine_version
--    ).
--
--    `manifest_canonical` is reserved for `eval-candle-integrity-and-manifest`
--    to fill in once pinned-fixture hashes are available. Stored NULL by this
--    migration.
CREATE TABLE IF NOT EXISTS determinism_receipts (
    run_id              TEXT PRIMARY KEY,
    receipt_hash        TEXT NOT NULL,          -- hex sha256 over canonical inputs
    engine_version      TEXT NOT NULL,          -- semver of the xvision-engine crate
    schema_version      TEXT NOT NULL,          -- decisions/fills schema version at mint time
    manifest_canonical  TEXT,                   -- reserved: eval-candle-integrity-and-manifest
    created_at          TEXT NOT NULL           -- RFC3339 UTC
);

-- 2. Findings: V2E trace-surface backref columns.
--
--    `evidence_cycle_ids_json` — JSON array of cycle_id ULIDs that motivated
--    this finding. Stored as TEXT (JSON) so SQLite doesn't need a separate
--    junction table; consumers decode via serde.
--
--    `produced_by_check` — identifier of the check that emitted this finding
--    (e.g. 'lookahead_prober', 'broker_rule_engine', 'extractor', 'legacy').
--    Backfilled to 'legacy' for all pre-026 rows.
ALTER TABLE eval_findings ADD COLUMN evidence_cycle_ids_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE eval_findings ADD COLUMN produced_by_check       TEXT NOT NULL DEFAULT 'legacy';
