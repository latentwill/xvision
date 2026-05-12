-- Skills registry — workspace-level reusable modules that agent slots can
-- reference. See docs/superpowers/plans/2026-05-11-agents-page-v1.md
-- §Skills. v1 ships the registry CRUD only; the runtime side (where a skill
-- actually does something during an agent's execution) lands later when
-- specific skill kinds light up.

CREATE TABLE IF NOT EXISTS skills (
    skill_id        TEXT PRIMARY KEY,           -- ULID
    name            TEXT NOT NULL UNIQUE,
    description     TEXT NOT NULL DEFAULT '',
    kind            TEXT NOT NULL,              -- 'tool' | 'prompt_fragment' | 'evaluator'
    config_json     TEXT NOT NULL DEFAULT '{}', -- skill-specific JSON config (no schema yet in v1)
    archived        INTEGER NOT NULL DEFAULT 0, -- bool
    created_at      TEXT NOT NULL,              -- RFC3339 UTC
    updated_at      TEXT NOT NULL               -- RFC3339 UTC
);

CREATE INDEX IF NOT EXISTS idx_skills_archived ON skills(archived);
CREATE INDEX IF NOT EXISTS idx_skills_name ON skills(name);
CREATE INDEX IF NOT EXISTS idx_skills_kind ON skills(kind);
