# Standard List Component — Spec

**Date:** 2026-05-20
**Surface:** Vite dashboard SPA (`frontend/web/src/`)
**Status:** Draft for user review
**Related:**
- `team/intake/2026-05-19-list-component-design-intake.md` — wave intake
- `docs/design/FilterSearchLists.zip` (`design_handoff_lists/`) — hifi design package
- `frontend/web/src/styles/tokens.css` — existing token system (folio-dark)
- `frontend/web/src/components/responsive/useViewportMode.ts` — existing breakpoint hook
- `CLAUDE.md` — "Frontend UI rule: no popups" (adopted 2026-05-17)

## Goal

Replace the ad-hoc search/filter/sort UI on every xvision SPA list page
(Strategies, Eval Runs, Decisions, Trade Ledger, Open Positions, Journal,
plus dashboard mini-lists) with a single standardized list component
sourced from the `FilterSearchLists` hifi design package. After this
wave lands, no list in the dashboard should own its own
search/filter/sort UI; if a list can't be expressed via `<ListCard>`,
the component grows, not the list.

This document is **phase 0** (the `list-component-spec` track on the
Reserved board). It resolves the seven open questions in the intake
plus one additional conflict the intake didn't surface (mobile
bottom-sheet vs. the project no-popups rule), and decomposes the
follow-on work into contract-sized tracks.

## Scope

This spec covers:

- Resolution of the seven open questions from the intake, plus one
  additional question on mobile filter UX.
- The `useListState` hook's prop contract and return shape.
- The `<ListCard>` / `<ListToolbar>` / `<ListActiveChips>` /
  `<MListCard>` / `<MListRow>` component contracts, in terms of what
  ports verbatim from the handoff and what diverges.
- A `<ResponsiveListCard>` wrapper so host pages never fork
  desktop/mobile call sites.
- The 4-state contract (loading, empty, error, populated) every list
  must surface.
- The localStorage key scheme for density persistence.
- The decomposition into seven follow-on tracks and the dependency
  graph between them.

This spec does **not** cover:

- Backend changes (TanStack Query hooks stay as-is; the component
  consumes `rows: T[]` and is presentation-only).
- The deferred follow-ups in the handoff README (multi-select filters,
  saved views, column-header sorting, server-side pagination,
  user-visible density toggle in the toolbar). All five remain
  parked; revisit after Phase 2 ships.
- Touching `frontend/web/src/components/mobile/` chrome (sidebar,
  topbar). The list component is route-body only.

## Decisions

1. **Token reconciliation is a near-no-op for folio-dark; selective
   add-only for black/light/folio-dark.** xvision's
   `frontend/web/src/styles/tokens.css` already ports the handoff's
   token set verbatim into the `:root` and `[data-theme="folio-dark"]`
   blocks (same hex values: `--bg #0F0E0C`, `--gold #D4A547`, etc.).
   The handoff adds no token name xvision doesn't already define.
   The `[data-theme="black"]` and `[data-theme="light"]` blocks define
   the same token names with theme-appropriate values, so the
   component picks them up automatically. Decision: **port no new
   tokens**; the `list-component-tokens-reconcile` track becomes a
   short audit + visual-diff pass rather than a token-set rewrite.

2. **`useListState` is a generic hook with a fixed return shape.** API
   surface locked here:

   ```ts
   type FilterDef = {
     id: string;
     label: string;
     options: { value: string; label: string }[]; // index 0 = default
   };

   type SortOption = { value: string; label: string };

   type ListState<T> = {
     search: { value: string; setValue: (s: string) => void };
     filters: Array<{
       def: FilterDef;
       value: string;
       setValue: (v: string) => void;
     }>;
     sort: { value: string; setValue: (v: string) => void; options: SortOption[] };
     rows: T[]; // already filtered + sorted
     // raw, pre-filter row count — used for the active-chip "show N results" hint
     totalRows: number;
   };

   function useListState<T>(opts: {
     rows: T[];
     filters?: FilterDef[];
     sortOptions?: SortOption[];
     filterFn?: (row: T, query: string, values: Record<string, string>) => boolean;
     sortFn?: (rows: T[], sortKey: string) => T[];
     initialSort?: string; // defaults to sortOptions[0].value
   }): ListState<T>;
   ```

   The hook is **presentation-only when `filterFn`/`sortFn` are
   omitted**: it still tracks UI state (chip rendering, sort dropdown
   value) but doesn't transform `rows`. Server-side-filtered lists
   pass already-filtered `rows` and skip the predicates.

   Optional URL sync is a sibling hook, **not** baked into
   `useListState`:

   ```ts
   function useListUrlState(listId: string, state: ListState<unknown>): void;
   ```

   It reads `?q=...&<filterId>=...&sort=...` on mount and writes back
   on change via `useSearchParams`. Hosts opt in per list; the
   component does not require it.

