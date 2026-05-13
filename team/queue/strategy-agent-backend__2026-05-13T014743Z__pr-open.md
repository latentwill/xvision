---
from: strategy-agent-backend
to: all
topic: pr-open
created_at: 2026-05-13T01:47:43Z
ack_required: false
---

# `strategy-agent-backend` PR open: [#96](https://github.com/latentwill/xvision/pull/96)

Track branch: `strategy-agent-backend`  
Base: `main`

## What landed

- Handoff follow-up from `team/status/inspector-run-cta.md`:
  - `/eval-runs` now applies strategy-scoped filtering from `?strategy=<id>`
    by calling `GET /api/eval/runs?strategy_bundle_hash=<id>`
  - run list query key now includes filter params to avoid stale cache reuse
  - active filter banner + clear action in `EvalRunsRoute`
  - existing Inspector launcher preselection behavior is preserved
- Execution-board coordination artifacts for this track:
  - claim/progress/goal queue messages
  - live status file updates

## Files

- `frontend/web/src/api/eval.ts`
- `frontend/web/src/routes/eval-runs.tsx`
- `frontend/web/src/routes/eval-runs.test.tsx`
- `frontend/web/src/routes/home.tsx`
- `team/queue/strategy-agent-backend__2026-05-13T013700Z__claim.md`
- `team/queue/strategy-agent-backend__2026-05-13T013937Z__progress.md`
- `team/queue/strategy-agent-backend__2026-05-13T014223Z__goal.md`
- `team/status/strategy-agent-backend.md`

## Verification

- `npm run typecheck` (frontend/web) ✅
- `npm test -- src/routes/eval-runs.test.tsx src/routes/home.test.tsx` (frontend/web) ✅
