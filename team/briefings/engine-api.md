# Briefing — `engine-api` track

You are working on **Engine API Foundation** (Plan #3 in `v1-shipping-plan.md`).

## Plan

[`docs/superpowers/plans/2026-05-10-engine-api-foundation.md`](../../docs/superpowers/plans/2026-05-10-engine-api-foundation.md)

## Why this matters

This is the **critical path**. Every subsequent v1-test plan writes its CLI
handlers and MCP tools as thin wrappers around `engine::api::<domain>::<fn>(ctx, req)`.
Without this scaffolding, every downstream track has to reinvent its dispatcher
shape — and we'll get parallel implementations of the same business logic in
CLI handlers and MCP handlers.

## Skills required

Before starting:
- `superpowers:executing-plans` — for the overall plan execution loop
- `superpowers:test-driven-development` — every phase is TDD: failing test → implement → green
- `superpowers:verification-before-completion` — never claim a phase done without running its tests

## Phases (from the plan)

- [x] Phase 1 — Migration + crate plumbing (sqlx + `001_api_audit.sql`)
- [x] Phase 2 — `api::mod` types (ApiContext, Actor, ApiError, ApiResult)
- [x] Phase 3 — `api::audit::record` + Outcome enum
- [x] Phase 4 — `api::strategy::{list, get}` representative ops
- [x] Phase 5 — `api/README.md` documenting the pattern for downstream plans

## Branch / worktree

- Worktree: `.worktrees/engine-api`
- Branch: `feature/engine-api-foundation`
- Open the PR titled "feat(engine): typed api foundation (#3)" when all phases land green.

## Cross-track contracts you own

When you commit, downstream tracks pick up these guarantees:

1. `xvision_engine::api::ApiContext` is constructable with a `SqlitePool`,
   `Actor`, and `xvn_home: PathBuf`.
2. `Actor` has all four variants: `Cli`, `Mcp`, `AgentRunner`, `Scheduler`.
   The latter two are unused in v1 test but must compile.
3. `ApiError` has variants `NotFound`, `Validation`, `Conflict`, `Internal`,
   `Db(sqlx::Error)`, `Other(anyhow::Error)`.
4. `audit::record(ctx, domain, op, target, args_json, outcome, duration_ms)`
   is the single audit entry-point.
5. Migration `001_api_audit.sql` lives at
   `crates/xvision-engine/migrations/001_api_audit.sql` and creates table
   `api_audit`.

When all of those are committed, post a queue message:

```
team/queue/engine-api__<utc>__phase-a-complete.md
to: all
ack_required: false

Engine API Foundation merged to main. Downstream tracks may now begin.
Pattern documented in crates/xvision-engine/src/api/README.md.
```

## Tips specific to this plan

- The `StrategyBundle` field names in Phase 4 (`agent_id`, `template_name`)
  may not match the merged shape from Plan #1. Read
  `crates/xvision-engine/src/bundle/manifest.rs` first and adjust the bindings.
- The `FilesystemStore::load` error type may not be `anyhow::Error`. Adjust
  the `map_err` in `api::strategy::get` to match the real shape.
- `tempfile` may not yet be a dev-dep on `xvision-engine`. Check `Cargo.toml`
  before writing tests; add to `[dev-dependencies]` if missing.
- Run `cargo test -p xvision-engine` after every phase to catch regressions
  in adjacent code paths.

## Completion definition

- All five phases committed individually.
- `cargo test -p xvision-engine` green.
- `cargo build --workspace` green.
- PR opened against `main` with title `feat(engine): typed api foundation (#3)`.
- `team/MANIFEST.md` updated to flip Phase A → Phase B for downstream tracks.
- Queue message `engine-api__*__phase-a-complete.md` posted.