3. **Mobile bottom sheet ports verbatim — operator-approved
   exemption to the no-popups rule, scoped to mobile list filters.**
   The handoff's `<MListSheet>` is a focus-stealing overlay that
   paints over the list body. xvision's no-popups rule (`CLAUDE.md`,
   adopted 2026-05-17) forbids sheets in the general case. The user
   reviewed the handoff's mobile prototype on 2026-05-20 and
   explicitly opted to keep the sheet UX — the affordance reads well
   on phone-class viewports and the alternative (inline expansion
   pushing the list down) cost more than it gained.

   This is logged as a **narrow, named exemption** to the no-popups
   rule: scoped to `<MListSheet>` rendered under `<MListCard>` on
   the phone breakpoint only. The rule continues to apply
   everywhere else (settings, agent windows, error recovery, share
   dialogs, etc. — all stay in-layout). Two follow-ups land with
   contract 1b to keep the exemption from leaking:

   - `CLAUDE.md` gets a third bullet under "Exceptions" naming
     `<MListSheet>` (mobile list filters) explicitly.
   - The sheet implementation must include the three production
     niceties the handoff README flags as missing: focus trap while
     open, swipe-to-dismiss, and body scroll-lock. Contract 1b
     owns this.

   Behavior keeps the handoff intent: tapping Filter or Sort
   triggers a slide-up sheet (`translateY(100%) → 0`, 220ms
   `cubic-bezier(.2,.7,.3,1)`), backdrop is `rgba(0,0,0,0.55)` +
   2px blur, tapping the backdrop or the Apply button dismisses,
   sheet max-height 88%, sheet body scrolls. Filter changes apply
   live (the Apply button dismisses; it does not commit a draft).
   Sort-focused mode hides the filter groups when the Sort pill
   triggers the open.

4. **Mobile parity uses a `<ResponsiveListCard>` wrapper.** Host pages
   render `<ResponsiveListCard>` once and pass the same props for
   both form factors; the wrapper picks `<ListCard>` (tablet +
   desktop) or `<MListCard>` (phone) under one breakpoint check via
   the existing `useViewportMode()` hook
   (`frontend/web/src/components/responsive/useViewportMode.ts`).
   Mapping: `phone → MListCard`, `tablet → ListCard`, `desktop →
   ListCard`. Tablet uses the desktop list-card shape; this matches
   how xvision's tablet split-pane shell already works
   (`TabletSplitShell.tsx`). Hosts that need to suppress one form
   factor (e.g. a desktop-only admin list) pass
   `mobileFallback="redirect:/m/<route>"` and the wrapper renders a
   redirect link instead of trying to mash the desktop shape onto a
   phone.

5. **Migration order is Eval Runs → Strategies → bundled tail.**
   - Phase 2a: `/eval-runs` (`eval-runs.tsx`) — highest-traffic
     surface, the handoff includes "Eval Runs V2" as the reference
     example, and the route already does query-param-backed
     filtering (good fit for `useListUrlState`).
   - Phase 2b: `/strategies` (`strategies.tsx`) — small, similar
     shape, fast to migrate.
   - Phase 2c: Decisions + Trade Ledger + Open Positions + Journal in
     **one bundled track** unless any one of them surfaces non-trivial
     complexity during the spec-stage walkthrough at the start of 2c.
     Each is a "compact density mini-list" surfaced inside a
     run-detail or home page; bundling avoids spinning four worktrees
     for shapes that share 90% of their code.
   - Phase 3 (deferred): `list-component-density-toggle` —
     user-facing density switcher in the toolbar. Parked behind
     follow-up demand; the per-list density default set by the host
     covers the immediate need.

