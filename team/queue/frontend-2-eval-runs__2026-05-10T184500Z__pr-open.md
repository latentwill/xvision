---
from: frontend-2-eval-runs
to: all
topic: pr-open
created_at: 2026-05-10T18:45:00Z
ack_required: false
---

# Frontend Plan 2 Eval Runs slice — PR #21 open

PR: https://github.com/latentwill/xvision/pull/21
Branch: `feature/frontend-2-eval-runs`
Worktree: `.worktrees/frontend-2-eval-runs`

## What landed

Plan 2 Tasks 3 + 12 — `GET /api/eval/runs` end-to-end + the matching
frontend `/eval-runs` page.

Backend:
- New `engine::api::eval::list_runs(ctx, &ListRunsRequest) -> Vec<RunSummary>`,
  wrapping `eval::store::RunStore::list(filter)` with the audit-trail pattern.
- `RunSummary` is a slimmer wire shape than full `Run` so the engine can
  add telemetry fields without growing the wire payload.
- ts-rs derives + `cargo xtask gen-types` emits `RunSummary.ts`.

Dashboard:
- `GET /api/eval/runs` with optional query filters (`strategy_bundle_hash`,
  `scenario_id`, `status`). Shape `{ "items": RunSummary[] }`.

Frontend:
- `api/eval.ts` typed fetcher + TanStack Query keys.
- `routes/eval-runs.tsx` real screen with four states (loading skeleton /
  empty / error w/ Retry / populated table). Status pills: gold completed
  / info running / default queued / warn cancelled / danger failed.

## Tested

- `cargo test -p xvision-dashboard --test http` — 7/7 (3 new for eval).
- `pnpm typecheck && pnpm build` green.
- Live smoke (port 8804): empty + filtered both return `{"items":[]}`.

## Coordination

- **Independent of eval-3.C (PR #17)** — touches `eval/store.rs` +
  `eval/attestation.rs` only. This PR is the first to add
  `engine::api::eval`.
- **Phase 3.D (CLI + MCP dispatch) will reuse this `list_runs` fn** —
  no rewrite expected when that lands; just adds `xvn eval list` CLI.
- Frontend Plan 2 Task 4 (run detail) and Task 14 (compare) are the
  next natural eval slices for this track.
