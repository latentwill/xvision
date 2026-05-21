# Alpaca Live — Phase 2: Live bar source

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the `LiveStream` `BarSource` (and the matching `WallClock` `Clock` + `RealBrokerFills` `FillSink`) introduced by Phase 1 — subscribing to Alpaca's crypto websocket for bar-close events, with a poll-based fallback for the disconnected state, multi-`(asset, granularity)` subscription, gap detection, reconnect-budget semantics, and a synchronous warmup fetch of `LiveConfig.warmup_bars` historical bars at launch.

**Architecture:** A new `xvision-data::alpaca_live` module wraps Alpaca's crypto websocket (`wss://stream.data.alpaca.markets/v1beta3/crypto/us`) using the existing reqwest+tokio dependencies + a websocket client (likely `tokio-tungstenite`). Subscriptions are keyed by `(asset_symbol, granularity)` pairs — forward-compatible with F30 multi-asset. The `LiveStream` `BarSource` impl maintains an internal `mpsc::Receiver<Ohlcv>` per subscription; `next_bar` await-pops the next closed bar. Gap detection compares incoming bar timestamps against the expected cadence and emits a `BarGap` trace event when a gap is detected. On websocket disconnect, the source switches to polling the Alpaca historical endpoint at the granularity period until the websocket reconnects; consecutive failures past a configurable budget abort the run. Warmup fetch runs synchronously at launch — pulls `warmup_bars` historical bars before any live bar dispatches, so the agent's `bar_history` window has real context at bar 1.

**Tech Stack:** Rust 2021, tokio, tokio-tungstenite (new dependency), reqwest 0.13 (existing), governor (existing rate limiter), serde_json. Alpaca paper credentials for testing — read from env (`ALPACA_API_KEY_ID` / `ALPACA_API_SECRET_KEY`) via the existing `xvision-config` mechanism.

**Reference spec:** `team/intake/2026-05-21-alpaca-live-eval-and-executor-refactor.md` §Decisions locked #6, §Track sequencing #3, §Open questions deferred to track contracts.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/xvision-data/Cargo.toml` | Modify | Add `tokio-tungstenite`, `futures-util` (already in workspace likely). |
| `crates/xvision-data/src/lib.rs` | Modify | `pub mod alpaca_live;` |
| `crates/xvision-data/src/alpaca_live.rs` | Create | `AlpacaLiveClient` — websocket connection management, subscription routing, message parsing, reconnect loop. Exposes `subscribe(asset, granularity) -> mpsc::Receiver<Ohlcv>`. |
| `crates/xvision-data/src/alpaca_live_poll.rs` | Create | `AlpacaLivePoll` — fallback path. Polls the historical bars endpoint at granularity cadence; emits closed bars to the same `mpsc::Sender`. Used during websocket-disconnected periods. |
| `crates/xvision-data/tests/alpaca_live_mock.rs` | Create | Integration tests using a wiremock websocket server (or recorded fixtures); cover happy-path bar arrival, gap detection, reconnect, disconnect-budget abort. |
| `crates/xvision-engine/src/eval/executor/live_source.rs` | Create | `LiveStream` impl of `BarSource` — wraps `AlpacaLiveClient` + handles the warmup-then-stream lifecycle. |
| `crates/xvision-engine/src/eval/executor/wall_clock.rs` | Create | `WallClock` impl of `Clock` — `now()` returns `Utc::now()`; `wait_until` uses `tokio::time::sleep_until`. |
| `crates/xvision-engine/src/eval/executor/real_broker_fills.rs` | Create | `RealBrokerFills` impl of `FillSink` — wraps a `BrokerSurface` (existing `AlpacaPaperSurface` or future live surface); `submit` calls `broker.submit_order` and translates broker errors to the `classify_run_failure` taxonomy. |
| `crates/xvision-engine/src/eval/bars.rs` | Modify | Extend the existing `load_bars` cache helper to support a `prefetch_for_live(asset, granularity, count_back) -> Vec<Ohlcv>` method for warmup. |
| `crates/xvision-engine/tests/live_source_smoke.rs` | Create | Smoke test against a mocked Alpaca websocket; not run by default in CI (requires network or wiremock setup); gated by `#[ignore]` + a manual `--ignored` flag. |

---

## Phase A — Websocket client

- [ ] A1. Add `tokio-tungstenite` dependency to `xvision-data/Cargo.toml`.
- [ ] A2. Implement `AlpacaLiveClient::connect(creds, env)` — opens the WS to `wss://stream.data.alpaca.markets/v1beta3/crypto/us`, authenticates with `{"action":"auth","key":..,"secret":..}`, awaits the success ack.
- [ ] A3. Implement `subscribe(asset, granularity) -> mpsc::Receiver<Ohlcv>` — sends `{"action":"subscribe","bars":["<symbol>"]}`. Multiple subscriptions per client supported.
- [ ] A4. Spawn a background reader task: parses incoming WS messages, dispatches `bar` events to the appropriate `mpsc::Sender` for the subscription. The `bar` message format from Alpaca: `{"T":"b","S":"BTC/USD","o":...,"h":...,"l":...,"c":...,"v":...,"t":...}` — convert to `Ohlcv` and forward.
- [ ] A5. Heartbeat handling — Alpaca sends `{"T":"success","msg":"connected"}` periodically; track last-message-received-at and trigger reconnect if silent for > 60s.

## Phase B — Reconnect + poll fallback

