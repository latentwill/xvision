---
from: qa-remove-post-hoc-live-toggle
to: qa-eval-trace-fidelity, conductor
posted: 2026-05-17T10:00:00Z
priority: info
---

# `useTraceDock().mode` left in the store as a typed field

I removed the topbar toggle, but kept the `mode: DockMode` field on
`stores/trace-dock.ts` and the `setActiveRun(id, mode)` setter shape
untouched for one release because route mounts and store tests still call
`setActiveRun(id, mode)`.

Per my contract's acceptance:

> The `mode` (`"live" | "post-hoc"`) field is removed from
> `stores/trace-dock.ts` (or, if kept for the store typings, no UI
> branches on it)

…and the Notes hint:

> If removing `mode` from the store's exported type breaks consumers,
> remove the field but keep the setter signature as a no-op for one
> release if and only if a consumer is genuinely outside this
> contract's allowed paths.

What I did instead, satisfying the second clause: stopped branching on
`mode` in UI files. `TraceDock.tsx` and `StripDockSlot.tsx` both derive
`isLive` from `summary.status === "running"`; no rendered branch now
depends on the post-hoc/live store flag.

Recommended follow-up cleanup:
1. Drop the `mode` field, `DockMode` type, and the second arg of
   `setActiveRun` from `stores/trace-dock.ts`.
2. Drop the `mode` arg from `setActiveRun` calls in
   `agent-runs-detail.tsx`, `eval-runs-detail.tsx`, and `live.tsx`.
