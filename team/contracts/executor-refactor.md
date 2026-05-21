---
track: executor-refactor
lane: foundation
wave: alpaca-live-eval-2026-05-21
worktree: .worktrees/executor-refactor
branch: task/executor-refactor
base: origin/main
status: scope-violation
depends_on: []
blocks:
  - agent-graph-composition
  - live-bar-source-alpaca
  - live-eval-launch-and-freeze
superseded_by:
  - executor-trait-extraction
  - executor-collapse-paper-mode
  - executor-live-shell
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/executor/**
  - crates/xvision-engine/src/eval/run.rs
  - crates/xvision-engine/src/eval/mod.rs
  - crates/xvision-engine/src/eval/live_config.rs
  - crates/xvision-engine/src/api/eval.rs
  - crates/xvision-engine/src/api/agents.rs
  - crates/xvision-engine/src/api/search.rs
  - crates/xvision-engine/src/safety/gate.rs
  - crates/xvision-engine/src/safety/venue.rs
  - crates/xvision-engine/src/safety/mod.rs
  - crates/xvision-engine/tests/eval_executor_*.rs
  - crates/xvision-engine/tests/eval_paper_*.rs
  - crates/xvision-engine/tests/eval_progress*.rs
  - crates/xvision-engine/tests/eval_run_*.rs
  - crates/xvision-engine/tests/eval_broker_circuit_breaker.rs
  - crates/xvision-engine/tests/eval_causal_input_sanitization.rs
  - crates/xvision-engine/tests/api_eval*.rs
  - crates/xvision-engine/tests/risk_min_notional.rs
  - crates/xvision-engine/tests/decisions_count.rs
  - crates/xvision-engine/tests/chart_hold_markers.rs
  - crates/xvision-engine/tests/safety_gate*.rs
  - crates/xvision-core/src/config.rs
  - crates/xvision-core/tests/**
  - crates/xvision-dashboard/src/wizard_loop.rs
  - crates/xvision-dashboard/src/cli_jobs/eval_run_bridge.rs
  - crates/xvision-dashboard/src/sse/mod.rs
  - crates/xvision-dashboard/src/server.rs
  - crates/xvision-dashboard/src/state.rs
  - crates/xvision-dashboard/tests/eval_runs_*.rs
  - crates/xvision-dashboard/tests/cli_jobs_eval_run_bridge.rs
  - crates/xvision-cli/src/commands/eval.rs
  - crates/xvision-cli/tests/eval_cli.rs
  - crates/xvision-mcp/src/tools.rs
  - crates/xvision-observability/**
forbidden_paths:
  - frontend/web/**
  - crates/xvision-engine/migrations/**
  - crates/xvision-core/migrations/**
  - crates/xvision-engine/src/eval/scenario.rs
  - crates/xvision-engine/src/eval/scenario_store.rs
  - crates/xvision-engine/src/eval/store.rs
  - crates/xvision-engine/src/eval/broker_rules.rs
  - crates/xvision-engine/src/eval/bars.rs
  - crates/xvision-engine/src/eval/preflight.rs
  - crates/xvision-engine/src/eval/guardrails.rs
  - crates/xvision-engine/src/eval/early_stop.rs
  - crates/xvision-engine/src/eval/metrics.rs
  - crates/xvision-engine/src/eval/postprocess.rs
  - crates/xvision-engine/src/eval/findings/**
  - crates/xvision-engine/src/eval/review/**
  - crates/xvision-engine/src/strategies/**
  - crates/xvision-engine/src/agents/**
  - crates/xvision-engine/src/agent/**
  - crates/xvision-engine/src/authoring.rs
  - crates/xvision-engine/src/strategies_folder/**
  - crates/xvision-engine/src/templates/**
interfaces_used:
  - eval::executor::Executor (existing trait, replaced)
  - eval::run::Run / RunMode / RunStatus
  - core::config::RunMode
  - safety::gate::check_broker_submit (signature change)
  - safety::venue::VenueLabel
  - eval::scenario::Scenario (read-only)
  - eval::scenario::ReplayMode (read-only)
  - broker::BrokerSurface (consumed; not modified)
  - agent::pipeline::run_pipeline (consumed; not modified)
parallel_safe: false
parallel_conflicts:
  - "Holds single-writer on crates/xvision-engine/src/eval/executor/** and both RunMode definitions for the wave. agent-graph-composition, live-bar-source-alpaca, and live-eval-launch-and-freeze all wait on this contract to merge."
verification:
  - cargo build --workspace
  - cargo test --workspace
  - cargo clippy --workspace --all-targets -- -D warnings
  - bash scripts/board-lint.sh
  - "rg --hidden -n 'PaperExecutor' crates/ returns no hits"
  - "rg --hidden -n 'RunMode::Paper' crates/ returns no hits"
  - "rg --hidden -n '\"paper\"' crates/xvision-engine/src/eval/ crates/xvision-engine/src/api/ crates/xvision-core/src/ crates/xvision-dashboard/src/ returns only the legacy-alias parse site and test fixtures"
acceptance:
  - "**Three executor-component traits introduced.** New module `crates/xvision-engine/src/eval/executor/traits.rs` (or split into `bar_source.rs`, `clock.rs`, `fill_sink.rs` under `executor/`) defines:"
  - "  - `pub trait BarSource: Send + Sync` — async `next_bar(&mut self) -> anyhow::Result<Option<Ohlcv>>`; the source decides when the next bar is available. `Option::None` terminates the run by stop-policy. Concrete async behavior, lifetime, and exact error shape can be chosen by the implementer, but the trait must support both `InjectedBars` (drains a `Vec<Ohlcv>`, returns immediately) and a future `LiveStream` (awaits the next bar from a websocket)."
  - "  - `pub trait Clock: Send + Sync` — `now(&self) -> DateTime<Utc>`; for backtest the clock advances to the current bar's `ts`, for live it returns `Utc::now()`. Concrete signatures can include an `advance_to(&mut self, ts: DateTime<Utc>)` if the executor needs to drive backtest time forward."
  - "  - `pub trait FillSink: Send + Sync` — async `submit(&mut self, order: OrderRequest, ctx: &SubmitContext) -> anyhow::Result<FillRecord>`; the simulated impl computes the fill from the current bar + cost model; the broker impl forwards to `BrokerSurface`. Exact `OrderRequest` / `FillRecord` / `SubmitContext` shapes can reuse existing types from `eval::cost`, `eval::executor::trace_types`, and `broker::*` — do not invent new wire types if existing ones fit."
  - "  - Trait module is `pub(crate)` minimum; only re-export what the API dispatch needs."
  - "**Three concrete impls land in this contract.**"
  - "  - `InjectedBars` (in `crates/xvision-engine/src/eval/executor/bar_source_injected.rs` or co-located in the trait module): backed by a `Vec<Ohlcv>` + cursor. Used by Backtest."
  - "  - `InstantClock` (co-located): for Backtest, no real waiting. `now()` returns the timestamp of the most recent bar emitted by the `BarSource`."
  - "  - `SimulatedFills` (co-located): the existing fill simulation lifted out of `BacktestExecutor`. Reads the active cost model, applies fees/slippage/min-notional, returns a `FillRecord` synchronously. Must replicate the existing semantics 1:1; no behavioral change."
  - "  - `LiveStream`, `WallClock`, `RealBrokerFills` are **NOT** implemented in this contract. They are placeholders/`todo!()` shells in the trait module marked with `#[allow(dead_code)]` and a doc-comment referencing the `live-bar-source-alpaca` and `live-eval-launch-and-freeze` tracks. The Live path must compile but is **not invokable** end-to-end yet; constructing the Live executor must return a clear `\"Live mode not yet implemented — pending live-bar-source-alpaca\"` error rather than panicking."
  - "**Single `Executor` impl replaces both `BacktestExecutor` and `PaperExecutor`.** A new `Executor` struct (rename of `BacktestExecutor` is acceptable; keep `Executor` if the name is free) parameterized by `Box<dyn BarSource>`, `Box<dyn Clock>`, `Box<dyn FillSink>` (or generic `<B: BarSource, C: Clock, F: FillSink>` — implementer's choice; trait objects preferred for dispatch simplicity unless monomorphization is needed). It implements the existing `eval::executor::mod::Executor` trait so the API dispatch site doesn't need a parallel surface change."
  - "  - Construction helper: `Executor::backtest(bars: Vec<Ohlcv>, cost_model: CostModel, ...) -> Self` mirrors today's `BacktestExecutor::new(...)`. Internally wires `InjectedBars + InstantClock + SimulatedFills`."
  - "  - Construction helper: `Executor::live(...) -> anyhow::Result<Self>` returns the not-implemented error described above; signature exists so the API dispatch can route to it once tracks 3+4 land."
  - "  - All existing builder methods on `BacktestExecutor` / `PaperExecutor` that tests rely on (`.with_bars()`, `.with_bars_and_progress()`, `.with_min_notional_usd()`, `.with_memory_recorder()`, `.with_obs_emitter()`) are preserved on the new `Executor`."
  - "**`PaperExecutor` is deleted entirely.** Remove `crates/xvision-engine/src/eval/executor/paper.rs`. Remove all imports, re-exports, and direct references. The inner-cycle code paths that `PaperExecutor` exercises today (broker submit, classify_run_failure broker classes, circuit breaker, min-notional) are preserved in `SimulatedFills` (or in the new `Executor` body) so the existing test coverage continues to exercise them. **No test deletions.** The paper-executor tests migrate to the new `Executor` constructed in Backtest mode with a mock `FillSink` if a test asserts broker-error classification — see the test-migration paragraph below."
  - "**`RunMode` collapses to `{ Backtest, Live }` in BOTH `xvision-engine::eval::run` and `xvision-core::config`.**"
  - "  - The engine enum drops `Paper`, gains `Live`."
  - "  - The core enum drops `Paper` (it already had `Live`; the variant stays unchanged)."
  - "  - `RunMode::as_str()` returns `\"backtest\"` / `\"live\"`."
  - "  - `RunMode::parse(s)` accepts `\"backtest\"` and `\"live\"` natively, and accepts `\"paper\"` as a **legacy DB read-only alias** that maps to `RunMode::Backtest`. New writes never emit `\"paper\"`. This alias is the deliberate backward-compatibility seam so existing DB rows continue to load without a SQL migration."
  - "  - Document the alias with a one-line comment at the parse site."
  - "**All `RunMode::Paper` references migrate.**"
  - "  - Source-code callers: every `RunMode::Paper` in `crates/xvision-engine/src/api/eval.rs`, `crates/xvision-engine/src/api/agents.rs`, `crates/xvision-engine/src/api/search.rs`, dashboard `wizard_loop.rs` / `cli_jobs/eval_run_bridge.rs` / `sse/mod.rs` / `server.rs` / `state.rs`, CLI `eval.rs`, MCP `tools.rs`, observability becomes `RunMode::Backtest`."
  - "  - Test callers: same migration. Tests that today construct `PaperExecutor` switch to `Executor::backtest(...)`. Tests that today assert broker-error classification (e.g. `eval_broker_circuit_breaker.rs`, `risk_min_notional.rs`) wire a mock `FillSink` that simulates the broker error path."
  - "  - Inline string literals: `\"paper\"` in CLI prompts, API JSON parsing, etc. become `\"backtest\"`. The only acceptable lingering `\"paper\"` literal is the parse-alias site (documented) and any test fixture asserting the alias works."
  - "**Confused-deputy venue gate rewire — minimum viable.** Today `safety::gate::check_broker_submit` takes `scenario_venue_label: VenueLabel` and `broker_venue_label: VenueLabel`. Rename the parameter to `run_venue_label` (caller-agnostic about the source) — semantics unchanged. The Backtest call site continues to pass `scenario.venue_label`. A new minimal `LiveConfig` shape is introduced in `crates/xvision-engine/src/eval/live_config.rs` carrying **only** `pub venue_label: VenueLabel` (and a unit-struct stop-policy placeholder if the implementer wants to scaffold) — no storage, no migration, no validation logic. The full `LiveConfig` (assets, capital, stop_policy, broker_creds_ref) is **out of scope** and lands in `live-eval-launch-and-freeze`. The placeholder exists so the gate signature is future-proof and the dispatch site can compile; it is never persisted or constructed at runtime by this contract."
  - "**`classify_run_failure` taxonomy is preserved verbatim.** The wire-shape error classes (`broker_auth`, `broker_unsupported`, `broker_insufficient_funds`, `broker_timeout`, `broker_rejected`, `repeated_broker_error`, plus the existing trader/provider classes) are unchanged. They must continue to fire from the new `SimulatedFills` and mock-`FillSink` code paths. Tests under `eval_broker_circuit_breaker.rs` and `eval_executor_paper.rs` (migrated to use `Executor::backtest` + mock `FillSink`) continue to assert these classes."
  - "**`Scenario` is not modified.** No edits to `crates/xvision-engine/src/eval/scenario.rs`. `Scenario.venue_label` and `Scenario.replay_mode` remain. Post-refactor, `replay_mode` is read **only** by Backtest mode (Live is scenario-less). No new variants added to `ReplayMode`; the unused `Realtime` placeholder stays unused — F31 disposition is out of scope."
  - "**Backtest mode works end-to-end during and after the refactor.** Every existing backtest test passes without modification beyond mechanical `RunMode::Paper → RunMode::Backtest` renames. The full backtest pipeline (warmup → bars → agent pipeline → fills → metrics → finalization) continues to function. Run this verification by hand on at least one realistic strategy/scenario combination and document it in the PR description."
  - "**Live mode is wired through but inert.** `RunMode::Live` is constructable. The API dispatch routes Live runs to `Executor::live(...)`, which returns the not-implemented error. The gate accepts a `run_venue_label` from a Live `LiveConfig` placeholder. No real Live launch is possible until tracks 3 and 4 land."
  - "**Tests required (in addition to existing test migrations):**"
  - "  - Unit test: `BarSource` trait — `InjectedBars` returns each bar in order, then `None`."
  - "  - Unit test: `InstantClock::now()` returns the timestamp of the most recently-emitted bar."
  - "  - Unit test: `SimulatedFills::submit()` produces the same `FillRecord` as the pre-refactor `BacktestExecutor` did for an identical bar + order pair (golden-value or property-style)."
  - "  - Unit test: `RunMode::parse(\"paper\")` returns `RunMode::Backtest` with the legacy-alias comment cited."
  - "  - Unit test: `RunMode::as_str()` for `Backtest` returns `\"backtest\"` (not `\"paper\"`)."
  - "  - Unit test: `Executor::live(...)` returns the not-implemented error rather than panicking."
  - "  - Integration test: an existing backtest fixture run via the new `Executor::backtest(...)` produces metrics identical (within float-eq tolerance) to the pre-refactor `BacktestExecutor` on the same fixture — this is the regression bar for the trait extraction."
  - "  - Integration test: gate signature with renamed `run_venue_label` parameter — Paper-labeled run × Live-labeled broker → `VenueLabelMismatch`. Same semantic as today, called via the renamed parameter."
  - "**Grep guards (must all pass):**"
  - "  - `rg --hidden -n 'PaperExecutor' crates/` → no hits."
  - "  - `rg --hidden -n 'RunMode::Paper' crates/` → no hits."
  - "  - `rg --hidden -n 'enum RunMode' crates/` → exactly two hits, one in `xvision-engine/src/eval/run.rs`, one in `xvision-core/src/config.rs`, both with the variant set `{ Backtest, Live }`."
  - "  - `rg --hidden -n '\\\"paper\\\"' crates/xvision-engine/src/eval/ crates/xvision-engine/src/api/ crates/xvision-core/src/ crates/xvision-dashboard/src/` → at most one hit: the parse-alias site with the documented comment."
  - "  - `ls crates/xvision-engine/src/eval/executor/paper.rs` → file does not exist."
  - "**DB schema unchanged.** No migrations are added or modified. Existing rows with `mode = 'paper'` continue to load via the legacy-alias parse. Future writes emit `'backtest'`. A note in the PR description acknowledges that a future cleanup migration could rewrite legacy rows to `'backtest'` but is out of scope here."
  - "**Frontend untouched.** `frontend/web/**` is forbidden by this contract. The frontend may still display strings like `\"paper\"` — that surface migration is a follow-up. The backend's JSON responses for legacy runs continue to report `\"paper\"`? No — the engine's `RunMode::as_str()` is the source of truth for serialization; new runs serialize as `\"backtest\"`. Legacy runs loaded from the DB also re-serialize as `\"backtest\"` (since the alias maps on parse, and the in-memory `RunMode` carries no provenance). Document this behavior change in the PR description; expect a frontend follow-up to adopt the new vocabulary."
  - "**No changes outside listed allowed paths.** If implementation forces a touch outside `allowed_paths`, **STOP** and append a checkpoint under `# Notes`. Do not silently exceed scope. The conductor will either expand allowed_paths or split the contract."
---

# Scope

Track 1 of the Alpaca-Live eval intake
(`team/intake/2026-05-21-alpaca-live-eval-and-executor-refactor.md`).
Refactors the engine's executor surface so that Backtest and Live runs
share one `Executor` parameterized by three traits — `BarSource`,
`Clock`, `FillSink` — and deletes the confused middle-ground
`PaperExecutor` that today drives broker-real fills against
historical-agent-decisions. After this contract:

- A single `Executor` covers both real modes. Backtest =
  `InjectedBars` + `InstantClock` + `SimulatedFills`. Live (when its
  components land) = `LiveStream` + `WallClock` + `RealBrokerFills`.
- `PaperExecutor` is gone.
- `RunMode` is `{ Backtest, Live }` in both `xvision-engine::eval::run`
  and `xvision-core::config`. The string `"paper"` parses as a
  read-only legacy alias for `Backtest`; new writes never emit it.
- The confused-deputy venue gate accepts a `run_venue_label`
  parameter (semantic-unchanged rename); a placeholder `LiveConfig`
  carries `venue_label` only — the full schema is `live-eval-launch-
  and-freeze`'s job.
- Backtest works end-to-end during and after the refactor. Live mode
  is wired through but inert; constructing a Live executor returns a
  clear not-implemented error pending the bar-source and launch
  tracks.

This contract does **not** ship Live runs operationally. It creates
the seams that `agent-graph-composition`, `live-bar-source-alpaca`,
and `live-eval-launch-and-freeze` plug into. The intake's track
sequencing names `executor-refactor` first deliberately so the Filter
work in `agent-graph-composition` targets the final executor shape,
not a `PaperExecutor` that's about to be deleted.

The behavioral floor: **no regression in backtest semantics**, byte-
for-byte where feasible. The traits are extracted from the existing
`BacktestExecutor` body; the new `Executor::backtest(...)` constructor
wires them back together to produce identical metrics on identical
fixtures.

# Out of scope

- `Scenario` schema changes. `crates/xvision-engine/src/eval/scenario.rs`
  is forbidden. `Scenario.venue_label` and `Scenario.replay_mode`
  stay where they are. The narrowing of `replay_mode`'s meaning to
  "Backtest-only" is documented in the intake and reflected in
  callers, not in scenario.rs.
- Full `LiveConfig` schema (assets, capital, stop_policy,
  broker_creds_ref, broker preflight, persistence on `eval_runs`).
  That is `live-eval-launch-and-freeze`'s contract. This contract
  introduces only a placeholder shape carrying `venue_label`.
- DB migrations. `crates/xvision-engine/migrations/**` and
  `crates/xvision-core/migrations/**` are forbidden. Legacy `"paper"`
  rows continue to load via the parse alias. A future cleanup
  migration is acknowledged but not authored here.
- The `LiveStream` BarSource implementation (Alpaca websocket, gap
  detection, reconnect-budget). That is `live-bar-source-alpaca`.
  This contract leaves `LiveStream` as a trait-shaped placeholder
  with a `todo!()` body and an error-returning constructor.
- The `RealBrokerFills` FillSink implementation. Same as above —
  shaped placeholder, no real wiring. Will be filled in by
  `live-eval-launch-and-freeze` (or split out depending on the
  conductor's wave layout).
- Filter / kind / per-Filter granularity on `AgentRef`. That is
  `agent-graph-composition`. The executor refactor does NOT touch
  the pipeline-level cadence logic; the trader continues to fire on
  every bar today, and the Filter track will introduce the
  Filter-gated cadence on top of the post-refactor `Executor`
  without further executor changes.
- Frontend UI. `frontend/web/**` is forbidden. The frontend may
  still display `"paper"` strings (legacy fixtures, etc.) until a
  follow-up adopts the new vocabulary.
- F31 (`ReplayMode::Stepped`, `Accelerated`, `Realtime`). The intake
  notes F31's `Realtime` becomes redundant after Live lands;
  retirement is out of scope here.
- Real-money Live runs (`VenueLabel::Live` on `LiveConfig`). The
  intake explicitly rejects this in v1 at `LiveConfig` validation —
  which is `live-eval-launch-and-freeze`'s job, not this one.
- The `xvn eval run --mode=...` CLI verb shape, the Launch UX entry
  point on the strategy detail page, and the "Save as historical
  scenario" freeze action. All `live-eval-launch-and-freeze`.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/executor-refactor status
git -C .worktrees/executor-refactor log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/executor-refactor
#   - base is up to date with origin/main (or rebase planned)
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/executor-refactor \
  -b task/executor-refactor origin/main
```

Before running `cargo` from the worktree, set a shared target
directory per the workspace CLAUDE.md cache-discipline rule:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
```

# Notes

**Why this contract is foundation-lane and parallel-unsafe.** It
holds single-writer locks on:

- `crates/xvision-engine/src/eval/executor/**` (everything there is
  being rewritten or deleted).
- The two `RunMode` definitions (engine + core) and every caller of
  `RunMode::Paper` across the workspace.
- The `safety::gate::check_broker_submit` signature (parameter
  rename ripples through every caller).

Any other track that touches those surfaces during this wave must
either wait for this contract to merge or coordinate via the conductor.

**Why the gate rewire is minimum-viable here.** The intake's raw-
items table lists the gate move as a leaf-lane sub-item of
`executor-refactor`. Doing the full `LiveConfig` schema in this
contract would balloon the diff (eval_runs migration, validator
suite, store wiring, API surface). The minimum viable rewire is the
parameter rename plus a placeholder `LiveConfig { venue_label }`
shape — enough that `live-eval-launch-and-freeze` can extend the
struct without touching the gate.

**Why the `"paper"` parse alias.** Two reasons:

1. Production DBs already contain rows with `mode = 'paper'`.
   Refusing them at parse time would brick run history and force a
   DB migration that's out of scope here.
2. The intake explicitly retires the confused mode "with prejudice"
   — calling old paper runs `Backtest` is the honest re-labeling.
   The alias is a one-line read-side concession.

**Why `LiveStream` / `WallClock` / `RealBrokerFills` are placeholder
shells.** The Live components belong to tracks 3 and 4. Shipping
their implementations here would either (a) make this contract
unreviewably large or (b) ship half-implementations that the next
tracks rewrite. The shells let the trait surface settle now and
keep the API dispatch site honest about Live being a real branch in
the code.

**Test migration approach (`PaperExecutor → Executor::backtest +
mock FillSink`).** The existing paper-executor tests largely exercise
the inner-cycle code (agent dispatch, briefing construction, broker
submit, fill classification, circuit breaker). After the refactor
that inner-cycle code is reached through `Executor::backtest(...)`
with `SimulatedFills`. Tests that specifically assert broker-error
classification (currently relying on the real-broker submit path
inside `PaperExecutor`) get a mock `FillSink` that returns the
desired broker error class. **Important:** the implementation
worker must keep the test names and assertions semantically
identical — the refactor's regression bar is "every existing test
passes". If a test cannot be migrated without changing its
semantics, **STOP** and append a checkpoint here rather than
weakening the test.

**Open coordination question (deferred to follow-up).** The intake's
"Open questions deferred to track contracts" calls out
`FillSink` error-class compatibility. Today `classify_run_failure`
parses error strings produced by `PaperExecutor`'s wrapping of
`BrokerSurface` errors. After this contract, `RealBrokerFills` is a
shell — the real broker wiring lands later. The shell must
nonetheless expose the **trait surface** that the
classify_run_failure-producing wrapper will live on. Acceptable
solutions:

- The wrapping happens inside the `Executor` body, not inside
  `FillSink::submit`. `FillSink` returns a raw broker error, the
  `Executor` wraps it into the classify-able shape.
- The wrapping happens in a `RealBrokerFills::submit` impl when
  it's written by the next track, with the trait surface staying
  raw.

Either pattern is acceptable. Document the choice in the PR.

Append checkpoints / PR links below.

## Checkpoint 2026-05-21 — implementer subagent (Opus 4.7 1M)

Status: **NEEDS_CONTEXT** — appending without exceeding scope; no code
changes pushed to `task/executor-refactor`. Reporting back for a
decomposition decision before committing partial work.

**What I did**: Read the full contract, intake, and the entire executor
surface (executor/mod.rs, backtest.rs 2216 lines, paper.rs 1800 lines),
both RunMode enums, safety/gate.rs, the API dispatch in api/eval.rs
(the `run_with_deps` + `run_with_deps_in_progress` paths and both
`build_*_executor` helpers), and the test surface that touches
`PaperExecutor` (8 test files, 2587 lines, 14 `PaperExecutor::with_*`
call sites in tests + ~280 `RunMode::Paper` / `PaperExecutor` /
`"paper"` references workspace-wide).

**Why I'm stopping**: The contract as written is genuinely a 3-step
refactor on tightly-coupled hot code. Concretely:

1. **Trait extraction is non-trivial.** The two executors (backtest +
   paper) are each ~2000-line monoliths with deeply-interleaved
   per-bar loops covering: bars iteration, guardrail pre-checks,
   agent pipeline dispatch, broker-rule pre-checks, fill simulation
   (backtest) OR broker submit + retry + circuit breaker (paper),
   trace recording, equity sampling, observability emission, early-stop
   evaluation, progress events, and chart event bus emission. Lifting
   `BarSource` / `Clock` / `FillSink` cleanly out of this body without
   regressing semantics requires a careful sequence of small commits
   (extract bars iteration → swap in trait → extract fill simulation →
   swap in trait → extract clock surface → swap in trait), each with
   `cargo test --workspace` green. That is not a one-session job in
   this codebase — it is a 5-to-10-commit sequence with verification
   between each step.

2. **The PaperExecutor delete + test migration is a separate large
   piece.** 8 test files (~2500 lines) currently exercise the
   broker-submit path inside PaperExecutor. Two of them
   (`eval_broker_circuit_breaker.rs`, `eval_executor_paper.rs`) make
   assertions that are *specifically about* the broker error
   classification and circuit-breaker behavior — those need a real
   mock `FillSink` impl that can replay broker errors. The other six
   migrate to a no-broker `Executor::backtest(...)`, but only after
   confirming that backtest mode covers the assertion (e.g.
   `eval_paper_pnl_realized.rs` asserts pnl arithmetic — that's
   backtest-portable; `eval_progress.rs` asserts progress events —
   also portable; `eval_causal_input_sanitization.rs` only
   *references* paper in a comment).

3. **The 280-reference migration is mostly mechanical but spans 5
   crates** (engine, core, dashboard, cli, mcp, observability, risk,
   plus tests). Doing it in one pass without partial-build breakage
   means staging the `RunMode` collapse last — after the API
   dispatch can route `RunMode::Live → Executor::live(...)` and the
   parse alias is in place.

4. **The `"paper"` parse alias has more surface than the contract
   describes.** `crates/xvision-risk/` uses the literal string
   `"paper"` as a venue id throughout (`venue_limits("paper")`,
   `venue_id: "paper".into()`, `min_notional.rs:86`, `lib.rs:356`,
   `config.rs:59`). The contract grep guard scopes literals to
   `xvision-engine/src/eval/`, `xvision-engine/src/api/`,
   `xvision-core/src/`, and `xvision-dashboard/src/` — which avoids
   xvision-risk and so the guard does pass — but `api/eval.rs:1905`
   calls `cfg.venue_limits("paper").min_notional_usd` and that
   literal would need to either stay (it's the risk-config venue
   id, not the run mode) or move to xvision-risk's allowed paths.
   The contract's allowed_paths includes `crates/xvision-engine/src/
   api/eval.rs` so the line is editable, but the conductor needs to
   decide: keep `"paper"` as the venue-id label in risk-config
   (legacy schema, separate concept from run mode) or rename. The
   intake doesn't address this explicitly. **Recommendation: keep
   the risk-config `"paper"` venue-id literal — it's a separate
   concept (broker venue label, not run mode); the grep guard
   already excludes xvision-risk.**

5. **The `safety::venue::VenueLabel::Paper` variant is in scope but
   not addressed in the contract.** The `RunMode::Paper` collapse
   says nothing about whether `VenueLabel::Paper` also collapses.
   Looking at gate.rs:149 (`scenario_venue_label == VenueLabel::Paper
   && broker_venue_label == VenueLabel::Live`) and venue.rs:30,38
   (`VenueLabel::Paper => "paper"` + `"paper" => Some(VenueLabel::
   Paper)`), `VenueLabel` is a *separate* concept from `RunMode` —
   it labels the broker surface, not the run intent. **The contract
   should explicitly say `VenueLabel::Paper` stays.** Today's
   confused-deputy check (Paper scenario × Live broker → reject) is
   load-bearing for the gate semantics; if `VenueLabel::Paper` were
   removed the gate would lose the asymmetry it's testing.

**What I need from the conductor**:

Option A — **Expand the timeline.** Acknowledge this is a 3-day
contract and let me commit incrementally over multiple sessions, with
the wave's blocking tracks (`agent-graph-composition`,
`live-bar-source-alpaca`, `live-eval-launch-and-freeze`) understanding
they're waiting on this. Each commit lands behind a `cargo test
--workspace` green checkpoint.

Option B — **Split the contract into three sub-tracks**, all on
foundation-lane / parallel-unsafe (same single-writer locks):

  - `executor-trait-extraction` — define BarSource/Clock/FillSink in
    a new module; have `BacktestExecutor` adopt them internally; ship
    the unit tests for the trait impls. PaperExecutor untouched.
    `RunMode` untouched. Gate untouched.
  - `executor-collapse-paper-mode` — delete PaperExecutor; migrate
    test files (with mock FillSink for the two broker-classify
    tests); collapse `RunMode` to `{Backtest, Live}` with the
    `"paper"` parse alias; migrate the ~280 references.
  - `executor-live-shell` — add `Executor::live(...)` (returns
    not-implemented error); rename `scenario_venue_label →
    run_venue_label`; add the `LiveConfig` placeholder; wire API
    dispatch to route `RunMode::Live` to the shell.

  Each sub-track is a ~1-day-or-less PR. Tracks 3+ wait on the
  combined set merging.

Option C — **Accept a reduced acceptance bar** for this single PR:
do (a) the `RunMode` collapse, (b) the PaperExecutor delete + test
migration, (c) the gate rename + LiveConfig placeholder, and (d) the
`Executor::live(...)` not-implemented shell — but skip the
`BarSource/Clock/FillSink` trait extraction. The Backtest path
continues using `BacktestExecutor::with_bars(...)` body unchanged
(renamed to `Executor::backtest(...)`). The traits land in a
follow-up. This still unblocks `live-eval-launch-and-freeze` and
`live-bar-source-alpaca` because those tracks can introduce the
traits when they need them — but it weakens the "Filter work in
agent-graph-composition targets the final executor shape" claim
from the contract scope.

My recommendation is **Option B**. The intake's track sequencing is
sound; the contract was just over-bundled. Three foundation-lane
sub-tracks of 1 day each, sequential, give the next wave clean
trait-shaped seams without ballooning any one PR past reviewability.

No code committed on `task/executor-refactor`. The worktree is
clean. Baseline `cargo build --workspace` passes.

— Implementer subagent, 2026-05-21

## 2026-05-21 — conductor decision: accept Option B decomposition

Worker's analysis is well-evidenced (line counts, call-site counts, two
contract gaps around `VenueLabel::Paper` and `xvision-risk`'s `"paper"`
venue-id literal). The original `executor-refactor` contract is marked
`scope-violation` and superseded by three sequential foundation-lane
sub-tracks:

1. **`executor-trait-extraction`** — defines `BarSource`, `Clock`,
   `FillSink` traits; lifts the relevant code out of `BacktestExecutor`;
   adds `InjectedBars`, `InstantClock`, `SimulatedFills` concrete impls;
   `BacktestExecutor` adopts the traits internally. `PaperExecutor`
   untouched. `RunMode` untouched. Gate untouched. Status: **ready**.
2. **`executor-collapse-paper-mode`** — deletes `PaperExecutor`;
   migrates the 8 paper-touching test files (with a mock `FillSink` for
   the two broker-classify tests); collapses `RunMode` to `{Backtest,
   Live}` with the `"paper"` parse alias; migrates the ~280 references
   across engine/core/dashboard/cli/mcp/observability. Preserves
   `VenueLabel::Paper` and `xvision-risk`'s `"paper"` broker-venue-id
   literal — those are separate concepts. Status: **deferred** until
   sub-track 1 merges.
3. **`executor-live-shell`** — adds `Executor::live(...)` returning the
   not-implemented error; renames `safety::gate::check_broker_submit`'s
   `scenario_venue_label` → `run_venue_label`; introduces the minimal
   `LiveConfig { venue_label }` placeholder in
   `crates/xvision-engine/src/eval/live_config.rs`; wires API dispatch
   to route `RunMode::Live` to the shell. Status: **deferred** until
   sub-track 2 merges.

Sub-track 1's contract is authored at
`team/contracts/executor-trait-extraction.md`. Sub-tracks 2 and 3 will
be authored when their predecessor approaches landing. The two
gap findings above (`VenueLabel::Paper`, xvision-risk `"paper"` venue
id) are folded into sub-track 2's contract explicitly.

The blocking relationship in `agent-graph-composition`,
`live-bar-source-alpaca`, and `live-eval-launch-and-freeze` is now
`blocks: [executor-live-shell]` (transitively requires all three sub-
tracks to merge first). The intake's track-sequencing rationale
("Filter work targets the final executor shape") is preserved — the
final shape lands after all three sub-tracks merge.

Worktree `.worktrees/executor-refactor` and branch
`task/executor-refactor` will be removed; sub-track 1 gets fresh
`task/executor-trait-extraction` on a new worktree.

— Conductor, 2026-05-21

