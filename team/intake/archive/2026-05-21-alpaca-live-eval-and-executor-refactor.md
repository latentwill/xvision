# Intake — 2026-05-21 — Alpaca Live eval + executor refactor

Source: design conversation initiated by the operator while attempting to
intake "Alpaca Live Scenario for testing." The framing surfaced a long-
standing semantic confusion ("it's an Eval, but it's not using a
Scenario?") that, on inspection of `crates/xvision-engine/src/eval/`, is a
real architectural fault line rather than a docs gap. This intake locks
the decisions that resolve it and sequences the work.

## Findings (concrete, from the current codebase)

1. **`Scenario` is defined as historical-only.**
   `crates/xvision-engine/src/eval/scenario.rs::validate_v1` rejects any
   `time_window.end > Utc::now()`. The struct's docstring states it is
   "a frozen evaluation context (asset window, venue settings, replay
   mode, lineage)." Forward-facing windows are unrepresentable.
2. **`PaperExecutor` is a Frankenstein.** It submits orders through a
   real `BrokerSurface` (e.g. Alpaca paper) but drives the agent's
   decisions from a historical `Vec<Ohlcv>`. The result is that the
   strategy makes decisions based on a past reality (e.g. BTC at $30k in
   Aug 2024) and receives fills at current market (BTC at today's
   price). It is useful only as a broker-plumbing test, not as a P&L-
   meaningful mode. Today's UI presents it to users as if it were honest
   paper trading.
3. **`RunMode` is inconsistent across crates.**
   `xvision-engine::eval::run::RunMode` has `{ Backtest, Paper }`.
   `xvision-core::config::RunMode` has `{ Backtest, Paper, Live }`. No
   `LiveExecutor` exists. `Live` in the core config is unreachable.
4. **`VenueLabel` and the confused-deputy gate already exist** for the
   real-money safety story (`crates/xvision-engine/src/safety/venue.rs`),
   but the data model they sit on (Paper-scenario vs. Live-broker) does
   not contemplate scenario-less live runs.
5. **`ReplayMode::Realtime` exists in the enum and is never used.** It
   is a placeholder shape with no executor path.

## Design principle

A "Live" run in v1 is **a bounded forward-test that terminates and
optionally leaves a backtestable artifact behind**. It is the dress
rehearsal before any real-money strategy execution. Real-money
continuous trading is a future milestone and is explicitly **out of
scope** here.

The executor architecture should be honest about what differs between
modes: the agent itself does not change across Backtest / Live. Only
three orthogonal scaffolding pieces change — where bars come from, how
the clock advances, and where fills happen.

## Decisions locked

1. **Purpose of Live.** Forward-test dress rehearsal. No real money in
   this milestone. `VenueLabel::Live` rejected at `LiveConfig`
   validation in v1.
