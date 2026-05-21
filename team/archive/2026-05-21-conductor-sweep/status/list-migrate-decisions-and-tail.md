---
track: list-migrate-decisions-and-tail
contract: team/contracts/list-migrate-decisions-and-tail.md
status: ready-for-review
owner: claude-opus-4-7
claimed_at: 2026-05-21
worktree: .worktrees/list-migrate-decisions-and-tail
branch: task/list-migrate-decisions-and-tail
---

# Status

## 2026-05-21 — execution

Claimed the closing track of the lists-v1 wave (phase 2c). Branch
`task/list-migrate-decisions-and-tail` was already created from
`origin/main` (last commit `a37af89 lists(2a): migrate /eval-runs…`).
2b (`list-migrate-strategies`) had not landed at branch-cut, so the
forbidden-path constraint on `routes/strategies.tsx` had to be relaxed
for the cross-cutting hook lift (see "Forbidden-path note" below).

### Migration summary

- **`routes/scenarios.tsx`** — migrated to `<ResponsiveListCard
  listId="scenarios">` with `useListState` + `useListUrlState`.
  Toolbar exposes:
  - search (matches `display_name` + asset symbols + granularity over
    the currently-paged slice)
  - Source filter (`any`/`canonical`/`user`/`clone`/`generated`)
  - Archived filter (`exclude`/`include`) — the only filter that
    round-trips to the backend via `ListScenariosFilter.include_archived`
  - Sort: `Recently added` (default, preserves backend DESC),
    `Name A → Z`, `Name Z → A`
  - 4-state body (loading / empty / error / populated) wired through
    `<ResponsiveListCard>` props.
  - Mobile branch renders `<MListRow>` with source-pill colouring.

- **`routes/agents.tsx`** — migrated to `<ResponsiveListCard
  listId="agents">`. Toolbar exposes:
  - search (matches `name` + `description` + `tags`)
  - Shape filter (`all`/`single`/`multi`) — agent role is free text
    per `AgentRef` inside `Strategy`, so we can't filter by role from
    the agents list alone; shape (single-slot vs. multi-slot) is the
    next-best categorical filter.
  - Archived filter (`exclude`/`include`) — round-trips to backend.
  - Sort: `Recently updated` (default), `Name A → Z`, `Name Z → A`.
  - Mobile branch uses `<MListRow>` with the Draft/Validated/In use/
    Archived status pill colours.

- **Backend pagination** — both routes keep `useServerPagination` for
  `limit`/`offset` and render a thin pager strip below the
  `<ResponsiveListCard>` (same carve-out as 2a — `<MListCard>` has no
  footer slot so the pager lives outside the card).

- **URL state** — `useListUrlState` writes `?q=…&source=…&archived=…
  &shape=…&sort=…` for both routes. Both routes also bridge the
  backend-relevant filter (`archived`, plus `source` on scenarios) from
  list state → local state so the TanStack Query key refetches when
  the user flips the filter.

### `<ListPagination>` primitive deletion

- Lifted `useServerPagination` (and `DEFAULT_PAGE_SIZE` /
  `PAGE_SIZE_OPTIONS`) into a new sibling file
  `frontend/web/src/components/primitives/useServerPagination.tsx`.
- Migrated the JSX (formerly `<ListPagination>`) into a new
  `<ServerPagerStrip>` export in the same file. Same visual UI,
  renamed because the data-flow is "drive the strip from
  `useServerPagination`" — keeping the primitive next to the hook
  removes a layer of indirection.
- Deleted `frontend/web/src/components/primitives/ListPagination.tsx`.
  No test file existed for it (the old TSX shipped without one).
- Updated all four list-route importers
  (`scenarios.tsx`, `agents.tsx`, `eval-runs.tsx`, `strategies.tsx`)
  to import from `useServerPagination.tsx` and to render
  `<ServerPagerStrip>` instead of `<ListPagination>`. Verification:

  ```
  $ rg "<ListPagination" frontend/web/src/
  (no matches)
  $ rg "from \"@/components/primitives/ListPagination\"" frontend/web/src/
  (no matches)
  ```

- Acceptance criterion `<ListPagination> primitive deletion` and the
  related `rg` check are met.

### Decisions / Trade Ledger / Open Positions / Journal (spec tail)

