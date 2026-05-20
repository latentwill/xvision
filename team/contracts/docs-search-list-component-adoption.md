---
track: docs-search-list-component-adoption
lane: leaf
wave: docs-lists-metric-polish-2026-05-21
worktree: .worktrees/docs-search-list-component-adoption
branch: task/docs-search-list-component-adoption
base: origin/main
status: deferred                                                  # P2 optional follow-up; only opens if docs-ui-prototype-alignment plus the audit confirm the sidebar is complex enough to warrant the list component
depends_on:
  - docs-ui-prototype-alignment                                   # the prototype alignment must land first; this builds on top
  - list-search-filter-completion-audit                           # the audit determines whether docs nav qualifies as a "list"
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/features/docs/DocsSidebar.tsx                # if it exists; the sidebar list
  - frontend/web/src/features/docs/DocsSidebar.test.tsx
  - frontend/web/src/routes/docs/index.tsx                        # mount-site changes only
forbidden_paths:
  - frontend/web/src/features/docs/DocsMarkdown.tsx               # reader styling is owned by docs-ui-prototype-alignment
  - frontend/web/src/components/lists/**                          # phase-1 primitives locked
  - frontend/web/src/routes/docs/content/**                       # content owned by docs-user-and-agent-wiki intake
  - frontend/web/src/api/docs.ts
  - crates/**
interfaces_used:
  - "@/components/lists"                                          # ResponsiveListCard, useListState (lightweight subset — see Notes)
parallel_safe: true
parallel_conflicts: []
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- features/docs/DocsSidebar
  - pnpm --dir frontend/web lint
acceptance:
  - **Conductor gates this contract.** Status starts at `deferred`. The conductor flips to `ready` only after (a) `docs-ui-prototype-alignment` lands AND (b) the audit doc concludes docs navigation qualifies as a list-like surface AND the operator confirms the sidebar is complex enough to warrant the standard treatment.
  - **Docs sidebar adopts the list component shape — lightly.** If activated, the sidebar uses the same search/filter-chip primitives the rest of the SPA uses (so the visual treatment is unified) without converting docs navigation into a heavy data table. The intake calls this out: *"adopt the same list search/chip visual treatment for docs navigation without turning docs into a heavy data table."*
  - **Search remains keyboard-first.** Docs operators tab through sections; the existing keyboard behavior must keep working after the migration. No regressions on `?slug=` deep-linking.
  - **No popups.** Mobile docs nav remains inline/accordion per `docs-ui-prototype-alignment` decisions. `<MListSheet>` exemption does not apply.
  - **One test added.** A new or extended test asserts: search hydrates from URL state if URL state is added, otherwise asserts the section grouping is preserved.
  - **Backout-friendly.** If the visual outcome is worse than the prototype-aligned sidebar from track #1, the worker reverts. P2 means optional.

---

# Scope

Track #5 of `team/intake/2026-05-21-docs-lists-metric-polish.md`. P2,
**optional follow-up**. Once `docs-ui-prototype-alignment` lands and
the audit document confirms docs navigation behaves enough like a
list-like surface to benefit from the shared list-component treatment,
this track adopts the same search/chip visual idiom for the docs
sidebar — without turning docs nav into a heavy data table.

# Out of scope

- Markdown reader styling. Owned by `docs-ui-prototype-alignment`.
- Docs content. Owned by `docs-user-and-agent-wiki` intake.
- A new list system. Phase-1 components must be used as-is.
- Backend changes.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/docs-search-list-component-adoption status
git -C .worktrees/docs-search-list-component-adoption log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/docs-search-list-component-adoption -b task/docs-search-list-component-adoption origin/main
```

# Notes

P2 = optional. The intake explicitly frames this as
*"Optional follow-up if the docs sidebar page list is complex enough."*
If the audit concludes the docs nav has < ~20 entries and uses
section grouping that doesn't benefit from filter chips, this contract
gets archived without ever leaving `deferred` status.

When/if activated, the worker should use a lightweight subset of the
list component (just the search input + chips, not a paginated table
shell). The goal is visual cohesion with the rest of the SPA, not
forcing docs into a list paradigm where it doesn't fit.
