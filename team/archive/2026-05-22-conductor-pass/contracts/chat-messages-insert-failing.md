---
track: chat-messages-insert-failing
lane: leaf
wave: qa-chat-rail-2026-05-21
worktree: .worktrees/chat-messages-insert-failing
branch: task/chat-messages-insert-failing
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/chat_session/**
  - crates/xvision-engine/tests/chat_session*.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/agents/**
  - crates/xvision-engine/src/authoring.rs
  - crates/xvision-engine/src/api/strategy.rs
  - crates/xvision-engine/src/strategies/**
  - crates/xvision-engine/src/strategies_folder/**
  - crates/xvision-dashboard/**
  - frontend/web/**
interfaces_used:
  - chat_session::store::Store (insert / next_seq / session lookup)
  - SQLite connection pool (workspace shared)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-engine chat_session
  - cargo clippy -p xvision-engine -- -D warnings
acceptance:
  - **Real error is captured.** The `.context("insert chat_messages row")` wrap at `crates/xvision-engine/src/chat_session/store.rs:98` no longer hides the underlying SQLx error. The wrapper either includes `e.to_string()` of the SQLx error in the context, or logs the structured SQLx error at error level on the way out. Operator-visible message names the SQLite error class (`UNIQUE constraint failed`, `FOREIGN KEY constraint failed`, `database is locked`, pool timeout, etc.).
  - **Root-cause investigation documented in the PR.** Three sequential failures in one operator session — describe what state the chat session and the strategy-draft transaction were in at the time. Three plausible root causes to rule in/out:
    - `(session_id, seq)` unique-constraint collision (the seq counter did not advance because a prior insert failed and the seq read came from a rolled-back transaction).
    - FK on `session_id` referencing a `chat_sessions` row that was rolled back by an enclosing transaction that ALSO ran the failed `create_strategy` write.
    - Connection-pool / transaction-state corruption — the same connection was returned to the pool mid-transaction after the strategy write failed, then reused for the next message insert.
  - **Fix lands at the root cause, not as a retry loop.** No `for _ in 0..3 { try_insert(); }` band-aids.
  - **Reproduction test.** Add a unit/integration test under `crates/xvision-engine/tests/` (or sibling) that reproduces the failing condition deterministically: e.g., interleave a failing strategy-write transaction with a chat-message insert and assert the insert succeeds (or fails with a clear error code that the dashboard can surface, not a silent context wrap).
  - **No retry without a justification comment.** If the fix needs any retry (e.g., for `database is locked`), the retry is explicit with a comment naming the SQLite error class and a bounded loop count.
  - **Operator-facing error string is informative.** The chat surface (whatever consumes this error) now sees something more useful than "stream error: insert chat_messages row" — at minimum, the SQLite error class.
  - **No changes outside listed allowed paths.**
---

# Scope

Three sequential `insert chat_messages row` stream errors hit the
operator's 2026-05-21 session on consecutive messages ("yes" /
"Summarize this week" / "can you finish the strategy"). The
`.context("insert chat_messages row")` wrap at
`crates/xvision-engine/src/chat_session/store.rs:87-98` swallows
the actual SQLx error, so we cannot see what the database was
saying.

Start by surfacing the real error. Then reproduce. Then fix at
the root cause. The strong hypothesis (because the failures
clustered immediately after a series of failed `create_strategy`
writes) is that a shared SQLite connection / transaction was left
in a broken state by the strategy-write rollback and the next
chat-message insert inherited it. But do not assume — verify
with the captured error code first.

# Out of scope

- Any changes outside `crates/xvision-engine/src/chat_session/`
  and its tests. The chat-rail wire format, the dashboard's
  stream handler, the wizard tool dispatch — all untouched.
- New schema. If a `(session_id, seq)` unique constraint is
  missing or wrong, that becomes a migration request in a
  follow-up contract — not done here.
- A unified retry framework. Out of scope; this is a targeted
  root-cause fix.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/chat-messages-insert-failing status
git -C .worktrees/chat-messages-insert-failing log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/chat-messages-insert-failing \
  -b task/chat-messages-insert-failing origin/main
```

# Notes

Parallel-safe with everything else in this wave. The path overlap
with `templates-elimination` is minimal (templates-elimination
touches `crates/xvision-engine/src/...` but explicitly forbids
`chat_session/**` paths). No coordination needed.

Append checkpoints / PR links below.
