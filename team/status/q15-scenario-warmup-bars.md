---
track: q15-scenario-warmup-bars
status: ready-for-pr
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

## 2026-05-16 — implementation complete, ready for PR

Three commits on `task/q15-scenario-warmup-bars`:

1. `eb3b4c0` — data-model substrate (warmup_bars + min_warmup_bars,
   contract path correction, OWNERSHIP update, status file).
2. `39f6cc7` — `eval::bars::load_warmup_bars`, backtest+paper
   `with_warmup` builders, per-decision `bar_history` slice, preflight
   wiring with actionable cache-miss error.
3. `6bdf74d` — Context-bars UI field, `--warmup-bars` CLI flag, full
   test suite (2 executor canaries + 2 scenario serde tests + 4 strategy
   helper tests + 3 CLI round-trip tests + 9 `ScenarioForm` tests).

### Verification (all green)

- `cargo test -p xvision-engine --test eval_executor_warmup` → 2/2 pass
- `cargo test -p xvision-engine --lib eval::scenario::warmup_bars_tests` → 2/2
- `cargo test -p xvision-engine --lib strategies::tests::min_warmup_bars` → 4/4
- `cargo test -p xvision-cli --test scenario_cli scenario_warmup` → 3/3
- `pnpm --dir frontend/web test -- ScenarioForm` → 9/9
- `bash scripts/board-lint.sh` → clean
- `pnpm --dir frontend/web typecheck` → clean

### Pre-existing failures (NOT introduced by this work)

- `crates/xvision-mcp` lib test: `missing field reasoning in DecisionRow`
  — out-of-scope; fails on `origin/main`.
- `xvision-engine` lib: `authoring::validate_draft_reports_missing_agent_for_fresh_template`
  + 3 `eval::postprocess::tests::*` — fail on `origin/main`.
- `eval_run_scenario::backtest_missing_cache_and_fixture_returns_actionable_validation`
  — fails on `origin/main` (test depends on cache state that is shared
  across runs).

### Out-of-scope captured in PR description

- Scopes touched outside this contract's `allowed_paths`: tests scattered
  across the engine + dashboard crates and `eval-runs.test.tsx` had to
  gain the new `warmup_bars` / `min_warmup_bars` fields to keep the
  workspace compiling. These were mechanical struct-literal updates;
  no test logic changed.
- `frontend/web/src/api/types.gen/` regenerated only the three files I
  intentionally touched (`Scenario`, `CreateScenarioRequest`,
  `ScenarioMutations`); other auto-regenerated files were reverted to
  avoid drift in the `ts(optional)` vs `: T | null` convention used
  elsewhere.

### Followups (not blocking this PR)

- The `xvision-mcp` `reasoning` field error + `eval::postprocess` test
  flakes look like Q10 leftover — worth a tracking note in FOLLOWUPS.md.
- Surfacing the `warn_on_warmup_mismatch` warning to the dashboard
  preflight UI (today it only lands in `tracing::warn`).
- A real DB migration is unnecessary for warmup_bars but eval-review
  may want a column for indexing/filtering later.
