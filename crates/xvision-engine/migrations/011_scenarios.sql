-- 006_scenarios.sql
-- Custom Scenario v2 (Task 2): persistent storage for user-authored
-- scenarios and their tags. Built/derived/imported scenarios share one
-- table; the `source` column discriminates and `parent_scenario_id`
-- links derived rows back to their origin.
--
-- See docs/superpowers/plans/2026-05-11-custom-scenario-2-scenario-table-cli.md
-- Task 2. Rows are immutable post-insert except for `archived_at` — the
-- trigger below enforces that invariant at the DB layer.

CREATE TABLE IF NOT EXISTS scenarios (
    id                  TEXT PRIMARY KEY,
    parent_scenario_id  TEXT,
    source              TEXT NOT NULL,
    display_name        TEXT NOT NULL,
    description         TEXT NOT NULL DEFAULT '',
    body_json           TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    created_by          TEXT NOT NULL,
    archived_at         TEXT,
    FOREIGN KEY (parent_scenario_id) REFERENCES scenarios(id)
);
CREATE INDEX IF NOT EXISTS scenarios_by_source       ON scenarios(source);
CREATE INDEX IF NOT EXISTS scenarios_by_parent       ON scenarios(parent_scenario_id);
CREATE INDEX IF NOT EXISTS scenarios_by_archived_at  ON scenarios(archived_at);

CREATE TABLE IF NOT EXISTS scenario_tags (
    scenario_id TEXT NOT NULL,
    tag         TEXT NOT NULL,
    PRIMARY KEY (scenario_id, tag),
    FOREIGN KEY (scenario_id) REFERENCES scenarios(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS scenario_tags_by_tag ON scenario_tags(tag);

-- Reject non-archived_at UPDATEs: rows are immutable post-insert.
DROP TRIGGER IF EXISTS scenarios_no_update;
CREATE TRIGGER scenarios_no_update
    BEFORE UPDATE OF id, parent_scenario_id, source, display_name, description, body_json, created_at, created_by
    ON scenarios
BEGIN
    SELECT RAISE(ABORT, 'scenarios rows are immutable (only archived_at may change)');
END;
