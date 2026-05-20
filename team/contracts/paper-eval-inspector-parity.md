---
track: paper-eval-inspector-parity
lane: integration
wave: qa-2026-05-19
worktree: .worktrees/paper-eval-inspector-parity
branch: task/paper-eval-inspector-parity
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/executor/paper.rs        # only if a missing persist call is the root cause
  - crates/xvision-engine/src/api/eval.rs                   # diagnosis hooks; mode-dispatch read sites
  - crates/xvision-engine/src/api/eval/runs.rs              # detail-endpoint response shape if it forks per mode
  - crates/xvision-engine/tests/eval_executor_paper.rs      # add a parity test
  - crates/xvision-engine/tests/api_eval_run.rs             # add a paper-detail response shape test
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.tsx     # if the mobile inspector forks the same way
  - frontend/web/src/features/decisions/**                  # if a paper-mode branch in DecisionsPanel exists
forbidden_paths:
  - crates/xvision-engine/src/eval/executor/backtest.rs     # parity work — do not alter backtest's shape
  - crates/xvision-engine/src/eval/executor/mod.rs          # executor trait surface stays put
  - crates/xvision-engine/src/eval/store.rs                 # storage layer is shared; do not fork it
  - crates/xvision-engine/migrations/**                     # no schema changes
  - frontend/web/src/routes/eval-runs.tsx                   # list route, owned by lists-v1 phase 2a (#399 merged) — out of scope
  - crates/xvision-execution/**                             # broker surface is correct; the gap is engine→frontend, not broker→engine
interfaces_used:
  - DecisionRow                                             # crates/xvision-engine/src/eval/store.rs::DecisionRow
  - RunStore::record_decision                               # storage call from paper executor
  - RunStore::record_equity                                 # equity-curve persistence
  - LiveDecisionRow                                         # crates/xvision-engine/src/api/chart.rs (SSE event payload)
  - DecisionRowDto                                          # frontend/web/src/features/decisions/positions.ts
  - eval-runs detail loader (`getEvalRunDetail`)
parallel_safe: false                                        # touches executor + detail route; coordinate via team/queue/ if other paper-mode tracks open
parallel_conflicts: []
verification:
  - cargo test -p xvision-engine eval_executor_paper
  - cargo test -p xvision-engine api_eval_run
  - cargo test --workspace
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- routes/eval-runs-detail
  - pnpm --dir frontend/web lint
acceptance:
  - **Root-cause established first.** Worker confirms whether paper runs persist `DecisionRow`s, equity samples, and broker-fill metadata to the same tables that the backtest executor uses. The intake's diagnosis path (api/eval.rs RunMode dispatch around `:803-1292`) is the starting point, not the conclusion. Document the finding in `Notes:` before writing fix code.
  - **Paper inspector renders the same decisions table as backtest.** `frontend/web/src/routes/eval-runs-detail.tsx::DecisionsPanel` (`:702-...`) shows every decision row for paper runs the same way it shows backtest. Side label uses the `decision-side-label-sell-vs-short` mapping already shipped via #341 commit `bc92de7` (BUY / SHORT / SELL / COVER / HOLD).
  - **PnL summary parity.** The strategy-summary top panel of a paper eval inspector surfaces the same metrics as backtest: absolute terminal PnL in account currency, % return, equity-curve sparkline if applicable. Source: same loader query as backtest; no paper-only fork in the summary card.
  - **Order/fill visibility.** Buy/sell orders submitted by the paper executor appear in the inspector. Two paths are acceptable; pick the one matching the root-cause finding:
      a. If broker-fill metadata is persisted but the loader ignores it for paper mode → fix the loader.
      b. If the paper executor records the *decision* but not the resulting broker `fill` → add the persist call that backtest has, OR (if backtest doesn't emit fills either and the existing surface is "decisions only") clarify in `Notes:` that "orders dont show" means decision rows weren't reaching the table, and that the decision-table fix subsumes it.
  - **Backtest behaviour unchanged.** Existing backtest inspector tests pass without modification. The fix is additive on the paper-mode side.
  - **One new engine test.** `eval_executor_paper.rs` (or a new helper) asserts that after a short paper run with a mocked broker surface and a synthetic bar series, the persisted decision rows match a backtest reference for the same scenario. No live Alpaca credentials required.
  - **One new frontend test.** `eval-runs-detail.test.tsx` asserts that a paper-mode detail fixture renders the decisions table, the PnL summary, and the order list. Use a paper-mode fixture distinct from the existing backtest fixture.
  - **Mode label is the only intentional difference.** Per the intake: "the underlying mode label is the only visual difference." Visually compare against the existing backtest inspector (operator can sanity-check by running the same strategy against a tiny scenario in each mode).

---

# Scope

Track #6 of QA Round 4 (`team/intake/2026-05-19-qa-operator-round-4.md`).
Operator-observed gap: a **paper** eval inspector is missing the PnL
column / summary the **backtest** inspector has, and buy/sell orders
the paper executor submits do not render in the inspector. Backtest
parity is the target — the underlying mode label is the only intended
visual difference.

The intake's diagnosis path:

1. `crates/xvision-engine/src/api/eval.rs:669,1067` — RunMode dispatch
   differs for Paper vs Backtest. Confirm whether paper runs persist
   decision rows, fills, and equity snapshots to the same tables /
   endpoints as backtest.
2. Frontend: confirm the eval-runs-detail loader hits the right API
   for paper runs. If the data is present but the loader requests a
   backtest-only endpoint, that's a one-line fix; if the data isn't
   being persisted in the paper path, this is an engine-side gap.

Root-causing the gap is the first deliverable. Code change scope
depends on the finding — the contract allows both an engine-side fix
(persist call missing) and a frontend-side fix (loader fork). The
"alpha: fix the root cause, don't suppress" rule from
`feedback_alpha_root_cause` applies.

# Out of scope

- Adding a *new* metric column to backtest in the same PR (this is
  parity, not feature work).
- Changing the broker surface or `BrokerSurface` trait
  (`crates/xvision-execution/**` is forbidden — the gap is engine→
  frontend, not broker→engine).
- Migrating list-page rendering (lists-v1 phase 2 owns `eval-runs.tsx`).
- Changing the executor trait, the `RunStore` schema, or any of the
  shared interfaces that backtest depends on.
- Schema migrations. If the gap is "paper doesn't persist X" and the
  fix requires a new column, escalate to the conductor for a
  migration-claim contract update first.
- Touching `eval-compare.tsx` (#262 already wired multi-arm summaries
  there).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/paper-eval-inspector-parity status
git -C .worktrees/paper-eval-inspector-parity log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/paper-eval-inspector-parity -b task/paper-eval-inspector-parity origin/main
```

# Notes

The two sibling QA Round 4 tracks that look related but are **not**
this contract:

- `eval-id-resurface-no-truncate` — already landed (#341 commit
  `7c7c55a`). Do not retouch the inspector header.
- `decision-side-label-sell-vs-short` — already landed (#341 commit
  `bc92de7`). The SELL/COVER distinction already works on paper; verify
  it does, then move on.

Acceptance asks for a **paper-mode test fixture** in the frontend test
suite. None exists today (the existing fixtures are backtest-only). The
worker should add one — small JSON next to the existing detail fixture
under `frontend/web/src/routes/__fixtures__/` (or wherever the existing
backtest fixture lives — verify the path on intake).