These routes named in
`docs/superpowers/specs/2026-05-20-standard-list-component.md:360`
**do not exist as standalone list pages in the current SPA.**
Per-cycle decisions are surfaced inside `eval-runs-detail.tsx` and
`agent-runs-detail.tsx`; there is no `/decisions`, `/positions`,
`/ledger`, or `/journal` route in `frontend/web/src/routes.tsx`. The
contract carves this out explicitly — "the actual tail today is
`scenarios.tsx` + `agents.tsx`" — and asks me to document the
absence here before opening the PR. Done.

If a Decisions or Journal route ships during a v2 follow-up, the
migration pattern is fully reusable; expand scope via a contract-
update PR before touching it.

### Forbidden-path note

The contract's `forbidden_paths` includes `routes/eval-runs.tsx`
(owned by 2a, already merged) and `routes/strategies.tsx` (owned by
2b, not yet merged). The user-supplied step-6 cleanup instruction
explicitly authorises updating "all four importers (the 4 list route
files)" so the `<ListPagination>` JSX could be deleted. That cleanup
makes one-line import-path changes plus a `<ListPagination>` →
`<ServerPagerStrip>` rename in those two routes. No semantic change,
no test impact (eval-runs.test.tsx still passes 14/14, and there is
no strategies route-level test on origin/main).

If 2b merges before this PR, the rebase will be a straight import-path
collision on those two lines; mark `theirs` for any edits 2b made to
those routes.

### Regime-label filter (contract item, deferred)

The contract called for "Filters scoped to what `ListScenariosFilter`
already exposes (symbol, timeframe, regime label per #360)". The
`ListScenariosFilter` ts-rs type currently exposes `source`, `tags`,
`include_archived`, `parent_scenario_id`, `limit`, `offset` — no
`regime` field. PR #360 (mentioned in the contract) appears not to
have landed the regime-label backend filter; the SPA toolbar covers
the available dimensions (source + archived + free-text search across
symbol + granularity). When the engine grows a regime-label filter
field, drop it into `SOURCE_FILTER`/`ARCHIVED_FILTER` siblings — the
toolbar wiring handles arbitrary `FilterDef`s.

### Verification

```
$ pnpm --dir frontend/web typecheck
> tsc -b   (clean)

$ pnpm --dir frontend/web test -- routes/scenarios routes/agents --run
✓ src/routes/agents.test.tsx (6 tests) 162ms
✓ src/routes/scenarios.test.tsx (5 tests) 201ms
✓ src/routes/scenarios-detail.test.tsx (7 tests) 206ms
Test Files  3 passed (3)
Tests       18 passed (18)

$ pnpm --dir frontend/web test
Test Files  4 failed | 74 passed (78)
Tests       5 failed | 630 passed (635)
```

The 5 remaining failures are the pre-existing ones tracked in the
intake brief: `agent-runs-detail.test.tsx > inspector selection
fallback`, `MarkdownView.test.tsx > does not render raw HTML`,
`TraceDock.test.tsx > inspector selection fallback`, and two
`InlineEditField.test.tsx > StrategyDetailRoute (edit cycle stability)`
cases. Out of scope per the contract; not touched.

`pnpm lint` is not wired in `frontend/web/package.json` (no `lint`
script). Skipped.

### Tests added

- `frontend/web/src/routes/scenarios.test.tsx` — 5 tests covering
  empty state CTA, populated row rendering, backend-filter
  passthrough, URL hydration, and live search filter.
- `frontend/web/src/routes/agents.test.tsx` — 6 tests covering empty
  state CTA, populated row rendering, backend-filter passthrough, URL
  hydration, live search filter, and shape-filter URL hydration.

Both files install `stubMatchMediaDesktop()` (copied from
`eval-runs.test.tsx`) so jsdom can mount `<ResponsiveListCard>` (which
calls `window.matchMedia` via `useViewportMode`).

## Open follow-ups (out-of-scope for 2c)

- `ListPagination` primitive comments still appear in `api/agents.ts`
  and `api/eval.ts` JSDoc explaining the `total` field. Plain prose,
  no symbol references — safe to leave for an opportunistic sweep.
- Regime-label filter on scenarios — see #360 status above.
