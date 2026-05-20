---
track: list-component-port-mobile
lane: foundation
wave: lists-v1
worktree: .worktrees/list-component-port-mobile
branch: task/list-component-port-mobile
base: origin/main
status: ready
depends_on: []                # implements the spec; can develop in parallel with 1a
blocks:
  - list-component-tokens-reconcile   # ResponsiveListCard imports MListCard
  - list-migrate-eval-runs            # consumer (mobile branch)
  - list-migrate-strategies           # consumer (mobile branch)
  - list-migrate-decisions-and-tail   # consumer (mobile branch)
stacking: none
allowed_paths:
  - frontend/web/src/components/lists/MListCard.tsx          # NEW
  - frontend/web/src/components/lists/MListCard.test.tsx     # NEW
  - frontend/web/src/components/lists/MListRow.tsx           # NEW
  - frontend/web/src/components/lists/MListSheet.tsx         # NEW — the exemption to no-popups (mobile list filters only)
  - frontend/web/src/components/lists/MListSheet.test.tsx    # NEW
  - frontend/web/src/components/lists/useListState.ts        # SHARED with 1a — read-only here; the mobile components consume the same hook
  - frontend/web/src/components/lists/index.ts               # SHARED with 1a — append mobile exports
  - CLAUDE.md                                                # third bullet under "Exceptions" — name the MListSheet exemption explicitly
