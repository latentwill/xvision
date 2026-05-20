---
track: list-migrate-eval-runs
lane: integration
wave: lists-v1
worktree: .worktrees/list-migrate-eval-runs
branch: task/list-migrate-eval-runs
base: origin/main
status: ready
depends_on: []                        # phase 1 already merged (#390, #395, #396)
blocks:
  - list-migrate-strategies           # 2b waits for 2a so call-site patterns are settled
  - list-migrate-decisions-and-tail   # 2c waits for 2b
stacking: none
allowed_paths:
  - frontend/web/src/routes/eval-runs.tsx
  - frontend/web/src/routes/eval-runs.test.tsx
  - frontend/web/src/lib/run-display.ts                       # tweaks allowed only if a filter/sort key needs a new accessor
  - frontend/web/src/api/eval.ts                              # only if useListUrlState surfaces a new query param requiring a typed accessor
forbidden_paths:
  - frontend/web/src/components/lists/**                      # phase 1 is locked; growth happens via a separate contract
  - frontend/web/src/components/primitives/ListPagination.tsx # do NOT delete in this contract — strategies + scenarios + agents still consume it
  - frontend/web/src/routes/strategies.tsx                    # 2b
  - frontend/web/src/routes/scenarios.tsx                     # 2c
  - frontend/web/src/routes/agents.tsx                        # 2c
  - crates/**                                                 # frontend only
interfaces_used:
  - "@/components/lists"                                      # ResponsiveListCard, useListState, useListUrlState, FilterDef, SortOption
  - "@/api/eval"                                              # listRunsPaged, evalKeys
  - "@/api/strategies"                                        # listStrategies, displayStrategyName
  - "@/api/scenarios"                                         # listScenarios, displayScenarioName
parallel_safe: false                                          # serial with 2b/2c per spec Decision 5
parallel_conflicts: []
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- routes/eval-runs
  - pnpm --dir frontend/web lint
acceptance:
  - **`/eval-runs` desktop renders inside `<ResponsiveListCard listId="eval-runs">`** — the current bespoke `<Card>`+`<Topbar>`+manual filter inputs+inline `<ListPagination>` shape is replaced by a single `<ResponsiveListCard>` mount. `<Topbar>` stays (it's chrome, not list body).
  - **`useListState` drives search + filters + sort** — filters: `strategy` (existing query param), `mode` (paper/live), `status` (queued/running/completed/failed/cancelled). Sort options: "Recently started" (default, `started_at DESC`), "Recently completed", "Strategy A-Z", "Status". Search box matches against the strategy display name and the short run id (`ShortRunId`). Server-side sort by `started_at DESC` is preserved as the source ordering; client-side sort re-keys the visible page.
  - **`useListUrlState("eval-runs", state)` is wired** — `?q=…&strategy=…&mode=…&status=…&sort=…` round-trips. Existing `?strategy=…` deep links keep working (the rename is a superset; old links still hydrate the `strategy` filter). The unrelated `?start=1` flag from `eval-runs.tsx:63` is untouched (spec line 437).
  - **`<ResponsiveListCard renderRow renderMobileRow>`** — desktop branch reuses the current `RunsTable` row shape (status pill, started_at, strategy/scenario, duration, cost, actions). Mobile branch ports the current mobile card to `<MListRow>` with: title=`displayStrategyName(...)`, badge=status pill colour (mapped to `MListRowBadgeColor`), subtitle=`displayScenarioName(...)`, meta=`evalRunDisambiguator`, rightTop=cost (4-sig-fig per F-9 rule), rightSub=relative time.
  - **Pagination consolidates onto `useListState` + `useServerPagination`** — current call site already uses `useServerPagination(totalFromServer)` from `@/components/primitives/ListPagination`. Keep that hook for the offset wiring (it owns the query-key `limit`/`offset` contract with the backend that #397 shipped) but render the controls via `<ListCard>`'s footer slot instead of a standalone `<ListPagination>` mount. End state: no `<ListPagination>` JSX in this route file.
  - **4-state body** — loading (skeleton rows), empty (with `<Link to="/eval-runs/new">New run</Link>` emptyAction), error (`<ApiError>` summary + retry), populated. All four states render inside `<ResponsiveListCard>` per spec Decision 7. Today's bespoke "no runs yet" copy is replaced by the standard empty state.
  - **Chart preview block stays put** — the "latest run chart" preview above the list is chrome, not list body. Untouched. The list card mounts directly below it.
  - **Capsule selection and `evalRunDisambiguator`** — `RunsTable` currently needs the full sibling pool (`allItems`) plus the paginated slice (`items`) to compute "Run #3/7" labels. Preserve that: pass `allRows` to `renderRow` separately or read from `useListState`'s `totalRows` + a pre-pagination accessor. No regression in the disambiguator output.
  - **Tests** — `eval-runs.test.tsx` adapts to the new mount. Existing test scenarios (filter by strategy, paginate, empty state) must still pass; add at least one test for URL state hydration (`?q=…&strategy=…` on mount → state populated → toolbar reflects).
  - **No regressions** — `pnpm --dir frontend/web test -- routes/eval-runs` passes. The 4 pre-existing failures called out on #397 (TraceDock, agent-runs-detail, two InlineEditField) remain pre-existing — do not introduce a 5th.
  - **No deletion of `ListPagination` primitive** — strategies, scenarios, agents still consume `ListPagination` until 2b/2c migrate them. Removing the primitive itself is the final cleanup step on 2c.

---

# Scope

Phase 2a of the standard list component wave (spec Decision 5,
`docs/superpowers/specs/2026-05-20-standard-list-component.md:358`).
Migrates `frontend/web/src/routes/eval-runs.tsx` from its bespoke
list layout to the phase-1 `<ResponsiveListCard>` + `useListState` +
`useListUrlState` stack. Lands the F-2 (search/filter) outcome from
QA Round 7 by wiring the `<ListToolbar>` against the existing eval
filter knobs.

The route currently composes: a `<Card>` wrapper, a manual filter row
that reads `?strategy` from `useSearchParams`, a `<RunsTable>` body,
and a `<ListPagination>` primitive footer. Backend pagination shipped
in #386/#397 — `useServerPagination` already drives `limit`/`offset`
through the TanStack key. This contract keeps the offset wiring but
moves search, filter, sort, and pagination *controls* under the
unified `<ListCard>` shape.

# Out of scope

- Strategies, scenarios, agents lists — those are 2b/2c.
- Removing the `frontend/web/src/components/primitives/ListPagination.tsx`
  primitive. Other routes still consume it. Final deletion is the
  closing edit of 2c.
- Backend API changes. `listRunsPaged` shape is locked.
- Adding new filter dimensions beyond what the route already supports.
  Phase 2a is a migration, not a feature add.
- Touching the `eval-runs-detail.tsx` inspector route or the mobile
  `eval-runs-detail-mobile.tsx`. Those are not list pages.
- Touching `eval-compare.tsx`. Not a list route.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/list-migrate-eval-runs status
git -C .worktrees/list-migrate-eval-runs log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/list-migrate-eval-runs -b task/list-migrate-eval-runs origin/main
```

# Notes

The spec assumes `<MListFilterPanel>` for mobile filters, but phase 1b
shipped `<MListSheet>` instead (the `CLAUDE.md` no-popups exemption
in #395 is scoped exactly to it). 2a uses `<MListSheet>` per the
shipped components, not the spec's earlier `<MListFilterPanel>`
name. Cross-checked the phase-1 archive
(`team/archive/2026-05-20-lists-v1-phase-1/`) before authoring.

`useServerPagination` lives at
`frontend/web/src/components/primitives/ListPagination.tsx` and owns
the offset/limit query-key contract introduced by #397. Importing
it into the new layout is fine; the goal is to stop rendering its
sibling `<ListPagination>` JSX, not to rip out the pagination state
hook.
