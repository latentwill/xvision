---
track: list-migrate-strategies
lane: integration
wave: lists-v1
worktree: .worktrees/list-migrate-strategies
branch: task/list-migrate-strategies
base: origin/main
status: merged
depends_on:
  - list-migrate-eval-runs            # 2a must merge first so the migration pattern is settled
blocks:
  - list-migrate-decisions-and-tail
stacking: none
allowed_paths:
  - frontend/web/src/routes/strategies.tsx
  - frontend/web/src/routes/strategies.test.tsx
  - frontend/web/src/api/strategies.ts                        # only if useListUrlState surfaces a new query param requiring a typed accessor
forbidden_paths:
  - frontend/web/src/components/lists/**
  - frontend/web/src/components/primitives/ListPagination.tsx
  - frontend/web/src/routes/eval-runs.tsx                     # 2a
  - frontend/web/src/routes/scenarios.tsx                     # 2c
  - frontend/web/src/routes/agents.tsx                        # 2c
  - frontend/web/src/routes/strategies-detail.tsx             # detail view, not a list
  - frontend/web/src/routes/strategies-new.tsx                # authoring flow, not a list
  - crates/**
interfaces_used:
  - "@/components/lists"
  - "@/api/strategies"
parallel_safe: false                                          # serial with 2a/2c
parallel_conflicts: []
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- routes/strategies
  - pnpm --dir frontend/web lint
acceptance:
  - **`/strategies` desktop renders inside `<ResponsiveListCard listId="strategies">`** — the bespoke filter+pagination layout is replaced by `<ResponsiveListCard>`. `<Topbar>` stays.
  - **`useListState` drives search + filters + sort** — filters at minimum: a "Pipeline shape" filter sourced from the agents composition (`Strategy.agents[].role`), e.g. trader-only vs multi-agent, with the exact filter set finalised during implementation against the current `StrategyListItem` shape. Sort options: "Recently added" (default — backend sort already DESC per #386), "Name A-Z", optionally "Most runs". Search matches `display_name` and a portion of the ULID.
  - **`useListUrlState("strategies", state)` is wired** — `?q=…&shape=…&sort=…` round-trips via `react-router-dom`'s `useSearchParams`.
  - **Mobile branch** — `<MListRow>` row: title=`display_name`, badge=pipeline shape colour, subtitle=agent count + provider summary, meta=created_at relative, rightTop=last-run cost/result chip when applicable.
  - **Pagination** — keep `useServerPagination` for offset/limit (the `{items,total}` envelope from #397 is canonical). Render controls via `<ListCard>` footer; no standalone `<ListPagination>` JSX.
  - **4-state body** — loading skeleton, empty with `<Link to="/strategies/new">New strategy</Link>` emptyAction, error with `<ApiError>` + retry, populated.
  - **Tests** — `strategies.test.tsx` adapts. At minimum: filter/sort/url-hydrate test, empty state, list renders.
  - **No regressions** — `pnpm --dir frontend/web test -- routes/strategies` clean. No new entries in the pre-existing-failures list.
  - **No deletion of `ListPagination`** — scenarios + agents still consume it.

---

# Scope

Phase 2b of the standard list component wave (spec Decision 5).
Migrates `frontend/web/src/routes/strategies.tsx` to
`<ResponsiveListCard>` + `useListState` + `useListUrlState`. Mirrors
the 2a migration patterns to keep the call sites consistent.

# Out of scope

- Strategy authoring (`strategies-new.tsx`) — flow, not a list.
- Strategy detail view (`strategies-detail.tsx`) — has tab-internal
  lists; revisit in 2c only if any single tab list rises to the
  surface.
- Backend changes. `listStrategiesPaged` envelope from #397 is locked.
- Adding new filter dimensions beyond what the current page exposes
  unless the migration to `<ListToolbar>` makes one trivial.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/list-migrate-strategies status
git -C .worktrees/list-migrate-strategies log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/list-migrate-strategies -b task/list-migrate-strategies origin/main
```

# Notes

Sequenced after 2a so the call-site idioms (where `useListState` is
constructed, where `useListUrlState` is wired, how
`useServerPagination` interleaves with `<ListCard>`'s footer slot)
are settled. Lift the 2a pattern verbatim; don't reinvent.
