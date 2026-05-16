---
track: q15-scenario-warmup-bars
lane: foundation
wave: q15
worktree: .worktrees/q15-scenario-warmup-bars
branch: task/q15-scenario-warmup-bars
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/scenarios/**
  - crates/xvision-engine/src/strategies/**            # min_warmup_bars derivation only
  - crates/xvision-engine/src/bars/cache.rs
  - crates/xvision-cli/src/commands/scenario/**
  - crates/xvision-dashboard/src/routes/scenarios/**
  - frontend/web/src/features/scenarios/**
forbidden_paths:
  - crates/xvision-engine/migrations/**                # add migration via separate reservation step
  - frontend/web/src/features/eval-runs/**
  - frontend/web/src/features/chat-rail/**
interfaces_used:
  - BarsCache::range
  - BacktestExecutor::iterate_decisions
  - ScenarioRecord
  - StrategyManifest::indicator_config
parallel_safe: false
parallel_conflicts:
  - q15-agent-max-tokens-from-model   # both may touch eval executor (loosely)
verification:
  - cargo test -p xvision-engine eval::executor::warmup
  - cargo test -p xvision-engine scenarios::warmup_bars
  - cargo test -p xvision-cli scenario::warmup
  - corepack pnpm --dir frontend/web test -- scenarios-create
acceptance:
  - Scenario record carries optional `warmup_bars: u32` (default 200 for new scenarios).
  - Backtest executor fetches `warmup_bars` of pre-window bars and feeds them to indicators marked `is_warmup = true`; decision loop iterates only non-warmup bars.
  - Strategy manifest exposes `min_warmup_bars` derived from indicator config (e.g. longest EMA period × 2).
  - Eval preflight warns when `scenario.warmup_bars < strategy.min_warmup_bars`.
  - QA15 reproducer (30-bar 1d window, EMA5/EMA13 strategy) produces a real crossover decision at bar 1 once `warmup_bars >= 13`.
  - Bars-cache miss for the warmup window fails preflight with an actionable error.
  - `xvn scenario create --warmup-bars N` and `--update --warmup-bars N` round-trip.
  - Scenario authoring UI surfaces a "Context bars" field with helper text.
---

# Scope

Implements the warmup-bars half of QA15 per
`docs/superpowers/specs/2026-05-16-q15-eval-resilience-and-contracts.md` §2.
Removes the "decisions in a vacuum" failure where bar-1 indicators have no
history. Re-introduces a configurable warmup window distinct from the
artificial 200-bar gate removed in PR #177 (that gate blocked the run;
this preloads context so the run can start).

# Out of scope

- Indicator-library additions or rewrites.
- Reasoning-token / max-tokens (separate track `q15-agent-max-tokens-from-model`).
- Eval JSON export (separate track `q15-eval-json-export`).
- Live/paper warmup beyond reading the same cache as backtest.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/q15-scenario-warmup-bars -b task/q15-scenario-warmup-bars origin/main
```

# Notes

- If a new DB column is needed on `scenarios`, reserve migration 006 via
  `team/MANIFEST.md` and `v1-shipping-plan.md` in the same commit.
- The QA15 reproducer transcript ("No EMA cross evident from single bar...")
  is in `team/intake/2026-05-16-q15.md` and should be the canary test.
