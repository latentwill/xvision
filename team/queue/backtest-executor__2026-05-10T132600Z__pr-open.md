---
from: backtest-executor
to: all
topic: pr-open
created_at: 2026-05-10T13:26:00Z
ack_required: false
---

# `backtest-executor` track — PR #32 open

PR: https://github.com/latentwill/xvision/pull/32
Branch: `feature/eval-3b-backtest`
Worktree: `.worktrees/backtest-executor`
Base: `origin/main` @ `2fd469d` (parent of `12c2438`, the claim commit)

## What landed

Plan #5 Phase 3.B Task 5 — `BacktestExecutor`. Replaces PR #26's
"backtest mode not yet supported" `Validation` rejection with a real
fixture-replay executor.

After merge:
```
xvn eval run --mode backtest --strategy <agent_id> --scenario <scenario_id>
```
runs end-to-end against the bundled synthetic fixtures with no broker
credentials required.

## Key API change

`engine::api::eval::run_with_deps` now takes
`broker: Option<Arc<dyn BrokerSurface>>` (was `Arc<dyn BrokerSurface>`).
- Paper mode requires `Some(broker)` — rejects with `Validation`
  otherwise.
- Backtest mode ignores the broker.
- The env-bound `run` only constructs `AlpacaPaperSurface::from_env`
  for paper mode, so backtest doesn't need Alpaca env vars.

## Files this PR touches

- `crates/xvision-engine/src/eval/executor/backtest.rs` (new)
- `crates/xvision-engine/src/eval/executor/mod.rs` (additive)
- `crates/xvision-engine/src/api/eval.rs` (executor selection in
  `run` / `run_inner`; broker now Option)
- `crates/xvision-engine/tests/api_eval_run.rs` (replace
  `run_rejects_backtest_mode_until_executor_lands` with
  `run_with_deps_completes_backtest_run_with_mocks` +
  `run_rejects_paper_mode_without_broker`)

## Tested

- 5 `simulate_fill` unit tests (flat-flat noop, long-from-flat slips up,
  flat closes long + books realized, long-when-long noop,
  short-from-long reverses + books realized)
- backtest round-trip integration test against
  `flash-crash-2024-08` synthetic fixture
- `cargo test --workspace` — **429 passed, 0 failed**

## Notes for downstream

- Open PR #28 (`xvn eval compare`) is unrelated — touches the CLI +
  new `eval/compare.rs`. No overlap with this PR's files.
- Open PR #27 (`xvn provider add/remove/check`) — no overlap.
- Open PR #29 (Plan #7 Phase 5 docs) — no overlap.
- Open PR #31 (`strategy-2a-mcp-authoring`) — no overlap; touches
  xvision-mcp.
- The `eval::api::run_with_deps` signature change is the only place a
  downstream caller would need to update; `xvision-engine` is the only
  caller in the workspace today.

## Out of scope (deferred — called out in PR body)

- Multi-asset universes (v1 backtest is BTC-only)
- Indicator panel injection into pipeline seed (both executors)
- Win-rate from realized-PnL pairs (Phase 3.C work)
- SSE progress (Phase 3.D Task 13)
- Migrating xvision-eval baselines to LLM-shim templates (Phase 3.E)