6. **No feature flag; rip and replace per phase.** xvision's UI
   surface area is small enough and the handoff is hifi enough that
   coexistence isn't worth the maintenance tax. Each phase-2 track
   ships as a single PR that replaces the existing list shape; the
   migration is reversible by `git revert`. No `feat:lists-v2` gate.

7. **The 4-state contract.** Every list surface, both form factors:
   - **Loading** — `<ListSkeleton>` with the same row template as
     populated state, 6 placeholder rows on full density, 3 on
     compact. Skeletons use `--surface-elev` shimmer at 80% opacity.
     Triggers on `q.isPending`.
   - **Empty** — full-width row with the `empty` message (defaults
     to `"No <noun> yet."`), padded 28px desktop / 36px mobile, and
     a primary CTA below the message when the host passes
     `emptyAction={<Link to="/strategies/new">Create one</Link>}`.
     Triggers on `q.data && q.data.length === 0`.
   - **Error** — full-width row with muted text, an `<ApiError>`
     summary line, and a "Retry" button wired to `q.refetch()`.
     Triggers on `q.isError`.
   - **Populated** — table rows on desktop, `<MListRow>` cards on
     mobile. Triggers on `q.data && q.data.length > 0`.

   The skeleton, empty, and error states render **inside** the
   `<ListCard>` body so the toolbar stays visible and operable — you
   can still type a search while the list is reloading. This matches
   xvision's existing pattern (`strategies.tsx` already does this).

8. **Density persistence is per-list, per-user, localStorage.** Key
   scheme: `xvn:list:<listId>:density`, value `"full" | "compact"`.
   `listId` is a stable string the host passes (e.g.
   `"eval-runs"`, `"strategies"`, `"home-recent-runs"`). The
   per-list default density comes from the host's `density` prop;
   the localStorage entry, when present, **overrides** the host
   default. Phase 3's user-visible toggle writes to this key. The
   key is namespaced under `xvn:list:` so it's grep-discoverable and
   trivially clearable from devtools.

## Component contracts — what ports verbatim, what diverges

### `useListState` — port the hook from `list-toolbar.jsx`

- Generic type added (see Decision 2).
- `setX` setters are stable across renders (the handoff version
  already does this via `useCallback`).
- `derivedRows` is memoized on `[rows, search, filters, sort]`.
- No URL sync inside the hook (see Decision 2).

### `<ListCard>` desktop — port verbatim

- 1px border, 6px radius, `--surface-card` background.
- Header (optional): serif italic 22px title, count pill (mono 11.5px),
  subtitle, right actions slot.
- Toolbar slot beneath header; `<ListActiveChips>` row beneath toolbar
  when any filter is non-default or search is non-empty.
- Table body via `renderRow` (caller-owned cells).
- Optional footer slot.
- Empty-state rendering inside the body — see Decision 7.

Diverges from the handoff in **two** small ways:

- The handoff hardcodes the `/` keyboard shortcut hint inside the
  search input. xvision's existing global shortcut layer registers
  `/`; the component renders the hint chip but does not bind the
  shortcut. The host page (or the global layer) owns the
  `focusSearch()` callback exposed via a ref.
- The handoff's `<select>` filter pills use OS-native dropdowns. We
  keep the native `<select>` — it's the right call (a11y, mobile
  pickers) — but apply Tailwind utility classes for the colored
  states (`data-active`) instead of inline styles, so the styling
  lives next to the rest of the SPA's Tailwind config.

### `<ListToolbar>` desktop — port verbatim

- 32px controls, gold-active states per the handoff.
- Search input is 280px default; `compact` density collapses it to a
  32×32 icon button until clicked.
- Sort pill is 180px (full) / 120px (compact).

### `<ListActiveChips>` desktop — port verbatim

