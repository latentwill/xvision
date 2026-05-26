# Multi-Asset Follow-Up Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Implement task-by-task, with a clean worktree per track.

**Status:** Post-merge follow-up for PRs #592 and #593.

**Goal:** Finish the product and maintenance work intentionally left after the first multi-asset merge: close missing UI/CLI edges, add the next execution modes only when their contracts are safe, and pay down the Clawpatch fixture debt that masked real regressions during review.

**Base:** `origin/main` after merge commit `3d232430b0d465f69ee6e568a7f2d9bd4bf9fb4e`.

**Reference:** `docs/superpowers/plans/2026-05-24-multi-asset-strategies.md`

## Related Plans And Ownership Boundaries

Two sibling follow-up plans share the live-execution surface. To avoid
duplicated or colliding work, ownership is split as follows:

- **`docs/superpowers/plans/2026-05-25-cline-live-followups.md`** owns the
  live execution loop end to end: splitting the executor into backtest/live
  branches, the single-asset live loop (L1, §3), the multi-asset live fanout
  (L2, §4), Alpaca paper broker wiring, and Cline trajectory recording.
  **All live executor code lives there, not here.**
- **`docs/superpowers/plans/2026-05-25-testnet-venue-plan.md`** owns the
  `Venue`/`Environment` model, `BrokerSurface` contract hardening (T1), and
  Orderly/Bybit testnet venues. The cross-venue broker-safety contract tests
  that gate live execution are built there; Alpaca paper fill tests are built
  in the cline-live plan.

This plan therefore does **not** implement any live executor behaviour. Its
only live-related deliverable is a thin **validation-layer safety gate**
(Phase 2) that rejects multi-asset live runs until cline-live L2 lands, plus
operator copy explaining the rejection. Everything else here is backtest,
authoring UI, signals, CLI/MCP audit, and test-fixture cleanup.

## Scope Summary

Shipped in #592/#593:

- Strategies own `asset_universe`; scenarios are asset-free.
- Backtest supports `ExecutionMode::PerAsset` with `CapitalMode::Pooled`.
- `eval run --assets` and experiment-run asset subsets thread into backtest.
- Dashboard authoring can edit `asset_universe`; scenario forms are asset-free; eval run detail has a per-asset rollup.

Known remaining work:

- Scenario chart/detail still uses a BTC/USD standalone preview asset because scenarios have no strategy context.
- Live execution remains single-asset gated.
- `Portfolio`, `Custom`, and non-pooled capital modes parse but reject.
- Pair/Global signals and cross-asset selector agents are schema-ready but not runtime-ready.
- Clawpatch still reports low-severity test fixture duplication across CLI/dashboard/engine tests.

## Phase 1 — Scenario Preview Asset Surface

**Problem:** Asset-free scenarios still need an operator-chosen asset for standalone chart previews and bar-cache fetches. The current post-merge fix makes this explicit as BTC/USD, but that is a preview default, not a complete UI.

**Files likely touched:**

- `crates/xvision-engine/src/api/chart.rs`
- `crates/xvision-dashboard/src/routes/scenarios.rs`
- `frontend/web/src/api/chart.ts`
- `frontend/web/src/routes/scenarios-detail.tsx`
- `frontend/web/src/components/chart/ScenarioChart.tsx`
- `frontend/web/src/components/scenario/useBarsFetchJob.ts`

Tasks:

- [ ] Add optional `asset` query param to `GET /api/scenarios/:id/chart`.
- [ ] Keep default `BTC/USD` for backward compatibility when the param is absent.
- [ ] Include the resolved preview asset in `ScenarioChartPayload` or reuse existing cache metadata if the payload already exposes it sufficiently.
- [ ] Add a compact preview-asset selector on scenario detail, near the chart timeframe selector.
- [ ] Thread selected asset into chart query keys and bars-fetch CLI jobs.
- [ ] Update tests for BTC default, ETH/SOL override, cache-key isolation, and UI fetch args.

Acceptance:

- `pnpm --dir frontend/web typecheck`
- `pnpm --dir frontend/web test scenarios-detail.test.tsx`
- `cargo test -p xvision-dashboard --test http scenario_chart_returns_cache_status_for_canonical`
- New backend test proving `asset=ETH/USD` computes an ETH-specific cache key.

