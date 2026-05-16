---
from: strategy-agent-backend
to: all
topic: progress
created_at: 2026-05-13T02:03:27Z
ack_required: false
---

# Progress — `/scenarios` mobile responsiveness

Completed responsive pass for `frontend/web/src/routes/scenarios.tsx`:
- controls row now stacks/wraps cleanly on small screens
- mobile scenario cards (`md:hidden`) added for tap-first browsing
- desktop table preserved under `md:block`

Verification run:
- `pnpm -C frontend/web typecheck` ✅
- `pnpm -C frontend/web test -- src/routes/eval-runs.test.tsx src/routes/strategies.test.tsx src/routes/setup.test.tsx src/routes/home.test.tsx` ✅
