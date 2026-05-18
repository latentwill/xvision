# qa-decisions-30day-count — status

**Contract:** `team/contracts/qa-decisions-30day-count.md`
**Branch:** `task/qa-decisions-30day-count`
**Worktree:** `.worktrees/qa-decisions-30day-count`
**Claimed:** 2026-05-18
**Status:** in-progress

## Investigation snapshot

Reproduced the bug by walking the backtest replay loop in
`crates/xvision-engine/src/eval/executor/backtest.rs:360-373`:

```rust
for (i, bar) in bars.iter().enumerate() {
    // cadence gate ...
    // Need a next bar to fill against.
    let Some(next_bar) = bars.get(i + 1) else {
        break;
    };
    // emit decision, simulate fill against next_bar.open, persist row.
}
```

The loop reserves the final bar as the fill source, so an N-bar input
yields N-1 decisions. This was even documented as the intended
behavior in `eval_progress_backtest.rs:142-145`
("30 bars should produce one decision for each bar with a next-open
fill") — that test asserted `n_decisions == 29` for 30 bars.

That documentation was wrong from the operator's perspective. A
30-day backtest is supposed to produce 30 decisions. The final bar
of the window should still get a decision; without a T+1 bar to fill
against, it can fall back to its own close (the same close the trader
already saw in the seed).

## Chosen fix

Fall back to `bar.close` as the fill source for the final bar
instead of breaking out of the loop. Single-location change in
`backtest.rs`:

```rust
// OLD: skips the last bar entirely (loses a decision).
let Some(next_bar) = bars.get(i + 1) else { break; };

// NEW: use bar.close when there is no T+1.
let next_bar_open = bars.get(i + 1).map(|b| b.open).unwrap_or(bar.close);
```

All four references to `next_bar.open` in the loop body (seed JSON,
simulate_fill, equity-calc, HoldMarker) become `next_bar_open`.
`total_decision_bars` updated from `bars.len().saturating_sub(1).max(1)`
to `bars.len().max(1)`.

Rationale for fixing at the executor instead of the loader: the
fixture path (`load_ohlcv_fixture`) is not bounded by
`time_window`, so an N-bar fixture replayed against an N-bar scenario
would still produce N-1 decisions under any loader-only fix. The
executor invariant is the source of truth: N bars in → N decisions
out. PaperExecutor already follows this convention.

## Tests

- New regression file `crates/xvision-engine/tests/decisions_count.rs`
  with parameterized coverage at 5 / 30 / 100 bars asserting
  `decisions.len() == bars.len()`, first/last decision keyed to
  first/last bar.
- Single-bar window guard test asserting executor preflight rejects
  with the "at least 2" message.
- Updated `eval_progress_backtest.rs::backtest_executor_runs_30_day_fixture_without_200_bar_warmup`
  from `n_decisions == 29` → `30`, plus a new assertion that the
  final decision is keyed to the last input bar.

`cargo test -p xvision-engine --test decisions_count --test eval_progress_backtest`
→ 4 / 6 pass.

Full `cargo test -p xvision-engine --tests --no-fail-fast` →
307 passed, 4 lib + 8 integration test failures across other files
are **pre-existing on origin/main** (verified by stashing the diff
and re-running) — unrelated to this PR.

## Out-of-scope confirmations

- No migration changes.
- No `xvision-execution` changes.
- No frontend changes.
- The `eval/dispatcher.rs` and `eval/data/**` paths in the contract's
  `allowed_paths` were not touched — the fix is purely at the
  executor's per-bar loop.

## Checkpoints

- 2026-05-18 — worker branch created.
- 2026-05-18 — root cause identified: backtest.rs:371-373 early-break.
- 2026-05-18 — fix landed at the executor; regression tests added.

## PR

(pending)
