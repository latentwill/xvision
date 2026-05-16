# qa10-backtest-short-window-replay claim

Claimed: 2026-05-16T02:49:03Z
Owner: claude
Worktree: current workspace (no `.worktrees/` clone; board lists this track as "current workspace")
Branch: `qa10-backtest-short-window-replay`

Scope:

- Remove the hard 200-bar `WARMUP_BARS` gate in
  `crates/xvision-engine/src/eval/executor/backtest.rs` so literal
  30-day / Day-1 eval windows can run from available bars.
- Keep only the mechanically required constraints: at least one decision
  bar and one following bar to fill against.
- Update the bar-clock progress denominator so it reflects "decision
  bars" without the warmup skip.
- Add a focused regression in
  `crates/xvision-engine/tests/eval_progress_backtest.rs` that proves a
  30 daily-bar fixture produces 29 decisions starting from the first
  bar.
