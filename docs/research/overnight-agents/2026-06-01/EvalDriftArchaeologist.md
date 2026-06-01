# EvalDriftArchaeologist

**Date:** 2026-06-01
**Source:** 100x run `.100x/runs/20260601_002720/`
**Script:** `scripts/eval-drift-archaeologist.sql`

## Surfaces Inspected

- `crates/xvision-engine/migrations/002_eval.sql` — `eval_runs`, `metrics_json`
- `crates/xvision-engine/migrations/014_eval_agent_id.sql` — `strategy_bundle_hash` renamed to `agent_id`
- `crates/xvision-engine/migrations/027_run_bars_manifest.sql` — `bars_content_hash`, `manifest_canonical`
- `crates/xvision-engine/src/eval/metrics.rs` and related API code for metric JSON shape

## Findings

Eval drift analysis is useful only when runs are comparable. The useful cohort key is:

```text
agent_id + scenario_id + manifest_canonical
```

Rows with `manifest_canonical IS NULL` are explicitly bucketed as `INCOMPARABLE`.

The generated SQL reports:

- comparable completed cohorts with more than one run
- return/sharpe/drawdown ranges within comparable cohorts
- a count of completed runs that cannot be compared because `manifest_canonical` is missing

## Why This Is Useful

This is the safest form of silent regression archaeology: it refuses to compare rows where candle/data identity cannot be established. It can still produce a durable by-product on null data: an inventory of how many historical runs are incomparable.

## Waste Avoided

The agent does not compare arbitrary green CI runs. It does not infer drift from scenario names alone. It avoids fake signal by separating incomparable rows.

## Files Changed

- `scripts/eval-drift-archaeologist.sql`
- `docs/research/overnight-agents/2026-06-01/EvalDriftArchaeologist.md`

## Verification

Run against an engine DB:

```bash
sqlite3 "$XVN_HOME/engine.db" < scripts/eval-drift-archaeologist.sql
```

Expected behavior:

- returns comparable cohorts if present
- reports `INCOMPARABLE` or null-count rows for pre-manifest history
- performs only read-only `SELECT` statements

## Residual Risks

- The SQL assumes JSON metric keys remain stable. If `MetricsSummary` changes, update the extraction fields.
- `manifest_canonical` is necessary but not always sufficient: engine version and seed should also be considered for strict reproducibility claims.
