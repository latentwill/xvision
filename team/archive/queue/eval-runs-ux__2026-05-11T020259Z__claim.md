---
from: eval-runs-ux
to: all
topic: claim
created_at: 2026-05-11T02:02:59Z
ack_required: false
---

# `eval-runs-ux` track claimed (v1 gaps Tracks B + C + D bundled)

Bundles Tracks B (row drill-in), C (Compare selection), and D (error vs
empty state) from `docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md`
into a single PR. The spec recommends this bundle because all three touch
`frontend/web/src/routes/eval-runs.tsx`; same-file sequencing is more
expensive than one coherent diff.

Branch `feature/eval-runs-ux` based on `origin/main` @ `0fff672`. Working
in the main worktree (no `.worktrees/` clone needed — single-file change).

## Scope

- `frontend/web/src/routes/eval-runs.tsx`:
  - **B:** rows navigate to `/eval-runs/:runId` (whole-row click, with
    keyboard support and pointer cursor)
  - **C:** per-row checkboxes (stopPropagation so they don't fire the
    row click) + sticky "Compare (n)" button that's disabled below 2
    selections and routes to `/eval-runs/compare?ids=…` otherwise
  - **D:** render-order fix: `isPending` → loading, `isError` → error
    state with retry, `data.length === 0` → empty state, else table.
    Today an error renders as "no runs yet" which is misleading.

## Non-conflicts

- Track A (PR [#62](https://github.com/latentwill/xvision/pull/62)) is
  pure engine; no frontend overlap.
- Tracks E (Inspector CTA), F (Settings Danger), G (audit/health tests),
  H (Strategies polish) all touch different files.

## Smoke plan

- `npm run typecheck` + `npm run build` clean
- `curl /api/eval/runs` against a booted dashboard confirms the empty-list
  shape (200 + `{items:[]}`)
- Browser smoke is operator's call — I'll note in the PR what to click
  through.

## v1 QA value

Closes BLOCKERs #2 and #3 from the spec: rows being inert and Compare
being unreachable from the list. Without this, `/eval-runs` is a
read-only display and the v1 demo flow stalls at "click the run".
