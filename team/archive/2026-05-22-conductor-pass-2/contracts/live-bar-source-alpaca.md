---
track: live-bar-source-alpaca
lane: foundation
wave: alpaca-live-eval-2026-05-21
worktree: .worktrees/live-bar-source-alpaca
branch: task/live-bar-source-alpaca
base: origin/main
status: ready
depends_on:
  - executor-trait-extraction
blocks:
  - executor-live-shell
  - live-eval-launch-and-freeze
stacking: none
allowed_paths:
  - crates/xvision-data/src/alpaca_live.rs
  - crates/xvision-data/src/alpaca_live_poll.rs
  - crates/xvision-data/src/lib.rs
  - crates/xvision-data/Cargo.toml
  - crates/xvision-data/tests/alpaca_live_*.rs
  - crates/xvision-engine/src/eval/executor/live_source.rs
  - crates/xvision-engine/src/eval/executor/wall_clock.rs
  - crates/xvision-engine/src/eval/executor/real_broker_fills.rs
  - crates/xvision-engine/src/eval/executor/mod.rs
  - crates/xvision-engine/src/eval/bars.rs
  - crates/xvision-engine/tests/eval_executor_live_*.rs
  - team/contracts/live-bar-source-alpaca.md
forbidden_paths:
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/executor/traits.rs
  - crates/xvision-engine/src/eval/scenario.rs
  - crates/xvision-engine/src/eval/run.rs
  - crates/xvision-engine/src/safety/**
  - crates/xvision-engine/src/api/**
  - crates/xvision-engine/migrations/**
  - crates/xvision-core/**
  - crates/xvision-execution/**
  - crates/xvision-dashboard/**
  - crates/xvision-cli/**
  - crates/xvision-mcp/**
  - crates/xvision-memory/**
  - frontend/**
  - team/board.md
  - team/MANIFEST.md
  - decisions/**
interfaces_used:
  - xvision_engine::eval::executor::traits::{BarSource, Clock, FillSink, FillRequest, FillRecord}
  - xvision_core::market::Ohlcv
  - xvision_execution::broker_surface::{BrokerSurface, OrderRequest, OrderConfirmation, Side}
  - xvision_data::alpaca::{BarGranularity, MarketBar}
  - apca::data::v2::stream::{Bar, CustomUrl, MarketData, RealtimeData}
  - apca::Client
  - tokio::sync::{mpsc, watch}
  - chrono::{DateTime, Utc}
  - async_trait::async_trait
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo build --workspace
  - cargo test -p xvision-data
  - cargo test -p xvision-engine --tests eval_executor_live
  - cargo clippy -p xvision-data -p xvision-engine -- -D warnings
  - cargo fmt -p xvision-data -p xvision-engine --check
  - bash scripts/board-lint.sh
acceptance:
  - "`xvision-data::alpaca_live::AlpacaLiveClient` connects to the crypto websocket via `apca::data::v2::stream::CustomUrl<Crypto>` (`wss://stream.data.alpaca.markets/v1beta3/crypto/us`) using credentials read from env or a builder."
  - "Per-(asset, granularity) subscription: the client can `subscribe_bars(asset, granularity)` and receive a channel/stream of `MarketBar` rows. Multiple concurrent subscriptions on different (asset, granularity) tuples are supported (forward compatible with F30 multi-asset)."
  - "Gap detection: when the receive loop observes a bar whose timestamp is more than 1 granularity tick ahead of the previous bar's timestamp, the client logs `tracing::warn!(target = \"xvision_data::alpaca_live\", gap_secs, asset, granularity)` and emits a `GapDetected { from, to }` event on the subscription channel."
  - "Reconnect budget: `AlpacaLiveClient::with_reconnect_budget(n)` sets the max consecutive reconnect attempts (default 5). After exhaustion, the subscription emits a `BudgetExhausted` error and the stream ends. Backoff is exponential with jitter, capped at 30s."
  - "`xvision-data::alpaca_live_poll::AlpacaLivePoll` provides a REST polling fallback using the existing `xvision_data::alpaca` historical fetcher. Polls at the granularity cadence, deduplicates by `MarketBar.timestamp`, and returns only bars strictly newer than the last delivered one."
  - "Both clients can be unit-tested without network: `AlpacaLiveClient` exposes a `from_message_stream(impl Stream<Item = apca::data::v2::stream::DataMessage>)` builder; `AlpacaLivePoll` exposes a `with_fetcher(impl Fn(...) -> ...)` constructor. Tests pin gap detection, dedup, and reconnect-budget behaviour."
  - "`xvision-engine::eval::executor::live_source::LiveStream` implements `BarSource`. Construction takes (asset, granularity, ws_client, poll_fallback, warmup_bars). `LiveStream::new_with_warmup(...)` performs a **synchronous** historical fetch of `warmup_bars` bars before the first call to `next_bar()` returns a live bar; `next_bar()` drains the warmup buffer first, then yields live bars."
  - "Disconnected-websocket fallback: while the websocket subscription is in a reconnecting state, `LiveStream` consumes bars from `AlpacaLivePoll` instead. On websocket reconnect, the live stream resumes and the polling fallback pauses. Test pins the handoff with a scripted mock."
  - "`xvision-engine::eval::executor::wall_clock::WallClock` implements `Clock`. `now()` returns `chrono::Utc::now()` (or an injected `now_fn` for tests); `advance_to()` is a no-op (the wall clock takes no instruction)."
  - "`xvision-engine::eval::executor::real_broker_fills::RealBrokerFills` implements `FillSink`. Wraps `Arc<dyn BrokerSurface>`; translates `FillRequest` to `xvision_execution::broker_surface::OrderRequest` (market-only v1 per `eval/broker_rules.rs`), awaits `submit_order`, translates the resulting `OrderConfirmation` into a `FillRecord` with `FillProvenance::from_broker(...)` (or equivalent existing constructor). Broker errors map onto the existing `classify_run_failure` taxonomy (`broker_auth`, `broker_unsupported`, `broker_insufficient_funds`, `broker_timeout`, `broker_rejected`)."
  - "Unit tests for `RealBrokerFills` use the existing `MockBrokerSurface` (scripted via `xvision_execution::broker_surface::MockBrokerSurface` or an equivalent test double in `tests/`). Tests cover: market-buy translation, market-sell translation, broker-rejected error class propagation, and no-op handling (`action == \"hold\"` never reaches submit, mirroring `SimulatedFills`)."
  - "`eval/executor/mod.rs` registers the new modules with `pub mod live_source; pub mod wall_clock; pub mod real_broker_fills;` and re-exports `LiveStream`, `WallClock`, `RealBrokerFills`. No other behavioral changes to `mod.rs`."
  - "`eval/bars.rs` exposes a public helper `load_warmup_window(ctx, asset, granularity, now, warmup_bars) -> Result<Vec<Ohlcv>>` that wraps the existing `load_bars` with a computed `start = now - warmup_bars * granularity`, `end = now` window. The helper is reused by `LiveStream::new_with_warmup` so warmup goes through the same cache + singleflight path as backtest scenarios."
  - "No engine wiring: nothing in this PR makes a Live run *runnable* end-to-end. The `Executor` site still dispatches via `Backtest`/`Paper`; the executor-live-shell track will replace that dispatch with a unified `Executor` that picks the right trait impls per `RunMode`. The trait impls landed here are reachable only from unit tests."
  - "`PaperExecutor` is NOT deleted in this PR — that's `executor-collapse-paper-mode`. This track only adds the Live trait impls alongside the existing Backtest/Paper code."
  - "Network calls are gated behind real env credentials (`APCA_API_KEY_ID`, `APCA_API_SECRET_KEY`); test suites use stubs. No test in this PR's verification block requires network."
  - "`cargo build --workspace` clean; `cargo test -p xvision-data` and `cargo test -p xvision-engine --tests eval_executor_live` pass; `cargo clippy -p xvision-data -p xvision-engine -- -D warnings` clean; `cargo fmt --check` clean; `bash scripts/board-lint.sh` shows no NEW violations attributable to this track (pre-existing failures on unrelated tracks are acceptable as long as their state is unchanged)."
---

# Scope

Sub-track 3 of the 2026-05-21 Alpaca-Live executor refactor (intake:
`team/intake/2026-05-21-alpaca-live-eval-and-executor-refactor.md`).

This track lands the **Live impls** of the three executor seam traits
introduced by `executor-trait-extraction` (PR #487, merged), plus the
Alpaca-side connectivity modules they depend on:

| Trait | Live impl in this track | Backtest impl (already on main) |
|---|---|---|
| `BarSource` | `LiveStream` (Alpaca crypto websocket + poll fallback + warmup) | `InjectedBars` |
| `Clock` | `WallClock` (`Utc::now()`-driven) | `InstantClock` |
| `FillSink` | `RealBrokerFills` (forwards to `BrokerSurface::submit_order`) | `SimulatedFills` |

The track **does not** wire these impls into a runnable `Executor`.
That happens in `executor-live-shell` (sub-track 4 of the refactor),
which collapses the current `BacktestExecutor` + `PaperExecutor` into a
single `Executor` parameterised on the three traits and picks the impl
trio per `RunMode`. The trait impls landed here are reachable only from
unit tests until that wiring lands.

Splitting like this keeps the review surface bounded: the live
connectivity logic (websocket subscription, gap detection, reconnect
budget, polling fallback) is the genuinely novel piece and deserves
focused review against mocks; the engine-side dispatch rewire is a
mechanical refactor and lands separately.

# What this PR does NOT do (sequenced into later tracks)

- **No `Executor` collapse / `PaperExecutor` deletion.** That's
  `executor-collapse-paper-mode`. `PaperExecutor` and `BacktestExecutor`
  both still exist on disk after this PR; the new Live trait impls sit
  alongside them in `eval/executor/`.
- **No unified `Executor` dispatch.** `executor-live-shell` replaces
  `RunMode`-keyed dispatch with the trait-trio selection. Until then,
  no production code path constructs a `LiveStream` / `WallClock` /
  `RealBrokerFills` — only the unit tests in this PR do.
- **No `LiveConfig` schema / storage / validation.** That's
  `live-eval-launch-and-freeze`. The `LiveStream::new_with_warmup`
  constructor takes its parameters (asset, granularity, warmup_bars,
  reconnect_budget) directly today; `LiveConfig` parses those into a
  builder in the next track.
- **No safety-gate rewire.** The confused-deputy gate keeps reading
  from `Scenario` in this PR; the rewire to `LiveConfig` is
  `live-eval-launch-and-freeze`.
- **No multi-asset (F30).** Subscription is per-`(asset, granularity)`
  from day one (forward-compatible plural shape), but the v1 launch
  surface in `live-eval-launch-and-freeze` will enforce
  `assets.len() == 1` until F30 lands.
- **No real-money trading.** `VenueLabel::Live` rejection at
  `LiveConfig` validation lands in `live-eval-launch-and-freeze`.
  `RealBrokerFills` against `AlpacaPaperSurface` is fine in this PR;
  against `AlpacaLiveSurface` is stubbed (returns an error) at the
  `BrokerSurface` level today, so even a careless wiring couldn't reach
  real money.

# Track dependency

Hard build dependency on `executor-trait-extraction` (PR #487).
Without those traits the `impl BarSource for LiveStream` blocks won't
compile. #487 landed on `origin/main` as `2d43670`; this contract bases
off that commit.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/live-bar-source-alpaca status
git -C .worktrees/live-bar-source-alpaca log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/live-bar-source-alpaca
#   - base contains 2d43670 (executor-trait-extraction)
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/live-bar-source-alpaca \
    -b task/live-bar-source-alpaca origin/main
```

# Error-class stability contract

`RealBrokerFills` errors must map onto the existing
`classify_run_failure` taxonomy so downstream consumers (eval-review,
SSE event bus, dashboard run-detail page) parse the same wire shapes.
The mapping is:

| Broker condition | Class |
|---|---|
| 401 / 403 from Alpaca | `broker_auth` |
| Unsupported asset (rejected by `broker_rules`) | `broker_unsupported` |
| 422 with insufficient buying power | `broker_insufficient_funds` |
| Network timeout / connect failure | `broker_timeout` |
| Other 4xx / 5xx with a body | `broker_rejected` |
| Repeated `broker_timeout`s above the circuit-breaker threshold | `repeated_broker_error` |

The existing `classify_broker_error_message` in
`xvision-execution/src/broker_surface.rs` is the source of truth for
this taxonomy; `RealBrokerFills` calls it (or its equivalent) rather
than inventing a parallel classification path.

# Notes

Free-form. Append checkpoints, surprises, links to PRs. Do not edit
history above the line.

- 2026-05-21 — contract drafted. Sequenced behind `executor-trait-
  extraction` (PR #487, merged 2026-05-21). Sub-track 4
  (`executor-live-shell`) and sub-track 5
  (`live-eval-launch-and-freeze`) are not yet drafted; they consume
  the artifacts from this PR.
