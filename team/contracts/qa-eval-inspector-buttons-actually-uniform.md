---
track: qa-eval-inspector-buttons-actually-uniform
lane: leaf
wave: qa-operator-2026-05-18
worktree: .worktrees/qa-eval-inspector-buttons-uniform
branch: conductor/qa-eval-inspector-buttons-uniform
base: origin/main
status: claimed
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.tsx
forbidden_paths:
  - crates/**
  - frontend/web/src/routes/eval-runs.tsx
  - frontend/web/src/components/primitives/Card.tsx
  - frontend/web/src/styles/**
interfaces_used:
  - SummaryCard action-row layout
  - Mobile RunActions row
parallel_safe: true
parallel_conflicts: []
verification:
  - npm --prefix frontend/web run typecheck
  - npm --prefix frontend/web test -- eval-runs-detail
  - npm --prefix frontend/web test -- eval-runs-detail-mobile
acceptance:
  - The Stop / Retry / Download JSON / Delete buttons in the eval
    inspector action row render at the same visual width regardless
    of which subset is visible (`inflight` shows Stop; `canRetry`
    shows Retry; `terminal` shows Download / Delete). The previous
    `grid grid-flow-col auto-cols-fr` approach did NOT equalize
    widths in an unconstrained inline-grid — `1fr` collapses to
    content size when the grid has no explicit container width — so
    the visual result was still uneven.
  - Fix: each button takes `min-w-[16ch]` so the column floor is the
    widest natural label ("Preparing JSON…" + padding). All visible
    buttons read at the same width on both the desktop SummaryCard
    and the mobile RunActions row.
  - Inspector card borders feel softer: the SummaryCard's outer
    Card override uses `border-border-soft` (a darker tone) instead
    of the default `border-border`, so the multi-card stacked layout
    no longer reads as harsh on the dark theme. The Card primitive
    is NOT modified — the override is local to the inspector via
    className.
  - No regression on existing eval-runs-detail / mobile tests.
  - No `border-white` / `border-gray-100` / `border-gray-200` /
    `#fff` on dark mode (CLAUDE.md rule — pre-existing compliance,
    confirmed by repo-wide grep).
---

# Scope

Round-2 contract `eval-inspector-header-polish` (merged via PR #255)
claimed to give Stop / Retry / Download uniform widths via
`grid grid-flow-col auto-cols-fr`. That pattern only equalizes
columns when the grid has a known total width; in an unconstrained
inline-grid (no `w-full`, no fixed `w-`), `1fr` collapses to
content size and each column ends up at its own button's natural
width. Operator (2026-05-18) reported the buttons still render at
different sizes.

Follow-up fix: pin each button to `min-w-[16ch]` so the floor matches
the widest natural label ("Preparing JSON…" with its padding). Apply
to both desktop SummaryCard and mobile RunActions.

While there, soften the SummaryCard outer border via className
override (`border-border-soft` instead of the default
`border-border`). Operator described the inspector borders as
"hard white" — on the folio-dark theme the `--border` token
(`#2a2618`) reads as a 3-shade lift over `--bg` (`#0f0e0c`), which
the operator finds harsh given the inspector stacks several cards.
`--border-soft` (`#221f15`) is a softer 1-shade lift. The Card
primitive itself stays untouched so other surfaces aren't disturbed.

# Out of scope

- The Card primitive itself
  (`frontend/web/src/components/primitives/Card.tsx`).
- The eval-runs list surface (`eval-runs.tsx`) — its row chrome is
  separate.
- Theme token redefinition (`frontend/web/src/styles/tokens.css`).
- Any backend / engine changes.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
```

This contract is small enough to ship as a single combined
contract + code PR off `conductor/qa-eval-inspector-buttons-uniform`.

# Notes

PR #255 ("eval inspector header polish") was the original attempt;
it landed the `grid grid-flow-col auto-cols-fr` shell but missed the
unconstrained-grid caveat. This contract is the visual follow-up the
operator asked for at the end of the round-2 wave.
