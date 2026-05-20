---
track: list-component-tokens-reconcile
lane: integration
wave: lists-v1
worktree: .worktrees/list-component-tokens-reconcile
branch: task/list-component-tokens-reconcile
base: origin/main
status: ready
depends_on:
  - list-component-port-desktop   # wrapper imports ListCard
  - list-component-port-mobile    # wrapper imports MListCard
blocks:
  - list-migrate-eval-runs
  - list-migrate-strategies
  - list-migrate-decisions-and-tail
stacking: none
allowed_paths:
  - frontend/web/src/components/lists/ResponsiveListCard.tsx       # NEW
  - frontend/web/src/components/lists/ResponsiveListCard.test.tsx  # NEW
  - frontend/web/src/components/lists/index.ts                     # append wrapper export
  - frontend/web/src/styles/tokens.css                             # audit only — expected zero delta for folio-dark; minor adds (if any) per audit
  - docs/design/handoff-audit/list-component-tokens-audit.md       # NEW — audit report (delta table per theme)
forbidden_paths:
  - frontend/web/src/routes/**                                 # no call-site migration this track
  - frontend/web/src/components/lists/ListCard.tsx             # 1a
  - frontend/web/src/components/lists/MListCard.tsx            # 1b
  - frontend/web/src/components/lists/MListSheet.tsx           # 1b
  - frontend/web/src/components/lists/useListState.ts          # 1a
  - frontend/web/src/styles/globals.css                        # owned by decision-side-label-sell-vs-short — audit only, no edits this track
  - crates/**
  - CLAUDE.md                                                  # 1b owns the no-popups exemption append
interfaces_used:
  - frontend/web/src/components/lists/ListCard
  - frontend/web/src/components/lists/MListCard
  - frontend/web/src/components/responsive/useViewportMode
parallel_safe: false
parallel_conflicts:
  - list-component-port-desktop      # this track imports its surface; rebase blocked on 1a's PR merge
  - list-component-port-mobile       # this track imports its surface; rebase blocked on 1b's PR merge
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- components/lists
  - pnpm --dir frontend/web lint
acceptance:
  - **Token audit report** — `docs/design/handoff-audit/list-component-tokens-audit.md` documents, per theme (`folio-dark`, `black`, `light`), the delta between `frontend/web/src/styles/tokens.css` and `docs/design/FilterSearchLists.zip` (`design_handoff_lists/styles.css`). Expected outcome: **zero delta for folio-dark** (xvision's `[data-theme="folio-dark"]` block ports the handoff `:root` verbatim). For `black` and `light`, the audit documents which tokens the lists components consume and confirms each theme's value reads as intended on each lists surface (`<ListCard>`, `<MListCard>`, `<MListSheet>`).
  - **Visual diff** — on each of the three themes, screenshot each lists surface against the handoff's `design_handoff_lists/Lists.html` and `Lists Mobile.html` open in a browser. Differences are noted in the audit doc with a verdict (`accept` / `fix-in-this-track` / `fix-in-phase-2`).
  - **No token rewrite** unless the audit identifies a token name the handoff defines that xvision does not. If such a delta is found, this track adds it to all four theme blocks (`:root`, `[data-theme="folio-dark"]`, `[data-theme="black"]`, `[data-theme="light"]`) with theme-appropriate values; the spec's Decision 1 (no-rewrite default) still holds — surface the addition in the PR description so review can confirm scope.
  - **No `globals.css` edits** — owned by `decision-side-label-sell-vs-short`. If the audit finds a base style (e.g. font-family stack) the lists components require, file a follow-up; do not edit `globals.css` in this contract.
  - **`<ResponsiveListCard>` component** — props superset of `<ListCard>` + `<MListCard>` (host passes both `columns` and `renderMobileRow`; the wrapper consumes only what the active branch needs). Picks `<ListCard>` for `tablet | desktop`, `<MListCard>` for `phone`, via the existing `useViewportMode()` hook (`frontend/web/src/components/responsive/useViewportMode.ts`). One breakpoint check per render; no DOM dual-mount.
  - **`mobileFallback` prop** — optional. When set to `"redirect:/m/<route>"`, the wrapper renders a redirect link on the phone breakpoint instead of attempting to render `<MListCard>`. Used by desktop-only admin lists that haven't authored a mobile shape. Default behavior is to render `<MListCard>`.
  - **Vitest coverage** — `ResponsiveListCard.test.tsx`: covers (a) renders `<ListCard>` on desktop, (b) renders `<MListCard>` on phone, (c) tablet matches desktop, (d) `mobileFallback` renders a redirect on phone instead of `<MListCard>`. Mock `useViewportMode` per-test.
  - **Barrel export** — append `ResponsiveListCard` to `frontend/web/src/components/lists/index.ts`. After this contract, the barrel exports the complete public API: hook (`useListState`, `useListUrlState`), desktop (`ListCard`, `ListToolbar`, `ListActiveChips`), mobile (`MListCard`, `MListRow`, `MListSheet`), wrapper (`ResponsiveListCard`), types (`FilterDef`, `SortOption`, `ListState`).
  - **No call-site changes** — phase-2 migration tracks own the route edits. This contract just stages the wrapper that those tracks will consume.

---

# Scope

Phase 1c of the standard list component wave. Two pieces of work:

1. **Token audit.** Spec Decision 1 declares that xvision's existing
   token system already ports the handoff's `:root` block verbatim
   for `folio-dark`, and that `black`/`light` themes define the same
   token names. This contract verifies that declaration empirically:
   diff `frontend/web/src/styles/tokens.css` against
   `docs/design/FilterSearchLists.zip` (`design_handoff_lists/styles.css`),
   visual-diff each theme on the handoff canvas, and produce an
   audit report documenting deltas. Surface any missing token names
   as additions; the rest stays untouched.

2. **`<ResponsiveListCard>` wrapper.** A thin component that picks
   `<ListCard>` (tablet + desktop) or `<MListCard>` (phone) via
   `useViewportMode()`, so host pages render the wrapper once and
   pass the same props for both form factors. Spec Decision 4.

Rebase-blocked on 1a and 1b — the wrapper imports both. If 1a's
`useListState` signature shifts during review, 1b and 1c both rebase;
this contract is the integration point that consumes both surfaces.

# Out of scope

- Editing `<ListCard>`, `<MListCard>`, `<MListSheet>`, or `useListState`
  — those are 1a/1b. If a wrapper-driven API change is needed, push it
  back to the upstream contract via PR comments; do not edit here.
- Migrating any route to `<ResponsiveListCard>` — phase-2 work.
- Adding new themes. The audit documents the existing three; new
  themes are a separate intake.
- Replacing the existing `useViewportMode()` hook. The wrapper consumes
  it as-is.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/list-component-tokens-reconcile status
git -C .worktrees/list-component-tokens-reconcile log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/list-component-tokens-reconcile -b task/list-component-tokens-reconcile origin/main
```

# Notes

Run the visual-diff against an actual browser session, not just
screenshots from CI — the iPhone-frame chrome in the mobile canvas
is decorative; only the inner 390×844 surface counts. Use Chrome
DevTools device emulation set to 390×844 (or iPhone 14 Pro) for
the mobile visual-diff.

The audit report is the deliverable, not a chore — phase-2 tracks
read it to know whether per-route theme tweaks are needed. Keep it
short (one table per theme, one verdict per delta).

`useViewportMode` returns `phone | tablet | desktop` via
`matchMedia` breakpoints at 768px / 1280px. Tablet rendering uses
the desktop list-card shape, matching how
`TabletSplitShell.tsx` already handles split-pane layouts.
