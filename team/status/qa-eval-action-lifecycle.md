# qa-eval-action-lifecycle — status

**Contract:** `team/contracts/qa-eval-action-lifecycle.md`
**Branch:** `task/qa-eval-action-lifecycle`
**Worktree:** `.worktrees/qa-eval-action-lifecycle`
**Claimed:** 2026-05-18
**Status:** in-progress (pushed, PR open)

## Contract amendment

The contract's original `allowed_paths` referenced `frontend/web/src/stores/eval-capsule.ts` /
`frontend/web/src/stores/eval-capsule.test.ts`. No such files exist — the
"eval capsule" the operator named is the `RunStatusStrip` rendered by
`StripDockSlot` reading from the trace-dock store. The contract was amended
in this PR to swap the placeholder paths for the real ones:

- added: `frontend/web/src/features/agent-runs/StripDockSlot.tsx` (+ test)
- removed: `frontend/web/src/stores/eval-capsule.ts` (+ test) — never existed
- `frontend/web/src/features/agent-runs/TraceDock.tsx` stays forbidden
  (owned by `qa-trace-dock-resizable`)
- `frontend/web/src/features/agent-runs/RunStatusStrip.tsx` added to
  forbidden so the per-pixel design from the prior trace-dock-ux-polish
  track stays single-writer

## Changes shipped

- **`eval-runs-detail.tsx`** — added an unmount cleanup effect that calls
  `useTraceDock.getState().setActiveRun(null, "post-hoc")` so the
  floating capsule no longer bleeds onto `/eval-runs` or any other route
  after the operator navigates away. Threaded a `deleteRun` mutation
  through SummaryCard and added a Delete button to the action grid
  (picks up the `grid-flow-col auto-cols-fr` width treatment from
  eval-inspector-header-polish). Retry is now enabled for `cancelled`
  runs alongside `failed`.
- **`eval-runs-detail-mobile.tsx`** — same lifecycle changes mirrored:
  Retry enabled for cancelled, Delete added to the RunActions grid.
  Removed the gating `if (!canRetry && !terminal) return null` so the
  Delete affordance shows for any run (the Delete button is always
  needed; the inflight case implicitly suppresses Retry/Download).
- **`features/agent-runs/StripDockSlot.tsx`** — `isLive` now requires
  BOTH `summary.status === "running"` AND `mode === "live"` from the
  dock store. Backend-lag scenarios (eval cancelled but agent-run
  summary still reports running) no longer keep the capsule timer
  ticking. `deriveTone` takes the dock mode too so the pulsing LIVE
  tone only renders when the inspector still considers the run live.
  Added a `frozenSummary` fallback that derives `duration_ms` from
  `finished_at - started_at` when the backend hasn't flushed
  `duration_ms` yet — keeps the cancelled capsule reading
  `X.Ys` instead of `—`.

### Tests

- `eval-runs-detail.test.tsx` — 3 new render cases: Retry button
  visible on cancelled runs; Delete button calls the DELETE route and
  navigates to `/eval-runs`; trace-dock `activeRunId` clears on
  inspector unmount. Updated the existing action-grid count case to
  expect 3 buttons (Retry + Download + Delete) on failed runs.
  `deleteRun` added to the `vi.mock` factory.
- `features/agent-runs/StripDockSlot.test.tsx` — 2 new cases:
  capsule freezes (no pulsing LIVE dot) when mode flips to `post-hoc`
  even if the agent-run summary still says `running`; duration falls
  back to `finished_at - started_at` when `duration_ms` is null.
  `getAgentRun` mocked so the slot can render past the loading state.

## Verification

```bash
npm --prefix frontend/web run typecheck       # clean
npm --prefix frontend/web test -- --run \
  eval-runs-detail eval-runs.test StripDockSlot # 53/53 passing
npm --prefix frontend/web run build           # clean
```

The contract listed `pnpm --dir frontend/web lint` as a verification
step; this repo doesn't have a `lint` script wired in `package.json`,
so it's skipped (would have been a no-op).

## Notes

Append checkpoints / PR links below.
