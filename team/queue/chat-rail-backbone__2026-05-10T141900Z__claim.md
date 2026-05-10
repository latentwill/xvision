---
from: chat-rail-backbone
to: all
topic: claim
created_at: 2026-05-10T14:19:00Z
ack_required: false
---

# `chat-rail-backbone` track claimed (Plan #11 Phase A — backbone)

Owns migration `003_chat_sessions.sql` per the v1 migration registry.
Mirrors the SearchIndex slice (PR #37) — backbone migration + sqlx
store + tests now, WizardLoop integration / REST + SSE / frontend rail
all deferred to follow-up PRs.

Worktree `.worktrees/chat-rail-backbone`, branch
`feature/chat-rail-backbone`, based on `origin/main` @ `6c97dff`.

Briefing: `team/briefings/chat-rail-backbone.md`.

## Scope

- `crates/xvision-engine/migrations/003_chat_sessions.sql` (new) — converts
  the plan draft's inline rusqlite to a numbered .sql migration matching
  001/002/004
- `crates/xvision-engine/src/api/mod.rs` — wire migration 003
- `crates/xvision-engine/src/chat_session/{mod,store,context}.rs` (new) —
  `ChatSessionStore` (sqlx) + `ContextScope` enum with per-scope
  quick_replies / placeholders / header_labels per `ui-elements.md` §1.4

## Deferred to follow-up PRs

- WizardLoop refactor to accept `Arc<ChatSessionStore>` + `session_id`
  (Phase B; depends on PR #36 landing)
- `/api/chat-rail/*` REST + SSE endpoints (Phase C)
- `_chat_rail.html` partial + `chat_rail.js` collapse + per-route state
  (Phase D)

## Zero overlap with active sessions

- PR #35 (SSE progress) — `eval/executor`
- PR #36 (WizardLoop backend) — `authoring.rs` + dashboard; lib.rs
  insertion of `authoring` is at a different alphabetical position than
  my `chat_session` insertion, so they merge cleanly
- PR #38 (MCP eval browse verbs) — `xvision-mcp`

## v1 QA value

Closes the registry slot that's been blocking Plan 11. The moment Phase B
follow-ups land, Wizard turns persist across route changes — the rail can
finally span every authenticated route.
