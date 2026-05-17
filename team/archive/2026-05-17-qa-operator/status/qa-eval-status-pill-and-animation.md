---
track: qa-eval-status-pill-and-animation
status: pr-open
last_update: 2026-05-17
worker: Claude (xvision conductor session)
branch: task/qa-eval-status-pill-and-animation
worktree: .worktrees/qa-eval-status-pill-and-animation
pr: 240
pr_url: https://github.com/latentwill/xvision/pull/240
commits:
  - 5371f22 — qa: collapse streaming capsule into running pill; lock status-pill source
---

## Outcome

PR #240 open. Branch pushed.

The route's status `<Pill>` was already wired correctly from `summary.status`
with `animated={summary.status === "running"}` (#193). The visible
redundancy operators reported was the separate inline "streaming" capsule
between the pill and the metric grid (lines 371–376 pre-change), which
showed for both `queued` and `running` runs. Removed it — one animated pill
on running runs; one tone-coloured pill on terminal runs.

Tests against `EvalRunDetailRoute` now lock the four acceptance criteria:

  - pill text == `summary.status`; never "completed" while running
  - `.xvn-pill-animated` + `data-running="true"` + `aria-busy="true"` while running
  - no `streaming` element on running runs
  - animation strips off on terminal runs

## Verification

- `pnpm --dir frontend/web typecheck` — clean
- `pnpm --dir frontend/web test --run eval-runs-detail status-pill` — 27/27 pass
- `pnpm --dir frontend/web test` (full) — 245/245 pass
- `pnpm --dir frontend/web build` — clean
- `pnpm --dir frontend/web lint` — script does not exist (no-op in
  contract verification list); flagged in PR test plan.

## Notes

- Allowed-paths spec mentioned `features/agent-runs/StatusPill.tsx` as a
  potential extraction site; no extraction was needed to satisfy the
  acceptance criteria. Leaving the route-level inline pill in place
  avoids churn that would conflict with `qa-eval-trace-fidelity` /
  `qa-trace-error-surfacing` work in `features/agent-runs/`.
- `prefers-reduced-motion` is honored upstream in
  `frontend/web/src/styles/globals.css` (lines 205–214). Unchanged.
- Build deleted `crates/xvision-dashboard/static/.gitkeep` as a
  side-effect; restored before commit so the PR diff stays to the two
  intended files.
