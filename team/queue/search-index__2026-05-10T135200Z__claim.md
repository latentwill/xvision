---
from: search-index
to: all
topic: claim
created_at: 2026-05-10T13:52:00Z
ack_required: false
---

# `search-index` track claimed (Plan #12 — Command Palette backbone)

Picking up the backbone slice of Plan #12 (build-order item 12 in
`v1-shipping-plan.md`). Owns migration `004_search_index.sql` per the
migration registry.

The Wizard track (Plan 9) is in flight in another loop; Plan 12 is fully
independent (different files, different tables). Backbone landing now
unblocks both Wizard and the future ⌘K UI.

Worktree `.worktrees/search-index`, branch `feature/command-palette-backbone`,
based on `origin/main` @ `a9c04b0`.

Briefing: `team/briefings/search-index.md`.

## Scope of *this* PR (backbone only)

- `crates/xvision-engine/migrations/004_search_index.sql` (new) — FTS5
  virtual table, normalized to a numbered .sql file (the plan draft uses
  inline rusqlite; we match how 001/002 ship)
- `crates/xvision-engine/src/api/mod.rs` (1-line additive — wire migration 004)
- `crates/xvision-engine/src/search/{mod,index}.rs` (new) — SearchKind +
  IndexEntry types, upsert/delete/search async fns on the SqlitePool (sqlx)
- `crates/xvision-engine/src/lib.rs` (1-line additive — `pub mod search;`)
- Unit tests: upsert+search by title, dedup, tags, kind filter, empty-query
  fast-path

## Deferred to follow-up PRs

- Per-artifact indexer hooks (one line per writer's success path)
- `engine::api::search::query` API surface + audit
- `xvn search <query>` CLI dogfood
- Dashboard `/api/search` endpoint (Plan 2d-dependent)
- ⌘K modal + JS (Plan 2d-dependent)
- Bootstrap reindex on dashboard startup

## Zero overlap with active sessions

- PR #27 / PR #29 / PR #31 / PR #32 / PR #34 — different crates/files
- Wizard loop — `xvision-dashboard` + frontend
- `eval-3d-compare` worktree — `xvision-cli` + `eval/compare`

`api/mod.rs` and `lib.rs` get one-line additive insertions; `search/`
module is new.

## v1 QA value

After this lands, any future writer can call
`SearchIndex::upsert(pool, &entry).await?` to make an artifact searchable.
Plan 12's UI work has a working backend the moment Plan 2d ships its
dashboard chrome.