## Phase 2 — Multi-Asset Live Safety Gate (validation layer only)

**Problem:** Backtest is multi-asset, but the live loop (owned by the
cline-live plan) starts single-asset. Until cline-live L2 (§4) ships
multi-asset fanout, a multi-asset strategy submitted to live mode must be
rejected at the validation layer with actionable text — never silently run on
one asset.

**Boundary:** This phase touches **only the request-validation/API layer**. It
does **not** split the executor loop, build single-asset live, or implement
multi-asset live — those are cline-live `§3` (L1) and `§4` (L2). The design
note here is an *input* the cline-live plan consumes for its L2 contract, not
executor code authored in this plan.

**Files likely touched:**

- `crates/xvision-engine/src/api/eval.rs` (live request validation only)
- `crates/xvision-engine/tests/eval_executor_live_*` (gate tests only)
- dashboard/CLI copy surfaces for the rejection message

**Files explicitly NOT touched here** (owned by cline-live):
`crates/xvision-engine/src/eval/executor/{backtest,live_source}.rs`,
`crates/xvision-execution/src/*`.

Tasks:

- [ ] Write a design note for live multi-asset invariants: broker position lookup, order submit asset, per-asset risk checks, pooled vs per-asset capital semantics, and kill-switch behavior. Hand it to the cline-live plan as the L2 contract; do not implement it here.
- [ ] Add a live preflight in `api/eval.rs` validation that rejects multi-asset strategies (`asset_universe.len() > 1`) in live mode with a clear, actionable message.
- [ ] Add tests proving the gate rejects multi-asset live and passes single-asset live unchanged (no executor-loop changes).
- [ ] Add dashboard/CLI copy that explains why live multi-asset is unavailable when rejected.
- [ ] Cross-link both plans, noting this gate is removed when cline-live L2 lands.

Acceptance:

- A multi-asset live request rejects cleanly with actionable text at the validation layer.
- No silent fallback to the first asset in live mode.
- No edits to the executor loop or `xvision-execution` (those belong to cline-live).
- Existing live tests still pass.

## Phase 3 — Execution And Capital Modes

**Problem:** `ExecutionMode` and `CapitalMode` are now strategy data, but only `PerAsset + Pooled` is implemented.

Modes to design:

- `ExecutionMode::Portfolio`
- `ExecutionMode::Custom`
- `CapitalMode::PerAsset`
- Future custom capital policy variants

Tasks:

- [ ] Write behavior specs for `Portfolio` vs `PerAsset`: one agent call over a portfolio briefing vs one call per asset.
- [ ] Define prompt/briefing shape for portfolio mode, including open positions for all assets and market snapshots per asset.
- [ ] Define equity and drawdown accounting for `CapitalMode::PerAsset`.
- [ ] Add not-implemented tests first for every unsupported combination, then replace one mode at a time with real behavior.
- [ ] Add CLI/dashboard labels for mode compatibility and disabled states.

Acceptance:

- Unsupported combinations remain explicit validation errors.
- Implemented modes have focused executor tests and one API-level eval run test.
- Strategy authoring cannot save an unsupported mode without clear feedback, unless the mode is intentionally stored as experimental and rejected at run time.

## Phase 4 — Pair/Global Signal Producers And Selector Agent

**Problem:** `SignalScope::{Pair, Global, Custom}` exists, but runtime producers still behave as per-asset filters.

Tasks:

- [ ] Define how a Filter declares its output scope in strategy/agent config.
- [ ] Add `Global` filter execution that runs once per timestamp and fans signal output into downstream per-asset trader calls.
- [ ] Add `Pair` signal tests for pair-specific cache keys and briefing selection.
- [ ] Design a cross-asset selector capability that can choose a subset/ranking before Trader calls.
- [ ] Add trace/observability labels for signal scope, active assets, and selector outputs.

Acceptance:

- Multi-filter per-asset isolation tests remain green.
- New tests prove global signals are not recomputed redundantly per asset.
- Pair/global signals appear in trader briefings only when scope matches the current asset context.

## Phase 5 — CLI And MCP Surface Audit

**Problem:** Core CLI paths were updated, but the long tail should be audited after the asset-free scenario change.

**Boundary:** Audit only **scenario asset-free** copy here. Live paper-only
help copy is owned by cline-live `§5`; venue/environment CLI/dashboard copy is
owned by the testnet plan `T5`. Do not edit those surfaces in this track.

