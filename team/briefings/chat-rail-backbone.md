# Track briefing — `chat-rail-backbone`

**Plan:** [Chat Rail Persistence](../../docs/superpowers/plans/2026-05-10-chat-rail-persistence-plan.md). v1 build-order item #11.

**Worktree:** `.worktrees/chat-rail-backbone`
**Branch:** `feature/chat-rail-backbone`
**Base:** `origin/main` @ `6c97dff`

## Why this track

Plan 11 owns migration `003_chat_sessions.sql` per
[`v1-shipping-plan.md`](../../v1-shipping-plan.md) §"Migration reservations".
Without it the rail Wizard turns into is per-session-only. Independent of
every open PR — backend-only `xvision-engine` change, no dashboard or
frontend touches.

Mirrors the SearchIndex slice that just shipped (PR #37): backbone
migration + sqlx-backed store + tests now, integration deferred.

## Scope of *this* PR (Phase A backbone only)

1. `crates/xvision-engine/migrations/003_chat_sessions.sql` — `chat_sessions`
   + `chat_messages` tables, normalized to a numbered `.sql` migration
   (the plan draft uses inline rusqlite — same conversion the v1 plan's
   migration registry calls out).
2. `crates/xvision-engine/src/api/mod.rs` — wire migration 003 between 002
   and 004 in `ApiContext::open`.
3. `crates/xvision-engine/src/chat_session/` — new module:
   - `store.rs` — `ChatSessionStore` over a `SqlitePool`. Methods:
     `create_session(scope) -> session_id`, `append(session_id, role, blocks) -> ChatMessage`
     (atomically computes next seq), `load_history(session_id)`,
     `touch(session_id)`, `delete_session(session_id)`.
   - `context.rs` — `ContextScope` enum (Workspace, Route, Run, Strategy,
     Deployment, Compare, JournalFilter, Selection, Seed) + the per-scope
     `quick_replies()` + `placeholder()` + `header_label()` data per the
     plan's §1.4 chip table.
4. Tests: round-trip insert + load_history; monotonic seq assignment;
   delete cascades messages; ContextScope quick_replies returns the
   expected count for each scope.

## Out of scope (deferred — call out in PR body)

- **WizardLoop refactor** to accept `Arc<ChatSessionStore>` + `session_id`
  (Phase B) — depends on PR #36's `wizard_loop.rs` landing + integration.
- **`/api/chat-rail/*` endpoints** (Phase C) — depends on Phase B.
- **`_chat_rail.html` partial + `chat_rail.js`** (Phase D) — frontend;
  depends on Phase C.

## Files this track touches (zero overlap with active sessions)

- `crates/xvision-engine/migrations/003_chat_sessions.sql` (new)
- `crates/xvision-engine/src/api/mod.rs` (one-line additive — embed +
  execute migration 003)
- `crates/xvision-engine/src/lib.rs` (one-line additive — `pub mod
  chat_session;` between `bundle` and `error`)
- `crates/xvision-engine/src/chat_session/{mod,store,context}.rs` (new)

Active PRs checked:
- PR #35 (eval-3d-progress / SSE) — `xvision-engine/src/eval/executor`
- PR #36 (WizardLoop backend) — `xvision-engine/src/authoring.rs` +
  dashboard + lib.rs (alphabetical insertion of `authoring` before
  `bundle`; my `chat_session` insertion goes between `bundle` and `error`
  — different position, no conflict)
- PR #38 (MCP eval browse verbs) — `xvision-mcp`

`api/mod.rs` and `lib.rs` get one-line additive insertions; `chat_session/`
module is new. Zero file overlap.

## v1 QA value

After this lands, every future Wizard turn can persist with
`store.append(&sid, "assistant", &blocks).await?`. The Wizard PR #36
authors hooked this up the moment migration 003 + the store exist. Owns
the registry slot that's been blocking Plan 11 for three days.
