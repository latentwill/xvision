-- Agents module — workspace-level reusable agent records.
-- See docs/superpowers/plans/2026-05-11-agents-page-v1.md Task 1.
-- Migration registry: v1-shipping-plan.md §"Migration reservations".
--
-- An agent is a named bundle of (system_prompt + provider + model) per slot.
-- Slots are user-named free text; the default agent has one slot named
-- `main`. Pipeline-stage names (intern, trader, risk, executor) are valid
-- conventions, not enforced — see CLAUDE.md terminology table.

CREATE TABLE IF NOT EXISTS agents (
    agent_id        TEXT PRIMARY KEY,           -- ULID
    name            TEXT NOT NULL UNIQUE,
    description     TEXT NOT NULL DEFAULT '',
    tags_json       TEXT NOT NULL DEFAULT '[]', -- JSON array of strings
    archived        INTEGER NOT NULL DEFAULT 0, -- bool
    created_at      TEXT NOT NULL,              -- RFC3339 UTC
    updated_at      TEXT NOT NULL               -- RFC3339 UTC
);

CREATE INDEX IF NOT EXISTS idx_agents_archived ON agents(archived);
CREATE INDEX IF NOT EXISTS idx_agents_name ON agents(name);
CREATE INDEX IF NOT EXISTS idx_agents_updated ON agents(updated_at DESC);

-- One row per slot inside an agent. Slot `name` is user-defined free text
-- (e.g., 'main', 'trader', 'risk_check'). Position carries ordering only —
-- it has no semantic meaning in v1.
CREATE TABLE IF NOT EXISTS agent_slots (
    agent_id        TEXT NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,
    slot_index      INTEGER NOT NULL,
    name            TEXT NOT NULL,
    provider        TEXT NOT NULL,              -- references config.toml providers[].name
    model           TEXT NOT NULL,
    system_prompt   TEXT NOT NULL,
    max_tokens      INTEGER NOT NULL DEFAULT 4096,
    PRIMARY KEY (agent_id, slot_index)
);

CREATE INDEX IF NOT EXISTS idx_agent_slots_agent ON agent_slots(agent_id);
