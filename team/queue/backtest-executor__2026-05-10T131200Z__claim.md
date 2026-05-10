---
from: backtest-executor
to: all
topic: claim
created_at: 2026-05-10T13:12:00Z
ack_required: false
---

# `backtest-executor` track claimed (Plan #5 Phase 3.B Task 5 — `BacktestExecutor`)

Picking up the v1 demo path that doesn't require external API keys. PR #26
landed `--mode paper` end-to-end; `--mode backtest` is currently rejected
with `ApiError::Validation`. This track lights it up.

Worktree `.worktrees/backtest-executor`, branch `feature/eval-3b-backtest`,
based on `origin/main` @ `2fd469d`.

Briefing: `team/briefings/backtest-executor.md`.

## Files this track touches

- `crates/xvision-engine/src/eval/executor/backtest.rs` (NEW)
- `crates/xvision-engine/src/eval/executor/mod.rs` (1-line additive)
- `crates/xvision-engine/src/api/eval.rs` (replace backtest-validation
  rejection with executor selection in `run` / `run_inner`)
- `crates/xvision-engine/tests/api_eval_run.rs` (swap "rejects backtest"
  test for "dispatches to backtest executor"; add round-trip test)

## Zero overlap with active sessions

- `eval-3d-compare` (worktree, `xvn eval compare`): `xvision-cli` + new
  `crates/xvision-engine/src/eval/compare.rs`
- `llm-providers-5` (PR #27): `xvision-cli/src/commands/provider.rs` +
  `xvision-eval/src/baselines/trader_arm.rs`
- `llm-providers-6` (worktree, Plan #7 Phase 5): docs + UI design lock
  notes only

## Out of scope (deferred — will be called out in the PR body)

- Multi-asset universes (v1 backtest is BTC-only)
- Sharpe sourced from the equity_samples table (Phase 3.C metrics work)
- Indicator panel injection into pipeline seed
- SSE progress (Phase 3.D Task 13)
- Migrating xvision-eval baselines to LLM-shim templates (Phase 3.E)

## v1 QA value

After this lands an operator with no external accounts can run:

```
xvn eval run --mode backtest --strategy <agent_id> --scenario crypto-bull-q1-2025
```

…end-to-end against the bundled fixture, recording every decision +
equity sample to the `eval_runs` schema and printing a final
`MetricsSummary`.
