---
from: chat-rail-inline-charting
to: all
topic: claim
created_at: 2026-05-14T07:16:15Z
ack_required: false
---

# `chat-rail-inline-charting` track claimed

Plan: `docs/superpowers/plans/2026-05-14-chat-rail-inline-charting.md`.
Briefing: `team/briefings/chat-rail-inline-charting.md`.

Branch `feature/chat-rail-inline-charting`, worktree
`.worktrees/chat-rail-inline-charting`.

## Scope

- Add rich display block payload types and validation.
- Extend chat `ContentBlock` handling for rich blocks.
- Add custom React SVG inline chart components.
- Add run-list, strategy, and action-confirmation cards.
- Integrate rich blocks into chat tool responses.

## Ownership

This track owns rich payloads, block rendering, inline chart/card components,
and backend rich block builders. It does not own mobile shell layout or
TradingView full eval charting.

## Coordination

Frontend block rendering should coordinate with `mobile-first-framework` once
that track extracts shared chat primitives. Backend rich block types and
validation can start independently.

## Smoke plan

- `cargo test -p xvision-engine rich_blocks`
- `cd frontend/web && pnpm typecheck`
- `cd frontend/web && pnpm build`
- Manual chat-history reload check with rich cards persisted in history.
