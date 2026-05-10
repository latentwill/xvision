-- 0002: Rename setup_id → cycle_id and setups → cycles.
--
-- Rationale: "setup" is overloaded with the `xvn setup` CLI verb (config init).
-- The id ties one InternBriefing → TraderDecision → outcome together, which is
-- naturally a "cycle" through the pipeline. See plan
-- docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md.
--
-- SQLite ALTER TABLE RENAME COLUMN (>= 3.25) propagates references inside
-- the schema (foreign keys, indexes, triggers, views) automatically as long as
-- legacy_alter_table is OFF. Some SQLite builds default it to ON; set it
-- explicitly here so the propagation behavior is deterministic.

PRAGMA legacy_alter_table = OFF;

-- Step 1: rename the parent table
ALTER TABLE setups RENAME TO cycles;

-- Step 2: rename the primary-key column on cycles
ALTER TABLE cycles RENAME COLUMN setup_id TO cycle_id;

-- Step 3: rename setup_id on each child table. Foreign-key clauses are
-- updated automatically; indexes built on the column are also updated.
ALTER TABLE briefings        RENAME COLUMN setup_id TO cycle_id;
ALTER TABLE decisions        RENAME COLUMN setup_id TO cycle_id;
ALTER TABLE risk_outcomes    RENAME COLUMN setup_id TO cycle_id;
ALTER TABLE executions       RENAME COLUMN setup_id TO cycle_id;
ALTER TABLE traces           RENAME COLUMN setup_id TO cycle_id;

-- Step 4: rename indexes that have "setup" in the name. The columns inside
-- them have already been renamed by Step 3; the index names are cosmetic.
DROP INDEX IF EXISTS idx_decisions_setup;
DROP INDEX IF EXISTS idx_executions_setup;
DROP INDEX IF EXISTS idx_traces_run_setup;

CREATE INDEX IF NOT EXISTS idx_decisions_cycle    ON decisions(cycle_id);
CREATE INDEX IF NOT EXISTS idx_executions_cycle   ON executions(cycle_id);
CREATE INDEX IF NOT EXISTS idx_traces_run_cycle   ON traces(run_id, cycle_id);