- Leading "Active" label, gold-tinted chips, "Clear all" link at
  end. Suppressed in `compact` density.

### `<MListCard>` mobile — port verbatim

- Sticky header, scrollable body.
- 38px search pill always visible.
- 32px control row: `<MListFilterPill>` + `<MListSortPill>` — both
  trigger `<MListSheet>` (sort pill opens in sort-focused mode).
- Active-chips row below the control row.
- Body: `renderRow` returning `<MListRow>` cards.
- No `columns` prop, no `footer` prop. `rightAction` slot lives in
  the header.

### `<MListRow>` mobile — port verbatim

- Min-height ~64px, 1px border, 8px radius, `--surface-card`
  background.
- Left column: title (13.5px mono), badge pill, subtitle, meta.
- Right column: `rightTop` (15px Cormorant 500), `rightSub` (11px
  mono muted).
- Badge palette: `gold | warn | danger | info | muted`.

### `<MListFilterPanel>` mobile — new, replaces `<MListSheet>`

- Inline panel rendered below the control row, in flow.
- Same anatomy as the sheet body (group label, pill group per filter,
  radio list for sort), without the slide-up animation, backdrop,
  drag handle, or scroll-lock.
- No "Apply" button — changes apply live (the sheet design's "Apply"
  was a dismiss, not a commit).
- Expand/collapse is animated via height transition only; no
  position: fixed, no transform.
- When collapsed, the control row is the only thing visible.

### `<ResponsiveListCard>` — new, thin wrapper

```tsx
<ResponsiveListCard
  listId="eval-runs"
  title="Eval runs"
  count={runs.length}
  density="full"
  toolbar={list}
  columns={[...]}
  rows={list.rows}
  renderRow={renderRunRow}
  renderMobileRow={renderRunMRow}
  emptyAction={<Link to="/eval-runs/new">New run</Link>}
/>
```

- Picks `<ListCard>` for `tablet | desktop`, `<MListCard>` for
  `phone`, via `useViewportMode()`.
- Forwards every prop. `columns` is only consumed by the desktop
  branch; `renderMobileRow` only by the mobile branch. Hosts pass
  both.
- One breakpoint check per render; no DOM dual-mount.

## Open questions resolved (vs. the intake's seven)

| # | Intake question | Decision (this spec) |
|---|---|---|
| 1 | Token reconciliation strategy | Near-no-op; folio-dark already matches. Selective audit only. (Decision 1) |
| 2 | `useListState` API surface | Generic, fixed return shape; URL sync as sibling hook. (Decision 2) |
| 3 | Migration order | Eval Runs → Strategies → bundled tail. (Decision 5) |
| 4 | Mobile parity strategy | `<ResponsiveListCard>` wrapper using `useViewportMode()`. (Decision 4) |
| 5 | Backward compatibility | Rip and replace per phase; no flag. (Decision 6) |
| 6 | Empty / loading / error state | 4-state contract inside the card body. (Decision 7) |
| 7 | Density toggle persistence | Per-list, localStorage, `xvn:list:<id>:density`. (Decision 8) |

## Open question added by this spec

| # | Added question | Decision (this spec) |
|---|---|---|
| 8 | Mobile bottom sheet (handoff) vs. no-popups rule (xvision) | Sheet rejected; inline `<MListFilterPanel>`. (Decision 3) |

## Track decomposition (follow-on contracts)

Phase 0 (this spec) is the gate. Phases 1a/1b/1c can land in parallel
after merge. Phase 2 tracks ship serially per list. Phase 3 is
deferred.

