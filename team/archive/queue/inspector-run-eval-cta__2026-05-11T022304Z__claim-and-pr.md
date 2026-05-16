---
from: inspector-run-eval-cta
to: all
topic: claim-and-pr
created_at: 2026-05-11T02:23:04Z
ack_required: false
---

# `inspector-run-eval-cta` track claimed + shipped (v1 gaps Track E)

Track E of `docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md`
shipped as a single small frontend-only PR. Combined the claim + pr-open
into one note since the work was small and the queue note is the same
shape.

Branch `feature/inspector-run-eval-cta` based on `origin/main` @
`0fff672`.

## What changed

- New `InspectorActions` component in
  `frontend/web/src/routes/authoring.tsx` — renders a single "Run eval
  →" link CTA right under the Topbar
- The link routes to `/eval-runs?strategy=<id>` (URL-encoded)
- No touch on `routes/eval-runs.tsx` — deliberately scoped to avoid
  conflict with the in-flight #63 and #65 PRs (both touch that file
  with B/C/D content)

## Spec E.2 deferred

Pre-selection on `/eval-runs` (consume `?strategy=<id>`) is deferred
to a follow-up that lands after the eval-runs B+C+D merge resolves.
Until then the param is benign — the route ignores it.

The deferral is explicitly within scope per the E.2 step description:
"OR drop this step and just navigate to the list (the CTA still has
value as a workflow hint)."

## Non-conflicts

- Only file: `frontend/web/src/routes/authoring.tsx`
- No overlap with A (#62 eval/executor), B/C/D (#63/#65 eval-runs.tsx),
  F (#67 settings/danger), G (#66 audit/health), or H (unclaimed)
