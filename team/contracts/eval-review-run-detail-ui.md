---
track: eval-review-run-detail-ui
lane: leaf
wave: eval-review
worktree: .worktrees/eval-review-run-detail-ui
branch: task/eval-review-run-detail-ui
base: origin/main
status: ready
depends_on:
  - eval-review-api-cli
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/eval-runs-detail.tsx
  - frontend/web/src/features/eval-runs/review/**
  - frontend/web/src/api/eval-review.ts
  - frontend/web/src/themes/**                # only review panel theme tokens, not theme system
forbidden_paths:
  - crates/**
  - frontend/web/src/features/chat-rail/**
  - frontend/web/src/features/scenarios/**
  - frontend/web/src/themes/index.ts          # theme registry owned by color-themes track
interfaces_used:
  - GET /api/eval/runs/:id/reviews
  - GET /api/eval/reviews/:id
  - POST /api/eval/runs/:id/review
parallel_safe: false
parallel_conflicts:
  - any-track-editing eval-runs-detail.tsx
verification:
  - corepack pnpm --dir frontend/web test -- eval-runs-detail
  - corepack pnpm --dir frontend/web typecheck
  - corepack pnpm --dir frontend/web build
acceptance:
  - Review panel renders on `/eval-runs/:id`.
  - Agent picker shows seeded review agent profiles (Fast Trader, Reasoning, Risk, Research).
  - Verdict badge + confidence value rendered.
  - Sections: executive summary, key findings, risks, evidence, recommended next tests, open questions.
  - Regenerate-with-another-agent action functional.
  - Empty / inconclusive states styled.
---

# Scope

Add the Review panel to the existing `/eval-runs/:id` route. No parallel
page; extend `eval-runs-detail.tsx` and add a `features/eval-runs/review/`
folder for the sub-components.

# Out of scope

- Cross-run review compare view.
- Persistent review filters / saved views.
- New theme tokens — reuse what the `color-themes-light-dark` track already
  established.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/eval-review-run-detail-ui -b task/eval-review-run-detail-ui origin/main
```

# Notes

- `eval-runs-detail.tsx` is on the conflict-zone list. Coordinate with any
  active track touching that file before starting.
- The visible scrollbar treatment from `qa10-eval-chat-scrollbars-controls`
  is now baseline; reuse those styles for the review panel scroll container.
