# Track briefing — `backtest-executor`

**Plan:** [Plan #5 — Eval Engine](../../docs/superpowers/plans/2026-05-08-eval-engine-plan.md), Phase 3.B Task 5 — `BacktestExecutor`. Explicitly deferred from PR #26.

**Worktree:** `.worktrees/backtest-executor`
**Branch:** `feature/eval-3b-backtest`
**Base:** `origin/main` @ `2fd469d`

## Why this track

PR #26 landed `xvn eval run --mode paper` end-to-end. Backtest mode is rejected with `ApiError::Validation("backtest mode not yet supported …")`. Without backtest, every demo path requires an Alpaca paper account + Anthropic key. Backtest replays a parquet fixture deterministically — anyone with the workspace can run it. This is the missing piece for an out-of-the-box v1 demo.

## Scope (in this PR)

1. **`crates/xvision-engine/src/eval/executor/backtest.rs`** — new `BacktestExecutor` impl of the existing `Executor` trait. Walks the scenario's OHLCV fixture in chronological order; on each cadence boundary calls `run_pipeline`, parses `TraderOutput`, simulates fill against the next bar's open with slippage + taker fees, tracks position internally (no broker), records decisions + equity samples through `RunStore`, computes metrics on completion via the existing `eval::metrics::*` helpers.
2. **`executor/mod.rs`** — `pub mod backtest;` + `pub use backtest::BacktestExecutor;`.
3. **`engine::api::eval::run` / `run_with_deps`** — replace the early-validation rejection of `RunMode::Backtest` with executor selection. For backtest mode, no broker is needed; for paper mode the existing surface keeps working unchanged. Adjust `run_rejects_backtest_mode_until_executor_lands` test → `run_dispatches_to_backtest_executor` (or similar).
4. **One round-trip integration test** — `MockDispatch` emits alternating `long_open` / `flat` decisions; `BacktestExecutor` walks `test-fixture-btc-2024-01`; assert run completes + decisions persist + equity curve non-empty + metrics computed.

## Out of scope (deferred — call out in PR body)

- Multi-asset universes — v1 backtest is BTC-only (matches the existing PaperExecutor BTC reference).
- Sharpe sourced from full equity samples table — Phase 3.C metrics work; this PR uses the same in-memory `equity_to_returns` path PaperExecutor uses.
- Indicator panel injection into the pipeline seed — PaperExecutor doesn't pass it either; pipeline currently consumes its own market context. Adding panel injection cleanly is its own task.
- SSE progress (Phase 3.D Task 13).
- Migrating `xvision-eval` baselines to LLM-shim templates (Phase 3.E Task 14).

## Files this track touches

- `crates/xvision-engine/src/eval/executor/backtest.rs` (NEW)
- `crates/xvision-engine/src/eval/executor/mod.rs` (1-line additive)
- `crates/xvision-engine/src/api/eval.rs` (executor selection in `run` + `run_inner`)
- `crates/xvision-engine/tests/api_eval_run.rs` (replace "rejects backtest" test, add round-trip)

Zero overlap with currently-active worktrees (`eval-3d-compare`, `llm-providers-5`, `llm-providers-6`).

## v1 QA value

```
$ xvn eval run --mode backtest --strategy <agent_id> --scenario crypto-bull-q1-2025
```

…runs end-to-end without any external API keys. This is the demo command for anyone who can't (or won't) sign up for Alpaca paper.
