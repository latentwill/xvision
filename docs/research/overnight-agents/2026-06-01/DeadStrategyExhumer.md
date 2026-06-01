# DeadStrategyExhumer

**Date:** 2026-06-01
**Source:** 100x run `.100x/runs/20260601_002720/`
**Script:** `scripts/dead-strategy-exhumer.sh`

## Surfaces Inspected

- `crates/xvision-engine/src/strategies/agent_ref.rs` — `AgentRef`
- `crates/xvision-engine/migrations/005_agents.sql` — agent tables
- `crates/xvision-engine/migrations/014_eval_agent_id.sql` and `022_eval_runs_agents_agent_id.sql` — eval run agent linkage
- Strategy filesystem convention under `$XVN_HOME/strategies`

## Findings

The useful feature is an orphan inventory, not automatic deletion. The generated script:

- requires `XVN_HOME` or `--home`
- scans `$XVN_HOME/strategies/**/*.json`
- extracts each strategy/agent id
- checks for references in `eval_runs.agent_id` and `agent_runs.strategy_id`
- labels rows as `ACTIVE`, `CONFIRMED_ORPHAN`, or `UNKNOWN`

## Why This Is Useful

xvision's strategy/agent vocabulary has changed over time. An explicit orphan report lets the operator separate active eval participants, truly unreferenced JSON artifacts, and unknown/malformed files that should not be deleted automatically.

## Waste Avoided

The agent does not delete anything and does not call a dormant strategy dead unless the available ledgers show no references. Ambiguous production boundaries remain ambiguous.

## Files Changed

- `scripts/dead-strategy-exhumer.sh`
- `docs/research/overnight-agents/2026-06-01/DeadStrategyExhumer.md`

## Verification

Without an xvision home:

```bash
bash scripts/dead-strategy-exhumer.sh
```

Expected: exits with an explanatory `XVN_HOME` error.

With a local install:

```bash
bash scripts/dead-strategy-exhumer.sh --home "$XVN_HOME"
```

Expected: prints active/orphan inventory and performs no writes.

## Residual Risks

- The current script assumes JSON files carry `id` or `agent_id`. If strategy manifests use a different nested shape, the extractor should be upgraded to parse the canonical manifest schema.
- `agent_runs.strategy_id` may historically carry a strategy id rather than a workspace agent id. Treat orphan as a cleanup candidate, not proof for deletion.
