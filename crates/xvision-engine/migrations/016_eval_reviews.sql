-- Eval Review Agent persistence foundation. See
-- `docs/superpowers/specs/2026-05-15-eval-review-agent.md` and the
-- execution-board track `eval-review-data-model` for context.
--
-- This migration is intentionally limited to the data-model layer:
--   * `agent_profiles` — review-agent personas (fast-trader/reasoning/
--     risk/research) used by the downstream engine/API/UI tracks.
--   * `eval_reviews` — parent artifact persisted per (run × profile)
--     review request, plus raw model output for audit.
--
-- The `eval_findings` extension (review-linked columns) is split into
-- migration 017 because SQLite `ALTER TABLE ADD COLUMN` cannot use
-- `IF NOT EXISTS` and must be column-checked at apply time.

CREATE TABLE IF NOT EXISTS agent_profiles (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    type            TEXT NOT NULL,                -- 'fast-trader' | 'reasoning' | 'risk' | 'research' | custom
    provider        TEXT NOT NULL,                -- provider name; matches settings.providers entries
    model           TEXT NOT NULL,
    temperature     REAL NOT NULL,
    max_tokens      INTEGER NOT NULL,
    system_prompt   TEXT NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,   -- 0/1; SQLite has no real bool
    created_at      TEXT NOT NULL,                -- RFC3339 UTC
    updated_at      TEXT NOT NULL                 -- RFC3339 UTC
);

CREATE INDEX IF NOT EXISTS idx_agent_profiles_type
    ON agent_profiles(type);
CREATE INDEX IF NOT EXISTS idx_agent_profiles_enabled
    ON agent_profiles(enabled);

CREATE TABLE IF NOT EXISTS eval_reviews (
    id                  TEXT PRIMARY KEY,                 -- ULID
    eval_run_id         TEXT NOT NULL,
    agent_profile_id    TEXT NOT NULL,
    status              TEXT NOT NULL,                    -- 'queued' | 'running' | 'completed' | 'failed'
    verdict             TEXT,                             -- 'promising' | 'weak' | 'failed' | 'inconclusive'
    confidence          REAL,                             -- [0.0, 1.0]
    score               INTEGER,                          -- 0..100
    summary             TEXT,                             -- executive summary the model produced
    raw_output_json     TEXT,                             -- audit copy of the model's strict-JSON reply
    error               TEXT,                             -- populated when status = 'failed'
    created_at          TEXT NOT NULL,                    -- RFC3339 UTC
    updated_at          TEXT NOT NULL,                    -- RFC3339 UTC
    FOREIGN KEY (eval_run_id)      REFERENCES eval_runs(id),
    FOREIGN KEY (agent_profile_id) REFERENCES agent_profiles(id)
);

CREATE INDEX IF NOT EXISTS idx_eval_reviews_run
    ON eval_reviews(eval_run_id);
CREATE INDEX IF NOT EXISTS idx_eval_reviews_status
    ON eval_reviews(status);
CREATE INDEX IF NOT EXISTS idx_eval_reviews_profile
    ON eval_reviews(agent_profile_id);

-- Seed the canonical review-agent profiles. `INSERT OR IGNORE` keeps the
-- migration idempotent — re-applying never overwrites operator edits.
-- The engine track owns the final JSON-contract scaffolding; what we
-- seed here is the persona prompt + sensible defaults. Provider/model
-- default to Anthropic `claude-sonnet-4-6`, mirroring the workspace
-- `sensible_default_model` fallback used elsewhere; operators are free
-- to point a profile at any configured provider/model later.

INSERT OR IGNORE INTO agent_profiles
    (id, name, type, provider, model, temperature, max_tokens, system_prompt, enabled, created_at, updated_at)
VALUES
    (
        'fast-trader-agent',
        'Fast Trader',
        'fast-trader',
        'anthropic',
        'claude-sonnet-4-6',
        0.2,
        4000,
        'You are the Fast Trader review agent. Read the eval run payload and produce a quick tactical read: did the strategy work, what is obviously broken, and is it worth more time. Be decisive, short, and practical. Call out pass/fail issues that are visible without deep causal analysis. Do not invent metrics, trades, or events that are not in the payload.',
        1,
        '2026-05-16T00:00:00Z',
        '2026-05-16T00:00:00Z'
    ),
    (
        'reasoning-agent',
        'Reasoning',
        'reasoning',
        'anthropic',
        'claude-sonnet-4-6',
        0.2,
        8000,
        'You are the Reasoning review agent. Read the eval run payload and explain WHY the strategy worked or failed. Tie every claim to specific evidence in the payload. Be explicit about uncertainty, distinguish what the payload supports from what it does not, and never invent metrics, trades, market regime data, or events that are not present. Where the payload is sparse, say so and mark the review inconclusive.',
        1,
        '2026-05-16T00:00:00Z',
        '2026-05-16T00:00:00Z'
    ),
    (
        'risk-agent',
        'Risk',
        'risk',
        'anthropic',
        'claude-sonnet-4-6',
        0.2,
        8000,
        'You are the Risk review agent. Read the eval run payload through a downside lens: drawdown shape and concentration, tail-risk indicators, leverage and exposure, position sizing, stop behavior, and failure modes. Be strict about robustness, overfitting, and one-trade-driven results. Flag risks clearly even when the headline metrics look good. Never invent positions, orders, or market metadata that are not in the payload.',
        1,
        '2026-05-16T00:00:00Z',
        '2026-05-16T00:00:00Z'
    ),
    (
        'research-agent',
        'Research',
        'research',
        'anthropic',
        'claude-sonnet-4-6',
        0.2,
        8000,
        'You are the Research review agent. Use the eval run payload to propose the next experiments: scenario expansions, mutations, hypothesis tests, and parameter sweeps that would either confirm or break the strategy. Ground every suggestion in evidence from the payload and explain what each next test would prove. Never invent results or artifacts that are not in the payload.',
        1,
        '2026-05-16T00:00:00Z',
        '2026-05-16T00:00:00Z'
    );
