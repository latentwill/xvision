-- Migration 069: nanochat filter agent — trained_models + autoresearch_runs +
-- autoresearch_experiments. See spec 2026-06-13-nanochat-filter-agent.md §Data Model.

CREATE TABLE IF NOT EXISTS trained_models (
    model_id             TEXT PRIMARY KEY,
    display_name         TEXT NOT NULL,
    source_strategy_id   TEXT,
    source_strategy_name TEXT,
    run_tag              TEXT NOT NULL,
    checkpoint_path      TEXT NOT NULL,
    weights_format       TEXT NOT NULL DEFAULT 'safetensors',
    weights_sha256       TEXT NOT NULL,
    input_spec           TEXT NOT NULL,
    base_model           TEXT NOT NULL DEFAULT 'gpt2-nanochat',
    label_strategy       TEXT NOT NULL,
    label_config         TEXT NOT NULL,
    best_acc             REAL,
    best_loss            REAL,
    holdout_samples      INTEGER,
    promoted             INTEGER NOT NULL DEFAULT 0,
    live_approved        INTEGER NOT NULL DEFAULT 0,
    created_at           TEXT NOT NULL,
    autoresearch_run_id  TEXT
);

CREATE TABLE IF NOT EXISTS autoresearch_runs (
    run_id             TEXT PRIMARY KEY,
    run_tag            TEXT NOT NULL,
    source_strategy_id TEXT,
    label_strategy     TEXT NOT NULL,
    label_config       TEXT NOT NULL,
    git_branch         TEXT NOT NULL,
    worktree_path      TEXT NOT NULL,
    status             TEXT NOT NULL,
    started_at         TEXT NOT NULL,
    stopped_at         TEXT,
    experiments        INTEGER NOT NULL DEFAULT 0,
    best_acc           REAL,
    best_model_id      TEXT
);

-- Concurrency guard: at most one autoresearch run with status='running' at a time.
CREATE UNIQUE INDEX IF NOT EXISTS idx_autoresearch_single_running
    ON autoresearch_runs (status) WHERE status = 'running';

CREATE TABLE IF NOT EXISTS autoresearch_experiments (
    experiment_id    TEXT PRIMARY KEY,
    run_id           TEXT NOT NULL,
    git_commit       TEXT NOT NULL,
    val_acc          REAL,
    val_loss         REAL,
    peak_vram_mb     REAL,
    training_seconds REAL,
    status           TEXT NOT NULL,
    description      TEXT NOT NULL,
    created_at       TEXT NOT NULL
);
