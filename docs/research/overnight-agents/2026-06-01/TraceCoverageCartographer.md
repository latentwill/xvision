# TraceCoverageCartographer

**Date:** 2026-06-01
**Source:** 100x overnight suite — docs/tmp/2026-06-01-100x-overnight-research-agent-suite.md
**Script:** scripts/trace-coverage-cartographer.sql

## Surfaces Inspected

Engine DB schema (via migrations under `crates/xvision-engine/migrations/`):
- `eval_runs` (002, 014, 022)
- `agent_runs`, `spans`, `model_calls`, `tool_calls` (018)
- `checkpoints` (018 — agent-run replay checkpoints; not to be confused with `chat_checkpoints` from 044)
- `determinism_receipts` (sourced from the eval attestation / determinism system)
- `eval_decisions`, `eval_findings`, `eval_equity_samples` (002)

Core DB schema (separate SQLite file, accessed via ATTACH):
- `risk_outcomes` (0001, post-rename: `cycle_id` column after 0002)
- `cycles` (renamed from `setups` in 0002)

## Schema Map

### Engine DB

| Table | PK | Key columns | Notes |
|---|---|---|---|
| `eval_runs` | `id` (ULID) | `agent_id`, `agents_agent_id`, `scenario_id`, `manifest_canonical`, `status`, `metrics_json`, `bars_content_hash` | `agent_id` = strategy bundle hash (migration 014); `agents_agent_id` = workspace agent ULID (migration 022, nullable) |
| `eval_decisions` | `(run_id, decision_index)` | `action`, `conviction`, `pnl_realized` | One row per trader decision |
| `eval_findings` | `id` | `run_id`, `kind`, `severity`, `summary` | LLM-extracted findings about a run |
| `eval_equity_samples` | `(run_id, timestamp)` | `equity_usd` | Equity curve samples |
| `agent_runs` | `id` | `eval_run_id`, `strategy_id`, `retention_mode`, `status` | Links to eval_runs via `eval_run_id` FK |
| `spans` | `id` | `run_id`, `parent_span_id`, `kind`, `status`, `duration_ms` | `run_id` → `agent_runs.id` |
| `model_calls` | `span_id` | `provider`, `model`, `cost_usd`, `prompt_hash`, `response_hash`, `input_token_count`, `output_token_count` | One-to-one with spans where `kind = 'model.call'` |
| `tool_calls` | `span_id` | `tool_name`, `origin`, `tool_hash` | One-to-one with spans where `kind = 'tool.call'` |
| `checkpoints` | `id` | `run_id`, `input_hash`, `output_hash` | Replay checkpoints; `output_hash` nullable when `retention_mode = hash_only` |
| `determinism_receipts` | `run_id` | `receipt_hash`, `manifest_canonical` | One-to-one with `eval_runs`; run_id FK |

### Core DB (separate file, requires ATTACH)

| Table | PK | Key columns |
|---|---|---|
| `cycles` | `cycle_id` | (briefing trigger record) |
| `risk_outcomes` | `(cycle_id, arm_name)` | `risk_decision_json` (serialized RiskDecision) |

## SQL View Description

The script `scripts/trace-coverage-cartographer.sql` runs five passes:

1. **eval_runs coverage** — group by status; count runs with metrics, agent links, agent_run records, and determinism receipts.
2. **agent_runs coverage** — group by status + retention_mode; sum spans, model_calls, tool_calls.
3. **checkpoint coverage** — per retention_mode: count checkpoints with vs. without `output_hash`. Identifies the fraction of runs where payload-level replay is possible.
4. **determinism_receipts gap** — count completed eval_runs that have no matching determinism_receipt row.
5. **orphan checkpoints** — count checkpoint rows whose `run_id` has no corresponding `agent_runs` row.

## Null-Result Protocol

On a fresh install with no eval runs, all queries return zero-row results. This is valid and expected. The queries are read-only and safe to run against any state of the database.

## Cross-DB Note

Passes 1–5 target the engine DB only. Risk outcomes (core DB) require a separate `ATTACH DATABASE 'path/to/core.db' AS core;` statement before running any join involving `risk_outcomes`. The cartographer SQL does not include that join to remain engine-DB-only by default. See `scripts/risk-veto-taxonomy.sql` for core-DB queries.

## Files Changed

None. Read-only audit. Deliverable: `scripts/trace-coverage-cartographer.sql`.

## Verification

```bash
sqlite3 "$XVN_HOME/engine.db" < scripts/trace-coverage-cartographer.sql
```

## Residual Risks

- Two "checkpoints" tables exist: `checkpoints` (migration 018, agent-run replay) and `chat_checkpoints` (migration 044, chat-rail authoring). Any future SQL that references `checkpoints` must be explicit about which it means.
- `agents_agent_id` is nullable — pre-migration-022 rows have no workspace agent link. Cross-run joins via this column will drop historical rows.
- The `eval_runs.manifest_canonical` column determines drift comparability. Runs without this field cannot be compared (see EvalDriftArchaeologist for the incomparable cohort protocol).
