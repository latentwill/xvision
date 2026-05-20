---
track: list-component-port-desktop
lane: foundation
wave: lists-v1
worktree: .worktrees/list-component-port-desktop
branch: task/list-component-port-desktop
base: origin/main
status: ready
depends_on: []                # implements the spec; spec PR is the operator gate, not a code dep
blocks:
  - list-component-tokens-reconcile   # ResponsiveListCard imports ListCard
  - list-migrate-eval-runs            # consumer
  - list-migrate-strategies           # consumer
  - list-migrate-decisions-and-tail   # consumer
stacking: none
allowed_paths:
  - frontend/web/src/components/lists/ListCard.tsx          # NEW
  - frontend/web/src/components/lists/ListCard.test.tsx     # NEW
  - frontend/web/src/components/lists/ListToolbar.tsx       # NEW
  - frontend/web/src/components/lists/ListActiveChips.tsx   # NEW
  - frontend/web/src/components/lists/useListState.ts       # NEW
  - frontend/web/src/components/lists/useListState.test.ts  # NEW
  - frontend/web/src/components/lists/useListUrlState.ts    # NEW
  - frontend/web/src/components/lists/index.ts              # NEW barrel (mobile + wrapper append later)
forbidden_paths:
  - frontend/web/src/routes/**                              # no call-site migration this track (phase 2)
  - frontend/web/src/components/mobile/**                   # mobile port is contract 1b
  - frontend/web/src/components/lists/MListCard.tsx         # mobile (1b)
  - frontend/web/src/components/lists/MListRow.tsx          # mobile (1b)
  - frontend/web/src/components/lists/MListSheet.tsx        # mobile (1b)
  - frontend/web/src/components/lists/ResponsiveListCard.tsx # wrapper (1c)
  - frontend/web/src/styles/tokens.css                      # tokens-reconcile (1c)
  - CLAUDE.md                                               # no-popups exemption is 1b's edit
  - crates/**                                               # frontend only
interfaces_used:
  - frontend/web/src/components/primitives/Icon            # existing icon component
  - frontend/web/src/components/primitives/Pill             # existing pill primitive (count chip uses it)
parallel_safe: true
parallel_conflicts:
  - list-component-port-mobile      # both touch frontend/web/src/components/lists/index.ts (barrel); split exports by component, rebase the smaller diff
  - list-component-tokens-reconcile # 1c rebases onto 1a's exports
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- components/lists
  - pnpm --dir frontend/web lint
acceptance:
  - **`useListState<T>` hook** — signature and return shape locked in spec Decision 2. Memoizes derived rows on `[rows, search, filters, sort]`. Setters (`search.setValue`, `filters[i].setValue`, `sort.setValue`) are stable across renders. When `filterFn`/`sortFn` are omitted the hook still tracks UI state but returns `rows` unchanged.
  - **`useListUrlState(listId, state)` sibling hook** — reads `?q=…&<filterId>=…&sort=…` on mount, writes back on change via `react-router-dom` `useSearchParams`. Opt-in per host page; not invoked from inside `useListState`.
  - **`<ListCard>` component** — props match spec section "Component contracts — `<ListCard>` desktop". Renders header (optional title, count pill, subtitle, actions slot), toolbar slot, active-chips slot, table body (`renderRow`), optional footer. 1px `--border` border, 6px `--radius-card` radius, `--surface-card` background.
  - **`<ListToolbar>` component** — 32px controls, search input default 280px (collapses to 32×32 icon button in `compact` density), filter pills (native `<select>` under transparent absolutely-positioned overlay for OS-correct dropdown), sort pill 180px (full) / 120px (compact). Gold-active state when a filter is non-default (`data-active="true"` attribute → Tailwind utility classes).
  - **`<ListActiveChips>` component** — leading "Active" label (10.5px uppercase mono `--text-3`), gold-tinted chips per non-default filter (each chip click resets that filter to option index 0), trailing "Clear all" link. Suppressed in `compact` density.
  - **4-state contract inside the body** — `<ListCard>` renders `<ListSkeleton>` (6 placeholder rows full / 3 compact, `--surface-elev` shimmer 80%) when `loading=true`, `<ListEmpty>` with optional `emptyAction` slot when `rows.length === 0`, `<ListError>` with `<ApiError>` summary + retry button when `error`, populated rows otherwise. All four states render inside the card body — the toolbar stays operable.
  - **Density persistence hook (read-only on this track)** — `useListDensity(listId, defaultDensity)` reads `xvn:list:<listId>:density` from localStorage with the host default as fallback. Phase 3 writes; this contract just wires the read.
  - **Keyboard hint chip renders but is not bound here** — the `/` chip is shown next to the search input. Host page (or global shortcut layer) owns the actual `focusSearch()` binding; `<ListCard>` exposes a `searchRef` via `forwardRef` for the host.
  - **Vitest coverage** — `useListState.test.ts` covers: default sort initialization (`initialSort` falls through to `sortOptions[0].value`), filter predicate composition with search, sort key change re-derives. `ListCard.test.tsx` covers: header rendering, all four body states, active chips reset, density toggle visual delta. Test patterns mirror `frontend/web/src/components/primitives/*.test.tsx`.
  - **Barrel exports** — `frontend/web/src/components/lists/index.ts` exports `ListCard`, `ListToolbar`, `ListActiveChips`, `useListState`, `useListUrlState`, plus types `FilterDef`, `SortOption`, `ListState`. Mobile + wrapper exports appended by 1b/1c.
  - **No call-site changes** — no edits under `frontend/web/src/routes/` or `frontend/web/src/features/`. Migration is phase-2 work.
  - **No new dependencies** — uses existing Tailwind + `react-router-dom` + `@tanstack/react-query` already in `package.json`.

---

# Scope

Phase 1a of the standard list component wave. Ports the **desktop**
half of the hifi handoff at `docs/design/FilterSearchLists.zip` into
`frontend/web/src/components/lists/`. Implements the
`useListState<T>` generic hook locked in
`docs/superpowers/specs/2026-05-20-standard-list-component.md`
(Decision 2), plus `<ListCard>` / `<ListToolbar>` /
`<ListActiveChips>` with the 4-state body contract (Decision 7).

Adds the sibling `useListUrlState` hook so hosts can opt into URL
state sync without baking it into `useListState`.

No call-site changes — `/eval-runs`, `/strategies`, etc. stay on
their bespoke list UIs until phase 2.

# Out of scope

- Mobile components (`<MListCard>`, `<MListRow>`, `<MListSheet>`) — contract 1b.
- `<ResponsiveListCard>` wrapper and the tokens audit — contract 1c.
- The user-facing density toggle in the toolbar (host-default density is
  honored via the `density` prop; localStorage *read* is wired so phase 3
  can light up the toggle without re-plumbing). The toggle itself is the
  deferred phase-3 track.
- Multi-select filters, saved views, column-header sorting, server-side
  pagination — all parked per the handoff README's "Open follow-ups"
  section and the spec's non-goals.
- Touching CLAUDE.md (the no-popups exemption append is 1b's responsibility
  since the sheet is what triggers it).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/list-component-port-desktop status
git -C .worktrees/list-component-port-desktop log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/list-component-port-desktop -b task/list-component-port-desktop origin/main
```

# Notes

Source files in the handoff: `list-toolbar.jsx` and `styles.css` from
`docs/design/FilterSearchLists.zip`
(`design_handoff_lists/`). Reference, not paste-as-is — the handoff
is inline-Babel-transpiled prototype code; this contract ports the
component to React + TypeScript + Tailwind and the existing token
system. Open `design_handoff_lists/Lists.html` in a browser when
visual-diffing.

Existing icon component is at
`frontend/web/src/components/primitives/Icon.tsx` — use its icon
names. The handoff references `search`, `sliders`, `plus`, `chevR`;
confirm each one exists or add it in this contract (allowed: Icon.tsx
edits to register missing icon names; not allowed: replacing the
icon system).
