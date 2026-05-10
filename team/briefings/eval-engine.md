# Briefing — `eval-engine` track (Phase 3.A only)

You are working on **Phase 3.A of the Eval Engine plan**: the eval module
foundations. The plan spans 16 tasks across 5 phases (3.A → 3.E); this
briefing scopes only **Tasks 1–3** (Phase 3.A).

## Plan

[`docs/superpowers/plans/2026-05-08-eval-engine-plan.md`](../../docs/superpowers/plans/2026-05-08-eval-engine-plan.md)

## Why this scope cut

The full eval engine plan (~1240 lines, 16 tasks) is too large for a single
PR. Phase 3.A is the keystone: it lays the SQLite migration, run / scenario
types, and the run store + event store that every subsequent eval task
builds on. Once Phase 3.A merges, Phases 3.B (executors), 3.C (metrics +
findings), 3.D (compare + CLI + MCP), and 3.E (polish) become parallel-
launchable as separate tracks.

## Skills required

Before starting:
- `superpowers:executing-plans` — for the overall plan execution loop
- `superpowers:test-driven-development` — every task is TDD: failing test → impl → green
- `superpowers:verification-before-completion` — never claim a phase done without running its tests

## Phase 3.A scope (this track, this PR)

### Task 1 — SQLite migration `002_eval.sql` + Run + RunStatus types

- Owns migration `002_eval.sql` per the registry in `v1-shipping-plan.md`.
- Tables: `eval_runs`, `eval_events`, `eval_attestations`, `scenarios`.
- Rust types: `Run`, `RunStatus`, `RunMode` in `crates/xvision-engine/src/eval/run.rs`.
- TDD: write `tests/eval_run_types.rs` for serde round-trip + status transitions.

### Task 2 — Scenario type + canonical scenario set

- `Scenario` struct in `crates/xvision-engine/src/eval/scenario.rs`.
- A `canonical_scenarios()` fn returning the 4 baseline BTC-only fixture
  scenarios (per `v1-shipping-plan.md` decision: "BTC-only for v1 test").
- TDD: `tests/eval_scenario.rs` verifying the canonical set is non-empty,
  all entries reference BTC-only assets, and the scenario IDs are unique.

### Task 3 — RunStore + EventStore

- `RunStore` in `crates/xvision-engine/src/eval/store.rs` — sqlx wrapper
  over the migrated tables. Methods: `create`, `update_status`, `get`,
  `list`, `record_event`, `read_events`.
- TDD: `tests/eval_store.rs` covering empty list, create-then-get,
  status transitions, event ordering.

## What you do NOT do (out of scope, deferred to follow-up PRs)

- ❌ `Executor` trait, `BacktestExecutor`, `PaperExecutor` — Phase 3.B
- ❌ Metrics computation (Sharpe, drawdown, win rate) — Phase 3.C Task 7
- ❌ Findings extractor — Phase 3.C Task 8
- ❌ Signed attestation — Phase 3.C Task 9
- ❌ Run-set comparison — Phase 3.D Task 10
- ❌ `xvn eval` CLI — Phase 3.D Task 11
- ❌ Eval MCP verbs — Phase 3.D Task 12
- ❌ SSE progress endpoint — Phase 3.D Task 13
- ❌ Migrate `xvision-eval` baselines to LLM-shim templates — Phase 3.E
- ❌ `engine::api::eval::*` module — write only the migration + types + store;
  the api dispatch layer can land in the Phase 3.B PR with the executors.

The PR's exit criterion is: foundation in place such that Phase 3.B can be
written as a fresh implementation against this stable surface.

## Branch / worktree

- Worktree: `.worktrees/eval-engine`
- Branch: `feature/eval-engine-foundation`
- PR title: `feat(eval): module foundations — migration 002 + Run/Scenario types + RunStore`

## Cross-track contracts you produce

When this lands, downstream Phase B tracks pick up:

1. `crates/xvision-engine/migrations/002_eval.sql` — the only owner of the
   eval-domain tables. Other plans must NOT add eval tables in their own
   migrations.
2. `xvision_engine::eval::run::{Run, RunStatus, RunMode}` — every executor
   constructs `Run`s; every dashboard query reads them.
3. `xvision_engine::eval::scenario::{Scenario, canonical_scenarios}` —
   the canonical fixture scenarios eval runs against.
4. `xvision_engine::eval::store::RunStore` — the persistence layer for runs
   + events. Phase 3.B executors call `record_event` per decision /
   fill / status change.

When complete, post a queue message:

```
team/queue/eval-engine__<utc>__phase-3a-pr-open.md
to: all
ack_required: false

Eval Engine Phase 3.A merged. Phase 3.B (executors) is now unblocked —
PaperExecutor wraps Arc<dyn BrokerSurface>; BacktestExecutor reads from
fixture parquet + records to RunStore.
```

## Tips specific to this plan

- **Migration `002_eval.sql` claims its number from `v1-shipping-plan.md`
  §"Migration reservations".** That registry says `002_eval.sql` is owned
  by this plan. Don't claim a different number.
- The `Run` type wants both `RunStatus` (queued / running / completed /
  failed / cancelled — see `ui-elements.md` status pill set) and `RunMode`
  (backtest / paper).
- `RunStore` follows the same sqlx pattern as `xvision_engine::api::audit::record`
  — see `crates/xvision-engine/src/api/audit.rs` for the canonical shape.
- `tests/` files mirror the per-domain test pattern from the engine API
  foundation (`tests/api_strategy.rs` is a good template).
- BTC-only constraint per `v1-shipping-plan.md` §Preconditions — every
  canonical scenario MUST reference BTC/USD only. The Alpaca symbol map
  is hardcoded BTC/USD at `crates/xvision-execution/src/alpaca.rs`.
- `cargo test -p xvision-engine` after every task to catch regressions
  in adjacent code paths (api/* tests in particular).

## Completion definition

- Three tasks implemented with TDD discipline (failing test → impl → green).
- `cargo test -p xvision-engine` green (existing 11 api/* tests + new eval
  tests, target ~25+ pass / 0 fail).
- `cargo build --workspace` green.
- PR opened against `main` titled `feat(eval): module foundations — migration 002 + Run/Scenario types + RunStore`.
- Queue message `eval-engine__*__phase-3a-pr-open.md` posted.
- `team/MANIFEST.md` updated to mark eval-engine Phase 3.A complete and
  list Phase 3.B as ready for pickup.
