CREATE TABLE IF NOT EXISTS agent_slot_optimizations (
    optimization_id          TEXT PRIMARY KEY,
    target_agent_id          TEXT NOT NULL,
    child_agent_id           TEXT,
    slot                     TEXT NOT NULL,
    method                   TEXT NOT NULL,
    demo_source              TEXT NOT NULL,
    reproducible             INTEGER NOT NULL,
    holdout_split            TEXT NOT NULL,
    cohort_query             TEXT NOT NULL,
    train_observation_ids_json   TEXT NOT NULL,
    dev_observation_ids_json     TEXT NOT NULL,
    holdout_observation_ids_json TEXT NOT NULL,
    train_hash               TEXT NOT NULL,
    dev_hash                 TEXT NOT NULL,
    holdout_hash             TEXT NOT NULL,
    prompt_prefix_chars      INTEGER NOT NULL,
    status                   TEXT NOT NULL,
    created_at               TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agent_slot_optimizations_target
    ON agent_slot_optimizations(target_agent_id, created_at);

CREATE INDEX IF NOT EXISTS idx_agent_slot_optimizations_child
    ON agent_slot_optimizations(child_agent_id);
