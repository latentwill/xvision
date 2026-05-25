-- Down: drop the optimization store tables + their indexes (Phase 3.5).
-- Children first so the FK references unwind cleanly.
DROP INDEX IF EXISTS idx_agent_lineage_parent;
DROP TABLE IF EXISTS agent_lineage;
DROP INDEX IF EXISTS idx_optimization_snapshots_run;
DROP TABLE IF EXISTS optimization_snapshots;
DROP TABLE IF EXISTS optimization_demos;
DROP INDEX IF EXISTS idx_optimization_candidates_run;
DROP TABLE IF EXISTS optimization_candidates;
DROP INDEX IF EXISTS idx_optimization_runs_agent;
DROP TABLE IF EXISTS optimization_runs;
