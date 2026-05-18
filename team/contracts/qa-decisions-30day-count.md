---
track: qa-decisions-30day-count
lane: integration
wave: qa-operator-2026-05-18
worktree: .worktrees/qa-decisions-30day-count
branch: task/qa-decisions-30day-count
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/dispatcher.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/mod.rs
  - crates/xvision-engine/src/data/**
  - crates/xvision-engine/tests/decisions_count.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-execution/**
  - frontend/web/**
interfaces_used:
  - Scenario bar iteration
  - eval dispatcher loop
  - decisions table write path
parallel_safe: false
parallel_conflicts:
  - "qa-trace-broker-spans: also edits crates/xvision-engine/src/eval/executor/. Coordinate disjoint regions via team/queue/."
  - "alpaca-paper-crypto-submit: holds single-writer claim on eval/executor/paper.rs. Avoid that file or stack."
verification:
  - cargo test -p xvision-engine
  - cargo test -p xvision-engine --test decisions_count
  - cargo clippy -p xvision-engine -- -D warnings
acceptance:
  - A 30-bar scenario (inclusive start and end bar) yields exactly 30
    decision rows in the `decisions` table — not 29.
  - Written investigation note in `team/status/qa-decisions-30day-count.md`
    identifying the off-by-one location (scenario bar slicer, eval
    dispatcher loop bound, or backtest executor early-exit).
  - Fix at the root cause — no pad-with-empty-row or wrap-with-+1
    workarounds.
  - Regression test in `crates/xvision-engine/tests/decisions_count.rs`
    parameterizes over scenarios of 1, 5, 30, and 100 bars and asserts
    `decisions.len() == bar_count` for each, with the first decision
    keyed to the first bar and the last decision keyed to the last bar
    (so neither end is dropped).
  - No regression on the existing eval / backtest tests.
---

# Scope

Operator reported (2026-05-18): a 30-day strategy run produced only 29
decisions. Likely off-by-one in scenario bar iteration (exclusive end),
the dispatcher loop bound, or an early-exit in the backtest executor
on the final bar. Investigate, fix the root cause, and pin behavior
with a parameterized regression test.

This is small but matters: a missing terminal-bar decision means the
last position never closes in the metrics summary, which corrupts
PnL / hit-rate analysis downstream.

Coordinates with `qa-decisions-position-pnl` — that contract surfaces
the close behavior, but if the close is missing because the bar is
missing, this contract is the root fix.

# Out of scope

- Decisions-table UI rendering (`qa-decisions-position-pnl`).
- Broker call spans (`qa-trace-broker-spans`).
- Scenario data ingestion / data-source bugs unless the off-by-one
  is in the bar slicer. If a deeper data-pipeline fix is needed,
  file a contract update.
- Adding migrations.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/qa-decisions-30day-count status
git -C .worktrees/qa-decisions-30day-count log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/qa-decisions-30day-count \
  -b task/qa-decisions-30day-count origin/main
```

# Notes

Investigation order:

1. Reproduce: configure a backtest scenario of exactly 30 daily bars
   and run it. Confirm `SELECT count(*) FROM decisions WHERE cycle_id IN (...)` returns 29.
2. Inspect the scenario bar slicer — likely under
   `crates/xvision-engine/src/data/` — for inclusive vs exclusive end.
3. Inspect the dispatcher loop in
   `crates/xvision-engine/src/eval/dispatcher.rs`.
4. Inspect the backtest executor's final-bar handling in
   `crates/xvision-engine/src/eval/executor/backtest.rs`.

Append checkpoints / PR links below.
