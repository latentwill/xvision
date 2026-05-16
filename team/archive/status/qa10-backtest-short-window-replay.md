---
track: qa10-backtest-short-window-replay
worktree: current workspace
branch: qa10-backtest-short-window-replay
phase: review
last_updated: 2026-05-16T02:50:00Z
---

# What I'm doing right now

Opening PR for the backtest short-window replay fix. Scope is the
executor warmup gate only — no other eval/runtime tracks touched.

# Blocked on

nothing

# Next up

Hand off to QA so a real 30-day eval (e.g. `flash-crash-2024-08` Day-1
window) can be replayed end-to-end. After this merges,
`qa10-flash-crash-fixture-alignment` is the next unblocker for that
scenario.

# Delivered

- Removed the `WARMUP_BARS: usize = 200` constant in
  `crates/xvision-engine/src/eval/executor/backtest.rs` and dropped the
  per-bar `if i < WARMUP_BARS { continue; }` skip inside the decision
  loop.
- Tightened `with_bars` / runner preconditions from "needs
  `WARMUP_BARS + 1` bars" to "needs at least one decision bar plus one
  next bar to fill against" (`bars.len() < 2`). The bail message now
  reflects the new minimum.
- Re-derived the `RunTick` denominator from `bars.len() - 1` (bars
  with a following fill bar) instead of `bars.len() - WARMUP_BARS`, so
  bar-clock progress goes 0 → 100 across the actual replay window.
- New test
  `backtest_executor_runs_30_day_fixture_without_200_bar_warmup` in
  `crates/xvision-engine/tests/eval_progress_backtest.rs`: builds a
  synthetic 30-daily-bar series, runs `BacktestExecutor::with_bars`,
  and asserts the executor produces 29 decisions starting on the first
  bar timestamp. Previously this would fail the `bars.len() <=
  WARMUP_BARS + 1` precheck.

# Verification

```
cargo test -p xvision-engine --test eval_progress_backtest
```

All five tests in `eval_progress_backtest` pass, including the new
`backtest_executor_runs_30_day_fixture_without_200_bar_warmup`
regression and the four pre-existing progress-event tests. Other
unrelated suites (e.g. `api_eval_run`) were not touched and retain
their pre-existing state on `main`.
