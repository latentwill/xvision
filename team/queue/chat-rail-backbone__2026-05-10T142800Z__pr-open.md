---
from: chat-rail-backbone
to: all
topic: pr-open
created_at: 2026-05-10T14:28:00Z
ack_required: false
---

# `chat-rail-backbone` track — PR #44 open

PR: https://github.com/latentwill/xvision/pull/44
Branch: `feature/chat-rail-backbone`
Worktree: `.worktrees/chat-rail-backbone`
Base: `origin/main` @ `d6d8250`

## What landed

Plan #11 backbone — migration `003_chat_sessions.sql` + the
`ChatSessionStore` CRUD + the `ContextScope` enum that downstream
WizardLoop / REST / frontend rail PRs will consume. Plan draft's inline
rusqlite converted to numbered .sql + sqlx (matches 001/002/004; aligns
with `v1-shipping-plan.md` §"Migration reservations" guidance).

## Files this PR touches

- `crates/xvision-engine/migrations/003_chat_sessions.sql` (new)
- `crates/xvision-engine/src/api/mod.rs` (one-line additive — embed +
  execute migration 003)
- `crates/xvision-engine/src/lib.rs` (one-line additive — `pub mod
  chat_session;`)
- `crates/xvision-engine/src/chat_session/{mod,context,store}.rs` (new)

## Tested

- 15 unit tests (8 ContextScope + 7 ChatSessionStore) covering chip-set
  counts, route fallback, default behavior, JSON round-trip, monotonic
  seq, history order, ON DELETE CASCADE, scope persistence, scope update,
  last_activity_at update, forward-compat fallback on unknown scope
  variants
- `cargo test --workspace` — **504 passed, 0 failed**

## Hooks for downstream tracks

After this lands, the WizardLoop refactor can do:

```rust
let sid = ChatSessionStore::create_session(&pool, &ContextScope::Workspace).await?;
ChatSessionStore::append(&pool, &sid, "user", &user_blocks).await?;
let history = ChatSessionStore::load_history(&pool, &sid).await?;
ChatSessionStore::append(&pool, &sid, "assistant", &assistant_blocks).await?;
```

Natural follow-ups (each its own PR):

- Phase B — WizardLoop refactor to accept `Arc<ChatSessionStore>` +
  `session_id` (depends on PR #36 landing first)
- Phase C — `/api/chat-rail/*` REST + SSE endpoints in `xvision-dashboard`
- Phase D — `_chat_rail.html` partial + `chat_rail.js` (collapse,
  per-route state, quick replies, Start-fresh button)

## Zero overlap with active sessions

- PR #35 (eval-3d-progress / SSE) — `eval/executor`
- PR #36 (WizardLoop backend) — `xvision-dashboard` + `authoring.rs`;
  lib.rs insertions are at different alphabetical positions, no conflict
- PR #38 (MCP eval browse verbs) — `xvision-mcp`

## v1 progress

| Build-order item | Status |
|---|---|
| #11 Chat Rail Persistence | 🟡 backbone (this PR); B/C/D deferred |
| #12 Command Palette | 🟡 backbone (#37 merged); indexers/API/UI deferred |
| Migration 003 | ✅ this PR |
