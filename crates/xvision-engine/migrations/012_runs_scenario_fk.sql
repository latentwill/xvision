-- 007_runs_scenario_fk.sql
-- Enforce that eval_runs.scenario_id references scenarios.id, since SQLite
-- doesn't allow ALTER TABLE ADD CONSTRAINT after the fact. Use triggers
-- on INSERT and UPDATE for the same effect.
--
-- See docs/superpowers/plans/2026-05-11-custom-scenario-2-scenario-table-cli.md
-- Task 7. Migration 002 created eval_runs with scenario_id as a loose
-- TEXT reference; now that the scenarios table (migration 006) exists and
-- is seeded with the 4 canonical IDs (run_seed_if_needed on first
-- ApiContext::open), we tighten the link.

CREATE INDEX IF NOT EXISTS runs_by_scenario ON eval_runs(scenario_id);

DROP TRIGGER IF EXISTS runs_scenario_id_fk_insert;
CREATE TRIGGER runs_scenario_id_fk_insert
    BEFORE INSERT ON eval_runs
    WHEN NEW.scenario_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'foreign-key violation: eval_runs.scenario_id does not exist in scenarios')
    WHERE NOT EXISTS (SELECT 1 FROM scenarios WHERE id = NEW.scenario_id);
END;

DROP TRIGGER IF EXISTS runs_scenario_id_fk_update;
CREATE TRIGGER runs_scenario_id_fk_update
    BEFORE UPDATE OF scenario_id ON eval_runs
    WHEN NEW.scenario_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'foreign-key violation: eval_runs.scenario_id does not exist in scenarios')
    WHERE NOT EXISTS (SELECT 1 FROM scenarios WHERE id = NEW.scenario_id);
END;
