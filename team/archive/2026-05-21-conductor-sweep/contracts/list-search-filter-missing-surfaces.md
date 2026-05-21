---
track: list-search-filter-missing-surfaces
lane: integration
wave: docs-lists-metric-polish-2026-05-21
worktree: .worktrees/list-search-filter-missing-surfaces
branch: task/list-search-filter-missing-surfaces
base: origin/main
status: merged
depends_on:
  - list-search-filter-completion-audit                          # the audit defines the migration checklist
blocks: []
stacking: none
allowed_paths:
  # Worker picks routes/components from the audit's checklist. Recommended starting set:
  - frontend/web/src/routes/home.tsx                              # control-tower mini-lists if flagged by audit
  - frontend/web/src/routes/home.test.tsx
  - frontend/web/src/features/agent-runs/**                       # agent-runs / trace list views if flagged
  - frontend/web/src/components/chat/cards/**                     # chat-rail run/strategy/scenario lists if flagged
  - frontend/web/src/routes/settings/**                           # settings sub-lists if flagged
  - frontend/web/src/routes/providers.tsx                         # provider list if it qualifies
  - frontend/web/src/routes/brokers.tsx                           # broker list if it qualifies
  # Audit may surface more. Worker MUST update this contract via a contract-update PR before broadening allowed_paths.
forbidden_paths:
  - frontend/web/src/routes/eval-runs.tsx                         # covered by list-migrate-eval-runs (#399)
  - frontend/web/src/routes/strategies.tsx                        # covered by list-migrate-strategies (#400)
  - frontend/web/src/routes/scenarios.tsx                         # covered by list-migrate-decisions-and-tail
  - frontend/web/src/routes/agents.tsx                            # covered by list-migrate-decisions-and-tail
  - frontend/web/src/components/lists/**                          # phase-1 components locked; do not modify primitives here
  - frontend/web/src/components/primitives/ListPagination.tsx     # owned by list-migrate-decisions-and-tail (final deletion is theirs)
  - crates/**                                                     # frontend only
  - docs/superpowers/audits/**                                    # audit is read-only input here
interfaces_used:
  - "@/components/lists"                                          # ResponsiveListCard, useListState, useListUrlState, FilterDef, SortOption, MListCard, MListRow, MListSheet
parallel_safe: false                                              # depends on audit output
parallel_conflicts:
  - list-migrate-decisions-and-tail                               # if audit picks something this contract also covers, coordinate via team/queue/
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test --run
  - pnpm --dir frontend/web lint
acceptance:
  - **Audit-driven scope.** Worker reads `docs/superpowers/audits/2026-05-21-list-surfaces-audit.md` first. The PR description names every surface the worker migrated, and the audit's row for each migrated surface is updated to status "migrated in PR #<n>".
  - **Per-surface migration follows the phase-1 pattern.** Each migrated surface mounts `<ResponsiveListCard>` + drives state via `useListState` + (where URL persistence applies) `useListUrlState("<surface-id>", state)`. Mobile branch uses `<MListCard>` / `<MListRow>` / `<MListSheet>` per the spec.
  - **Every user-facing list has search and sort.** Per intake acceptance §`list-search-filter-missing-surfaces` bullet 1. If a surface has no natural search axis, the PR justifies the omission in code-comment or in the PR description, and the audit row records the rationale.
  - **Filters where meaningful.** Per intake acceptance bullet 2. If no filters are useful for a given list, the PR states why (in the audit row, not in inline comments).
  - **Recency sort default.** "Recently added" or equivalent is the default sort unless the route has a documented stronger default. Match the existing phase-2 pattern.
  - **Mobile filtering uses `<MListSheet>`.** Phone-breakpoint filters route through the operator-approved `<MListSheet>` bottom sheet (the only popup exemption in `CLAUDE.md`). No new modal/sheet primitives.
  - **No second list system.** Do not introduce a parallel toolbar/state machine. If the worker finds a list-like surface that resists the standard pattern, they file a contract-update note before forking.
  - **Tests cover at least one migrated surface end-to-end.** Existing test files extended; URL-state hydration + filter + sort + empty/loading/error states asserted on at least one of the migrated surfaces.
  - **No regressions.** `pnpm --dir frontend/web test --run` passes (modulo pre-existing failures, which the PR must enumerate and confirm were red on `origin/main` first).
  - **Audit feedback.** If the audit document was wrong about a surface (e.g. it categorized a non-list as a list), the PR amends the audit document in the same change. This is the one exception to the `docs/superpowers/audits/**` forbidden path — limited to row-status updates and corrections, not new sections.

---

# Scope

Track #3 of `team/intake/2026-05-21-docs-lists-metric-polish.md`.
Companion to the audit track (#2). Once the audit has identified every
list-like surface that lacks the standardized search/filter/sort
treatment, this contract migrates each of them — one PR per logical
group, batched at the worker's discretion — to the phase-1 list
component stack.

This is intentionally a broad contract. The audit defines the
checklist; the worker scopes batches against that checklist and may
split this into multiple PRs (each citing the audit row IDs they
close). If a batch grows to exceed reasonable PR size, the worker
opens a sub-contract via a contract-update PR rather than overgrowing
this one.

# Out of scope

- The four phase-2 routes (`eval-runs`, `strategies`, `scenarios`,
  `agents`). Already owned.
- Modifying the list-component primitives in
  `frontend/web/src/components/lists/**`. Phase 1 is locked.
- Deleting `ListPagination.tsx` JSX primitive — that's the final edit
  of `list-migrate-decisions-and-tail`.
- Building a new list system. Per intake §"Out of scope".
- Backend changes. List endpoints already returning `{items, total}`
  envelopes per #397; this is frontend-only.
- Surfaces that are explicitly "not a list" per the audit (the audit
  row's "migration decision = Not-a-list" must include the why).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/list-search-filter-missing-surfaces status
git -C .worktrees/list-search-filter-missing-surfaces log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/list-search-filter-missing-surfaces -b task/list-search-filter-missing-surfaces origin/main
```

# Notes

Contract status starts as `blocked` until the audit lands. The
conductor flips to `ready` when
`docs/superpowers/audits/2026-05-21-list-surfaces-audit.md` is on
`origin/main`.

The `allowed_paths` list is provisional — the audit may add surfaces
not anticipated here. Worker should expand `allowed_paths` via a
contract-update PR (reviewed by the conductor) before broadening
scope, not silently. Conversely, surfaces in `allowed_paths` that the
audit ruled out as not-a-list should be removed via the same
contract-update mechanism.

Don't bundle this with the docs UI track (`docs-ui-prototype-alignment`)
even if both touch the docs sidebar — keep concerns separable so
reviewers can reason about each independently.
