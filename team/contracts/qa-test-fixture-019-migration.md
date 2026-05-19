---
track: qa-test-fixture-019-migration
lane: leaf
wave: qa-operator-2026-05-19
worktree: .worktrees/qa-test-fixture-019-migration
branch: task/qa-test-fixture-019-migration
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agents/store.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/agents/model.rs
  - crates/xvision-engine/src/agents/templates.rs
  - crates/xvision-engine/src/agents/max_tokens_resolution.rs
interfaces_used:
  - The migration registry pattern at `crates/xvision-engine/src/api/mod.rs:30-51`
verification:
  - cargo test -p xvision-engine --lib agents::store::tests
  - cargo test -p xvision-engine --lib
acceptance:
  - All 7 `agents::store::tests::*` pass:
    - `get_returns_none_for_missing`
    - `name_exists_uniqueness_check`
    - `explicit_max_tokens_round_trips`
    - `none_max_tokens_round_trips_as_unset`
    - `create_then_get_round_trips`
    - `update_replaces_slots`
    - `list_excludes_archived_by_default`
  - The test fixture at `crates/xvision-engine/src/agents/store.rs:301-303`
    applies BOTH `005_agents.sql` (the original table) AND
    `019_agent_slot_prompt_version.sql` (the column `AgentStore`
    production code writes). Keep the explicit-migration-list pattern
    (mirrors `crates/xvision-engine/src/api/mod.rs:30-51`) rather than
    switching to `sqlx::migrate!("./migrations")` — the file-list
    approach makes future drift visible at the file site.
  - Brief inline comment at the fixture explains WHY both migrations
    are needed: `// 019 adds agent_slots.prompt_version, which
    AgentStore::insert_slot writes on every save. Without it, every
    test that creates an agent fails on insert.`
  - The fixture remains minimal: do NOT apply migrations beyond what
    the agents-store tests actually exercise. The fix is additive —
    005 + 019 only.
  - No production code changes. The bug is purely in the test
    fixture's migration list.
  - `cargo test -p xvision-engine --lib` baseline failures (other than
    these 6 fixed by this PR) — document in the PR body that they
    reproduce on `origin/main` with WIP stashed.
parallel_safe: true
parallel_conflicts: []
---

# Scope

`crates/xvision-engine/src/agents/store.rs:301-303` only applies
`005_agents.sql` in the in-memory test fixture:

```rust
// Apply only the agents migration — sufficient for store tests.
let migration = include_str!("../../migrations/005_agents.sql");
sqlx::query(migration).execute(&pool).await.unwrap();
```

That comment is stale. PR #296 (`harness-prompt-version-field`, F-3,
merged 2026-05-18) added migration `019_agent_slot_prompt_version.sql`,
which adds a NOT NULL DEFAULT '' column `prompt_version` to
`agent_slots`. `AgentStore::insert_slot` writes that column on every
save. Tests that call `store.create(...)` or `store.update(...)` fail
with:

```
table agent_slots has no column named prompt_version
```

6 of 7 `agents::store::tests::*` fail on `origin/main` from a fresh
build (the seventh — `get_returns_none_for_missing` — doesn't write,
so it passes).

Fix: extend the test fixture to apply 019 in addition to 005. Mirror
the explicit migration-list pattern from
`crates/xvision-engine/src/api/mod.rs:30-51`. Keep it minimal.

Anchor reading:

- PR #296 (`harness-prompt-version-field`) — added the migration.
- PR #329 (`qa-test-drift-2026-05-19`) worker reproduced the
  failure with stash-and-test evidence against `origin/main@06f6728`.
- `crates/xvision-engine/src/api/mod.rs:30-51` — the explicit-list
  migration pattern this fixture should mirror at small scale.

# Out of scope

- Switching the fixture to `sqlx::migrate!("./migrations")` (would
  apply 14+ unrelated migrations, slowing every test).
- Updating production code — `AgentStore` is correct; only the test
  fixture is out of date.
- Adding a "fixture-runs-all-migrations" helper. If repeated drift
  becomes a pattern, file a separate refactor track.
- Touching other crates' test fixtures (`chat_session/store.rs`,
  `search/index.rs`, etc.) even though they use the same pattern —
  only `agents/store.rs` has a confirmed broken state today.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/qa-test-fixture-019-migration status
git -C .worktrees/qa-test-fixture-019-migration log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/qa-test-fixture-019-migration \
  -b task/qa-test-fixture-019-migration origin/main
```

# Notes

Append checkpoints / PR links below.
