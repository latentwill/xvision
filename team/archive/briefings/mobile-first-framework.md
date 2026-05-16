# Track briefing — `mobile-first-framework`

**Plan:** [XVN Mobile-First Framework](../../docs/superpowers/plans/2026-05-14-mobile-first-framework.md).
**Spec:** [XVN Mobile-First Framework Design](../../docs/superpowers/specs/2026-05-14-mobile-first-framework-design.md).
**Prototype:** `docs/design/xvn/project/xvn mobile design.html`.

**Worktree:** `.worktrees/mobile-first-framework`
**Branch:** `feature/mobile-first-framework`

## Why this track

This track turns the mobile prototype into production responsive shell behavior:
phone chat-as-home, mobile drawer, all-functions sheet, dashboard chat pill,
tablet split, and desktop three-pane target.

## First implementation move

Start with Plan Task 1: extract reusable chat primitives from
`frontend/web/src/components/shell/ChatRail.tsx` without changing desktop
behavior. This creates the shared surface that both the mobile shell and the
inline-charting track need.

## Coordination notes

- `chat-rail-inline-charting` depends on the extracted chat primitives. If
  both tracks run concurrently, keep ownership clean:
  - `mobile-first-framework` owns shell/layout/mobile components.
  - `chat-rail-inline-charting` owns rich block types and card/chart renderers.
- Do not change backend chat session semantics in this track.
- Do not implement inline chart payloads here; use placeholders only if needed
  to prove layout.

## Verification

- `cd frontend/web && pnpm typecheck`
- `cd frontend/web && pnpm build`
- Manual viewport checks at 390 x 844, 768 x 1024, and 1280 x 800.
