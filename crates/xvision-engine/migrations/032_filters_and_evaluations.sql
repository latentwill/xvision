-- 032_filters_and_evaluations
-- Stage 2 of the Filter v1 plan (track-plan-touches-engine).
--
-- Adds two tables:
--
--   * `filters` — forward-declared persistence layer for filter specs.
--     Strategies in v1 carry their Filter inline in the JSON file. This
--     table is a v2 destination (Stage 4 wires CRUD against it). It's
--     created here so the schema lands alongside the runtime that
--     produces filter_id-keyed rows in eval_filter_evaluations.
--
--   * `eval_filter_evaluations` — per-bar plan-touch ledger. One row per
--     bar evaluated by a FilterGated strategy's runtime. The "plan
--     touches" naming reflects that each row is a record of when the
--     strategy's plan was exercised (Active{Trip|Hold}) vs skipped
--     (Inactive/Cooldown/CappedForDay/Suppressed/Warming).

CREATE TABLE IF NOT EXISTS filters (
    id            TEXT PRIMARY KEY,
    strategy_id   TEXT NOT NULL,
    display_name  TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'draft',
    -- Original DSL the filter was authored as (TOML or JSON form), kept
    -- for round-trip and operator inspection. Both forms parse to the
    -- same Filter struct.
    dsl_format    TEXT NOT NULL CHECK (dsl_format IN ('toml', 'json')),
    dsl_source    TEXT NOT NULL,
    -- ISO-8601 UTC timestamp set the last time the filter passed the
    -- v1 validator. NULL while the filter is still in draft and has
    -- not been validated.
    validated_at  TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS filters_strategy_id ON filters(strategy_id);

CREATE TABLE IF NOT EXISTS eval_filter_evaluations (
    run_id              TEXT NOT NULL,
    bar_index           INTEGER NOT NULL,
    ts                  TEXT NOT NULL,
    -- The filter that was evaluated. May be NULL for inline filters
    -- that don't have a row in `filters` yet (v1 strategies embed
    -- their filter in the JSON. Stage 4 promotes them).
    filter_id           TEXT,
    -- Inline display name copy so a row is interpretable without
    -- joining filters even when filter_id is NULL.
    filter_display_name TEXT NOT NULL,
    -- Stable string tag from ActivationDecision::tag().
    -- One of: 'warming', 'inactive', 'trip', 'hold', 'cooldown',
    -- 'capped_for_day', 'suppressed_in_position'.
    decision_tag        TEXT NOT NULL,
    -- Full ActivationDecision serialised as JSON for downstream tooling
    -- (`{"kind": "active", "transition": "trip"}` etc.). Lossless.
    decision_json       TEXT NOT NULL,
    -- Array of booleans, index aligns with the filter's flat condition
    -- list. JSON-encoded so a single column carries the whole vector.
    -- Empty array during warmup.
    conditions_passed   TEXT NOT NULL DEFAULT '[]',
    -- True when the tree itself evaluated to true on this bar
    -- (regardless of cooldown / cap / suppression).
    tree_true           INTEGER NOT NULL DEFAULT 0,
    -- Diagnostic counters carried forward from FilterState. Useful for
    -- audit / replay without needing to re-run the eval.
    in_warmup           INTEGER NOT NULL DEFAULT 0,
    in_cooldown         INTEGER NOT NULL DEFAULT 0,
    wakeups_today       INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (run_id, bar_index)
);

CREATE INDEX IF NOT EXISTS eval_filter_evaluations_run ON eval_filter_evaluations(run_id);
CREATE INDEX IF NOT EXISTS eval_filter_evaluations_trip ON eval_filter_evaluations(run_id, decision_tag);