2. **Live runs are scenario-less.** `eval_runs.scenario_id` becomes
   nullable when `mode = Live`. A new `LiveConfig` value rides on the
   run record. The first `Scenario` row that ever exists in a Live
   lineage is the operator-curated *frozen historical* one (see #5).
3. **Stop policy on `LiveConfig`.** `{ time_limit_secs?, bar_limit?,
   decision_limit? }`. At least one must be set; an empty policy is
   rejected by the validator. Limits combine with OR semantics — the
   first to trip terminates the run. `bar_limit` counts bars consumed
   at the highest-cadence input source (caps cheap dimension);
   `decision_limit` counts total LLM invocations across all `kind`s
   (caps expensive dimension); `time_limit_secs` is wall-clock.
4. **Bar persistence at termination is eager.** Realized bars, fills,
   decisions, and `FillProvenance` are sealed as part of the Run
   record atomically with the `Running → Completed` transition. No
   separate "freeze the bars" step.
5. **Scenario freeze is on operator request, not automatic.** A
   completed Live run exposes a "Save as historical scenario" action
   that materializes a new `Scenario` row with
   `source: ScenarioSource::Frozen` (new variant), `parent_scenario_id`
   null (the Live run is the parent in the run sense, not the
   Scenario sense), `data_source: AlpacaHistorical`,
   `replay_mode: Continuous`, `time_window` set to the actual realized
   start..end. The freeze form pre-seeds the cost-model fields (fees,
   slippage, fill_model) from realized values across the run's fills;
   the operator may accept or override. Most Live runs will not be
   frozen; only the ones the operator wants to preserve as a
   backtestable artifact.
6. **`LiveConfig` field set.** Required at launch: `strategy_id`,
   `assets: Vec<AssetRef>` (plural shape from day one for forward
   compatibility with F30 multi-asset; v1 validator enforces
   `len() == 1`), `capital`, `broker_creds_ref`, `stop_policy`,
   `venue_label` (defaults `Paper`; `Live` rejected in v1). Optional:
   `warmup_bars` (default 200), `safety_limits`, `display_name`,
   `description`, `tags`, `notes`. **Not on `LiveConfig`:**
   `granularity` (per-agent on the strategy via the agent-graph track —
   see Dependencies), `fees` / `slippage` / `latency` / `fill_model`
   (real broker provides these; recorded per fill as `FillProvenance`,
   not configured), `data_source` (implicit `AlpacaLive` from
   `broker_creds_ref`), `replay_mode` (implicit), `calendar` /
   `timezone` (derived from asset class), `bar_cache_policy` (implicit
   per-run key).
7. **Executor architecture: one `Executor`, three traits.** Refactor
   the existing executors so a single `Executor` is parameterized by
   `BarSource`, `Clock`, and `FillSink`. The two real run modes
   become:
    - **Backtest** = `InjectedBars` + `InstantClock` + `SimulatedFills`
    - **Live** = `LiveStream` + `WallClock` + `RealBrokerFills`

   The shipped surface `PaperExecutor` is **deleted entirely**. The
   confused middle-ground mode (broker-real fills against
   historical-agent-decisions) is retired with prejudice. `RunMode`
   collapses to `{ Backtest, Live }` in **both**
   `xvision-engine::eval::run` and `xvision-core::config`. All
   `RunMode::Paper` references migrate to `RunMode::Backtest` (the
   existing paper-eval tests are largely exercising the inner-cycle
   code, which after the refactor is reached through `Live` with a
   mock `BrokerSurface` / `FillSink`).
8. **Filter-gated trader cadence.** Live runs depend on the Filter
   concept being formalized in `agent-graph-composition`. The
   Filter fires on its own per-Filter granularity (configurable on
   the `AgentRef`, decoupled from any other agent's clock) and emits
   signals into downstream agents' briefings; the trader fires only
   when a Filter signal warrants. An "empty" / passthrough Filter
   that always emits the trigger signal is the degenerate case that
   produces trader-every-bar behavior — no special fallback is
   needed in the Live executor. The Live track does NOT redesign
   any of this; it consumes whatever the agent-graph track lands.
9. **No live-vs-replay comparison feature.** A backtest of a frozen
   scenario *is* the replay, using the same `Executor` with swapped
   traits. No special comparison surface, no special data contract
   beyond what `FillProvenance` and the existing trace already
   record. If divergence analysis becomes valuable later, it lands
   as a separate analytics track joining normal run records.

## Track sequencing (hard dependency chain)

The four tracks below must land **in this order**. Earlier tracks are
foundational; later tracks consume their surfaces.

The `executor-refactor` deliberately precedes `agent-graph-composition`
so Filter integration is built against the **final** executor shape,
not against `PaperExecutor` (which is being deleted). Implementing the
Filter's pipeline / executor wiring inside `PaperExecutor` only to
delete it weeks later is wasted reviewer attention, wasted test
authorship, and a needless migration step. The Filter track itself is
mostly pipeline-level (signal routing in `run_pipeline`, `kind` on
`AgentRef`, per-Filter `granularity`); the executor-side surface it
touches is small but should target one shape, not two.

1. **`executor-refactor`** — extract `BarSource` / `Clock` /
   `FillSink` traits from the existing `BacktestExecutor` and
   `PaperExecutor`; collapse to a single `Executor`; delete
   `PaperExecutor`; collapse `RunMode` to `{ Backtest, Live }` across
   both crates; migrate every `RunMode::Paper` reference. The
   `confused-deputy` venue-label gate is rewired to read from
   `LiveConfig` instead of `Scenario` post-refactor. Backtest mode
   continues to work end-to-end during and after the refactor — this
   track does not block on Live being implemented; it just creates
   the seams Live will plug into.
2. **`agent-graph-composition`** — formalize `kind`
   (`trader`/`filter`/`critic`/`intern`) on `AgentRef`, with per-kind
   I/O contracts; per-Filter `granularity` field on `AgentRef`;
   Filter emits user-named signals into downstream agents' briefings;
   strategy can declare graph edges that short-circuit downstream
   calls based on Filter output. (This is the same track listed in
   the 2026-05-21 eval-honesty intake — amended there to include the
   per-Filter granularity requirement and the coordination note that
   this track consumes the post-refactor unified `Executor`, not
   `PaperExecutor`.)
3. **`live-bar-source-alpaca`** — `LiveStream` implementation of
   `BarSource` against the Alpaca crypto websocket (canonical) with
   a polling fallback for the websocket-disconnected state; per-
   granularity subscription so a strategy with multiple Filters at
   different cadences gets independent streams; gap detection and
   reconnect-budget semantics; synchronous warmup fetch of
   `LiveConfig.warmup_bars` historical bars at launch before the
   first live bar fires.
4. **`live-eval-launch-and-freeze`** — `LiveConfig` schema and
   storage on `eval_runs`; nullable `scenario_id` for `mode = Live`;
   pre-launch validation rules; launch UX entry point and form;
   in-flight UX (reusing the existing SSE event bus); "Save as
   historical scenario" action with empirical cost-model seeding;
   freeze writes a `ScenarioSource::Frozen` row.

## Raw items → tracks

| Raw item | Track | Lane |
|---|---|---|
| Per-Filter `granularity` field on `AgentRef`; Filter fires on its own cadence, decoupled from other agents | `agent-graph-composition` | foundation |
| Extract `BarSource` / `Clock` / `FillSink` traits; collapse to a single `Executor` | `executor-refactor` | foundation |
| Delete `PaperExecutor` and every direct reference | `executor-refactor` | foundation |
| Collapse `RunMode` to `{ Backtest, Live }` in both `xvision-engine::eval::run` and `xvision-core::config`; migrate all `RunMode::Paper` callers | `executor-refactor` | foundation |
| Move the confused-deputy venue-label gate from `Scenario` to `LiveConfig` | `executor-refactor` | leaf |
| Alpaca crypto websocket `BarSource` with poll fallback; subscription keyed by `(asset, granularity)` pair (forward-compatible with F30 multi-asset) | `live-bar-source-alpaca` | foundation |
| Gap detection on the bar stream; reconnect-budget abort semantics | `live-bar-source-alpaca` | leaf |
| Synchronous warmup fetch of `warmup_bars` historical bars at launch | `live-bar-source-alpaca` | leaf |
| `LiveConfig` schema on `eval_runs`; nullable `scenario_id` for `mode = Live` | `live-eval-launch-and-freeze` | foundation |
| Pre-launch `LiveConfig` validation: `assets.len() == 1` (v1), each asset in `xvision_data::asset_whitelist::alpaca_crypto_asset`, stop-policy non-empty, broker creds reachable (mirror `eval-provider-preflight` pattern from the eval-honesty intake), `VenueLabel::Live` rejected, market-only order constraint inherited from `broker_rules.rs` | `live-eval-launch-and-freeze` | foundation |
| Launch UX entry point + form (TBD in track contract; likely a "Start Live" action on the strategy detail page) | `live-eval-launch-and-freeze` | leaf |
| In-flight UX surfaces (reuse existing SSE event bus; bar arrivals + filter signals + trader decisions + broker submits stream into the run-detail page) | `live-eval-launch-and-freeze` | leaf |
| "Save as historical scenario" action on completed Live runs; empirical cost-model seeding from `FillProvenance`; new `ScenarioSource::Frozen` variant | `live-eval-launch-and-freeze` | foundation |
| `ScenarioValidationError` branch: `AlpacaHistorical` requires past `time_window.end`, `Frozen` follows historical rules; freeze writes a `Frozen` row that round-trips through the validator | `live-eval-launch-and-freeze` | leaf |

## Multi-asset coordination (F30)

This intake's single-asset constraint is **a v1 gate, not an
architectural ceiling**. `LiveConfig.assets` is shaped plural from day
one so F30 (multi-asset scenarios, see `FOLLOWUPS.md` §F30) can lift
the `len() == 1` validator without a schema migration on
`eval_runs.live_config_json`. Three coordination points the conductor
should be aware of when F30 lands:

- **Filter × asset.** A Filter in the multi-asset world plausibly
  observes a subset of the strategy's assets — not necessarily all of
  them. Whether `AgentRef` gains an `assets: Option<Vec<AssetSymbol>>`
  field (default = inherit strategy assets) is a question for the
  `agent-graph-composition` track, not this one. The Live executor
  must consume whatever shape that track lands without re-baking the
  single-asset assumption into the `BarSource` subscription logic.
  `live-bar-source-alpaca` should subscribe per-(asset, granularity)
  pair, not per-granularity alone.
- **Frozen Scenario inherits Scenario constraints.** When freeze
  materializes a `ScenarioSource::Frozen` row, that row goes through
  `Scenario::validate_v1`, which today requires `asset.len() == 1`.
  Multi-asset Live freeze therefore stays blocked until F30 M2
  (immutable multi-asset scenarios) lifts the validator wall.
  Single-asset Live freeze works today against `validate_v1`
  unchanged.
- **`TraderDecision.asset`.** F18's partial field exists today,
  defaulted to `None` and resolved to the active scenario's single
  asset at downstream sites. Live mode inherits this resolution path
  unchanged in v1 (single asset, resolution trivial). When F30's full
  cascade lands, the Live executor will need to honor
  `TraderDecision.asset` for routing the broker submit — but the seam
  is already there; no new work in this intake.

## ReplayMode disposition post-refactor

The executor refactor leaves `Scenario.replay_mode` in place but
narrows its meaning: post-refactor it is read **only by Backtest mode**
(Backtest = `InjectedBars` + `InstantClock`). Live mode is scenario-
less and does not consult `replay_mode`. The placeholder
`ReplayMode::Realtime` variant remains unused.

F31 (`ReplayMode` extensions — Stepped, Accelerated, Realtime; see
`FOLLOWUPS.md` §F31) is **partially unblocked by this intake.** F31
states "Realtime is gated on live-paper mirror"; the live-paper mirror
*is* what `live-bar-source-alpaca` + `live-eval-launch-and-freeze`
deliver. After this intake's tracks land, F31's Realtime variant can
be retired entirely (its purpose was to fold realtime into the Backtest
executor, which the refactor renders unnecessary — realtime lives in
Live, period). F31's Stepped + Accelerated remain Backtest-mode
extensions and are unaffected here.

## Open questions deferred to track contracts

These are deliberately not decided in the intake; they belong in the
contract for the listed track. The conductor should resolve them
before that track starts.

- **CLI verb shape** (track: `live-eval-launch-and-freeze`). `xvn eval
  run --mode=live ...` vs. a sibling `xvn live run ...` verb. Today's
  `xvn eval run` family is the natural home; an additional verb
  duplicates surface area for no gain. Recommendation will land in
  the track contract.
- **`FillSink` error-class compatibility** (track: `executor-
  refactor`). The current `classify_run_failure` taxonomy
  (`broker_auth`, `broker_unsupported`, `broker_insufficient_funds`,
  `broker_timeout`, `broker_rejected`, `repeated_broker_error`) is
  produced by `PaperExecutor`'s error wrapping today. The new
  `RealBrokerFills` `FillSink` must produce the same classes —
  these are wire-shape commitments downstream consumers parse.
- **Multi-Filter signal cardinality per cycle** (track: `agent-
  graph-composition`). If two Filters fire in the same bar at the
  same granularity, does the trader run once with both signals or
  twice (once per signal)? Affects `decision_limit` accounting. The
  Live intake consumes whichever model the agent-graph track picks.
- **Bar source subscription key shape** (track: `live-bar-source-
  alpaca`). Per-granularity subscription works for single-asset
  today; per-`(asset, granularity)` pair is the forward-compatible
  shape. Recommendation: build the per-`(asset, granularity)` shape
  from the start so F30 multi-asset doesn't require a re-plumb.
- **Order-type constraint inheritance** (track: `live-eval-launch-
  and-freeze`). `crates/xvision-engine/src/eval/broker_rules.rs`
  enforces market-only v1. Live inherits this until the track
  contract says otherwise.

## Out of this intake

- Real-money continuous live production trading. The infrastructure
  to even *allow* `VenueLabel::Live` at runtime is deferred to a
  later milestone, after this track plus the kill-switch /
  per-strategy-verdict / safety-pause work in `v2b-broker-wallet-
  kill-switch` have hardened.
- Live-vs-replay comparison surface. Not in v1, not as a flagged
  follow-up. A backtest of a frozen scenario *is* the replay; the
  comparison is the operator's eyeball if they want it.
- Tick-driven or open-bar firing semantics. Cadence is per-Filter
  granularity (bars), period. Sub-bar latency is a strategy-design
  choice (pick a smaller granularity), not an execution-mode
  choice.
- Multi-asset Live runs (operationally). v1 validates
  `LiveConfig.assets.len() == 1`. The plural-shaped field unblocks
  F30 without a migration; lifting the validator wall is F30's job.
  See the "Multi-asset coordination" section above for the
  coordination points.
- Re-launching a frozen scenario as a Live run. Not supported; a
  frozen scenario is historical by definition, and a re-launch would
  need a fresh `LiveConfig` regardless. If the operator wants the
  same setup again, the Live launch form supports clone-from-prior-
  run (sourced from the prior Live run record, not from a frozen
  scenario).

## Cross-references

- `team/intake/2026-05-21-eval-honesty-and-agent-graph.md` — host of
  the `agent-graph-composition` track. The Filter row in that intake
  has been amended to include the per-Filter `granularity` field
  requirement and the dependency on `executor-refactor`.
- `team/archive/2026-05-21-conductor-sweep/contracts/v2b-broker-
  wallet-kill-switch.md` — existing `VenueLabel` / safety-limits /
  confused-deputy work. This intake reuses those primitives without
  modification.
- `crates/xvision-engine/src/eval/scenario.rs` — the historical-only
  `validate_v1` and the `ReplayMode::Realtime` placeholder both
  remain; the executor refactor leaves `Scenario` alone (it stays
  historical-only) and adds `LiveConfig` as the new sibling shape.
- `crates/xvision-engine/src/eval/broker_rules.rs` — v1 market-only
  order constraint inherited by Live.
- `FOLLOWUPS.md` §F18 (TraderDecision.asset cascade), §F30
  (multi-asset scenarios), §F31 (ReplayMode extensions). Coordination
  with each documented in the "Multi-asset coordination" and
  "ReplayMode disposition" sections above.
