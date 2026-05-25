-- 046_holdout.down.sql
--
-- Reverse of 046_holdout.sql. Drops the holdout-discipline table and its index.
-- The optimization-store tables (migration 045) are left intact.

DROP INDEX IF EXISTS idx_holdout_results_run;
DROP TABLE IF EXISTS optimization_holdout_results;
