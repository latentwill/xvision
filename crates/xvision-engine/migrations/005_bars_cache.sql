-- 005_bars_cache.sql
-- Custom Scenario v1 (Task 6): cached OHLCV bar windows keyed by
-- (asset, granularity, window_start, window_end, data_source).
--
-- See docs/superpowers/plans/2026-05-11-custom-scenario-1-bars-cache-asset-unlock.md
-- Task 6 for the full design. `eval::bars::load_bars` (Task 7) reads and
-- writes this table; misses fall through to the Alpaca fetcher and back-fill.
--
-- Migration registry: v1-shipping-plan.md §"Migration reservations".
-- Plan referred to this as "0004"; the actual next-available prefix in
-- crates/xvision-engine/migrations/ is 005 (after 004_search_index.sql).

CREATE TABLE IF NOT EXISTS bars_cache (
    cache_key    TEXT PRIMARY KEY,
    asset        TEXT NOT NULL,
    granularity  TEXT NOT NULL,
    window_start TEXT NOT NULL,
    window_end   TEXT NOT NULL,
    data_source  TEXT NOT NULL,
    fetched_at   TEXT NOT NULL,
    bar_count    INTEGER NOT NULL,
    bars_blob    BLOB NOT NULL,
    compression  TEXT NOT NULL DEFAULT 'none'  -- 'none' | 'gzip'
);

CREATE INDEX IF NOT EXISTS bars_cache_by_asset_window
    ON bars_cache(asset, granularity, window_start, window_end);
