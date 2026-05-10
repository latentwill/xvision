---
from: search-index
to: all
topic: pr-open
created_at: 2026-05-10T14:00:00Z
ack_required: false
---

# `search-index` track — PR #37 open

PR: https://github.com/latentwill/xvision/pull/37
Branch: `feature/command-palette-backbone`
Worktree: `.worktrees/search-index`
Base: `origin/main` @ `594c825`

## What landed

Plan #12 backbone — migration `004_search_index.sql` + the `SearchIndex`
CRUD that every future indexer hook will call. The plan draft's inline
rusqlite has been converted to a numbered .sql migration + sqlx (matches
how 001/002 ship; lines up with `v1-shipping-plan.md` §"Migration
reservations" guidance).

## Files this PR touches

- `crates/xvision-engine/migrations/004_search_index.sql` (new)
- `crates/xvision-engine/src/api/mod.rs` (one-line additive — embed +
  execute migration 004)
- `crates/xvision-engine/src/lib.rs` (one-line additive — `pub mod search;`)
- `crates/xvision-engine/src/search/{mod,index}.rs` (new)

## Tested

- 6 unit tests on the SearchIndex CRUD (upsert + search by title,
  upsert dedup, search by tag, kind filter, empty-query recency
  fast-path, delete)
- `cargo test --workspace` — **453 passed, 0 failed**

## Hooks for downstream tracks

After this lands, any writer can index its artifact with one line:

```rust
xvision_engine::search::SearchIndex::upsert(pool, &IndexEntry {
    artifact_id: bundle.manifest.id.clone(),
    kind: SearchKind::Strategy,
    title: bundle.manifest.display_name.clone(),
    summary: bundle.manifest.plain_summary.clone(),
    tags: bundle.manifest.regime_fit.clone(),
    updated_at: chrono::Utc::now(),
    href: format!("/authoring/{}", bundle.manifest.id),
}).await?;
```

Natural follow-ups (each its own PR):

- `bundle::store::save` indexer (Strategy)
- `eval::store::finalize` indexer (Run)
- `findings::extractor::record` indexer (Finding)
- `engine::api::search::query` audit-emitting wrapper
- `xvn search <query>` CLI dogfood
- Dashboard `/api/search` endpoint (Plan 2d)
- ⌘K modal + JS (Plan 2d)

## Zero overlap with active sessions

- PR #27 (provider add/remove/check) — different crates
- PR #29 (Plan #7 Phase 5 docs) — docs only
- PR #31 (strategy-2a-mcp-authoring, just merged) — `xvision-mcp`
- PR #32 (BacktestExecutor) — `xvision-engine/src/eval/executor`
- PR #34 (xvn eod) — `xvision-cli`
- Wizard loop (in flight) — `xvision-dashboard` + frontend
- `eval-3d-compare` — already merged as PR #28
