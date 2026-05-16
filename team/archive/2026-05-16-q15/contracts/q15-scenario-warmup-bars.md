---
track: q15-scenario-warmup-bars
lane: foundation
wave: q15
worktree: .worktrees/q15-scenario-warmup-bars
branch: task/q15-scenario-warmup-bars
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/scenario.rs              # scenarios live under eval/, not src/scenarios/
  - crates/xvision-engine/src/eval/scenario_store.rs
  - crates/xvision-engine/src/eval/bars.rs                  # bars cache wrapper lives at eval/bars.rs, not bars/cache.rs
  - crates/xvision-engine/src/api/scenario.rs
  - crates/xvision-engine/src/api/eval.rs                   # warmup wiring + preflight surface
  - crates/xvision-engine/src/strategies/**                 # min_warmup_bars derivation only
  - crates/xvision-cli/src/commands/scenario.rs            # single-file CLI module, not a subdirectory
  - crates/xvision-dashboard/src/routes/scenarios.rs
  - frontend/web/src/routes/scenarios-new.tsx              # SPA uses routes/+components, not features/scenarios/
  - frontend/web/src/routes/scenarios-detail.tsx
  - frontend/web/src/components/scenario/**
  - frontend/web/src/api/scenarios.ts
  - frontend/web/src/api/types.gen/**                      # ts-export regenerates type bindings
forbidden_paths:
  - crates/xvision-engine/migrations/**                    # add migration via separate reservation step
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/components/chat-rail/**
interfaces_used:
  - eval::bars::load_bars / BarCacheArgs
  - Scenario (eval/scenario.rs)
  - BacktestExecutor::with_bars
  - PaperExecutor::with_bars
  - PublicManifest (strategies/manifest.rs)
parallel_safe: false
parallel_conflicts:
  - q15-agent-max-tokens-from-model   # both may touch eval executor (loosely)
verification:
  - cargo test -p xvision-engine --test eval_executor_warmup
  - cargo test -p xvision-engine --lib eval::scenario::warmup_bars_tests
  - cargo test -p xvision-engine --lib strategies::tests::min_warmup_bars
  - cargo test -p xvision-cli --test scenario_cli scenario_warmup
  - pnpm --dir frontend/web test -- ScenarioForm
acceptance:
  - Scenario record carries `warmup_bars: u32` (default 200 for new scenarios; legacy rows hydrate to 200 via serde default).
  - Backtest + paper executors fetch `warmup_bars` of pre-window bars and feed them into the per-decision seed as `bar_history`; the decision loop iterates only the scenario-window bars.
  - Strategy manifest exposes `min_warmup_bars` (with a helper that derives a sensible default from `mechanical_params` indicator periods).
  - Eval preflight warns when `scenario.warmup_bars < strategy.min_warmup_bars`.
  - QA15 reproducer (30-bar 1d window, EMA5/EMA13 strategy) sees ≥ 13 history bars in the bar-1 seed once `warmup_bars >= 13` — unit-tested at the executor level.
  - Bars-cache miss for the warmup window fails preflight with an actionable error pointing at `xvn bars fetch`.
  - `xvn scenario create --warmup-bars N` and `xvn scenario clone --warmup-bars N` round-trip the value (scenarios are immutable post-insert; clone is the mutation path).
  - Scenario authoring UI surfaces a "Context bars" field with helper text linking to `min_warmup_bars`.
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

- `Scenario` rows are immutable (enforced by the `scenarios_no_update`
  trigger from migration `011_scenarios.sql`). `warmup_bars` is stored
  inside `body_json` via a serde-default field; no DB migration is needed.
  Mutating `warmup_bars` after creation is intentionally not supported —
  clone is the only mutation path.
- The QA15 reproducer transcript ("No EMA cross evident from single bar...")
  is in `team/intake/2026-05-16-q15.md`. The canary test is at the
  executor level: it asserts that the per-decision seed includes ≥ N prior
  bars when `warmup_bars = N`, which is the mechanism that lets the
  trader LLM detect crossovers at bar 1.
- Contract paths were corrected on 2026-05-16 to match actual repo layout
  (scenarios live under `eval/`, bars cache at `eval/bars.rs`, frontend
  uses `routes/+components/+api` rather than `features/scenarios/`).
  See OWNERSHIP.md for the updated map.
- PR: https://github.com/latentwill/xvision/pull/183 (merged 2026-05-16).

# Checkpoints

- 2026-05-16: claimed; worktree at `.worktrees/q15-scenario-warmup-bars`,
  branch `task/q15-scenario-warmup-bars`.
