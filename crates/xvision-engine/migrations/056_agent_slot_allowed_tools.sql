-- Per-slot tool allowlist for tool-based agent capabilities.
--
-- Empty array preserves legacy behavior: runtime callers may fall back to the
-- strategy manifest's required_tools when a saved agent slot has no explicit
-- tool allowlist.
ALTER TABLE agent_slots
ADD COLUMN allowed_tools_json TEXT NOT NULL DEFAULT '[]';
