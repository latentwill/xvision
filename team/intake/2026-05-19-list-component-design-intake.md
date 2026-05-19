# Intake — 2026-05-19 — Standardized list component (design handoff from `FilterSearchLists.zip`)

Design package dropped at `docs/design/FilterSearchLists.zip`
(operator request 2026-05-19). Asks for a **planning intake** — the
output is a spec under `docs/superpowers/specs/` before any contracts
open. This intake captures scope, what's in the package, what's
in-scope vs reference-only, the open questions a spec needs to
answer, and the suggested decomposition into contract-sized tracks.

## Source

- Package: `docs/design/FilterSearchLists.zip` (16 files, ~233 KB).
- README inside the bundle: `design_handoff_lists/README.md`
  ("Handoff: Standard List Component (Desktop + Mobile)").
- Operator framing: "Intake the SearchFilterLists.zip design
  component for lists and turn into intake for plan writing."
- Fidelity declared in the README: **High-fidelity (hifi). Final
  colors, typography, spacing, interaction states, and component
  anatomy are all locked. Implementation should be pixel-equivalent.
  Use the exact tokens, fonts, and measurements documented below.**

## Scope (one-line framing)

A single standardized list component that replaces ad-hoc
search/filter/sort UI scattered across xvision SPA list pages
(Strategies, Eval Runs, Decisions, Trade Ledger, Open Positions,
Journal). Two densities (`full`, `compact`), two form factors
(desktop, mobile). Search always available; "Recently added" is the
default sort everywhere.

## Package contents — in-scope vs reference-only

Per the workspace memory rule (design packages: components only,
context files are reference), the component files are in scope; the
page-layout demos and chrome are reference.

**In-scope component files** (port to React/TS):

| File | What it is | xvision target |
|---|---|---|
| `list-toolbar.jsx` | Desktop component: `<ListCard>`, `<ListToolbar>`, `<ListActiveChips>`, `useListState()` (~440 lines incl. styles) | `frontend/web/src/components/lists/ListCard.tsx` + siblings |
| `list-toolbar-mobile.jsx` | Mobile component: `<MListCard>`, `<MListRow>`, `<MListSheet>` (reuses `useListState`) | `frontend/web/src/components/lists/MListCard.tsx` + siblings |
| `shared.jsx` | `<Icon>` component used by both — replace with project's icon lib | Drop; use the existing icon source in `frontend/web/src/components/` |
| `styles.css` | Design tokens (CSS custom properties on `:root`) — colors, typography, spacing, radii | Port to Tailwind theme / CSS-vars (compare against existing token set in `frontend/web/src/styles/`) |
| `mobile-styles.css` | Mobile chrome (topbar, drawer, sheets) — referenced by the **mobile canvas only**, NOT by the production component (per README) | Reference only, do not port |

**Reference-only files** (DO NOT port — these are canvas / preview chrome):

- `Lists.html`, `Lists Mobile.html` — design canvases
- `design-canvas.jsx`, `ios-frame.jsx` — preview frame
- `list-anatomy.jsx`, `list-variants.jsx`, `list-mobile-anatomy.jsx` — spec diagrams
- `list-screens.jsx`, `list-mobile-screens.jsx` — full-page demos showing the lists in a sidebar+topbar shell
- `list-examples.jsx` — desktop wiring examples (use as a wiring reference when slotting the component into real xvision routes, but the file itself isn't ported)

Per the user's standing rule on intaking design packages (memory
`feedback_design_package_components_only.md`): page-layout demos
(`*-context.jsx`, `*-canvas.jsx`, chrome from `shared.jsx`) are
reference, never scope.

## Component contract (from the README)

- Search always available.
- Filters are domain-specific but use uniform UI.
- Sort always available; "Recently added" is option 1 and the default everywhere.
- Active filter state surfaces as removable chips.
- Two densities: `full` (primary pages) and `compact` (dashboard mini-lists).
- Two form factors: desktop (toolbar above a table) and mobile (toolbar above card-style rows, filters in a bottom sheet).

## What's locked vs what the spec must decide

**Locked by the design handoff** (per README; high-fi):

- Token system (colors, typography, spacing) — `styles.css` is the source of truth.
- Component anatomy: `ListCard` / `ListToolbar` / `ListActiveChips` (desktop); `MListCard` / `MListRow` / `MListSheet` (mobile).
- Interaction states (hover, focus, active filter chip removal).
- Mobile bottom-sheet behavior for filters.

**Open questions for the spec stage** (must be answered before contracts open):

