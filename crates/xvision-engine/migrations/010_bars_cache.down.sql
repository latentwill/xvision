-- 005_bars_cache.down.sql
-- Reverse of 005_bars_cache.sql. Not wired into the runtime migrator
-- (engine migrations are forward-only and idempotent via IF NOT EXISTS);
-- kept on disk for manual rollback / future tooling per the plan spec.

DROP INDEX IF EXISTS bars_cache_by_asset_window;
DROP TABLE IF EXISTS bars_cache;
