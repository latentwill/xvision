---
track: executor-trait-extraction
lane: foundation
wave: alpaca-live-eval-2026-05-21
worktree: .worktrees/executor-trait-extraction
branch: task/executor-trait-extraction
base: origin/main
status: ready
depends_on: []
blocks:
  - executor-collapse-paper-mode
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/executor/**
  - crates/xvision-engine/src/eval/mod.rs
  - crates/xvision-engine/tests/eval_executor_traits.rs
forbidden_paths:
  - frontend/web/**
  - crates/xvision-engine/migrations/**
  - crates/xvision-core/**
  - crates/xvision-dashboard/**
  - crates/xvision-cli/**
  - crates/xvision-mcp/**
  - crates/xvision-observability/**
  - crates/xvision-risk/**
  - crates/xvision-engine/src/eval/scenario.rs
  - crates/xvision-engine/src/eval/scenario_store.rs
  - crates/xvision-engine/src/eval/store.rs
  - crates/xvision-engine/src/eval/run.rs
  - crates/xvision-engine/src/eval/bars.rs
  - crates/xvision-engine/src/eval/broker_rules.rs
  - crates/xvision-engine/src/eval/preflight.rs
  - crates/xvision-engine/src/eval/guardrails.rs
  - crates/xvision-engine/src/eval/early_stop.rs
  - crates/xvision-engine/src/eval/metrics.rs
  - crates/xvision-engine/src/eval/postprocess.rs
  - crates/xvision-engine/src/eval/findings/**
  - crates/xvision-engine/src/eval/review/**
  - crates/xvision-engine/src/api/**
  - crates/xvision-engine/src/safety/**
  - crates/xvision-engine/src/strategies/**
  - crates/xvision-engine/src/agents/**
  - crates/xvision-engine/src/agent/**
interfaces_used:
  - eval::executor::Executor (existing trait, untouched)
  - eval::executor::BacktestExecutor (adopts new traits internally)
  - eval::cost (cost model types, read-only)
  - eval::executor::trace_types (read-only)
parallel_safe: false
parallel_conflicts:
  - "Holds single-writer on crates/xvision-engine/src/eval/executor/**. Sub-tracks executor-collapse-paper-mode and executor-live-shell wait on this contract to merge."
verification:
  - cargo build --workspace
  - cargo test --workspace
  - cargo clippy --workspace --all-targets -- -D warnings
  - bash scripts/board-lint.sh
  - "rg --hidden -n 'trait BarSource' crates/xvision-engine/src/eval/executor/ returns exactly one definition"
  - "rg --hidden -n 'trait Clock' crates/xvision-engine/src/eval/executor/ returns exactly one definition"
  - "rg --hidden -n 'trait FillSink' crates/xvision-engine/src/eval/executor/ returns exactly one definition"
  - "rg --hidden -n 'PaperExecutor' crates/ returns the same hit count as on origin/main (PaperExecutor untouched)"
acceptance:
  - "**Three new traits.** Defined in `crates/xvision-engine/src/eval/executor/` — either one new `traits.rs` file or three files (`bar_source.rs`, `clock.rs`, `fill_sink.rs`). Implementer's choice; whichever is cleaner."
  - "  - `pub trait BarSource: Send + Sync` — exposes a method that yields the next OHLCV bar or `None` to terminate. Backtest impl drains a `Vec<Ohlcv>` synchronously; future Live impl will await a websocket. The exact signature (async vs. sync, returned shape) is the implementer's call; document the choice and rationale in the PR. **Constraint:** it must be implementable both for an in-memory bar buffer AND a future polling/streaming source without re-shaping the trait."
  - "  - `pub trait Clock: Send + Sync` — returns the current logical timestamp. For backtest, advances to the most recent bar's `ts`. The trait must support a backtest-style advance (driven by the executor as it consumes bars) and a wall-clock `now()` (for Live)."
  - "  - `pub trait FillSink: Send + Sync` — accepts an order and produces a fill record. The simulated impl uses the current bar + cost model. Future broker impl forwards to `BrokerSurface`. Reuse existing types (`eval::cost::CostModel`, `eval::executor::trace_types::*`, `broker::*`) — do not invent new wire types if existing ones fit. The trait surface must NOT bake in `classify_run_failure`'s error-class wrapping (that stays at the executor level, so the broker impl in a later track can return raw errors and the executor wraps them)."
  - "**Three concrete impls land here.**"
  - "  - `InjectedBars` — backed by a `Vec<Ohlcv>` + cursor. Used by the Backtest path."
  - "  - `InstantClock` — `now()` returns the timestamp of the most recently-emitted bar from the active `BarSource`. No wall-clock side effects."
  - "  - `SimulatedFills` — lifts the existing fill simulation out of `BacktestExecutor::*` (fees, slippage, min-notional, cost model application). Behavior must be **identical** to today's `BacktestExecutor` fill path. No new fees, no new rounding, no new edge cases."
  - "  - `LiveStream`, `WallClock`, `RealBrokerFills` are NOT introduced here. The traits exist on their own merits; the Live impls are sub-track 3's job."
  - "**`BacktestExecutor` adopts the traits internally.** The existing struct keeps its name, its existing constructor, and its existing `eval::executor::mod::Executor` trait impl. Internally it now holds `Box<dyn BarSource>`, `Box<dyn Clock>`, `Box<dyn FillSink>` (or generics — implementer's choice; trait objects are simpler for dispatch) and delegates the bar iteration, clock progression, and fill production to those traits. Its public API and behavior do not change."
  - "  - The existing builder methods (`.with_bars()`, `.with_bars_and_progress()`, `.with_min_notional_usd()`, `.with_memory_recorder()`, `.with_obs_emitter()`) all continue to work and produce identical behavior."
  - "  - The internal per-bar loop in `BacktestExecutor::run(...)` now calls `bar_source.next_bar()`, `clock.advance_to(...)`, and `fill_sink.submit(...)` instead of the inlined logic."
  - "  - The behavioral floor is byte-identical metrics on identical fixtures. Sub-track 2 (paper-mode collapse) will continue to rely on this; sub-track 3 (live shell) will not."
  - "**`PaperExecutor` is NOT touched.** Out of scope. It continues to use whatever it used to. Sub-track 2 deletes it."
  - "**`RunMode` is NOT touched.** Out of scope. Sub-track 2 collapses it."
  - "**`safety::gate` is NOT touched.** Out of scope. Sub-track 3 renames the parameter."
  - "**No `LiveConfig` in this contract.** Sub-track 3 introduces the placeholder."
  - "**No new `RunMode::Live` plumbing.** Sub-track 3 wires it."
  - "**Tests required (new file `crates/xvision-engine/tests/eval_executor_traits.rs`):**"
  - "  - `InjectedBars` yields each bar in input order, then terminates with `None`."
  - "  - `InstantClock::now()` returns the timestamp of the most recently-emitted bar (or a documented zero/epoch value before the first bar)."
  - "  - `SimulatedFills` produces the same `FillRecord` shape and arithmetic as the pre-refactor inline fill code did for an identical bar + order pair. Use a fixed cost model and a hand-picked bar+order to make the assertion concrete (golden-value style)."
  - "  - Integration regression: at least one realistic backtest fixture (pick an existing one from the test corpus and reuse it; do not invent new fixtures) runs end-to-end through the new `BacktestExecutor` body and produces metrics **byte-identical** (or float-eq within tolerance) to the same fixture run via the existing `cargo test --workspace` baseline."
  - "**`cargo test --workspace` is the regression bar.** Every test that passes on `origin/main` continues to pass. No deletions, no skips, no behavior changes."
  - "**No changes outside listed allowed paths.** If implementation forces a touch outside `allowed_paths`, **STOP** and append a checkpoint under `# Notes`. Do not silently exceed scope."
---

# Scope

Sub-track 1 of the executor refactor (see
`team/contracts/executor-refactor.md` for the decomposition decision).

This contract introduces the `BarSource`, `Clock`, and `FillSink`
traits, ships the three concrete Backtest impls (`InjectedBars`,
`InstantClock`, `SimulatedFills`), and has `BacktestExecutor` adopt
them internally — **without** changing any external behavior,
modifying `PaperExecutor`, collapsing `RunMode`, touching the safety
gate, or introducing any Live plumbing. The full executor refactor
sequencing — paper delete + `RunMode` collapse + Live shell — happens
in sub-tracks 2 and 3 (`executor-collapse-paper-mode` and
`executor-live-shell`), each contracted separately and gated on this
one landing first.

Behavioral floor: byte-identical backtest metrics on identical
fixtures. The traits are extracted from `BacktestExecutor`'s body and
re-wired underneath the existing API surface; nothing observable
changes for any current caller, test, or downstream consumer.

The reason for this narrowed scope is documented in the parent
contract's "Checkpoint 2026-05-21 — implementer subagent" section:
the original bundled contract conflated three logically-separable
refactors on tightly-coupled hot code totalling ~6000 lines and ~280
call-site migrations. Splitting on the natural seam (introduce the
shape first; rewire callers later; add the new mode last) lets each
PR land at a reviewable size with `cargo test --workspace` green at
every commit.

# Out of scope

- `PaperExecutor` — file, struct, trait impl, all builder methods,
  all paper-executor tests. `crates/xvision-engine/src/eval/executor/paper.rs`
  is in `allowed_paths` (since the directory glob covers it) but
  **MUST NOT** be edited in this contract. Sub-track 2 deletes it.
- `RunMode` in either crate. Both `crates/xvision-engine/src/eval/run.rs`
  and `crates/xvision-core/src/config.rs` are forbidden here.
- `safety::gate::check_broker_submit` parameter rename. Forbidden.
- `safety::venue::VenueLabel`. Untouched.
- The `"paper"` legacy parse alias. Not introduced here.
- `LiveConfig` placeholder. Not introduced here.
- `Executor::live(...)` shell. Not introduced here.
- API dispatch (`crates/xvision-engine/src/api/eval.rs`). Untouched —
  the existing dispatch keeps calling `BacktestExecutor` and
  `PaperExecutor` by their current types and constructors.
- Every test file other than the one new `tests/eval_executor_traits.rs`.
  Existing tests must continue to pass without modification.
- Frontend, migrations, all other crates.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/executor-trait-extraction status
git -C .worktrees/executor-trait-extraction log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/executor-trait-extraction
#   - base is up to date with origin/main (or rebase planned)
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/executor-trait-extraction \
  -b task/executor-trait-extraction origin/main
```

Before running `cargo` from the worktree, set a shared target
directory per the workspace CLAUDE.md cache-discipline rule:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
```

# Notes

**Why this contract is foundation-lane and parallel-unsafe.** Holds
single-writer on `crates/xvision-engine/src/eval/executor/**` (the
trait module is being created and `BacktestExecutor`'s body is being
rewired internally). Sub-tracks 2 and 3 of the executor refactor
both target this same directory and must wait for this contract to
merge.

**Recommended implementation sequence** (the implementer can choose
their own, but this is the safe path):

1. Create the trait module — pure new code, no callers, just shapes.
   Run `cargo build` to make sure the traits compile.
2. Add the three concrete impls (`InjectedBars`, `InstantClock`,
   `SimulatedFills`) — still no callers. Run `cargo test` on the new
   `tests/eval_executor_traits.rs` unit tests for each impl.
3. Inside `BacktestExecutor`, replace the inlined bar iteration with
   `bar_source.next_bar()`. Run `cargo test --workspace`; expect
   green.
4. Replace the inlined fill production with `fill_sink.submit(...)`.
   Run `cargo test --workspace`; expect green.
5. Replace the inlined timestamp progression with `clock.advance_to(...)`
   / `clock.now()`. Run `cargo test --workspace`; expect green.
6. Final pass — clippy clean, board-lint clean, grep guards pass.
7. Open PR.

**Trait-object vs. generic decision.** The implementer chooses. Trait
objects (`Box<dyn BarSource>`) keep dispatch simple and avoid
monomorphization blowups when sub-track 3 lands the Live impl on the
same `BacktestExecutor` body shape. Generics (`<B: BarSource>`) avoid
dynamic dispatch in the hot per-bar loop. The behavioral floor is
identical metrics either way; pick whichever is cleaner.

**Why `SimulatedFills` must be byte-identical.** The downstream metric
suite (`eval::metrics`, `eval::postprocess`, the export pipeline) has
no awareness of the refactor. Tests assert exact `pnl_realized`,
`equity_curve`, decision counts, and broker-rule pre-check sequences.
A one-cent rounding drift in `SimulatedFills` would surface as a test
failure somewhere downstream — and trying to track that down across
the workspace is far more expensive than getting the fill math
identical the first time. The implementer should lift the existing
fill code verbatim (cut-and-paste, then re-shape into the trait body)
rather than rewriting from scratch.

**Append checkpoints / PR links below this line.**
