-- 026_run_bars_manifest.sql — candle integrity + data manifest columns on eval_runs.
--
-- Adds three optional columns to `eval_runs` to support:
--   (a) content-hash receipts: `bars_content_hash` is the sha256 hex digest of
--       the raw Parquet bytes loaded for a run, so re-pulled Alpaca bars that
--       differ hash-produce visible drift instead of a silent reproducibility leak.
--   (b) manifest-canonical consistency checks: `manifest_canonical` is the
--       sha256 hex digest of the JSON-canonical DataManifest (feed, adjustment,
--       timeframe, session_filter, calendar, timezone). Two runs with the same
--       bars but different feed=iex vs feed=sip are not comparable; the eval
--       engine refuses ComparisonReport::build on mismatched manifests unless
--       an explicit override is provided.
--   (c) the full manifest blob: `bars_manifest` stores the JSON-serialized
--       DataManifest so the dashboard can show field-level diffs when a mismatch
--       is detected, without re-deriving the manifest from the scenario.
--
-- Implementation choice: one consolidated JSON column (`bars_manifest`) rather
-- than discrete feed/adjustment/session_filter/calendar/timezone columns.
-- Rationale: no current consumer reads discrete columns; the consolidated JSON
-- is simpler to migrate idempotently and easier to extend. The `manifest_canonical`
-- hash remains discrete for fast equality-check queries.
--
-- All columns are nullable so pre-migration rows round-trip unchanged.
-- New runs populate these at scenario-start via `RunStore::set_bars_manifest`.
--
-- The `manifest_canonical` column is indexed for fast compare-refusal lookups:
-- the ComparisonReport::build path does a cross-join (all run pairs) so an
-- index on (manifest_canonical) makes the equality check sub-millisecond.

ALTER TABLE eval_runs ADD COLUMN bars_content_hash  TEXT;
ALTER TABLE eval_runs ADD COLUMN manifest_canonical  TEXT;
ALTER TABLE eval_runs ADD COLUMN bars_manifest       TEXT;

CREATE INDEX IF NOT EXISTS idx_eval_runs_manifest_canonical
    ON eval_runs(manifest_canonical);
