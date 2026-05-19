-- Revert 021_scenario_regime_labels.sql.
--
-- SQLite ALTER TABLE DROP COLUMN requires SQLite >= 3.35 (2021-03-12).
-- sqlx 0.8 bundles libsqlite3 >= 3.39, so the bare DROP COLUMN form is safe.
-- This matches the pattern used by 019 and 020 down migrations.

DROP INDEX IF EXISTS idx_scenarios_regime;
ALTER TABLE scenarios DROP COLUMN regime_derived;
ALTER TABLE scenarios DROP COLUMN trend_direction;
ALTER TABLE scenarios DROP COLUMN volatility_label;
ALTER TABLE scenarios DROP COLUMN regime_label;
