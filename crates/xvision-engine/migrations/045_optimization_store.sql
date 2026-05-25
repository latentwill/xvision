-- 045_optimization_store.sql
--
-- Phase 3.5 of the chat-rail / DSPy / strategy-agents wave
-- (docs/superpowers/specs/2026-05-24-chat-rail-and-strategy-agents-evaluation.md).
--
-- The OPTIMIZATION STORE: durable, reproducible record of offline prompt /
-- demonstration optimization runs produced by the `xvision-dspy` optimizer.
--
-- HARD INVARIANT: xvision-engine (where these tables live) must NOT depend on
-- xvision-dspy / dspy-rs. These are plain SQLite rows. The optimizer (CLI side)
-- produces the snapshot/demos; the engine persists and reads them as opaque
-- JSON blobs + scalar provenance columns. Nothing here imports the optimizer.
--
-- Reproducibility contract: a run is reconstructable from its persisted inputs
-- alone — corpus query, RNG seed, model identity, optimizer name+version,
-- signature hash, demos, and metric name. Those columns live on
-- `optimization_runs` (the run-level reproduction recipe); the per-candidate
-- search results and the accepted snapshot blob are children of a run.
--
-- Content-addressing: demo sets are stored once in `optimization_demos` keyed by
-- a `demo_set` content hash (sha256 of the canonical demos JSON). A run / snapshot
-- references a demo set by that hash rather than inlining the (potentially large)
-- payload per candidate, so warm-started lineage that reuses a demo set does not
-- duplicate the blob.
--
-- Tables:
--   * optimization_runs       — one optimization invocation + its repro recipe.
--   * optimization_candidates — per-round/arm instruction candidates + scores.
--   * optimization_demos      — content-addressed demo-set blobs (dedup by hash).
--   * optimization_snapshots  — the serialized OptimizationSnapshot per run +
--                               accept flag. The reproduction-of-record blob.
--   * agent_lineage           — child_agent_id ← parent_agent_id edge produced by
--                               an accepted optimization run (the lineage DAG).
--
-- Additive; no existing table is touched. Every statement uses IF NOT EXISTS so
-- a re-run is idempotent (matches the 042/043/044 convention).
--
-- Wired at runtime via `migrate_optimization_store` in `ApiContext::open` (the
-- hand-maintained registry; this repo does NOT apply migrations through
-- `sqlx::migrate!`). Without that wiring the tables never exist at runtime.

-- One optimization invocation. Holds the full reproduction recipe so the run can
-- be re-derived from persisted inputs alone.
CREATE TABLE IF NOT EXISTS optimization_runs (
    id            TEXT PRIMARY KEY,          -- ULID
    agent_id      TEXT NOT NULL,             -- the agent template being optimized (pre-mint local id)
    slot_name     TEXT NOT NULL,             -- free-text slot/role label within the agent
    capability    TEXT NOT NULL,             -- trader | filter | decision_grader | intern | chat_authoring
    optimizer     TEXT NOT NULL,             -- mipro | gepa | copro
    metric        TEXT NOT NULL,             -- metric name maximized (e.g. delta_sharpe)
    corpus_query  TEXT NOT NULL,             -- saved-query id or serialized filter (opaque)
    rng_seed      INTEGER NOT NULL,          -- RNG seed for demo sampling / search order
    -- model identity (provenance) — part of the reproduction recipe.
    model_provider TEXT,                     -- e.g. dummy | openai | anthropic
    model_name     TEXT,                     -- provider's model id, e.g. gpt-4o-mini
    signature_hash TEXT,                     -- sha256 of the bound signature shape
    optimizer_version TEXT,                  -- dspy-rs version / internal tag
    status        TEXT NOT NULL DEFAULT 'pending', -- pending | running | completed | failed
    created_at    TEXT NOT NULL              -- RFC3339 UTC
);

CREATE INDEX IF NOT EXISTS idx_optimization_runs_agent
    ON optimization_runs(agent_id, slot_name, created_at);

-- Per-candidate search result. A round/arm produced one instruction; the metric
-- value and the train/val split it was scored on are recorded so the search can
-- be inspected and the winner re-identified.
CREATE TABLE IF NOT EXISTS optimization_candidates (
    id              TEXT PRIMARY KEY,        -- ULID
    run_id          TEXT NOT NULL REFERENCES optimization_runs(id) ON DELETE CASCADE,
    candidate_index INTEGER NOT NULL,        -- 0-based ordinal within the run's search
    instruction     TEXT NOT NULL,           -- the candidate instruction string
    metric_value    REAL,                    -- scored metric value (NULL if unscored)
    split           TEXT NOT NULL DEFAULT 'train', -- train | val | test
    demo_set        TEXT,                    -- FK-by-hash into optimization_demos.demo_set
    selected        INTEGER NOT NULL DEFAULT 0     -- 1 ⇒ chosen as the run's winner
);

CREATE INDEX IF NOT EXISTS idx_optimization_candidates_run
    ON optimization_candidates(run_id, candidate_index);

-- Content-addressed demo-set blob store. `demo_set` is the sha256 (hex) of the
-- canonical demos JSON; `payload_json` is the canonical demos JSON itself. A
-- single demo set referenced by multiple candidates / snapshots is stored once.
CREATE TABLE IF NOT EXISTS optimization_demos (
    demo_set     TEXT PRIMARY KEY,           -- sha256 hex of payload_json (content address)
    payload_json TEXT NOT NULL,              -- canonical JSON array of {inputs, outputs}
    created_at   TEXT NOT NULL               -- RFC3339 UTC of first insert
);

-- The serialized OptimizationSnapshot per run. This is the reproduction-of-record:
-- the full snapshot JSON (instruction + demos + signature hash + metric + corpus
-- query + seed + optimizer name/version + lineage ids). `accepted` marks the
-- snapshot the operator promoted into a child agent.
CREATE TABLE IF NOT EXISTS optimization_snapshots (
    id             TEXT PRIMARY KEY,         -- ULID (== snapshot.id / lineage id)
    run_id         TEXT NOT NULL REFERENCES optimization_runs(id) ON DELETE CASCADE,
    snapshot_json  TEXT NOT NULL,            -- serialized OptimizationSnapshot (round-trips)
    signature_hash TEXT NOT NULL,            -- sha256 of the bound signature (denormalized for query)
    demo_set       TEXT,                     -- FK-by-hash into optimization_demos.demo_set
    accepted       INTEGER NOT NULL DEFAULT 0, -- 1 ⇒ promoted to a child agent
    created_at     TEXT NOT NULL             -- RFC3339 UTC
);

CREATE INDEX IF NOT EXISTS idx_optimization_snapshots_run
    ON optimization_snapshots(run_id, created_at);

-- The lineage DAG edge. An accepted optimization run that mints a child agent
-- records the parent → child edge plus the producing run, so an agent's
-- provenance is queryable both directions.
CREATE TABLE IF NOT EXISTS agent_lineage (
    child_agent_id      TEXT PRIMARY KEY,    -- the new agent minted from the accepted snapshot
    parent_agent_id     TEXT NOT NULL,       -- the agent it was optimized from
    optimization_run_id TEXT NOT NULL REFERENCES optimization_runs(id) ON DELETE CASCADE,
    created_at          TEXT NOT NULL        -- RFC3339 UTC
);

CREATE INDEX IF NOT EXISTS idx_agent_lineage_parent
    ON agent_lineage(parent_agent_id);
