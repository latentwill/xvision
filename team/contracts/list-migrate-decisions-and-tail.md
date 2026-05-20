---
track: list-migrate-decisions-and-tail
lane: integration
wave: lists-v1
worktree: .worktrees/list-migrate-decisions-and-tail
branch: task/list-migrate-decisions-and-tail
base: origin/main
status: ready
depends_on:
  - list-migrate-strategies
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/scenarios.tsx
  - frontend/web/src/routes/agents.tsx
  - frontend/web/src/components/primitives/ListPagination.tsx   # final deletion happens here
  - frontend/web/src/components/primitives/ListPagination.test.tsx
forbidden_paths:
  - frontend/web/src/routes/eval-runs.tsx                     # 2a
  - frontend/web/src/routes/strategies.tsx                    # 2b
  - frontend/web/src/routes/scenarios-detail.tsx              # detail view; the Runs tab is a tab-internal list — out of scope unless trivial
  - frontend/web/src/routes/agents-edit.tsx
  - frontend/web/src/components/lists/**
  - crates/**
interfaces_used:
  - "@/components/lists"
  - "@/api/scenarios"
  - "@/api/agents"
parallel_safe: false                                          # final integration; serial after 2a + 2b
parallel_conflicts: []
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- routes/scenarios routes/agents
  - pnpm --dir frontend/web lint
acceptance:
  - **Scenarios list (`/scenarios`)** — migrated to `<ResponsiveListCard listId="scenarios">` mirroring the 2a/2b patterns. Sort default "Recently added" (backend already DESC). Filters scoped to what `ListScenariosFilter` already exposes (symbol, timeframe, regime label per #360).
  - **Agents list (`/agents`)** — migrated to `<ResponsiveListCard listId="agents">`. Sort default "Recently updated" (backend already DESC). Filters at minimum on agent role, and on provider where it's surfaceable from the list payload.
  - **`useListUrlState` wired for both** — `?q=…&<filter>=…&sort=…`.
  - **`<ListPagination>` primitive deletion** — once `routes/scenarios.tsx` + `routes/agents.tsx` stop importing it, delete `frontend/web/src/components/primitives/ListPagination.tsx` and its test file. `useServerPagination` (the hook) stays — it's the shared offset/limit driver for `<ListCard>`'s footer slot across all four migrated routes. If `useServerPagination` lives in the same file as the deleted JSX component, lift it into a sibling `useServerPagination.ts` first so the unused JSX is the only thing that goes.
  - **No remaining `<ListPagination>` JSX imports** — `rg "from \"@/components/primitives/ListPagination\"" frontend/web/src/` returns at most the `useServerPagination` re-export site (zero hits if the hook moved out).
  - **Decisions / Trade Ledger / Open Positions / Journal** — the spec's "tail" routes don't exist as standalone list pages in the current SPA; revisit only if they're added during this contract's lifetime. Document the current absence in the contract Notes section before opening the PR.
  - **4-state body** for both lists — loading, empty (with appropriate emptyAction `<Link to="/scenarios/new">`, etc.), error, populated.
  - **Tests** — adapt both route tests; add URL-hydrate + filter coverage.
  - **No regressions** — `pnpm --dir frontend/web test` overall delta: 0 new failures vs. the 4 pre-existing on main.

---

# Scope

Phase 2c, the closing integration of the standard list component
wave (spec Decision 5,
`docs/superpowers/specs/2026-05-20-standard-list-component.md:360`).
Migrates the remaining xvision SPA list routes (scenarios, agents)
to `<ResponsiveListCard>` and deletes the standalone
`<ListPagination>` JSX primitive that #386/#397 introduced as a
transitional shim.

The spec named "Decisions, Trade Ledger, Open Positions, Journal" as
the tail, but those routes don't exist in the current SPA. The
actual tail today is `scenarios.tsx` + `agents.tsx`. If a Decisions
route lands during this contract's lifetime (e.g. via a V2B follow-
up), expand scope via a contract-update PR before touching it.

# Out of scope

- Detail-page tab lists (`scenarios-detail.tsx` Runs tab,
  `strategies-detail.tsx` tabs). Those are tab-internal and may
  warrant a separate cleanup contract. Carve out at start of 2c
  only if any tab list is structurally identical to the route-level
  pattern and trivially migrates.
- Backend changes.
- Phase 3 (`list-component-density-toggle`) remains deferred.
- Re-architecting `useServerPagination` if it gets lifted into its
  own file — the lift is a rename, not a refactor.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/list-migrate-decisions-and-tail status
git -C .worktrees/list-migrate-decisions-and-tail log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/list-migrate-decisions-and-tail -b task/list-migrate-decisions-and-tail origin/main
```

# Notes

The contract's `allowed_paths` includes
`frontend/web/src/components/primitives/ListPagination.tsx` because
the closing edit of 2c is its deletion. 2a and 2b explicitly forbid
that path — only 2c may touch it, and only after the last consumer
migrates.

Phase-1 archive: `team/archive/2026-05-20-lists-v1-phase-1/`. The
file layout under `frontend/web/src/components/lists/` is locked
and not editable by this contract.
