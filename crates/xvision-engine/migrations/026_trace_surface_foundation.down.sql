-- 026_trace_surface_foundation.down.sql — rollback for migration 026.
--
-- SQLite does not support DROP COLUMN natively in older versions. The
-- `evidence_cycle_ids_json` and `produced_by_check` columns added to
-- `eval_findings` cannot be removed via a simple ALTER TABLE in SQLite
-- < 3.35.0. The rollback here documents the intent; the application layer
-- handles the version gate.
--
-- The `determinism_receipts` table CAN be dropped cleanly.

DROP TABLE IF EXISTS determinism_receipts;

-- Note: SQLite ALTER TABLE DROP COLUMN requires >= 3.35.0. The down
-- migration documents the intended rollback; callers on older SQLite
-- versions must rebuild the table manually or skip the column drop.
-- Both columns default safely (empty array / 'legacy') and do not break
-- existing code on older schema versions.
--
-- ALTER TABLE eval_findings DROP COLUMN evidence_cycle_ids_json;
-- ALTER TABLE eval_findings DROP COLUMN produced_by_check;
