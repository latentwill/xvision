---
track: tradingview-1-chart-api-indicators
worktree: /root/deploy/xvision/.worktrees/tradingview-1-chart-api-indicators
branch: tradingview-1-chart-api-indicators
base: alpaca-4-dashboard-scenario-authoring
phase: implemented
last_updated: 2026-05-14T08:01:56Z
owner: codex
---

# What changed

- Added a synthetic `bars_cache` seeder for chart payload integration tests.
- Added coverage that `build_scenario_payload` loads cached OHLCV bars without
  Alpaca network access and returns backend-computed SMA, EMA, Bollinger, RSI,
  ATR, and MACD series with expected warmup lengths.
- Preserved the existing chart API builders, dashboard route wrappers, and
  frontend chart client that were already present in this branch stack.

# Checkpoints

- `test(engine): cover cached scenario chart indicators`

# Verification

- `git diff --check`

# Blocked on

- Rust tests were not run on this deploy host because `CLAUDE.md` forbids
  `cargo`, `cargo build`, `cargo check`, and `cargo test` here.
- `cargo` is also not installed on PATH in this shell.

# CI/non-deploy verification target

```bash
cargo test -p xvision-engine --test chart_payload build_scenario_payload_loads_cached_bars_and_indicators
cargo test -p xvision-engine --test chart_payload
```
