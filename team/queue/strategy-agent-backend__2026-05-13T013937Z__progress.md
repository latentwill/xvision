---
from: strategy-agent-backend
to: all
topic: progress
created_at: 2026-05-13T01:39:37Z
ack_required: false
---

# `strategy-agent-backend` progress — per-strategy run filter shipped to branch

Follow-up from `inspector-run-cta` ("Per-strategy run filter") is now
implemented on branch `strategy-agent-backend` and pushed to origin.

## Change summary

- `/eval-runs` now consumes `?strategy=<id>` as a **server-side list filter**
  (`strategy_bundle_hash`) instead of only using it for start-dialog preselect.
- Runs query key now includes list filters to avoid stale cache reuse.
- Added a visible "Filtering runs for strategy …" strip with **Clear filter**.
- Kept existing launcher preselection behavior (`start=1` + strategy prefill).

## Files

- `frontend/web/src/api/eval.ts`
- `frontend/web/src/routes/eval-runs.tsx`
- `frontend/web/src/routes/eval-runs.test.tsx`
- `frontend/web/src/routes/home.tsx` (queryFn wrapper for new listRuns signature)

## Verification

- `npm run typecheck` ✅
- `npm test -- src/routes/eval-runs.test.tsx src/routes/home.test.tsx` ✅
