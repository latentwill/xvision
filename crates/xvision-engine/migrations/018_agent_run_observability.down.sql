-- Rollback for migration 018. Order: children before parents.

DROP INDEX IF EXISTS events_run_idx;
DROP TABLE IF EXISTS events;

DROP TABLE IF EXISTS artifacts;

DROP INDEX IF EXISTS supervisor_notes_run_idx;
DROP TABLE IF EXISTS supervisor_notes;

DROP TABLE IF EXISTS sandbox_results;

DROP TABLE IF EXISTS approvals;

DROP TABLE IF EXISTS tool_calls;

DROP TABLE IF EXISTS model_calls;

DROP INDEX IF EXISTS checkpoints_run_seq_idx;
DROP TABLE IF EXISTS checkpoints;

DROP INDEX IF EXISTS spans_kind_idx;
DROP INDEX IF EXISTS spans_parent_idx;
DROP INDEX IF EXISTS spans_run_id_idx;
DROP TABLE IF EXISTS spans;

DROP INDEX IF EXISTS agent_runs_eval_idx;
DROP INDEX IF EXISTS agent_runs_started_idx;
DROP TABLE IF EXISTS agent_runs;
