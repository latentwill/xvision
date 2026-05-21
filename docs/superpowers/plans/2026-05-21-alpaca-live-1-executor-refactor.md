# Alpaca Live — Phase 1: Executor refactor

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse the two-executor world (`BacktestExecutor` + `PaperExecutor`) into a single `Executor` parameterized by three small traits — `BarSource`, `Clock`, `FillSink`. Delete `PaperExecutor` entirely. Collapse `RunMode` to `{ Backtest, Live }` across both `xvision-engine::eval::run` and `xvision-core::config`. Backtest mode must continue to work end-to-end during and after the refactor; Live mode is not implemented here (it lands in Phase 2 + 3).

**Architecture:** One `Executor` struct in `crates/xvision-engine/src/eval/executor/mod.rs` holds `&dyn BarSource`, `&dyn Clock`, `&dyn FillSink` (or owned generic params, depending on what borrows cleanly with the existing `async_trait` shape). The per-cycle inner code (pipeline dispatch, observability, min-notional gate, kill-switch, watchdog heartbeat, circuit breaker) lives in shared helpers reused across both modes. Today's `BacktestExecutor` becomes `InjectedBars` (`BarSource`) + `InstantClock` (`Clock`) + `SimulatedFills` (`FillSink`). `PaperExecutor` is deleted; nothing replaces it in this phase (Live's `LiveStream` + `WallClock` + `RealBrokerFills` arrive in Phase 2 + 3). The confused-deputy `VenueLabel` gate is rewired to read from `LiveConfig` (when present) rather than `Scenario.venue_label`.

**Tech Stack:** Rust 2021, tokio, async_trait, anyhow, existing `xvision-engine` / `xvision-core` workspace.

