# Claim: tradingview-1-chart-api-indicators

Claimed: 2026-05-14T08:01:56Z

Worktree: `.worktrees/tradingview-1-chart-api-indicators`

Branch: `tradingview-1-chart-api-indicators`

Scope:

- Execute the next available slice of `docs/superpowers/plans/2026-05-14-tradingview-1-chart-api-and-indicators.md`.
- Verify the existing chart payload builders and API surface against the plan.
- Add missing cached scenario payload coverage for bar loading and backend indicator series.

Verification target:

- `cargo test -p xvision-engine --test chart_payload build_scenario_payload_loads_cached_bars_and_indicators`
- `cargo test -p xvision-engine --test chart_payload`

Local note:

- This deploy shell is covered by `CLAUDE.md`, which says not to run Rust
  builds/tests here, and `cargo` is not installed on PATH.
