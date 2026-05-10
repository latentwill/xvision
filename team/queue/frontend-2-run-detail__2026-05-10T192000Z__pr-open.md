---
from: frontend-2-run-detail
to: all
topic: pr-open
created_at: 2026-05-10T19:20:00Z
ack_required: false
---

# Frontend Plan 2 Run Detail slice — PR #24 open (stacked on #21)

PR: https://github.com/latentwill/xvision/pull/24
Branch: `feature/frontend-2-run-detail`
Base: `feature/frontend-2-eval-runs` (PR #21)
Worktree: `.worktrees/frontend-2-run-detail`

## What landed

Plan 2 Tasks 4 + 13 — `GET /api/eval/runs/:id` end-to-end + the
matching `/eval-runs/:runId` frontend page. Closes the read-only
"click a run, see what it did" loop on top of PR #21's list.

Backend:
- `engine::api::eval::get_run(ctx, id) -> RunDetail` wraps
  `RunStore::{get, read_decisions, read_equity_curve}`. Maps "not
  found" → typed `ApiError::NotFound` so the dashboard returns 404
  with JSON body, not 500.
- `RunDetail` carries `summary` + `decisions[]` + `equity_curve[]`;
  ts-rs derives so the SPA stays typed end-to-end.

Dashboard:
- `GET /api/eval/runs/:id` thin handler. `DashboardError: From<ApiError>`
  forwards NotFound → 404 + `{"code":"not_found"}`.

Frontend:
- `api/eval.ts` adds `getRun(id)` + `evalKeys.run(id)`.
- `routes/eval-runs-detail.tsx` — real page (summary card with status
  pill + sharpe/maxDD/return/started/completed metrics; decisions
  table with action / conviction / size / fill / pnl-coloured;
  inline-SVG equity sparkline; typed 404 page with Back link;
  loading skeleton + retry-on-error mirroring /strategies and
  /eval-runs).

## Tested

- `cargo test -p xvision-dashboard --test http` — 9/9 (2 new:
  `eval_run_detail_returns_404_for_unknown` and
  `eval_run_detail_returns_summary_decisions_and_equity` which
  round-trips a Run + 2 decisions + 2 equity samples through the
  store and asserts every nested array surfaces correctly).
- `pnpm typecheck && pnpm build` green.
- `chrono` added to xvision-dashboard dev-deps so seeded tests
  build.

## Stacking

Targets `feature/frontend-2-eval-runs` (PR #21) so the diff stays
clean. When #21 merges, this PR's base auto-updates to `main` and
the diff reduces to the new commits only.

## Notes for downstream

- `engine::api::eval` continues unowned by session 1's
  eval-3.B/3.C/3.D work — they touch
  `eval/{store,attestation,findings}` only.
- Phase 3.D's CLI dispatch (`xvn eval show <id>`) can reuse this
  same `get_run` fn — no rewrite expected.
- Frontend Plan 2 Compare-runs (Task 14) is the next natural eval
  follow-up, building on this run-detail surface.
