---
track: qa-eval-action-lifecycle
lane: leaf
wave: qa-operator-2026-05-18
worktree: .worktrees/qa-eval-action-lifecycle
branch: task/qa-eval-action-lifecycle
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: declared:eval-inspector-header-polish
allowed_paths:
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.tsx
  - frontend/web/src/routes/eval-runs.tsx
  - frontend/web/src/routes/eval-runs-detail.test.tsx
  - frontend/web/src/routes/eval-runs-detail-mobile.test.tsx
  - frontend/web/src/routes/eval-runs.test.tsx
  - frontend/web/src/features/eval-runs/**
  - frontend/web/src/stores/eval-capsule.ts
  - frontend/web/src/stores/eval-capsule.test.ts
  - frontend/web/src/api/eval-runs.ts
  - frontend/web/src/api/eval-runs.test.ts
forbidden_paths:
  - crates/**
  - crates/xvision-engine/migrations/**
  - frontend/web/src/components/primitives/Pill.tsx
  - frontend/web/src/features/agent-runs/TraceDock.tsx
interfaces_used:
  - EvalRunSummary (existing)
  - useEvalRunLabels (existing)
  - run.status lifecycle (`running` | `completed` | `errored` | `cancelled`)
parallel_safe: false
parallel_conflicts:
  - "eval-inspector-header-polish: also writes eval-runs-detail{,.test,-mobile,-mobile.test}.tsx and eval-runs{,.test}.tsx. Single-writer claim is held there. Stack on its branch and rebase, OR wait for it to merge."
  - "qa-ui-polish-round2: edits eval-runs surface for the chart-name nit. Coordinate disjoint file regions if landing concurrently."
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run eval-runs-detail eval-runs eval-capsule
  - pnpm --dir frontend/web build
acceptance:
  - A cancelled run no longer shows a counting timer in the eval capsule.
    The capsule transitions to a terminal `Cancelled` state with the
    elapsed time frozen at the cancel moment.
  - Navigating from the inspector back to the eval list (or any other
    page) no longer leaves a stale running capsule visible. The capsule
    is bound to the active route subscription (or resets in the store
    on inspector unmount) and disappears immediately without requiring
    a refresh.
  - The Retry affordance works for runs in `cancelled` lifecycle state
    in addition to `completed` and `errored`. If the backend rejects
    re-run of cancelled rows today, this contract surfaces that as a
    classified UI error AND files a queue note to whichever engine
    contract owns the eval-run create path so the gate is fixed.
  - The eval inspector header gains a Delete action that removes the
    eval run via the existing DELETE route, alongside Stop / Retry /
    Download. The new button shares the same width treatment owned by
    `eval-inspector-header-polish` — coordinate via stacking.
  - Tests cover: (1) capsule reads `cancelled` from `run.status` and
    stops the timer; (2) navigating away from the inspector clears the
    capsule store; (3) Retry button is enabled for `cancelled` runs;
    (4) Delete button calls the DELETE route and redirects to the eval
    list on 2xx.
  - No `border-white` / `border-gray-100` / `border-gray-200` / `#fff`
    on dark mode (CLAUDE.md rule).
---

# Scope

Four related operator-reported bugs (2026-05-18) on the eval lifecycle
surface:

1. Cancelled run's capsule keeps the timer running even after the
   cancel completed (P1).
2. The capsule bleeds onto the eval list and other pages after the
   user navigates away from the inspector, and only disappears after
   a hard refresh (P2). The capsule store is not unmounting / resetting
   with the route.
3. Retry button does not work for cancelled runs (P1) — only completed
   and errored runs can be re-run today.
4. The eval inspector has no Delete affordance (P2). Today the only
   delete path is the eval list. Add the button to the inspector's
   action row so Stop / Retry / Download / Delete are symmetric.

The Stop / Retry / Download button-width fix and the metadata-strip
cleanup are owned by `eval-inspector-header-polish`. This track stacks
on top so the new Delete button picks up the same width treatment.

# Out of scope

- The Stop/Retry/Download width normalization and the redundant
  metadata strip (`eval-inspector-header-polish`).
- The trace dock surface (`trace-dock-ux-polish`,
  `trace-fullscreen-redesign`, `qa-trace-dock-resizable`).
- The streaming-text body fix (`model-call-streaming-text-passthrough`).
- Backend changes to the eval-run schema or status enum. If the cancel
  → cannot-retry behavior is a backend gate, that's an upstream fix —
  surface via queue note rather than expanding scope.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/qa-eval-action-lifecycle status
git -C .worktrees/qa-eval-action-lifecycle log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/qa-eval-action-lifecycle \
  -b task/qa-eval-action-lifecycle origin/main
```

Stacking note: `eval-inspector-header-polish` is `ready`, not yet
claimed at intake time. If it claims first, this contract rebases on
`task/eval-inspector-header-polish`. If this contract claims first,
flip the order with a contract update.

# Notes

Append checkpoints / PR links below.
