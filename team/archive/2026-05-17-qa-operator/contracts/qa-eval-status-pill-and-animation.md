---
track: qa-eval-status-pill-and-animation
lane: leaf
wave: qa-operator-2026-05-17
worktree: .worktrees/qa-eval-status-pill-and-animation
branch: task/qa-eval-status-pill-and-animation
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/routes/eval-runs-detail.test.tsx
  - frontend/web/src/features/agent-runs/StatusPill.tsx
  - frontend/web/src/features/agent-runs/StatusPill.test.tsx
  - frontend/web/src/features/eval-runs/**
forbidden_paths:
  - crates/**
  - frontend/web/src/components/primitives/Pill.tsx
  - frontend/web/src/features/agent-runs/TopbarModeToggle.tsx
  - frontend/web/src/features/agent-runs/TraceDock.tsx
parallel_safe: false
parallel_conflicts:
  - "mobile-eval-run-detail: claims eval-runs-detail.tsx as single-writer. Stack on its branch or wait."
  - "qa-remove-post-hoc-live-toggle: this track must not depend on the toggle's continued existence."
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web lint
  - pnpm --dir frontend/web test -- --run eval-runs-detail status-pill
  - pnpm --dir frontend/web build
acceptance:
  - The eval status capsule is sourced from `run.status` (lifecycle
    state — `running` | `completed` | `errored`), not from the
    most-recent span's state. While the run is `running`, the capsule
    never reads "Completed"
  - The running-state animation landed by `eval-running-animation`
    (#193) is visibly active during an active run
  - The separate "streaming" capsule is removed or merged into the
    single animated `running` pill — exactly one status indicator on
    the surface
  - `prefers-reduced-motion` is respected (mirror the rule from #193)
---

# Scope

Three connected status / animation bugs on the eval-run detail surface.
Originally part of `qa-eval-running-status-streaming`; the
SSE-event-surfacing half of that contract was split off and deferred
post-Phase-B observability (see Notes).

1. **"Completed" while running.** The eval bar status capsule
   currently labels the trailing span's state, not the run's
   lifecycle state. Source the pill from `run.status`. While the run
   is `running`, never say "Completed".
2. **Running animation missing.** The animation introduced by
   `eval-running-animation` (#193) doesn't appear during an active
   run. Likely a CSS / state plumbing regression on the eval-run
   detail surface. Restore.
3. **Streaming capsule redundancy.** A "streaming" capsule shows up
   alongside the "running" pill. Collapse to a single animated pill.

# Out of scope

- SSE-driven trace event surfacing — deferred to a post-Phase-B
  follow-up. Phase B (`agent-run-observability-ipc-emission`) is the
  prerequisite for real streaming events to flow.
- Removing the `POST-HOC⇄LIVE` toggle (owned by
  `qa-remove-post-hoc-live-toggle`).
- Span content / model fidelity (blocked: `qa-eval-trace-fidelity`).
- Trace JSON download (blocked: `qa-trace-json-download`).
- Re-opening `frontend/web/src/components/primitives/Pill.tsx` (closed
  out by `eval-running-animation`). If a non-trivial change is needed
  there, file a contract update first.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-eval-status-pill-and-animation \
  -b task/qa-eval-status-pill-and-animation origin/main
git -C .worktrees/qa-eval-status-pill-and-animation status
```

`eval-runs-detail.tsx` is currently single-writer-claimed by
`mobile-eval-run-detail`. Either wait for that to merge or stack via
`stacking: declared:mobile-eval-run-detail` and rebase later.

# Notes

This contract was split from `qa-eval-running-status-streaming` on
2026-05-17 once Phase B observability was identified as in-progress.
The deferred half — SSE event surfacing in the trace dock — will be
filed as a follow-up after `agent-run-observability-ipc-emission`
lands. That work would otherwise be guessing at empty events.

Implementation hints:

- `run.status` likely comes from `frontend/web/src/api/runs.ts` —
  `RunSummary` shape. Confirm it carries lifecycle state (not just
  the last span's state).
- The animated pill is in
  `frontend/web/src/components/primitives/Pill.tsx`; consume it but
  don't edit it. The motion-respect guard is part of the existing
  component.