1. **Token reconciliation.** xvision already has a design-token system
   (warm-dark gold-accent theme — see `MEMORY.md` entry on dark-mode
   borders and `docs/design/themes.md`). The handoff's tokens are
   close but not identical. Spec must decide: (a) overwrite xvision's
   tokens with the handoff's, (b) merge selectively, or (c) keep
   xvision's and surface any visual delta as design feedback to the
   handoff author. **Recommendation:** (b) merge selectively. The
   gold accent is the same family; reconcile only where the handoff
   adds a token xvision doesn't have (e.g. `--surface-elev`).

2. **`useListState` API surface.** The handoff defines a hook that
   owns search, filter, sort, and chip state. The spec must define
   the prop contract for `<ListCard rows={…} columns={…}>` so the
   host page can wire real data (TanStack Query results) without
   pulling state up into a parent.

3. **Migration order.** Which lists migrate first? Recommended
   sequencing:
   - Phase 1 — Eval Runs list (the highest-traffic surface; the
     handoff includes an "Eval Runs V2" reference in
     `list-examples.jsx`).
   - Phase 2 — Strategies list (similar shape).
   - Phase 3 — Decisions / Trade Ledger / Open Positions / Journal.
   Per-list contracts so each migration ships independently.

4. **Mobile parity.** The handoff ships mobile components separately
   (`list-toolbar-mobile.jsx`, distinct from desktop). xvision's
   current mobile routes (`*-mobile.tsx`) already render bespoke
   mobile shapes. Spec must decide whether each route's mobile
   variant adopts `MListCard` directly or whether a thin
   `<ResponsiveListCard>` wrapper picks desktop vs mobile under one
   breakpoint check. **Recommendation:** the wrapper — keeps the
   host page from having to fork mobile/desktop call sites.

5. **Backward compatibility.** xvision's current lists have their
   own search / filter / sort UIs (see e.g.
   `frontend/web/src/routes/eval-runs.tsx`, `:strategies.tsx`,
   `:decisions.tsx`). Spec must decide whether the migration is
   one-shot (rip and replace per list) or shimmed (both shapes
   coexist behind a feature flag). **Recommendation:** rip and
   replace per phase; xvision's UI is small enough and the handoff
   is hifi.

6. **Empty state, loading state, error state.** The handoff covers
   "2 states" on mobile but doesn't enumerate which two; check
   `list-mobile-screens.jsx`. Spec confirms a 3-state contract:
   loading skeleton, empty state with primary CTA, error state with
   retry.

7. **Density toggle persistence.** `full` vs `compact` density —
   per-list, per-user, or session-only? **Recommendation:** per-list,
   localStorage-backed.

## Suggested decomposition (draft — spec finalizes)

| Phase | Track | Scope |
|---|---|---|
| 0 | `list-component-spec` | Spec under `docs/superpowers/specs/<date>-standard-list-component.md` resolving the seven open questions above. |
| 1a | `list-component-port-desktop` | Port `list-toolbar.jsx` → `frontend/web/src/components/lists/ListCard.tsx` + siblings + Vitest. No call-site changes. |
| 1b | `list-component-port-mobile` | Port `list-toolbar-mobile.jsx` → `MListCard.tsx` + siblings + Vitest. No call-site changes. |
| 1c | `list-component-tokens-reconcile` | Merge the handoff's CSS variables into xvision's token set. No call-site changes. |
| 2a | `list-migrate-eval-runs` | Replace `/eval-runs` desktop + mobile lists with the new component. |
| 2b | `list-migrate-strategies` | Same for `/strategies`. |
| 2c | `list-migrate-decisions-and-tail` | Decisions, Trade Ledger, Open Positions, Journal — likely one bundled track unless any one of them surfaces non-trivial complexity in the spec stage. |
| 3 | `list-component-density-toggle` | Per-list `full` / `compact` toggle with localStorage persistence. Likely folds into 1a; carve out if it grows. |

Phase 0 is the gate. Phases 1a/1b/1c can land in parallel after the
spec. Phase 2 tracks ship serially per list. Phase 3 is optional and
may fold into Phase 1a.

## Verbatim ask

> 2) Intake the SearchFilterLists.zip design component for lists and
>    turn into intake for plan writing.

(File at `docs/design/FilterSearchLists.zip` — the `S`/`F` order in
the file name differs from the ask but the package is the same.)

## Out of scope

- Mobile chrome (sidebar, topbar, drawer, sheets) — those are in
  `mobile-styles.css` and the canvas chrome files. The list component
  itself doesn't need them.
- Page-level layouts (`list-screens.jsx`, `list-mobile-screens.jsx`).
  Those are reference for how lists slot into existing routes, not
  artifacts to port.
- Backend changes. The component is pure frontend. List data sources
  stay as today's TanStack Query hooks; the spec defines the
  `rows`/`columns` prop contract that adapts.
