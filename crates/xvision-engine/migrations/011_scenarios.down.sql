DROP TRIGGER IF EXISTS scenarios_no_update;
DROP INDEX IF EXISTS scenario_tags_by_tag;
DROP TABLE IF EXISTS scenario_tags;
DROP INDEX IF EXISTS scenarios_by_archived_at;
DROP INDEX IF EXISTS scenarios_by_parent;
DROP INDEX IF EXISTS scenarios_by_source;
DROP TABLE IF EXISTS scenarios;
