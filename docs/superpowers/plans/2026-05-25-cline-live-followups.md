# Cline Runtime + Alpaca Paper Live Follow-Up Plan

**Date:** 2026-05-25
**Status:** Post-merge follow-up plan.
**Scope:** Cline runtime completion plus Alpaca paper live execution. Orderly, Bybit, and other testnet venues are explicitly out of scope here and live in the separate testnet venue plan.

## Validated Current State

The Cline runtime unification branch landed the sidecar, replay, trajectory frame/store primitives, and the default Cline execution path. The remaining production gap is not the replay core; it is the live recording path from the sidecar event stream into persistent trajectory storage.

The live execution path is also not complete. `RunMode::Live` constructs an Alpaca paper broker, live stream, wall clock, and `RealBrokerFills`, and stores them in `Executor.live_runtime`. The executor loop still follows the backtest path, using injected bars, `InstantClock`, and `SimulatedFills`. With no injected bars, a live run fails before decisions. With injected bars, it is still simulated rather than broker-backed live execution.

The current live broker selection is hardwired to Alpaca paper semantics. Real-money Alpaca live is rejected by validation. Orderly and Bybit are not part of this plan.

## Current Plan

### 1. ClineSDK Trace Alignment

Audit real ClineSDK event behavior against the Rust event and trajectory model before expanding live recording. The audit must cover:

- session lifecycle events
- model request and response frames
- tool call and tool result pairing
- `submit_decision` lifecycle termination
- usage and cost metadata
- retries and provider failover
- cancellation and timeout behavior
- sidecar crash and recovery events
- terminal success, error, and corrupt-recording states

Deliverables:

- event mapping table: ClineSDK event -> sidecar protocol -> Rust `RunEvent` -> `TrajectoryFrame`
- gap list for events that are currently dropped, lossy, reordered, or ambiguous
- fixture recordings for representative success, tool failure, cancellation, and provider-error runs

### 2. Cline Live Recording Wiring

Wire production Cline runs into trajectory persistence.

Work items:

- thread `TrajectoryStore` and `RecordingId` through `AgentClient::spawn_with_event_sink`
- route sidecar `trajectory_frame` notifications through the normal notification parser and dispatcher
- persist frames from eval-side live Cline execution, not only seeded replay tests
- surface recording status, dropped frame count, and recovery reason in existing observability paths
- ensure replay uses the same persisted store that live recording writes

Done criteria:

- a real Cline run records frames into the persistent trajectory store
- replay can load that recording without direct test seeding
- dropped/corrupt/incomplete recordings are visible and typed

### 3. Alpaca Paper Live L1: Single-Asset Loop

Make `RunMode::Live` consume `Executor.live_runtime`.

Work items:

- split the executor run loop into backtest and live branches
- in the live branch, consume `LiveStream.next_bar()`
- execute one per-asset decision cycle per live bar
- submit orders through `RealBrokerFills`
- use `WallClock`, not `InstantClock`
- remove the dependency on injected backtest bars for live mode
- keep the first implementation single-asset, even if internals are shaped for fanout

Hermetic tests:

- mock live stream emits one bar
- mock broker receives the expected order
- no injected bars are required
- live loop uses broker-reported fill data, not simulated bar fills
- live loop exits cleanly on stream end, cancellation, and broker error

### 4. Alpaca Paper Live L2: Multi-Asset Loop

Build multi-asset live execution only after L1 works.

Work items:

- add live stream fanout across active assets
- preserve per-asset decision isolation
- maintain deterministic ordering for event emission and trajectory recording
- handle sparse bars, missing symbols, and reconnect gaps explicitly
- keep venue-specific code outside the executor decision loop

Done criteria:

- an Alpaca paper live run can subscribe to multiple assets
- each active asset can produce decisions and fills
- no path falls back to simulated fills

### 5. Live Config Cleanup For Current Scope

Clean up config only where needed for Alpaca paper correctness. Do not roll out the full venue/environment matrix in this plan.

Work items:

- document that current live mode means Alpaca paper only
- keep real-money `VenueLabel::Live` rejected
- make the paper-only validation error explicit and operator-friendly
- remove stale docs that say `Executor::live` is unimplemented
- clarify that `broker_creds_ref` is a credential lookup, not a venue selector

### 6. Migration, Docs, And Test Harness Cleanup

Close the supporting gaps found during review.

Work items:

- move trajectory migration ownership into the main API migrator
- add provider matrix documentation for xvision provider ids to Cline gateway ids
- replace stand-in crash/pool tests where practical with built-sidecar coverage
- add an eval-side test that uses the built sidecar path
- keep targeted Rust test runs small to avoid unnecessary target-dir growth

## Explicitly Out Of Scope

- Orderly testnet
- Bybit testnet
- Orderly mainnet
- Bybit mainnet
- Alpaca real-money live
- general venue/environment matrix rollout
- deep broker safety review

Those belong to `2026-05-25-testnet-venue-plan.md`.

