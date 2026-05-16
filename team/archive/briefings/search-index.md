# Track briefing — `search-index`

**Plan:** [Command Palette](../../docs/superpowers/plans/2026-05-10-command-palette-plan.md). v1 build-order item #12.

**Worktree:** `.worktrees/search-index`
**Branch:** `feature/command-palette-backbone`
**Base:** `origin/main` @ `a9c04b0`

## Why this track (and why this slice)

Plan 12 owns migration `004_search_index.sql` per
[`v1-shipping-plan.md`](../../v1-shipping-plan.md) §"Migration reservations".
Without the migration, no other track can index artifacts; without the index,
the ⌘K modal Plan 2d wants is dead.

The Wizard track (Plan 9) is in flight in another loop. Plan 12 is fully
independent of Plan 9 — different files, different DB tables. Shipping the
backbone now unblocks both:

1. The Wizard can call `engine::api::search::*` once the API surface lands.
2. Future indexer hooks (run finalize, bundle save) plug into the same surface.

## Scope of *this* PR (backbone only)

1. `crates/xvision-engine/migrations/004_search_index.sql` — FTS5 virtual
   table per the plan's schema, but normalized to a numbered `.sql` migration
   matching how 001/002 ship (instead of inline rusqlite as the plan draft
   uses).
2. `crates/xvision-engine/src/api/mod.rs` — wire migration 004 into
   `ApiContext::open`.
3. `crates/xvision-engine/src/search/` — new module:
   - `mod.rs` re-exports.
   - `index.rs` — `SearchKind` enum, `IndexEntry` struct, `upsert` /
     `delete` / `search` async fns on a `SqlitePool` (sqlx, not rusqlite —
     matches every other module's DB pattern).
4. Unit tests covering: upsert + search by title, upsert dedup, search by
   tags, kind filter, empty-query fast-path returns recent rows.

## Out of scope (deferred — call out in PR body)

- **Per-artifact indexer hooks** — wiring into `bundle::store::save`,
  `eval::store::finalize`, `findings::extractor::record`, etc. Each indexer
  is a one-line `index.upsert(&entry).await?` in the writer's success path
  and needs its own small PR.
- **`engine::api::search::query` API surface** — wraps the module fns and
  emits an audit row. Lands once at least one indexer is populating data.
- **`xvn search <query>` CLI** — dogfood/test surface. Trivial once the
  API surface exists.
- **Dashboard `/api/search` endpoint** — depends on Plan 2d's axum routing.
- **⌘K modal** — frontend; depends on Plan 2d's `base.html`.
- **Bootstrap reindex on dashboard startup** — needs the indexers to exist
  first.

## Files this track touches (zero overlap with active sessions)

- `crates/xvision-engine/migrations/004_search_index.sql` (new)
- `crates/xvision-engine/src/api/mod.rs` (1-line additive — embed + execute
  migration 004)
- `crates/xvision-engine/src/search/{mod,index}.rs` (new)
- `crates/xvision-engine/src/lib.rs` (1-line additive — `pub mod search;`)

Active sessions checked:
- PR #27 (`provider add/remove/check`) — `xvision-cli`, `xvision-eval`
- PR #29 (Plan #7 Phase 5 docs) — docs only
- PR #32 (BacktestExecutor) — `xvision-engine/src/eval/executor`
- PR #34 (`xvn eod`) — `xvision-cli`
- Wizard track (in another loop) — likely `xvision-dashboard` + frontend
- `eval-3d-compare` worktree — `xvision-cli` + `eval/compare`
- `llm-providers-5` (PR #27) — already noted

`api/mod.rs` and `lib.rs` get one-line additive insertions; `search/`
crate is new. Zero file overlap.

## v1 QA value

After this lands, any future writer can `index.upsert(&entry).await?` to
make an artifact ⌘K-searchable. The cost is one line per write site. Plan
12's UI work has a working backend the moment Plan 2d ships its dashboard
chrome.
