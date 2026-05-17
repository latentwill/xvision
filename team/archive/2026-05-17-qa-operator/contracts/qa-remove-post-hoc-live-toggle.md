---
track: qa-remove-post-hoc-live-toggle
lane: leaf
wave: qa-operator-2026-05-17
worktree: .worktrees/qa-remove-post-hoc-live-toggle
branch: task/qa-remove-post-hoc-live-toggle
base: origin/main
status: in-progress
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/features/agent-runs/TopbarModeToggle.tsx
  - frontend/web/src/features/agent-runs/TopbarModeToggle.test.tsx
  - frontend/web/src/features/agent-runs/TraceDock.tsx
  - frontend/web/src/features/agent-runs/TraceDock.test.tsx
  - frontend/web/src/stores/trace-dock.ts
  - frontend/web/src/stores/trace-dock.test.ts
  - frontend/web/src/routes/agent-runs-detail.tsx
  - frontend/web/src/routes/agent-runs-detail.test.tsx
  - frontend/web/src/routes/eval-runs-detail.tsx
forbidden_paths:
  - crates/**
  - frontend/web/src/api/**
parallel_safe: true
parallel_conflicts:
  - "qa-eval-trace-fidelity / qa-eval-running-status-streaming: both touch features/agent-runs/. Coordinate so trace-strip + status work doesn't restore the toggle."
  - "mobile-eval-run-detail: edits eval-runs-detail.tsx. Coordinate via team/queue/; if needed stack on its branch."
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run trace-dock agent-runs eval-runs-detail
  - pnpm --dir frontend/web build
acceptance:
  - `TopbarModeToggle` is removed from the rendered topbar
  - The `mode` (`"live" | "post-hoc"`) field is removed from
    `stores/trace-dock.ts` (or, if kept for the store typings, no UI
    branches on it)
  - Live runs continue to stream; completed runs continue to display
    historical trace data — without an operator-facing toggle
  - No dangling imports or test fixtures referencing `POST-HOC` / `LIVE`
    mode strings
---

# Scope

Remove the `POST-HOC⇄LIVE` topbar mode toggle. The toggle is at
`frontend/web/src/features/agent-runs/TopbarModeToggle.tsx`; its store
hook is `frontend/web/src/stores/trace-dock.ts`. Remove the toggle, its
test, and any conditional rendering that branches on `mode`. The
underlying behavior should default to whichever shape the run actually
is — live runs stream off the SSE channel landed by
`agent-run-observability` Phase A, completed runs render their stored
events.

# Out of scope

- Refactoring the rest of the trace dock UI (owned by
  `qa-eval-trace-fidelity` and `qa-eval-running-status-streaming`).
- Removing the SSE plumbing or the agent-run-observability event bus.
- Adding a replacement affordance (no replacement is intended).
- Touching engine code or migrations.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-remove-post-hoc-live-toggle \
  -b task/qa-remove-post-hoc-live-toggle origin/main
git -C .worktrees/qa-remove-post-hoc-live-toggle status
```

# Notes

Implementation hints:

- Grep `mode === "live"` and `mode === "post-hoc"` to find every
  branch. Most should be inside `TraceDock.tsx` and the related
  features files.
- The store can keep an `activeRunId` field without the `mode`. If
  removing `mode` from the store's exported type breaks consumers,
  remove the field but keep the setter signature as a no-op for one
  release if and only if a consumer is genuinely outside this
  contract's allowed paths.
- This was added by `[leaf] agent-run-observability-ui` (commit
  e05770b on `main`) — `git show e05770b -- frontend/web/src/features/agent-runs/TopbarModeToggle.tsx`
  for context on the original intent.
