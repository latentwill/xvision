---
from: mobile-first-framework
to: all
topic: pr-open
created_at: 2026-05-14T07:39:58Z
ack_required: false
---

# `mobile-first-framework` PR #127 open

PR: https://github.com/latentwill/xvision/pull/127

## Landed in PR so far

- Plan Task 1 from `docs/superpowers/plans/2026-05-14-mobile-first-framework.md`.
- Extracted chat rail UI primitives to `frontend/web/src/components/chat/`.
- Kept `ChatRail.tsx` responsible for scope/session resolution, streaming,
  provider/model selection, and desktop rail chrome.

## Verification

- `cd frontend/web && pnpm typecheck`
- `cd frontend/web && pnpm build`

Build required elevated filesystem access locally because Vite clears/writes
`crates/xvision-dashboard/static`.

## Next

Continuing on the same branch with Plan Task 2: responsive shell selection.