- [ ] B1. `AlpacaLiveClient::reconnect_loop` — exponential backoff up to a configurable budget (default: 5 retries, 1s/2s/5s/10s/30s).
- [ ] B2. Implement `AlpacaLivePoll` — polls `/v1beta3/crypto/us/bars?symbols=...&timeframe=1Hour&start=<last>&end=now&limit=...` at the granularity cadence (e.g. every 60s for 1Min, every 5min for 5Min, etc.).
- [ ] B3. Switch logic: when WS disconnects, the subscriber task starts the polling fallback for that `(asset, granularity)`; when WS reconnects, polling stops. Bars are deduped by timestamp at the `mpsc::Sender` site so a polling sample arriving simultaneously with a WS sample doesn't double-dispatch.
- [ ] B4. After exhausting the reconnect budget, the `mpsc::Sender` for every subscription is closed; the `LiveStream::next_bar` call sees `None` and the run terminates with a `[live_disconnect_budget_exhausted]` failure class (new — add to `classify_run_failure`).

## Phase C — Gap detection

- [ ] C1. The reader task tracks the last-bar timestamp per subscription. On each new bar, if the timestamp is more than `granularity * 1.5` past the previous, log a `BarGap` event via the existing observability emitter.
- [ ] C2. Gap events are non-fatal — the run continues. They surface in the trace and run-detail UI (Phase 3 wires the UI). Operator decides whether to abort by-policy later (out of scope for v1 Live).
- [ ] C3. Add a unit test: simulate a missed-bar scenario by injecting bars with timestamps that skip one granularity unit; assert one `BarGap` event emitted.

## Phase D — Warmup fetch

- [ ] D1. Implement `eval::bars::prefetch_for_live(asset, granularity, count_back) -> Vec<Ohlcv>` — pulls the most-recent `count_back` closed bars from the existing historical-bars fetcher / cache. Bars-cache code already exists (`xvision-engine/src/eval/bars.rs` from F30 M1).
- [ ] D2. `LiveStream::new(config) -> Self` runs `prefetch_for_live` synchronously before returning; bars are exposed via `BarSource::warmup_bars` to the unified `Executor`.
- [ ] D3. Edge case: warmup fetch returns fewer bars than requested (asset history starts later than `now - count_back * granularity`). Surface as a warning event; run continues with whatever was fetched.

## Phase E — Executor wiring + tests

- [ ] E1. Wire `LiveStream`, `WallClock`, `RealBrokerFills` into the unified `Executor` from Phase 1. The construction site (likely `api::eval::run` for Live mode) instantiates all three from `LiveConfig`.
- [ ] E2. End-to-end smoke test under wiremock: start a fake Alpaca WS server, run the executor against it, dispatch one bar, assert a decision-then-fill round-trip completes. `#[ignore]`-gated unless `XVN_LIVE_TEST=1`.
- [ ] E3. Manual smoke against Alpaca paper: with real creds in env, run a 5-minute backtest at 1Min granularity; confirm bars arrive, dispatches fire, fills land back. Document the smoke procedure in `crates/xvision-data/src/alpaca_live.rs` doc comment.

## Phase F — Multi-`(asset, granularity)` subscription

- [ ] F1. The websocket message format and subscribe call already support multiple symbols. Verify `subscribe(asset_a, gran_1)` + `subscribe(asset_b, gran_2)` produce two independent `mpsc::Receiver`s with non-interleaved bars.
- [ ] F2. The `LiveStream` `BarSource` for a strategy with multiple Watchers at different granularities multiplexes — `next_bar` returns the next bar from ANY subscription, tagged with `(asset, granularity)` so the executor routes it to the correct Filter / agent.
- [ ] F3. Test: subscribe to BTC@1Min + ETH@5Min on a fake WS; emit interleaved bars; confirm both subscriptions deliver independently.

## Verification

```bash
cargo test -p xvision-data
cargo test -p xvision-engine
cargo clippy --workspace -- -D warnings
cargo fmt --check
bash scripts/board-lint.sh
# Smoke (manual, requires Alpaca paper creds):
XVN_LIVE_TEST=1 cargo test -p xvision-data alpaca_live -- --ignored
```

## Acceptance

- `LiveStream` connects to Alpaca paper websocket, authenticates, subscribes to BTC/USD@1Hour, and emits the next closed bar within 1.5x granularity.
- Disconnect → poll fallback → reconnect cycle works end-to-end with one simulated network drop.
- Reconnect budget exhaustion produces a `[live_disconnect_budget_exhausted]` failure class on the run.
- Gap detection emits `BarGap` events for synthetic missed bars; non-fatal.
- Warmup fetch returns N bars synchronously; `BarSource::warmup_bars` exposes them to the executor at run start.
- Multi-subscription (multiple `(asset, granularity)` pairs) delivers bars independently and in-order per-subscription.

## Out of scope

- `LiveConfig` schema / persistence — Phase 3.
- Pre-launch validation — Phase 3.
- UI for live runs — Phase 3.
- Equities support (Alpaca equities use a different WS endpoint and authentication shape) — v1 is crypto-only per spec.
- Tick-level data — bar-close only per the intake's Q8.
- Open-bar / partial-bar firing — bar-close only.

## Source links

- `team/intake/2026-05-21-alpaca-live-eval-and-executor-refactor.md` — intake.
- `docs/superpowers/plans/2026-05-21-alpaca-live-1-executor-refactor.md` — Phase 1 (depended-on).
- `crates/xvision-data/src/alpaca.rs` — existing historical fetcher (sibling module).
- `crates/xvision-execution/src/alpaca.rs` — existing Alpaca broker surface (will be lifted from BTC-only by `multi-asset-alpaca-unlock` plan, but this Phase 2 does not block on that).
- `https://docs.alpaca.markets/docs/real-time-crypto-pricing-data` — Alpaca crypto WS reference.
