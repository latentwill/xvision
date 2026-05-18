---
track: ui-scrollbars-always-visible
lane: leaf
wave: qa-operator-2026-05-18-r3
worktree: .worktrees/ui-scrollbars-always-visible
branch: task/ui-scrollbars-always-visible
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/styles/**
  - frontend/web/src/index.css
  - frontend/web/src/theme/**
  - frontend/web/src/components/primitives/**
  - frontend/web/src/features/eval-runs/**
  - frontend/web/src/features/agent-runs/**
  - frontend/web/tailwind.config.ts
forbidden_paths:
  - crates/**
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.tsx
interfaces_used:
  - global CSS / Tailwind config
  - per-component overflow primitives
parallel_safe: true
parallel_conflicts:
  - "qa-eval-action-lifecycle / eval-inspector-header-polish: also touch eval-runs surfaces. This contract should not edit routes/eval-runs-detail*.tsx — keep the change inside features/eval-runs primitives + global CSS."
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run
  - pnpm --dir frontend/web build
acceptance:
  - On the eval inspector decisions list, when the content exceeds
    the box height, a visible scrollbar persists on macOS Safari /
    Chrome — not hidden until hover. Operator can see "more below"
    at a glance.
  - The same treatment applies to the SPA's other overflow surfaces:
    trace dock body, span inspector body, chat rail history list,
    settings panels. The contract status file lists each surface
    audited.
  - Visual treatment stays subtle (matches the existing muted
    palette; uses theme tokens, not raw colour). Cross-browser
    behaviour on Firefox/Chrome/Safari verified by the worker
    locally and recorded in the status file (no automated visual
    regression — manual smoke OK).
  - Implementation lives in a small global CSS rule plus a reusable
    primitive (e.g. `.scrollbar-stable` utility class or a
    `<ScrollArea>` primitive). Per-component opt-in is preferred
    over a global force-on, so we don't add bars to surfaces that
    legitimately don't need them.
  - No regression on mobile webviews (touch-driven scroll surfaces
    should still hide their bar — the persistent bar is desktop-only
    behaviour).
---

# Scope

Operator (2026-05-18): "Scrollbar needs to be on by default on
decisions in the eval inspector once it is longer than box (should
the the same for all scrollbars so they are visible, otherwise no
indicator for user that there is more data in the box…)".

macOS auto-hide scrollbars are the default for any browser on macOS
that respects the system preference. Operators using the dashboard
on macOS Safari/Chrome can't tell the box is scrollable until they
try. Add a global utility / primitive that pins the scrollbar
visible on overflow surfaces, then opt in the surfaces operators
hit most.

# Out of scope

- A full design-system refactor of all overflow surfaces.
- Custom-painted scrollbar (e.g. react-scrollbars-custom). Native
  scrollbar styling is good enough and avoids the keyboard /
  accessibility regressions custom components ship with.
- Mobile / touch surfaces (those should keep the auto-hide behaviour).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/ui-scrollbars-always-visible status
git -C .worktrees/ui-scrollbars-always-visible log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/ui-scrollbars-always-visible \
  -b task/ui-scrollbars-always-visible origin/main
```

# Notes

The standard cross-browser pattern is:

```css
.scrollbar-stable {
  overflow-y: auto;
  scrollbar-gutter: stable;
  scrollbar-width: thin;          /* Firefox */
  scrollbar-color: var(--scrollbar-thumb) var(--scrollbar-track);
}
.scrollbar-stable::-webkit-scrollbar { width: 8px; height: 8px; }
.scrollbar-stable::-webkit-scrollbar-thumb {
  background: var(--scrollbar-thumb); border-radius: 4px;
}
.scrollbar-stable::-webkit-scrollbar-track {
  background: var(--scrollbar-track);
}
```

Plus opt-in on the per-surface containers. Avoid `overflow: overlay`
(deprecated) and avoid `-webkit-overflow-scrolling: touch` on
non-mobile.

Append checkpoints / PR links below.