**Reference spec:** `team/intake/2026-05-21-alpaca-live-eval-and-executor-refactor.md` §Decisions locked #7, §Track sequencing #1.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/xvision-engine/src/eval/executor/mod.rs` | Modify | New `BarSource`, `Clock`, `FillSink` traits. Unified `Executor` struct + impl. Delete `Executor` trait if it becomes redundant (today there's an `async_trait Executor` over `BacktestExecutor`/`PaperExecutor`). |
| `crates/xvision-engine/src/eval/executor/backtest.rs` | Refactor | Becomes the `InjectedBars` + `InstantClock` + `SimulatedFills` impls. The orchestration code moves into the unified `Executor`. |
| `crates/xvision-engine/src/eval/executor/paper.rs` | **Delete** | Entire file removed. Any inner-cycle helpers that lived here move into shared helpers in `mod.rs`. |
| `crates/xvision-engine/src/eval/run.rs` | Modify | `RunMode` becomes `{ Backtest, Live }`. `RunMode::Paper` removed. `RunMode::parse` updates. |
| `crates/xvision-core/src/config.rs` | Modify | `RunMode` mirrors the engine enum — `{ Backtest, Live }`. |
| `crates/xvision-engine/src/eval/store.rs` | Modify | `RunStore` queries that match on `RunMode` strings update; persisted rows with `"paper"` are migrated to `"backtest"` via a one-shot migration. |
| `crates/xvision-engine/migrations/<next>_paper_to_backtest.sql` | Create | `UPDATE eval_runs SET mode = 'backtest' WHERE mode = 'paper';` Reserve migration number with the conductor. |
| `crates/xvision-engine/migrations/<next>_paper_to_backtest.down.sql` | Create | Down migration (logically irreversible — note in the file that paper-mode runs cannot be restored). |
| `crates/xvision-engine/src/safety/gate.rs` | Modify | `confused-deputy` gate reads from a `&LiveConfig` parameter instead of `&Scenario` (`LiveConfig` doesn't exist yet — for Phase 1, parameterize the gate so Phase 3 can pass it; pass `None` for backtest mode). |
| `crates/xvision-engine/src/eval/review/payload.rs` | Modify | `run_mode_str` drops the `Paper` arm. |
| `crates/xvision-engine/src/eval/compare.rs` | Modify | Drop `RunMode::Paper` references. |
| `crates/xvision-engine/src/eval/postprocess.rs` | Modify | Test data using `RunMode::Backtest`; no behavioral change. |
| `crates/xvision-engine/src/eval/batch_store.rs` | Modify | Same — tests updated. |
| `crates/xvision-engine/src/eval/export.rs` | Modify | Many test sites with `RunMode::Backtest`; sanity-check none reference `Paper`. |
| `crates/xvision-engine/tests/api_eval_run.rs` | Modify | Update any test referencing paper mode. |
| `crates/xvision-engine/tests/eval_store.rs` | Modify | Same. |
| `crates/xvision-engine/tests/eval_run_scenario.rs` | Modify | Same. |
| `crates/xvision-engine/tests/eval_broker_circuit_breaker.rs` | Modify | The circuit-breaker tests use paper-eval today; rehome under `Backtest` mode with a mock `FillSink` that produces the same broker-error class strings. |
| `crates/xvision-engine/tests/broker_rules_integration.rs` | Modify | Same. |
| `crates/xvision-engine/tests/api_eval_min_notional.rs` | Modify | Same. |
| `crates/xvision-engine/tests/safety_gate.rs` | Modify | Gate now reads `LiveConfig` parameter; backtest path passes `None`. |
| `frontend/web/src/api/types.gen/RunMode.ts` | Regenerate via ts-rs | Drops `Paper`. |
| Various frontend call sites | Modify | Anywhere the UI offered "Paper" as a launch option — remove the option; for now, Backtest is the only launch path until Phase 3 lands Live. |
| `docs/superpowers/specs/2026-05-14-alpaca-paper-eval-surface-design.md` | (already SUPERSEDED) | No further change. |

---

## Phase A — Traits + unified Executor (the core extraction)

- [ ] A1. Define `BarSource` trait in `eval/executor/mod.rs`. Methods: `next_bar(&mut self) -> Option<Ohlcv>` (or async equivalent), `warmup_bars(&self) -> &[Ohlcv]`, `len_hint(&self) -> Option<usize>`. Document async vs. sync semantics in the trait doc-comment.
- [ ] A2. Define `Clock` trait. Methods: `now(&self) -> DateTime<Utc>`, `wait_until(&self, deadline: DateTime<Utc>) -> impl Future` (or sync no-op for `InstantClock`).
- [ ] A3. Define `FillSink` trait. Methods: `submit(&self, order: OrderRequest) -> Result<FillReport>`. The error type produces the existing `classify_run_failure` taxonomy (broker_auth / broker_unsupported / etc.); see `eval/executor/mod.rs` lines 89–170 for the existing error-class wire shape.
- [ ] A4. Implement `InjectedBars` (wraps `Vec<Ohlcv>`) as the backtest `BarSource`. Move logic from `backtest.rs`.
- [ ] A5. Implement `InstantClock` — wall-clock-skip. Move from `backtest.rs`.
- [ ] A6. Implement `SimulatedFills` — fold the existing sim-fill code from `backtest.rs` (slippage / fees / fill_model). Must preserve `FillProvenance` writes.
- [ ] A7. Write the unified `Executor::run` method. Single per-bar loop. Calls `BarSource::next_bar`, runs pipeline, calls `FillSink::submit` for any orders, updates equity. Shares all today's gates (min-notional, kill-switch, watchdog, circuit breaker).

## Phase B — Delete PaperExecutor

- [ ] B1. Identify every direct reference to `PaperExecutor` (grep `PaperExecutor`).
- [ ] B2. For each call site, replace with the unified `Executor` + appropriate trait impls. Most paper-eval callers are tests; the production path was a CLI flag that is being removed.
- [ ] B3. Delete `crates/xvision-engine/src/eval/executor/paper.rs`.
- [ ] B4. Remove the `pub mod paper;` line in `eval/executor/mod.rs`.
- [ ] B5. Update `eval/executor/mod.rs::Executor` (the trait) — either remove it entirely if the unified `Executor` is a concrete struct, or simplify to a single `BacktestExecutor` (now an alias for `Executor` with backtest trait impls). Document the choice in the file's header doc-comment.
- [ ] B6. `cargo build --workspace` must pass. `cargo test --workspace` must pass after Phase D test updates.

## Phase C — RunMode collapse + DB migration

- [ ] C1. Edit `xvision-engine::eval::run::RunMode` to `{ Backtest, Live }`. Delete `Paper`. Update `parse`, `Display`, `Debug`, any serde annotations.
- [ ] C2. Mirror in `xvision-core::config::RunMode`.
- [ ] C3. Create migration `<next>_paper_to_backtest.sql` + down. Reserve number with conductor via `team/MANIFEST.md`.
- [ ] C4. `RunStore::load_run_row` (around line 1417 in `store.rs`) — handle legacy `"paper"` mode strings by mapping to `Backtest` (defensive — DB migration should have already updated them).
- [ ] C5. Add a one-shot startup check: log a warning if any `eval_runs.mode = 'paper'` rows remain post-migration.

## Phase D — Test sweep

- [ ] D1. Grep `RunMode::Paper` across `crates/`; replace each with `RunMode::Backtest`.
- [ ] D2. Grep `"paper"` (lowercase string) across `crates/`; identify any string-typed mode references; update.
- [ ] D3. The circuit-breaker + broker-rules tests exercise the broker-error class taxonomy. Rehome under `Backtest` mode with a `MockFillSink` that produces the same wire-shape error strings (`broker_auth`, `broker_unsupported`, etc.). Confirm `classify_run_failure` test coverage doesn't regress.
- [ ] D4. The safety-gate tests use `Scenario.venue_label`. Migrate to passing `Option<&LiveConfig>` to the gate; backtest tests pass `None`.
- [ ] D5. `cargo test --workspace` passes with zero `Paper` references remaining.
- [ ] D6. `cargo clippy --workspace -- -D warnings` passes.

## Phase E — Frontend cleanup

- [ ] E1. Regenerate ts-rs types — `RunMode.ts` drops `"Paper"`.
- [ ] E2. Grep frontend for `RunMode.Paper` / `"paper"` string usage. Remove options from launch forms.
- [ ] E3. The eval-runs list / detail UI may have paper-specific badges or copy — replace with backtest copy. Live mode launches arrive in Phase 3; for now, only backtest is launchable.
- [ ] E4. `pnpm --dir frontend/web typecheck` passes.
- [ ] E5. `pnpm --dir frontend/web test --run` passes.
- [ ] E6. `pnpm --dir frontend/web lint` passes.

## Verification

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
pnpm --dir frontend/web typecheck
pnpm --dir frontend/web test --run
pnpm --dir frontend/web lint
bash scripts/board-lint.sh
# Migration smoke:
rm -f /tmp/xvn-refactor-test.db && cargo run -p xvn -- doctor
# Run a known backtest and confirm metrics match a pre-refactor baseline:
cargo run -p xvn -- eval run --strategy <known-strategy> --scenario <known-scenario>
```

## Acceptance

- `PaperExecutor` is deleted; no source file references the name.
- `RunMode` is `{ Backtest, Live }` in both `xvision-engine` and `xvision-core`.
- A backtest run with the same strategy/scenario pair produces metrics within rounding of a pre-refactor baseline (no behavioral regression).
- All `classify_run_failure` test cases pass; broker-error class strings are unchanged at the wire.
- `confused-deputy` venue gate accepts an `Option<&LiveConfig>`; backtest mode passes `None` without panicking.
- Migration is reversible to the extent practical (down marks paper-mode unrecoverable but is otherwise clean).
- Frontend offers no Paper launch option; types compile; tests pass.

## Out of scope

- `LiveStream` / `WallClock` / `RealBrokerFills` impls — those land in Phase 2 + 3.
- `LiveConfig` struct itself — Phase 3 introduces it. Phase 1 only needs the gate to accept an `Option<&LiveConfig>` parameter; the type can be a stub or imported from a Phase 3 preview module.
- Any Filter / agent-graph composition wiring — that's a separate track (see `docs/superpowers/plans/2026-05-21-filter-v1.md`). The unified `Executor` is the surface Filter v1 Stage 2 will hook into.

## Source links

- `team/intake/2026-05-21-alpaca-live-eval-and-executor-refactor.md` — design intake / decisions.
- `crates/xvision-engine/src/eval/executor/mod.rs` — where the refactor lives.
- `crates/xvision-engine/src/eval/executor/paper.rs` — to be deleted.
- `team/archive/2026-05-21-conductor-sweep/contracts/paper-eval-inspector-parity.md` — historical paper-eval inspector parity contract; will not survive the refactor but listed here so the conductor doesn't reopen it.
