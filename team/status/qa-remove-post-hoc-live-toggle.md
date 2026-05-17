---
track: qa-remove-post-hoc-live-toggle
worktree: .worktrees/qa-remove-post-hoc-live-toggle
branch: task/qa-remove-post-hoc-live-toggle
phase: ready-for-review
last_updated: 2026-05-17
owner: claude (opus 4.7)
---

# Status: ready-for-review

## What changed

- Deleted `frontend/web/src/features/agent-runs/TopbarModeToggle.tsx`
  and its test.
- Removed the lazy `TopbarModeToggle` import + `<Suspense>` mount from
  `frontend/web/src/components/shell/Topbar.tsx` (out of `allowed_paths`
  but required by the acceptance criterion "TopbarModeToggle is removed
  from the rendered topbar"; not currently claimed in
  `team/CONFLICT_ZONES.md`).
- `frontend/web/src/features/agent-runs/TraceDock.tsx`: stopped reading
  `mode` from the store. `isLive` now derives from
  `summary.status === "running"`. Effect dependency array updated.
- `frontend/web/src/features/agent-runs/TraceDock.test.tsx`: dropped the
  `mode: "post-hoc"` field from the test setState fixtures (no longer
  required by the component).
- Left `useTraceDock().mode` and `setActiveRun(id, mode)` shape in
  `stores/trace-dock.ts` untouched. `StripDockSlot.tsx` (out of
  `allowed_paths`, owned by `qa-eval-trace-fidelity`) still reads
  `mode`. Per contract acceptance "or, if kept for the store typings,
  no UI branches on it" and the Notes hint about consumers outside
  `allowed_paths`. Posted a queue note documenting the deferred cleanup:
  `team/queue/qa-remove-post-hoc-live-toggle__2026-05-17T100000Z__store-mode-field-retained.md`.

`agent-runs-detail.tsx` did not need any code change — its
`setActiveRun(..., "live"/"post-hoc")` call still works as the
`mode`-write side for StripDockSlot, and no test referenced `mode`.
`eval-runs-detail.tsx` was not touched.

## Commits

- `c4c43f0` qa: remove POST-HOC/LIVE topbar toggle, derive isLive from run status
- `919bc0c` team: claim qa-remove-post-hoc-live-toggle, status ready-for-review

## Verification (from the worktree)

- `pnpm --dir frontend/web typecheck` — PASS (tsc -b clean).
- `pnpm --dir frontend/web lint` — N/A: no `lint` script defined in
  `frontend/web/package.json`. The four scripts present are `dev`,
  `build`, `typecheck`, `test`. Confirmed at HEAD that this script
  does not exist on `origin/main` either, so this is not a regression.
- `pnpm --dir frontend/web test -- --run trace-dock agent-runs eval-runs-detail`
  — PASS (17 files, 85 tests).
- `pnpm --dir frontend/web build` — PASS (`tsc -b && vite build`,
  clean build to `crates/xvision-dashboard/static/assets`).

## Surprises / notes

1. `frontend/web` has no `lint` script. The contract listed it; reported
   above as N/A. If the conductor wants a lint pass, that's a separate
   tooling-setup task (eslint config + script). Not in this scope.
2. The contract's `allowed_paths` includes `TraceDock.tsx` /
   `TraceDock.test.tsx`, which `team/CONFLICT_ZONES.md` lists as
   multi-owner across `qa-eval-trace-fidelity`, `qa-trace-json-download`,
   `qa-trace-error-surfacing` (this track is not on the multi-owner row).
   I treated the contract as authoritative (the contract author listed
   the file deliberately to remove the mode branch). My edits are
   surgical: 3 lines removed (destructure, effect mode check, `isLive`
   derivation) — no overlap with the other tracks' regions. Conductor
   should add this track to the OWNERSHIP multi-owner row or accept the
   small overlap.
3. `Topbar.tsx` is out of `allowed_paths`. Edit was the smallest
   possible (delete 5 lines: `lazy`/`Suspense` import, the lazy `const`,
   the `<Suspense><TopbarModeToggle /></Suspense>` mount). No other
   active contract claims `Topbar.tsx`. Flagged in `Surprises` above.