forbidden_paths:
  - frontend/web/src/routes/**                               # no call-site migration this track
  - frontend/web/src/components/mobile/**                    # mobile chrome (topbar, sidebar) — out of scope
  - frontend/web/src/components/lists/ListCard.tsx           # desktop (1a)
  - frontend/web/src/components/lists/ListToolbar.tsx        # desktop (1a)
  - frontend/web/src/components/lists/ListActiveChips.tsx    # desktop (1a)
  - frontend/web/src/components/lists/ResponsiveListCard.tsx # wrapper (1c)
  - frontend/web/src/styles/tokens.css                       # tokens-reconcile (1c)
  - crates/**                                                # frontend only
interfaces_used:
  - frontend/web/src/components/lists/useListState           # the same hook 1a authors; consumed read-only here
  - frontend/web/src/components/responsive/useViewportMode   # used by 1c, not by 1b directly
  - frontend/web/src/components/primitives/Icon              # existing icon component
parallel_safe: true
parallel_conflicts:
  - list-component-port-desktop      # both touch frontend/web/src/components/lists/index.ts (barrel) and useListState.ts (1a authors, 1b reads). Disjoint regions in barrel; useListState is owned by 1a — 1b rebases if 1a's signature shifts during review.
  - list-component-tokens-reconcile  # 1c rebases onto 1b's exports
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- components/lists
  - pnpm --dir frontend/web lint
acceptance:
  - **`<MListCard>` component** — sticky header (serif 26px title, count, optional rightAction icon), 38px-tall always-visible search pill, 32px control row with `<MListFilterPill>` + `<MListSortPill>`, active-chips row below, body via `renderRow` returning `<MListRow>` cards. Props match spec section "Component contracts — `<MListCard>` mobile". No `columns` prop, no `footer` prop.
  - **`<MListRow>` component** — min-height ~64px, 1px `--border`, 8px radius, `--surface-card` bg. Left column: title (13.5px JetBrains Mono weight 500), badge pill, subtitle (12px mono `--text-2`), meta (11px mono `--text-3`). Right column: `rightTop` (15px Cormorant Garamond weight 500), `rightSub` (11px mono muted). Badge palette: `gold | warn | danger | info | muted` (18px tall, 7px pad, 3px radius, 9.5px mono uppercase, 0.08em letter-spacing). Active press state via `:active` → `--surface-hover`.
  - **`<MListSheet>` component** — slide-up bottom sheet, 220ms `cubic-bezier(.2,.7,.3,1)`, backdrop `rgba(0,0,0,0.55)` + 2px blur, max-height 88%, top-only 18px radius, drag handle. Tapping Filter pill opens with all filter groups visible; tapping Sort pill opens in **sort-focused mode** (filter groups hidden, only the sort radio list visible). Filter changes apply live; the gold "Show N results" button (46px, full-width) dismisses. Backdrop tap also dismisses. The same component handles both modes via a `focus: "filters" | "sort"` prop.
  - **Sheet production niceties** (spec Decision 3, required by the no-popups exemption):
    * **Focus trap** while open — `Tab` cycles inside the sheet, does not escape to the underlying list body. First focusable element receives focus on open; previous focus restored on close.
    * **Swipe-to-dismiss** — vertical drag on the sheet body (>120px or velocity > 0.5px/ms) closes the sheet. Drag <120px springs back.
    * **Body scroll-lock** — `document.body` gets `overflow: hidden` while open, restored on close. No iOS rubber-band leak.
  - **`<MListFilterPill>`** — 32px pill, leading `sliders` icon, gold count badge for non-default filter count, whole-pill gold state (border + bg + text) when any filter active. Tap opens `<MListSheet focus="filters">`.
  - **`<MListSortPill>`** — 32px pill, `flex: 1` (takes remaining width). Format: `Sort: <current label> ▾`. Tap opens `<MListSheet focus="sort">`.
  - **CLAUDE.md exemption append** — add a third bullet under "Exceptions" in the "Frontend UI rule: no popups" section: `MListSheet (mobile list filters only)` — naming the component, the surface, and the scope. Two-line max. No silent precedent.
  - **Vitest coverage** — `MListCard.test.tsx`: rendering, search wiring, both pill triggers open the sheet with the correct focus, active chip row visibility. `MListSheet.test.tsx`: focus trap (Tab does not escape), swipe-to-dismiss threshold, scroll-lock effect on body, sort-focused vs filter+sort rendering. Use Vitest + `@testing-library/react`; mirror existing patterns under `frontend/web/src/components/`.
  - **Barrel exports** — append to `frontend/web/src/components/lists/index.ts`: `MListCard`, `MListRow`, `MListSheet`. Wrapper export added by 1c.
  - **No call-site changes** — no edits under `frontend/web/src/routes/` or `frontend/web/src/features/`.
  - **No new dependencies** — uses existing Tailwind + the focus-trap behavior is hand-rolled (small enough to not need `focus-trap-react`); confirm bundle size doesn't regress.

---

# Scope

Phase 1b of the standard list component wave. Ports the **mobile**
half of the hifi handoff at `docs/design/FilterSearchLists.zip` into
`frontend/web/src/components/lists/`. The mobile shape is:
sticky header + always-visible search + filter/sort pills + active
chips row + scrollable column of `<MListRow>` cards, with filter
edits happening in `<MListSheet>` (a slide-up bottom sheet).

`<MListSheet>` is the **only sheet/overlay** allowed in the dashboard
SPA per the project no-popups rule
(`CLAUDE.md` "Frontend UI rule: no popups"). The user reviewed the
prototype on 2026-05-20 and explicitly opted to keep the sheet UX
over an inline expand-collapse panel; the spec
(`docs/superpowers/specs/2026-05-20-standard-list-component.md`
Decision 3) records this as a narrow, named exemption. This contract
makes that exemption real: adds the sheet **and** updates CLAUDE.md
so the exemption is visible at the same source of truth that enforces
the broader rule.

Consumes the `useListState<T>` hook authored by contract 1a — the
mobile components don't fork state; the same hook drives both form
factors. Coordinate the barrel-export edits with 1a (disjoint sections
in `index.ts`).

# Out of scope

- Desktop components (`<ListCard>`, `<ListToolbar>`, `<ListActiveChips>`) — contract 1a.
- `<ResponsiveListCard>` wrapper — contract 1c.
- Mobile chrome (sidebar, topbar) — owned by `frontend/web/src/components/mobile/`, not this track.
- Bottom-sheet variations elsewhere in the SPA. The no-popups rule continues to apply everywhere else; this exemption is scoped to mobile list filters only.
- Touching `crates/**` or backend behavior.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/list-component-port-mobile status
git -C .worktrees/list-component-port-mobile log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/list-component-port-mobile -b task/list-component-port-mobile origin/main
```

# Notes

Source files in the handoff: `list-toolbar-mobile.jsx`, `styles.css`
from `docs/design/FilterSearchLists.zip` (`design_handoff_lists/`).
Open `design_handoff_lists/Lists Mobile.html` in a browser for the
interactive prototype — the inner 390×844 surface is the production
target (the iPhone frame is decorative). Tap the Filter and Sort pills
to see both sheet modes.

The handoff's `mobile-styles.css` is the canvas chrome (topbar,
drawer) — do not port; it's not used by the production component.

If `focus-trap-react` or a similar small library makes the focus trap
substantially less error-prone than hand-rolling, document the tradeoff
in the PR description; bundle-size delta should be ≤2 KB gzipped to
clear review. Default is hand-rolled.

The "Apply" button in the sheet dismisses but does not commit — filter
state mutates live as the user taps pills. This matches the handoff
prototype's behavior; if it confuses operators in practice, file a
follow-up to switch to a draft+commit pattern.
