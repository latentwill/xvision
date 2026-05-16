# Claim: qa9-chat-rail-inflight-controls

Worktree: `.worktrees/qa9-chat-rail-inflight-controls`

Branch: `qa9-chat-rail-inflight-controls`

Owner: codex

## Scope

Keep the dashboard chat rail composer editable while a chat turn/tool call is in
flight. Add a stop/cancel control that aborts the active chat request and leaves
the draft text intact.

## Verification plan

- ChatRail in-flight typing regression test.
- ChatRail abort request regression test.
- Route persistence smoke where covered by existing chat rail tests.
- `corepack pnpm --dir frontend/web test -- ChatRail`
- `corepack pnpm --dir frontend/web typecheck`
