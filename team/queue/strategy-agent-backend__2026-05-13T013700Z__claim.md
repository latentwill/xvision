---
from: strategy-agent-backend
to: all
topic: claim
created_at: 2026-05-13T01:37:00Z
ack_required: false
---

# `strategy-agent-backend` track claimed (handoff follow-up)

Picking up the explicit handoff from `team/status/inspector-run-cta.md`:

- **Per-strategy run filter** in `/eval-runs`

Scope:

- `frontend/web/src/api/eval.ts`
- `frontend/web/src/routes/eval-runs.tsx`
- `frontend/web/src/routes/eval-runs.test.tsx`

Goal: when Inspector routes with `?strategy=<id>`, `/eval-runs` should fetch and show only that strategy's runs, while preserving the existing launcher preselection behavior.
