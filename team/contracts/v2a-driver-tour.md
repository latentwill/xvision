---
track: v2a-driver-tour
lane: leaf
wave: v2a
worktree: .worktrees/v2a-driver-tour
branch: task/v2a-driver-tour
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/features/onboarding/**
  - frontend/web/src/routes/index.tsx                  # mount tour, not refactor
  - frontend/web/package.json                          # add driver.js dep
forbidden_paths:
  - crates/**
  - frontend/web/src/features/eval-runs/**
  - frontend/web/src/features/chat-rail/**
interfaces_used:
  - LocalStorage workspace keys (read-only audit, then namespaced writes)
parallel_safe: true
parallel_conflicts: []
verification:
  - corepack pnpm --dir frontend/web test -- onboarding
  - corepack pnpm --dir frontend/web typecheck
acceptance:
  - First-run tour fires once after a clean workspace boots.
  - Tour can be re-triggered from Settings → General → "Restart tour".
  - Tour state stored under a namespaced key, ignores any other localStorage.
  - Tour is dismissible at every step; dismiss persists.
---

# Scope

V2A item 1 from `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md`:
add Driver.js-based first-run tour and a "restart tour" affordance in
Settings. Tour content focuses on the three primary surfaces (Strategies,
Scenarios, Eval Runs) — no deep tour of secondary surfaces in v1.

# Out of scope

- In-app docs/help route (separate track: `v2a-in-app-docs`).
- Example strategy/scenario seeding (separate track: `v2a-example-artifacts`).
- Tooltip/keyboard-shortcut overhauls.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/v2a-driver-tour -b task/v2a-driver-tour origin/main
```

# Notes

- Mobile Safari storage guards already landed; use the existing safe-storage
  helper rather than direct `localStorage` access.
