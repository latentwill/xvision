---
track: scenario-bars-estimate-ui
lane: leaf
wave: qa-operator-2026-05-18-r3
worktree: .worktrees/scenario-bars-estimate-ui
branch: task/scenario-bars-estimate-ui
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/scenarios-new.tsx
  - frontend/web/src/routes/scenarios-detail.tsx
  - frontend/web/src/routes/scenarios.tsx
  - frontend/web/src/routes/scenarios-detail.test.tsx
  - frontend/web/src/components/scenario/**
forbidden_paths:
  - crates/**
  - frontend/web/src/features/agent-runs/**
interfaces_used:
  - Scenario form state (existing)
  - bars-estimate selector / memoised calc
parallel_safe: true
parallel_conflicts: []
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run scenarios
  - pnpm --dir frontend/web build
acceptance:
  - On `/scenarios/new` (and any chat-rail scenario card that
    surfaces the bars estimate), setting the context-bars input to
    a positive integer immediately updates "Estimated bars to fetch"
    to a number greater than zero. Operator's repro
    ("Added 100 bars context in scenario, but it still says:
    Estimated bars to fetch: 0") no longer reproduces.
  - The estimate includes both the time-window-derived bar count
    and the operator-supplied context bars. The formula and unit
    test cover: (a) time-window only, (b) context-bars only,
    (c) both summed, (d) zero context bars degrades to the
    time-window-only number.
  - Unit test on the bars-estimate selector / memo proves the
    context-bars dependency is present.
---

# Scope

Operator (2026-05-18): "Added 100 bars context in scenario, but it
still says: Estimated bars to fetch: 0." The bars-estimate calc in
the scenario form isn't reading the context-bars input — likely a
missing dependency in the memoised selector.

Small leaf — audit the selector, add the missing dependency, add a
unit test that asserts the dependency is honoured.

# Out of scope

- Backend changes to how bars are fetched (Polygon / OpenRouter /
  data-source plumbing). The display gap is purely client-side.
- Redesigning the scenario form layout.
- Adding new scenario fields.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/scenario-bars-estimate-ui status
git -C .worktrees/scenario-bars-estimate-ui log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/scenario-bars-estimate-ui \
  -b task/scenario-bars-estimate-ui origin/main
```

# Notes

Append checkpoints / PR links below.
