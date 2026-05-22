---
track: track-plan-touches
lane: foundation
wave: filter-v1
worktree: .worktrees/track-plan-touches
branch: task/track-plan-touches
base: origin/main
status: in-progress
depends_on:
  - filter-v1
blocks:
  - track-plan-touches-engine
  - filter-v1-export-and-summary
  - filter-v1-frontend-types-and-panels
  - filter-v1-regression-fixtures
stacking: none
allowed_paths:
  - crates/xvision-filters/Cargo.toml
  - crates/xvision-filters/src/lib.rs
  - crates/xvision-filters/src/runtime.rs
  - crates/xvision-filters/src/indicators.rs
  - crates/xvision-filters/src/state.rs
  - crates/xvision-filters/tests/runtime.rs
  - crates/xvision-filters/tests/indicators.rs
  - Cargo.lock
  - team/contracts/track-plan-touches.md
forbidden_paths:
  - crates/xvision-engine/**
  - crates/xvision-cli/**
  - crates/xvision-dashboard/**
  - crates/xvision-mcp/**
  - crates/xvision-memory/**
  - frontend/**
  - team/board.md
  - team/board-v2.md
  - team/MANIFEST.md
interfaces_used:
  - "xvision_filters Stage 1 surface (Filter, ConditionTree, Condition, Operand, IndicatorRef, Operator, ActivationMode, ScanCadence, WakeInPosition, validate)"
  - chrono::{DateTime, Utc, Datelike}
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo build --workspace
  - cargo test -p xvision-filters
  - cargo clippy -p xvision-filters -- -D warnings
  - cargo fmt -p xvision-filters --check
  - bash scripts/board-lint.sh
acceptance:
  - "**xvision-filters gains three new modules** — `runtime.rs`, `indicators.rs`, `state.rs` — all engine-independent. `runtime.rs` exposes the per-bar entry point `RuntimeFilter::evaluate(state, bar, ctx) -> FilterEvalOutcome`. `indicators.rs` ships incremental implementations for all six v1 indicators (`ema_n`, `sma_n`, `rsi_n`, `atr_n`, `atr_pct_n`, `close`) plus an engine-independent `Bar` reduction. `state.rs` holds per-filter mutable runtime state (warmup countdown, previous-bar leaf cache for `crosses_*`, cooldown countdown in bars, daily wakeup counter)."
  - "**Indicator math is deterministic.** SMA is the trailing arithmetic mean of `period` closes. EMA seeds at bar `period` with SMA, then applies `α = 2 / (period + 1)`. RSI uses Wilder's smoothing on the first `period` deltas (so warmup = `period + 1` closes). ATR uses Wilder's smoothing on true range (warmup = `period + 1` closes). ATR% is `100 * ATR / close`. Tests cover known-value golden assertions for each indicator within 0.05."
  - "**Crosses detection is bar-transition-based.** `crosses_above` fires on bar t when the leaf was `false` on bar t-1 and `lhs[t] > rhs[t]` on bar t. The previous-bar leaf cache lives in `FilterState`. On the first post-warmup bar the cache is empty, so `crosses_*` does not fire — that is documented semantics, not a bug."
  - "**Warmup gating.** During warmup, `FilterEvalOutcome.decision == ActivationDecision::Warming { bars_left }`. The condition tree is not evaluated. Warmup is `max(warmup-per-indicator)` over the referenced indicators (SMA/EMA: `period`; RSI/ATR/ATR%: `period + 1`; Close: 0)."
  - "**Cooldown gating.** When the filter trips active (false → true), the cooldown countdown is armed to `Filter.cooldown_bars`. Subsequent bars where the tree is true report `Cooldown { bars_left }` and decrement the counter. Cooldown == 0 disables the gate."
  - "**Daily wakeup cap.** If `Filter.max_wakeups_per_day == Some(n)`, the per-UTC-day counter is checked before a Trip is recorded. When the cap is reached, the decision is `CappedForDay { wakeups_today }`. The counter rolls over when the bar's UTC date changes. Hold transitions never consume a wakeup."
  - "**WakeInPosition.** The runtime takes a caller-supplied `in_position: bool` per bar. When `in_position == true` and `wake_when_in_position == Never` or `OnInvalidationOrTargetOnly`, the filter is reported as `SuppressedInPosition` even if the conditions trip. (The engine-side wiring of OnInvalidationOrTarget gating lands in the follow-up contract.) `Always` does not suppress."
  - "**`ActivationDecision` is the public surface.** Variants: `Warming { bars_left } | Inactive | Active { transition: Trip|Hold } | Cooldown { bars_left } | CappedForDay { wakeups_today } | SuppressedInPosition`. `is_active()`, `is_trip()`, `tag()` helpers are exposed. The enum is `Serialize + Deserialize` so the engine can persist it and emit it as event payload."
  - "**Engine independence guard.** `rg --hidden -n 'use xvision_engine' crates/xvision-filters/` → no hits. `xvision-filters` does not depend on `xvision-engine`, `xvision-core`, or any sqlx — `Bar` is a local engine-independent OHLCV reduction. The follow-up contract (`track-plan-touches-engine`) maps `xvision_core::market::Ohlcv` → `xvision_filters::Bar` at the call site."
  - "**ts-rs derives** on `ActivationDecision`, `Transition` use the existing `ts-export` feature gate. The generated `.ts` files under `frontend/web/src/api/types.gen/` are produced but NOT committed by this contract (Stage 4 wires the frontend types)."
  - "**Tests required (in `crates/xvision-filters/src/{indicators,state,runtime}.rs` `#[cfg(test)] mod tests`):**"
  - "  - Indicators: SMA warmup + value, EMA seed + recurrence, RSI Wilder seed against a hand-computed reference, ATR Wilder seed, Close indicator no-warmup, warmup_bars = max across instances, duplicate refs share one instance."
  - "  - State: warmup matches max period, cooldown arm + tick, wakeup rollover across UTC midnight, collect_indicator_refs dedups."
  - "  - Runtime: inactive→active→hold transitions for `close > N`, cooldown suppresses re-trip, capped-for-day blocks extra trips, suppressed-in-position when `wake_when_in_position == Never`, crosses_above fires on transition (close vs ema_3 example)."
  - "**Grep guards:**"
  - "  - `rg --hidden -n 'fn evaluate' crates/xvision-filters/src/runtime.rs` → at least one (`RuntimeFilter::evaluate`)."
  - "  - `rg --hidden -n 'use xvision_engine\\|use xvision_core' crates/xvision-filters/` → no hits."
  - "  - `rg --hidden -n 'sqlx' crates/xvision-filters/` → no hits."
  - "**No changes outside listed allowed paths.** If implementation forces a touch outside `allowed_paths`, **STOP** and append a checkpoint under `# Notes`."
---

# Scope

Stage 2, Part 1 of the Filter v1 wave. Lands the **runtime side** of
Filter v1 inside the engine-independent `xvision-filters` crate:
incremental indicator math, per-filter mutable state, and the per-bar
evaluator. After this contract:

- `xvision-filters` exports a runtime that any caller can hand a
  validated `Filter` + a `FilterState` + a stream of `Bar`s and get back
  per-bar `ActivationDecision` values (`Warming`, `Inactive`,
  `Active { transition }`, `Cooldown`, `CappedForDay`,
  `SuppressedInPosition`).
- The crate has zero engine deps. The follow-up contract
  (`track-plan-touches-engine`) maps `xvision_core::market::Ohlcv` →
  `xvision_filters::Bar` at the call site, adds the per-bar hook in
  `crates/xvision-engine/src/eval/executor/backtest.rs`, the
  `ProgressEvent::FilterEvaluated` variant, the migration
  (`032_filters_and_evaluations.sql`), the `Strategy` field additions,
  and the trace-span replacement (audit + replace the existing "super"
  span with a Filter span carrying `decision` + `conditions_passed`).

The behavioral floor for the engine-side follow-up: **no regression in
`activation_mode == EveryBar` runs** (the default). New behavior fires
only when a strategy opts into `FilterGated`.

# Out of scope (this PR — Part 1)

- Engine wiring (`xvision-engine` is in `forbidden_paths`). Strategy
  field additions, per-bar hook, migration, ProgressEvent variant,
  trace-span replacement all move to `track-plan-touches-engine`.
- Live mode integration.
- Filter CRUD (REST/CLI/MCP). Stage 4.
- `OnInvalidationOrTargetOnly` engine-side gating semantics. The
  runtime exposes the field but the engine's invalidation/target
  event surface is not wired through.
- `xvision-indicators` shared-crate extraction (v1.5 chore).
- LLM-backed filters / SlotRuntime::Llm / Expr / edge graphs. All v1.5.
- Frontend types and panels (Stage 4).
- Regression golden fixtures (Stage 5).

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/track-plan-touches status
git -C .worktrees/track-plan-touches log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/track-plan-touches
#   - base is up to date with origin/main (or rebase planned)
```

# Error-code stability contract

This contract does not introduce new `E_FILTER_*` codes — only the
runtime surface (`ActivationDecision`). The follow-up
`track-plan-touches-engine` contract will introduce strategy-level
validation codes (`E_FILTER_ACTIVATION_MODE_NOT_IMPL`,
`E_FILTER_GATED_WITHOUT_FILTER`).

The Stage 1 codes (`E_FILTER_UNKNOWN_INDICATOR` etc.) are unchanged.

# Notes

Free-form. Append checkpoints, surprises, links to PRs. Do not edit
history above the line.

- 2026-05-21 — contract drafted in PR alongside implementation (Stage 1
  contract directed this — "Stage 2 contract will be drafted after this
  Stage 1 PR opens, not before"). Track slug `track-plan-touches`
  reflects the eventual per-bar plan-touch ledger; the Stage 1
  `blocks:` list refers to the same body of work as
  `filter-v1-backtest-evaluator`.
- 2026-05-21 — scope split into Part 1 (this contract: pure runtime in
  `xvision-filters`) and Part 2 (`track-plan-touches-engine`: engine
  wiring + migration + per-bar hook + trace-span replacement). Split
  driven by the build constraint on the implementer host
  (extndly-dev is RAM-constrained; large multi-crate PRs are hard to
  verify locally before push). Part 1 lands the foundation that
  Part 2 plugs into; Part 2 is mechanical glue on top.
- 2026-05-21 — `OnInvalidationOrTargetOnly` parked as a known gap.
  Runtime carries the field; engine-side will treat it as `Never`
  until the invalidation/target event-bus surface lands.
- 2026-05-21 — user-added scope item for Part 2: replace the existing
  "super" span in the eval trace with a Filter span carrying the
  decision + conditions_passed. Pending an audit of the span emitter
  path; tracked as part of `track-plan-touches-engine`'s acceptance.
