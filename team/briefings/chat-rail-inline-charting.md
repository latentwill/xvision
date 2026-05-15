# Track briefing — `chat-rail-inline-charting`

**Plan:** [Chat Rail Inline Charting](../../docs/superpowers/plans/2026-05-14-chat-rail-inline-charting.md).
**Spec:** [Chat Rail Inline Charting Design](../../docs/superpowers/specs/2026-05-14-chat-rail-inline-charting-design.md).
**Companion track:** [`mobile-first-framework`](./mobile-first-framework.md).

**Worktree:** `.worktrees/chat-rail-inline-charting`
**Branch:** `feature/chat-rail-inline-charting`

## Why this track

This track adds typed rich display blocks and custom SVG inline chart cards to
the chat rail. It deliberately does not use TradingView chart instances inside
chat messages; full eval charting remains owned by the TradingView chart plans.

## First implementation move

Start with Plan Tasks 1 and 2: add shared rich block payload types and
validation/downsampling guardrails. Frontend card rendering can proceed once the
types are stable.

## Coordination notes

- Coordinate with `mobile-first-framework` before changing the extracted chat
  renderer files. If the primitive extraction has not landed, either wait or
  make a small preparatory PR limited to rich block types.
- This track owns rich payloads, `ContentBlock` extensions, inline SVG chart
  renderers, and non-chart rich cards.
- Do not implement TradingView chart replacement here.

## Verification

- `cargo test -p xvision-engine rich_blocks`
- `cd frontend/web && pnpm typecheck`
- `cd frontend/web && pnpm build`
- Manual chat-history reload check with rich cards persisted in history.
