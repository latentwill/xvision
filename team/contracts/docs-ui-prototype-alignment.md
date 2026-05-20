---
track: docs-ui-prototype-alignment
lane: leaf
wave: docs-lists-metric-polish-2026-05-21
worktree: .worktrees/docs-ui-prototype-alignment
branch: task/docs-ui-prototype-alignment
base: origin/main
status: ready
depends_on: []
blocks:
  - docs-search-list-component-adoption                          # the optional sidebar-as-list followup depends on the prototype alignment landing first
stacking: none
allowed_paths:
  - frontend/web/src/routes/docs/index.tsx                       # docs route layout
  - frontend/web/src/routes/docs/index.test.tsx                  # if it exists; add presentation-state assertions
  - frontend/web/src/features/docs/DocsMarkdown.tsx              # markdown reader styling
  - frontend/web/src/features/docs/DocsMarkdown.test.tsx
  - frontend/web/src/features/docs/DocsSidebar.tsx               # if it exists; sidebar treatment
  - frontend/web/src/features/docs/**                            # other co-located docs feature components
  - frontend/web/src/routes/docs/docs.css                        # if a route-local stylesheet exists
forbidden_paths:
  - frontend/prototype/**                                        # prototype is a read-only reference; do not modify
  - frontend/web/src/api/docs.ts                                 # API stays stable
  - frontend/web/src/theme/themes.ts                             # use existing folio-dark tokens; do not add docs-specific tokens here
  - crates/xvision-dashboard/src/routes/docs/**                  # backend content/index — owned by docs-user-and-agent-wiki intake
  - frontend/web/src/components/lists/**                         # phase-1 list components locked
  - frontend/web/src/routes/docs/content/**                      # docs content is owned by the docs-user-and-agent-wiki intake (`team/intake/2026-05-20-docs-user-and-agent-wiki.md`)
interfaces_used:
  - "@/api/docs"                                                 # whatever the docs route already consumes — unchanged
  - existing folio-dark theme tokens                             # see frontend/web/src/theme/themes.ts
parallel_safe: true                                              # presentation-only; no overlap with other active tracks
parallel_conflicts: []
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- routes/docs
  - pnpm --dir frontend/web test -- features/docs
  - pnpm --dir frontend/web lint
acceptance:
  - **All existing behaviors preserved.** `?slug=<slug>` deep linking, sidebar/index filtering, section headers, loading/empty/error/populated states all continue to work. Existing tests pass without behavioural changes — this is presentation only.
  - **Folio-dark visual alignment.** `/docs` reads as part of the folio-dark dashboard surface, not a generic two-column form. Prototype reference: `frontend/prototype/styles.css`, `frontend/prototype/shared.jsx`, `frontend/prototype/screen-*.jsx`. Bring typography, surface spacing, sidebar treatment, active-state highlighting, and code-block styling into the same visual language as the rest of the SPA. Use the existing folio-dark theme tokens — do not invent new docs-specific tokens.
  - **Markdown reader ergonomics.** Comfortable measure (line length), clear heading hierarchy, code-block styling that matches the prototype's code treatment, table styling, link treatment (visited + hover states). The reader area is not full-width on wide displays — the prototype's content max-width applies.
  - **Mobile/tablet behavior is explicit.** Side-by-side desktop layout collapses cleanly to a stacked layout at the SPA's standard breakpoints. No text overlap, no clipped controls, no horizontal scroll. Match the existing dashboard responsive patterns (lists v1, agent-runs detail, eval-runs detail).
  - **No popups.** Per `CLAUDE.md` no-popups rule. The docs navigation stays inline / docked — no modal/sheet/popover sidebar even on mobile. If mobile requires a collapsing nav, use an accordion or inline drawer that doesn't steal focus. `<MListSheet>` exemption does not apply here — that's scoped to list filters.
  - **Behavior tests cover the four states.** Existing tests (or new ones) assert: (a) loading state renders, (b) empty state renders, (c) error state renders, (d) populated state renders with at least one section + at least one slug. If the existing tests already cover these, just verify they still pass after the visual rework.
  - **No content changes.** Docs content sits under `frontend/web/src/routes/docs/content/` and `crates/xvision-dashboard/src/routes/docs/` — both forbidden here. Content drift is owned by the `2026-05-20-docs-user-and-agent-wiki.md` intake.
  - **No API changes.** `frontend/web/src/api/docs.ts` stays as-is. If the worker needs a new field from the backend, escalate to the docs-user-and-agent-wiki intake before opening a contract-update PR here.

---

# Scope

Track #1 of `team/intake/2026-05-21-docs-lists-metric-polish.md`. The
`/docs` route already has the right behaviors — deep-linkable
`?slug=`, sidebar search, section grouping, loading/empty/error
handling, markdown rendering — but the presentation reads as a generic
two-column card. The shipped prototype (`frontend/prototype/`)
defines the folio-dark visual language for the rest of the dashboard;
docs has not been brought into alignment.

This is a presentation pass: keep behavior, replace the styling.

Intake §"Docs UI" is explicit: *"This intake should not reopen docs
content scope. Keep content refresh and wiki manifest work in the
existing 2026-05-20 docs intake. This wave is route presentation and
reader ergonomics."*

# Out of scope

- Docs content (owned by `2026-05-20-docs-user-and-agent-wiki.md`).
  All markdown files under `frontend/web/src/routes/docs/content/` and
  `crates/xvision-dashboard/src/routes/docs/` are forbidden.
- Wiki manifest plumbing or backend changes. Same other intake.
- Adopting the standard list component for the sidebar. That's the
  optional P2 followup `docs-search-list-component-adoption`.
- New theme tokens. Use what's in `frontend/web/src/theme/themes.ts`.
- Mobile-only redesign that introduces popups/sheets/drawers
  for navigation. Per CLAUDE.md no-popups rule.
- Public marketing docs site or any externally-rendered build.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/docs-ui-prototype-alignment status
git -C .worktrees/docs-ui-prototype-alignment log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/docs-ui-prototype-alignment -b task/docs-ui-prototype-alignment origin/main
```

# Notes

Prototype reference files (read-only):

- `frontend/prototype/README.md` — the visual treatment summary
- `frontend/prototype/styles.css` — token definitions and shared
  layout primitives (these don't all map 1:1 to the SPA's existing
  theme; the worker picks the closest existing token)
- `frontend/prototype/shared.jsx` — the chrome/topbar pattern
- `frontend/prototype/screen-*.jsx` — per-screen layouts to mine for
  heading/spacing/active-state idioms

The shipped SPA tokens live in `frontend/web/src/theme/themes.ts`. If
the prototype uses a CSS variable the SPA doesn't expose, pick the
nearest mapped token rather than introducing a new one. Adding tokens
is out-of-scope for this contract — note the gap in the PR description
and the conductor will decide whether a token-extension contract is
warranted.
