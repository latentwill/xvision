# Claim: alpaca-tradingview-qa-performance

Claimed: 2026-05-14T08:20:49Z

Worktree: `.worktrees/alpaca-tradingview-qa-performance`

Branch: `alpaca-tradingview-qa-performance`

Scope:

- Execute the operator docs and naming-discipline slice of the final
  Alpaca/TradingView QA plan.
- Document the Backtest versus Alpaca paper distinction.
- Record the six chart surfaces now covered by the stacked implementation
  tracks.
- Update F30/F32 follow-up status text with the 2026-05-14 execution path.

Verification target:

- `rg -n "Paper mirror|Backtest means|Current Chart Surfaces|2026-05-14" MANUAL.md docs/dashboard.md FOLLOWUPS.md`
- `git diff --check`