| Track slug | Phase | Depends on | Scope |
|---|---|---|---|
| `list-component-spec` | 0 | — | **This spec.** Merge to unblock 1a/1b/1c. |
| `list-component-port-desktop` | 1a | 0 | Port `useListState`, `<ListCard>`, `<ListToolbar>`, `<ListActiveChips>` → `frontend/web/src/components/lists/`. Vitest coverage. No call-site changes. |
| `list-component-port-mobile` | 1b | 0 | Port `<MListCard>`, `<MListRow>`, **new `<MListFilterPanel>`** → same dir. Vitest coverage. No call-site changes. |
| `list-component-tokens-reconcile` | 1c | 0 | Audit `tokens.css` vs. `styles.css` from the handoff; confirm zero deltas; visual-diff each theme on the handoff canvas; add `<ResponsiveListCard>` wrapper. |
| `list-migrate-eval-runs` | 2a | 1a + 1b + 1c | Replace `/eval-runs` (desktop + mobile) list with `<ResponsiveListCard>`. Wires `useListUrlState` for `?q&strategy&mode&status&sort`. |
| `list-migrate-strategies` | 2b | 2a | Same for `/strategies`. |
| `list-migrate-decisions-and-tail` | 2c | 2b | Decisions, Trade Ledger, Open Positions, Journal — one bundled track. Carve out at start of 2c if any one surfaces non-trivial complexity. |
| `list-component-density-toggle` | 3 (deferred) | 2c | User-facing density switcher in the toolbar. Parked. |

### Dependency graph

```
                       ┌─ list-component-port-desktop  ─┐
list-component-spec ───┼─ list-component-port-mobile   ─┼─ list-migrate-eval-runs ─ list-migrate-strategies ─ list-migrate-decisions-and-tail
                       └─ list-component-tokens-reconcile ┘                                                          │
                                                                                                                     └─ list-component-density-toggle (deferred)
```

### File layout (locked here so 1a/1b/1c don't collide)

All new component files land under
`frontend/web/src/components/lists/`:

```
lists/
├── ListCard.tsx                  # 1a
├── ListCard.test.tsx             # 1a
├── ListToolbar.tsx               # 1a
├── ListActiveChips.tsx           # 1a
├── MListCard.tsx                 # 1b
├── MListCard.test.tsx            # 1b
├── MListRow.tsx                  # 1b
├── MListFilterPanel.tsx          # 1b (new — replaces MListSheet)
├── ResponsiveListCard.tsx        # 1c
├── ResponsiveListCard.test.tsx   # 1c
├── useListState.ts               # 1a
├── useListState.test.ts          # 1a
├── useListUrlState.ts            # 1a
└── index.ts                      # 1a (barrel; 1b/1c append)
```

Hosts import `from "@/components/lists"`.

## Acceptance criteria for this spec

The spec is accepted when:

- The seven intake questions each map to a numbered decision above
  (audit: questions 1–7 → decisions 1, 2, 5, 4, 6, 7, 8).
- The mobile bottom-sheet conflict is explicitly resolved (decision
  3) rather than left as a downstream surprise.
- The decomposition table names every follow-on track with a stable
  slug, a phase number, and an explicit dependency.
- The file-layout block names every file each follow-on track
  creates, so contracts 1a/1b/1c don't collide at OWNERSHIP time.
- The user signs off on Decision 3 (the only decision that visibly
  diverges from the hifi handoff).

## Non-goals

- Multi-select filters, saved views, column-header sorting,
  server-side pagination, user-visible density toggle. All five
  remain parked per the handoff README's "Open follow-ups" section.
- Virtualization. Phase 1a's `<ListCard>` body is a plain `<tbody>`;
  add `react-virtual` if a list grows past ~1000 rows in
  practice. None do today.
- Replacing the existing `tokens.css` token system. (Decision 1.)
- Touching `frontend/web/src/components/mobile/` chrome.

## Notes for the contract authors

- Contracts 1a/1b should both follow the existing test patterns in
  `frontend/web/src/components/primitives/` and adjacent feature
  folders. Don't invent a test harness.
- Contract 1b's `<MListFilterPanel>` will be the first inline
  expand-collapse panel of its kind in the SPA; mirror the
  expand-collapse pattern in
  `frontend/web/src/features/agent-runs/SpanInspector.tsx` rather
  than rolling a new one.
- Contract 2a is the first list that gets URL state. Cross-check
  with the existing `?strategy=&start=1` query-param contract on
  `/eval-runs` (`eval-runs.tsx:63`) so the rename to
  `?strategy=&q=&mode=&status=&sort=` is a superset, not a
  breaking change. Migration of the existing `?start=1` flag is
  out of scope for 2a — leave it untouched.
