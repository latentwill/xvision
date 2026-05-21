---
track: track-plan-touches-engine
lane: foundation
wave: filter-v1
worktree: .worktrees/track-plan-touches-engine
branch: task/track-plan-touches-engine
base: task/track-plan-touches
status: in-progress
depends_on:
  - filter-v1
  - track-plan-touches
  - executor-trait-extraction
blocks:
  - filter-v1-export-and-summary
  - filter-v1-frontend-types-and-panels
  - filter-v1-regression-fixtures
stacking: declared:track-plan-touches
allowed_paths:
  - crates/xvision-engine/Cargo.toml
  - crates/xvision-engine/migrations/032_filters_and_evaluations.sql
  - crates/xvision-engine/migrations/032_filters_and_evaluations.down.sql
  - crates/xvision-engine/src/api/mod.rs
  - crates/xvision-engine/src/strategies/mod.rs
  - crates/xvision-engine/src/strategies/validate.rs
  - crates/xvision-engine/src/authoring.rs
  - crates/xvision-engine/src/eval/mod.rs
  - crates/xvision-engine/src/eval/filter_hook.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/progress.rs
  - crates/xvision-engine/src/eval/store.rs
  - crates/xvision-engine/tests/**
  - crates/xvision-dashboard/Cargo.toml
  - crates/xvision-dashboard/tests/**
  - Cargo.lock
  - team/contracts/track-plan-touches-engine.md
forbidden_paths:
  - crates/xvision-filters/**
  - crates/xvision-cli/**
  - crates/xvision-mcp/**
  - crates/xvision-memory/**
  - frontend/**
  - team/board.md
  - team/board-v2.md
  - team/MANIFEST.md
interfaces_used:
  - "xvision_filters runtime surface (Filter, ActivationMode, RuntimeFilter, FilterState, ActivationDecision, EvalContext, FilterEvalOutcome, Bar, validate)"
  - crate::eval::executor::traits::{BarSource, Clock, FillSink}
  - crate::eval::progress::{ProgressEvent, ProgressTx}
  - crate::strategies::Strategy
  - crate::eval::store::RunStore
  - sqlx::SqlitePool
parallel_safe: false
parallel_conflicts:
  - "Single-writer on crates/xvision-engine/src/eval/executor/backtest.rs per-bar loop body and the strategies::Strategy struct shape for the wave."
verification:
  - cargo build --workspace
  - cargo test --workspace
  - cargo clippy --workspace --all-targets -- -D warnings
  - cargo fmt --check
  - bash scripts/board-lint.sh
acceptance:
  - "**xvision-engine depends on xvision-filters** via a path dep added to `Cargo.toml`. Dependency direction is strictly engine → filters; filters never imports engine."
  - "**`Strategy` gains two new fields.** `activation_mode: ActivationMode` (default `EveryBar` for backward compat); `filter: Option<Filter>` (default `None`). Serde defaults preserve every existing strategy JSON file unchanged. The custom `Deserialize` impl (via `StrategyRaw`) mirrors the additions."
  - "**Strategy construction migrated.** Every `Strategy { ... }` struct literal across `crates/xvision-engine/src/` and `crates/xvision-engine/tests/` and `crates/xvision-dashboard/tests/` includes the two new fields. Default value is `ActivationMode::EveryBar` + `None`."
  - "**`FilterHook` lives in `crates/xvision-engine/src/eval/filter_hook.rs`.** Public surface: `FilterHook::new(strategy: &Strategy) -> anyhow::Result<Option<Self>>` returns `Ok(None)` for `EveryBar`, `Ok(Some(hook))` for `FilterGated`, and `Err(_)` for `CompiledRules` (E_FILTER_ACTIVATION_MODE_NOT_IMPL) or `FilterGated` without a filter (E_FILTER_GATED_WITHOUT_FILTER). `FilterHook::evaluate(&mut self, bar: &Ohlcv, in_position: bool) -> FilterEvalOutcome` runs the runtime once. `FilterHook::record(&self, pool, progress_tx, run_id, ts, outcome) -> anyhow::Result<()>` writes the `eval_filter_evaluations` row and emits the matching `ProgressEvent::FilterEvaluated`."
  - "**Per-bar hook lives in `crates/xvision-engine/src/eval/executor/backtest.rs` run_inner.** The hook is constructed once per run before the per-bar loop. Inside the loop, after `ProgressEvent::RunTick` emission (so the dashboard's progress bar advances on every bar even when the agent pipeline is skipped) and before the seed-build / pipeline call, the hook evaluates the bar. When the outcome is not `Active`, the per-bar body short-circuits via `i += 1; continue;` — no `seed` built, no `run_pipeline`, no `eval_decisions` row written for that bar."
  - "**`ProgressEvent::FilterEvaluated` variant added** with fields `{ run_id: String, bar_index: u64, ts: DateTime<Utc>, decision_tag: String, conditions_passed: Vec<bool>, tree_true: bool, trip: bool }`. Existing variants are untouched."
  - "**Migration `032_filters_and_evaluations.sql`** adds two tables: `filters` (forward-declared persistence layer; Stage 4 wires CRUD against it) and `eval_filter_evaluations` (per-bar ledger keyed `(run_id, bar_index)`). Reverse `.down.sql` drops both with their indices. The migration is registered in `crates/xvision-engine/src/api/mod.rs` via `migrate_filters_and_evaluations()` and called from the same `init()` chain as the other 031+ migrations."
  - "**Behavioral floor: no regression for `EveryBar` strategies.** Every existing test that constructs a `Strategy` (now with `activation_mode: ActivationMode::EveryBar, filter: None`) runs the per-bar loop as before. `FilterHook::new` returns `None`, the hook check is a single branch, and no `eval_filter_evaluations` rows are written."
  - "**Integration test `tests/eval_filter_hook.rs`** — *DEFERRED.* The acceptance bar for the integration test (runs a 200-bar fixture, asserts the agent pipeline is skipped on Inactive bars, asserts `eval_filter_evaluations` rows match the emitted events 1:1, asserts cooldown_bars suppresses re-trips) requires fixture setup that this contract's implementer host (extndly-dev, no Rust toolchain) cannot exercise. Tracked as a fast-follow once Parts 1+2 land on a build host. The unit tests in `xvision-filters` already cover the runtime semantics; the deferred integration test verifies the engine wiring."
  - "**`OnInvalidationOrTargetOnly` gating is treated as `Never` in v1.** Documented as a known gap. The runtime reports `SuppressedInPosition` from both `Never` and `OnInvalidationOrTargetOnly`; the engine has no hook to re-open the gate on invalidation/target events. Tracked as a follow-up."
  - "**Trace-span replacement (the \"super\" span audit) is deferred.** The intake mentioned replacing a possibly-pointless \"super\" span with a Filter span. An audit found `xvision.supervisor.note` in `crates/xvision-observability/src/otel.rs:416`, but that span is a meaningful operator-authored marker, not a summary — likely not the intended target. Deferred until the operator clarifies which surface to replace (eval trace dock UI? OTel span? cycle trace tree?). Tracked as the next sub-PR after this one merges."
  - "**Grep guards (must all pass):**"
  - "  - `rg --hidden -n 'FilterEvaluated' crates/xvision-engine/src/eval/progress.rs` → exactly one definition."
  - "  - `rg --hidden -n 'eval_filter_evaluations' crates/xvision-engine/migrations/` → exactly one CREATE TABLE."
  - "  - `rg --hidden -n 'use xvision_engine' crates/xvision-filters/` → no hits."
  - "  - `ls crates/xvision-engine/migrations/032_filters_and_evaluations.sql` → file exists."
  - "  - `rg --hidden -n 'activation_mode' crates/xvision-engine/src/strategies/mod.rs` → at least two hits (field + StrategyRaw + Deserialize)."
  - "**No changes outside listed allowed paths.** If implementation forces a touch outside `allowed_paths`, **STOP** and append a checkpoint under `# Notes`."
---

# Scope

Stage 2, Part 2 of the Filter v1 wave — the engine-side glue that
plugs the `xvision-filters` runtime (Part 1 / `track-plan-touches`)
into the backtest executor.

After this contract:

- `Strategy` carries `activation_mode` and an optional inline `filter`.
- The backtest `run_inner` constructs a per-run `FilterHook` and
  evaluates it inside the per-bar loop. When the filter says skip,
  the agent pipeline is not invoked for that bar (no LLM tokens, no
  decision row, no fill).
- Each bar's evaluation is persisted to `eval_filter_evaluations` and
  surfaced as a `ProgressEvent::FilterEvaluated`. These rows are the
  "plan touches" — the per-bar ledger of when the strategy's plan was
  exercised vs skipped.

The behavioral floor: **no regression for `EveryBar` strategies.** The
default mode runs the per-bar loop as today. New behavior fires only
when a strategy explicitly opts into `FilterGated`.

# Out of scope

- Live mode integration. The hook is wired only in the Backtest
  executor; the Live executor (when it ships) will plug into the same
  `RuntimeFilter` surface but is not exercised here.
- Filter CRUD (REST/CLI/MCP). Stage 4.
- `OnInvalidationOrTargetOnly` engine-side gate-reopen on invalidation
  /target events. Treated as `Never` in v1.
- Trace-span replacement (the "super" span audit). Pending operator
  clarification of which surface to replace.
- Frontend types and panels (Stage 4).
- Regression golden fixtures (Stage 5).

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/track-plan-touches-engine status
git -C .worktrees/track-plan-touches-engine log --oneline -3 task/track-plan-touches..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/track-plan-touches-engine
#   - base is task/track-plan-touches (Part 1)
```

# Notes

Free-form. Append checkpoints, surprises, links to PRs. Do not edit
history above the line.

- 2026-05-21 — split from `track-plan-touches` Part 1 because the
  implementer host (extndly-dev) has no Rust toolchain. Part 1 ships
  the pure-crate foundation; this contract ships the engine glue.
  Both PRs verified together on the operator's local build host.
- 2026-05-21 — `Strategy` constructors mass-migrated via brace-walking
  Python script (idempotent: skipped any block already containing
  `activation_mode`). Spot-check covered: every `Strategy {` literal
  in `crates/{xvision-engine,xvision-dashboard}/{src,tests}` has
  `activation_mode: xvision_filters::ActivationMode::EveryBar,
  filter: None,` appended before the closing brace.
- 2026-05-21 — "super" span audit deferred. Closest candidate is
  `xvision.supervisor.note` (otel.rs:416) but that's a meaningful
  guardrail marker, not summary noise. Awaiting operator clarification.