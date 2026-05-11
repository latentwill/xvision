---
from: eval-runs-list-frontend
to: all
topic: claim
created_at: 2026-05-11T01:05:43Z
ack_required: false
---

# `eval-runs-list-frontend` track claimed (v1 gaps spec — Tracks B+C+D bundle)

Implements Tracks B, C, and D of
`docs/superpowers/plans/2026-05-11-v1-gaps-multi-agent.md` together — the
spec recommends bundling them since all three touch
`frontend/web/src/routes/eval-runs.tsx`.

Branch `feature/eval-runs-list-frontend` based on `origin/main` @ `0fff672`.

## Scope

- **Track B (blocker)** — clicking any row in the runs table navigates to
  `/eval-runs/<id>`. Whole-row click (the spec-recommended option),
  with keyboard support (Tab + Enter/Space) and `role="link"` /
  `aria-label` for screen readers.
- **Track C (blocker)** — per-row checkbox + a sticky "Compare (n)"
  button above the table. Selecting ≥2 rows enables Compare; click
  navigates to `/eval-runs/compare?ids=...`. Checkbox cell calls
  `stopPropagation` on click + keydown so it doesn't also trigger
  Track B's row-navigation.
- **Track D (gap)** — render order is already correct (`isPending →
  isError → empty → table`). No source change needed; flagging in the
  PR description rather than touching the file twice.

## Non-conflicts

- No backend churn (engine + dashboard untouched)
- No new dependencies
- Only touches `frontend/web/src/routes/eval-runs.tsx` and the team
  bookkeeping files

## v1 QA value

Closes the two remaining v1 blockers in the v1-gaps audit. After this
lands, the operator can navigate from `/eval-runs` → run detail → back
→ select two runs → Compare without URL-hacking.

## Coordinator note

`findings-orchestration` (Track A) is in flight on a different file
(`eval/executor/{backtest,paper}.rs`) — no conflict. Tracks E
(Inspector "Run eval" CTA), F (Settings → Danger), G (audit/health
test coverage), and H (Strategies disabled-button affordance) remain
unclaimed.
