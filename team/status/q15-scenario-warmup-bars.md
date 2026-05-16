---
track: q15-scenario-warmup-bars
status: in-progress
worktree: .worktrees/q15-scenario-warmup-bars
branch: task/q15-scenario-warmup-bars
claimed_at: 2026-05-16
claimed_by: claude (Opus 4.7, 1M ctx)
---

# Progress log

## 2026-05-16 — claimed + contract corrected

- Created worktree on `task/q15-scenario-warmup-bars` from `origin/main`
  (`b97f098`).
- Surveyed: `eval/scenario.rs`, `eval/scenario_store.rs`, `eval/bars.rs`,
  `eval/executor/backtest.rs`, `eval/executor/paper.rs`, `api/scenario.rs`,
  `api/eval.rs`, `strategies/{mod,manifest}.rs`, `tools/{ohlcv,indicators}.rs`,
  `commands/scenario.rs`, `routes/scenarios.rs`, `components/scenario/ScenarioForm.tsx`,
  `api/scenarios.ts`.

## Key findings shaping the implementation

- **Scenarios are stored as `body_json` and immutable** (trigger from
  migration `011_scenarios.sql`). `warmup_bars` lives inside `body_json`
  via serde default, so no DB migration is required.
- **The pipeline seed today only carries `current_bar`** — there's no rolling
  indicator engine being fed bars. The "no EMA cross evident from single
  bar" failure from the QA15 reproducer is because the trader LLM is given
  exactly one bar per decision. Pragmatic fix: prepend the last
  `warmup_bars` bars to the seed as `bar_history: [...]` at each decision
  so the LLM can compute crossovers itself, and the `ohlcv` / `indicator_panel`
  tools (when used) see real history.
- **Contract path globs were stale** — scenarios live under `eval/`, bars
  cache lives at `eval/bars.rs`, and the SPA uses `routes/+components/+api`
  rather than `features/scenarios/`. Contract frontmatter + OWNERSHIP
  updated in the first commit on this branch.
- **"update --warmup-bars" interpretation**: scenarios are immutable, so
  the round-trip path is `xvn scenario create --warmup-bars N` and
  `xvn scenario clone --warmup-bars N` (mutation via clone). Treating
  `scenario update` as an alias for clone is out of scope here.

## Plan

1. Add `warmup_bars: u32` (default 200) to `Scenario`, `CreateScenarioRequest`,
   `ScenarioMutations`.
2. Add `min_warmup_bars: Option<u32>` to `PublicManifest` plus a helper that
   derives a value from `mechanical_params`.
3. Add `load_warmup_bars` helper in `eval::bars` that fetches a separate
   cache window for `[scenario.start - N*bar_seconds, scenario.start)`.
4. Wire warmup pre-fetch into `api::eval::build_{backtest,paper}_executor`;
   pass the bars to the executor as a `with_warmup` builder.
5. Backtest + paper executors: include rolling history in the per-decision
   seed and iterate only the decision window.
6. `api::eval` preflight: warn when `warmup_bars < min_warmup_bars`; error
   on bars-cache miss for the warmup window.
7. CLI `xvn scenario create --warmup-bars` (+ clone).
8. UI: Context bars field in `ScenarioForm.tsx` with helper text.
9. Tests across executor seed shape, scenario serde defaults, CLI flag,
   and `ScenarioForm` warmup field.
