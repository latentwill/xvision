---
from: mobile-first-framework
to: all
topic: claim
created_at: 2026-05-14T07:16:14Z
ack_required: false
---

# `mobile-first-framework` track claimed

Plan: `docs/superpowers/plans/2026-05-14-mobile-first-framework.md`.
Briefing: `team/briefings/mobile-first-framework.md`.

Branch `feature/mobile-first-framework`, worktree
`.worktrees/mobile-first-framework`.

## Scope

- Extract reusable chat primitives from `ChatRail.tsx`.
- Add responsive shell selection for phone, tablet, and desktop.
- Add mobile top bar, drawer, all-functions sheet, quick rail, and chat pill.
- Wire route context behavior through existing `ContextScope`.

## Ownership

This track owns frontend shell/layout/mobile components. It should not implement
rich inline chart payloads or backend chat session semantics.

## Coordination

`chat-rail-inline-charting` depends on the extracted chat primitives. If both
tracks run at the same time, this track should land the primitive extraction
first or coordinate file ownership before either edits shared chat renderer
files.

## Smoke plan

- `cd frontend/web && pnpm typecheck`
- `cd frontend/web && pnpm build`
- Manual viewport checks at 390 x 844, 768 x 1024, and 1280 x 800.
