# CacheShapeMiner

**Date:** 2026-06-01
**Source:** 100x run `.100x/runs/20260601_002720/`
**Script:** `scripts/cache-shape-miner.sql`

## Surfaces Inspected

- `crates/xvision-engine/migrations/018_agent_run_observability.sql` — `model_calls`, `tool_calls`, `spans`
- `crates/xvision-engine/migrations/025_agent_slot_cache_and_window.sql` — cache/window intent
- `crates/xvision-engine/src/agent/**` — agent execution and observability surfaces

## Findings

`PerStrategyVerdict` is not the correct primary data source yet. The existing durable ledger is `model_calls` joined through `spans`.

The generated SQL reports:

- cost and token totals by provider/model
- duplicate prompt-hash candidates across runs
- missing `response_hash` rates
- capability-path distribution

## Why This Is Useful

It identifies expensive, repeated model-call shapes that may justify cache partitioning or prompt-hash reuse. Even if no cache is added, it gives a cost map grounded in stored call records.

## Waste Avoided

The agent does not invent a `PerStrategyVerdict` cache plan from a planned-only surface. It mines what exists: model calls, prompt hashes, response hashes, provider/model, and costs.

## Files Changed

- `scripts/cache-shape-miner.sql`
- `docs/research/overnight-agents/2026-06-01/CacheShapeMiner.md`

## Verification

Run against the engine DB:

```bash
sqlite3 "$XVN_HOME/engine.db" < scripts/cache-shape-miner.sql
```

Expected behavior:

- reports model-call cost/caching candidates if traces exist
- returns empty/zero rows on a fresh DB
- performs only read-only `SELECT` statements

## Residual Risks

- `prompt_hash` equality does not always imply semantic cacheability; tool context, system prompt, and safety policy can differ outside the hash if the hash contract is incomplete.
- Cost fields may be null under some providers or retention modes. Treat missing cost as an instrumentation gap, not zero spend.