Surfaces to audit:

- `xvn scenario *`
- `xvn eval run`, `eval batch`, `eval compare`, `eval probe-lookahead`, `eval review`
- `xvn experiment run`
- `xvn strategy new/show/validate/clone`
- MCP tools in `crates/xvision-mcp/src/tools.rs`
- Dashboard wiki/generated docs and `.claude`/agent-facing CLI references

Tasks:

- [ ] Search all user-facing help/docs for stale `scenario asset` copy.
- [ ] Add CLI snapshot tests for `scenario create --help`, `eval run --help`, and `experiment run --help`.
- [ ] Add MCP schema tests proving scenario create/clone are asset-free and eval run exposes asset subset.
- [ ] Update operator docs to explain “scenario market descriptor” vs “strategy traded assets.”

Acceptance:

- `rg "scenario.*asset|asset.*scenario" README.md MANUAL.md docs crates frontend` has no stale user-facing instructions, except intentional historical docs.
- CLI and MCP schemas match the generated TypeScript types.

## Phase 6 — Clawpatch Fixture Debt

**Problem:** Clawpatch still reports low-severity duplication. One duplication finding (`eval_batch_run` migrations) already hid real failures. The rest should be handled as maintenance tracks, not mixed into feature work.

**Boundary:** Scope is CLI/dashboard/engine **eval-fixture** duplication. Cline
sidecar/pool/trajectory test-harness cleanup is owned by cline-live `§6` — do
not fold it into this track.

Open cleanup clusters:

- CLI tests: repeated scenario setup, repeated batch request literals, duplicated strategy fixtures, substring JSON assertions.
- Dashboard tests: duplicated Strategy fixtures, provider TOML skeletons, RunStore bootstrap.
- Engine tests: repeated migration ladders and eval-run fixture construction.

Tasks:

- [ ] Create shared CLI integration-test helpers for scenario creation, strategy seeding, and parsed JSON assertions.
- [ ] Create shared dashboard test helpers for `RunStore`, Strategy fixtures, and provider config TOML.
- [ ] Create or reuse engine test helpers for migrated `RunStore`/`ApiContext` setup.
- [ ] Convert `eval_batch_run` request construction to a local builder.
- [ ] Re-run `clawpatch review --since origin/main --mode deslopify` and revalidate fixed findings.

Acceptance:

- Clawpatch fixture-duplication findings are either fixed or explicitly triaged with rationale.
- `cargo test -p xvision-cli --test eval_batch_run --test eval_compare_report --test scenario_cli --test strategy_cli`
- `cargo test -p xvision-dashboard --test http`
- Relevant engine executor/API tests remain green.

## Suggested Track Split

Use separate PRs in this order:

1. `multi-asset-preview-asset-selector` — Phase 1.
2. `multi-asset-live-safety-gate` — Phase 2 validation-layer rejection + operator copy + design note. No executor code.
3. `multi-asset-clawpatch-fixtures` — Phase 6 cleanup, no product behavior.
4. `multi-asset-portfolio-mode-spec` — Phase 3 design and not-implemented tests (backtest only).
5. `multi-asset-global-signals` — Phase 4 Global signal producer.

Multi-asset **live execution** is intentionally **not** a track here. The
single-asset live loop (L1) and multi-asset live fanout (L2) are owned by
`2026-05-25-cline-live-followups.md` `§3`–`§4`. When cline-live L2 lands it
removes the Phase 2 gate; coordinate that removal across both plans.

## Non-Goals

- Do not reintroduce `Scenario.asset`.
- Do not silently choose `asset_universe[0]` in new multi-asset paths.
- Do not implement live multi-asset execution here — it is owned by `2026-05-25-cline-live-followups.md` `§4`. This plan only gates it at the validation layer (Phase 2).
- Do not touch the live executor loop, `live_source.rs`, or `xvision-execution` in this plan (cline-live owns them).
- Do not add `Venue`/`Environment` selection or testnet venues — owned by `2026-05-25-testnet-venue-plan.md`.
- Do not bundle broad fixture refactors into product PRs unless the fixture duplication is directly breaking that PR. Cline sidecar/pool/trajectory test-harness cleanup belongs to cline-live `§6`, not Phase 6 here.
