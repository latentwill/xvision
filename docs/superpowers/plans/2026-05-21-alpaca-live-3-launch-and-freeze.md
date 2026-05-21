# Alpaca Live — Phase 3: Launch + freeze

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the operator-facing surfaces for Live runs: `LiveConfig` schema + storage on `eval_runs`, pre-launch validation, launch UX (CLI + dashboard), in-flight UX (reusing existing SSE bus), and the operator-curated "Save as historical scenario" freeze action that produces a `ScenarioSource::Frozen` row from a completed Live run.

**Architecture:** A new `LiveConfig` value type sits on the `eval_runs` row as a JSON blob (`live_config_json TEXT NULL`), populated when `mode = Live` and `NULL` for backtest. `eval_runs.scenario_id` becomes nullable. The launch path validates `LiveConfig` (asset whitelist, stop-policy non-empty, broker creds reachable, `VenueLabel::Live` rejected v1, market-only inherited) and then constructs the unified `Executor` from Phase 1 with `LiveStream` + `WallClock` + `RealBrokerFills` from Phase 2. The dashboard's existing run-detail surfaces (chart, decisions, equity, traces) work unchanged — they read from `eval_runs` + `decisions` + `traces` and don't care about mode. The new "Save as historical scenario" action lives on the run-detail page for completed Live runs only; clicking it opens an inline form (no popup, per CLAUDE.md), pre-seeds cost-model fields from realized `FillProvenance`, accepts operator edits, and on submit materializes a new `Scenario` row with `source: ScenarioSource::Frozen`.

**Tech Stack:** Rust 2021, sqlx (existing), `xvision-engine` API layer, `xvision-dashboard` SSE bus (existing `RunEventBus`), React + TS frontend, ts-rs.

**Reference spec:** `team/intake/2026-05-21-alpaca-live-eval-and-executor-refactor.md` §Decisions locked #1, #2, #5, #6, §Track sequencing #4, §Open questions deferred to track contracts.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/xvision-engine/src/eval/live_config.rs` | Create | `LiveConfig` struct + `StopPolicy` substruct + `validate(&self)` impl. ts-rs derives. |
| `crates/xvision-engine/src/eval/run.rs` | Modify | `Run` carries an `Option<LiveConfig>`. `new_queued_live(...)` constructor. `mode = Live` precondition. |
| `crates/xvision-engine/migrations/<next>_live_config_on_runs.sql` | Create | `ALTER TABLE eval_runs ADD COLUMN live_config_json TEXT NULL; ALTER TABLE eval_runs ADD COLUMN scenario_id_nullable_workaround ...` (sqlite ALTER limitation — may need rebuild-table approach if the existing schema has scenario_id NOT NULL). Reserve number with conductor. |
| `crates/xvision-engine/src/eval/store.rs` | Modify | `RunStore::insert_run` / `load_run_row` handle nullable `scenario_id` and `live_config_json`. |
| `crates/xvision-engine/src/eval/scenario.rs` | Modify | New `ScenarioSource::Frozen` variant. `validate_v1` allows a `Frozen`-source row that obeys the historical rules (past time_window, single asset, etc.). |
| `crates/xvision-engine/src/api/eval.rs` | Modify | New `POST /eval/runs/live` endpoint accepting `LiveConfig`. Pre-launch validation invoked here. |
| `crates/xvision-engine/src/api/scenario.rs` | Modify | New `POST /scenarios/from-live-run/:run_id` endpoint that materializes a `Frozen` scenario from the completed Live run. |
| `crates/xvision-engine/src/eval/freeze.rs` | Create | `freeze_live_run_to_scenario(run_id, operator_overrides) -> Scenario` — pulls the realized bars from the bar cache, computes empirical fee/slip means from `FillProvenance`, builds the Scenario row, inserts. |
| `crates/xvision-cli/src/eval.rs` | Modify | New CLI verb form — likely `xvn eval run --mode=live --strategy=X --asset=BTC/USD --capital=10000 --stop-time=7d ...`. Open question per intake: dedicated `xvn live run` verb vs. flag on `eval run`. Resolve in this PR. |
| `frontend/web/src/api/types.gen/LiveConfig.ts` | Generate via ts-rs | LiveConfig + StopPolicy types. |
| `frontend/web/src/api/types.gen/ScenarioSource.ts` | Regenerate | Adds `Frozen` variant. |
| `frontend/web/src/features/eval/LaunchLiveForm.tsx` | Create | Inline form (no popup) for launching a Live run. Fields: strategy picker, asset picker (whitelist-validated), capital input, broker-creds picker (defaults if one configured), stop-policy editor (time / bar / decision — at least one required), warmup-bars override, safety-limits override, display_name, description, tags. |
| `frontend/web/src/features/eval/SaveAsHistoricalScenarioForm.tsx` | Create | Inline form on completed Live runs. Pre-seeded with realized cost-model values; operator may edit. On submit, calls `POST /scenarios/from-live-run/:run_id`. |
| `frontend/web/src/features/eval/RunDetail.tsx` | Modify | Conditional render: completed Live runs show the "Save as historical scenario" form below the metrics; Live runs in-flight show the existing SSE-driven streaming surfaces unchanged. |
| `frontend/web/src/features/strategies/StrategyDetail.tsx` | Modify | Add "Start Live run" action that navigates to the launch form pre-populated with the strategy. |
| `frontend/web/tests/components/LaunchLiveForm.test.tsx` | Create | Vitest covering form validation, asset whitelist enforcement, stop-policy non-empty rule, broker creds picker behavior. |
| `frontend/web/tests/components/SaveAsHistoricalScenarioForm.test.tsx` | Create | Vitest covering empirical-seed defaults, override flow, submit → API call, success → navigation to new scenario. |
| `crates/xvision-engine/tests/live_launch_validation.rs` | Create | Integration tests for the pre-launch validator. |
| `crates/xvision-engine/tests/freeze_to_scenario.rs` | Create | Integration test: simulate a completed Live run with N fills; call freeze; assert the resulting Scenario row round-trips through `validate_v1` and replays via Backtest mode to produce a Run with the same decision sequence (modulo LLM nondeterminism — assert on deterministic fields only). |

---

## Phase A — Data model

- [ ] A1. Define `LiveConfig` in `eval/live_config.rs`. Fields per intake §Decision 6: `strategy_id`, `assets: Vec<AssetRef>` (plural, len()==1 in v1), `capital`, `broker_creds_ref: String`, `stop_policy: StopPolicy`, `venue_label: VenueLabel` (default Paper), `warmup_bars: Option<u32>`, `safety_limits: Option<SafetyLimits>`, `display_name`, `description`, `tags`, `notes`.
- [ ] A2. Define `StopPolicy { time_limit_secs: Option<u64>, bar_limit: Option<u32>, decision_limit: Option<u32> }`.
- [ ] A3. Implement `LiveConfig::validate(&self) -> Result<(), LiveConfigValidationError>`:
  - `assets.len() == 1` (v1 single-asset)
  - each asset in `xvision_data::asset_whitelist::alpaca_crypto_asset`
  - `stop_policy` has at least one limit set
  - `venue_label != VenueLabel::Live` (v1 rejects real money)
  - `capital.initial > 0`
  - `broker_creds_ref` resolves to configured creds; creds are reachable (HTTP ping to `/v2/account` — mirror eval-provider-preflight pattern)
- [ ] A4. ts-rs derives — verify types generate cleanly to `frontend/web/src/api/types.gen/`.
- [ ] A5. Unit tests for each validation rule in `crates/xvision-engine/tests/live_config_validation.rs`.

## Phase B — Storage migration + Run shape

- [ ] B1. Migration `<next>_live_config_on_runs.sql`: add `live_config_json TEXT NULL` to `eval_runs`; make `scenario_id` nullable (SQLite ALTER restriction — may require rebuild-table approach: rename table, create new, copy rows, drop old). Reserve number via `team/MANIFEST.md`.
- [ ] B2. `Run` struct gains `live_config: Option<LiveConfig>` and `scenario_id: Option<String>`.
- [ ] B3. `RunStore::insert_run` writes `live_config_json` when `mode = Live`; `null` for backtest.
- [ ] B4. `RunStore::load_run_row` parses `live_config_json` into `LiveConfig` for Live runs; backtest rows stay as before.
- [ ] B5. Every existing JOIN against `eval_runs.scenario_id` audited: handle `NULL` (likely with `LEFT JOIN` or filter). Affected queries are in `eval/store.rs`, `api/eval.rs`, `api/scenario.rs`, `api/search.rs`.

## Phase C — Launch path

- [ ] C1. New API endpoint `POST /eval/runs/live` (`api/eval.rs`) accepts `LiveConfig` JSON. Calls `LiveConfig::validate`. On success, constructs a `Run` with `mode = Live`, persists, hands off to the executor.
- [ ] C2. Executor construction site: build `LiveStream` + `WallClock` + `RealBrokerFills` from Phase 2, run the unified `Executor` (Phase 1). The `confused-deputy` gate (already rewired in Phase 1) reads from `LiveConfig.venue_label`.
- [ ] C3. CLI verb — decide on shape per intake's open question. Recommend `xvn eval run --mode=live --strategy=... --asset=... --capital=... --stop-time=...` (single verb, swap form by flag) over a separate `xvn live run` to keep one entry point. Add the flag set; reuse the existing eval-run CLI scaffold.
- [ ] C4. The CLI's existing pre-launch checks (provider preflight per eval-honesty's `eval-provider-preflight`) extend to also call `LiveConfig::validate`.

## Phase D — In-flight surfaces

- [ ] D1. The existing `RunEventBus` (SSE) streams bar arrivals, watcher signals (a.k.a. Filter signals), trader decisions, broker submits. Verify all event kinds work in Live mode unchanged. Most likely fine — these are mode-agnostic — but spot-check with a smoke test.
- [ ] D2. Run-detail UI: a Live run in-flight shows the same chart / decisions / equity surfaces a Backtest run shows. No new components required for the in-flight view.
- [ ] D3. The status badge on the run row: ensure `mode = Live` is visually distinct (e.g. a "LIVE" pill next to the existing "Backtest" pill). The `venue_label` badge from `v2b-broker-wallet-kill-switch` already exists; reuse.

## Phase E — Freeze action

- [ ] E1. New API endpoint `POST /scenarios/from-live-run/:run_id` (`api/scenario.rs`). Validates the run is completed + `mode = Live`. Computes empirical means from `FillProvenance` (fee_bps, slip_bps), bundles them into proposed `VenueSettings` defaults. Returns a `FreezeProposal` value to the caller for review.
- [ ] E2. New API endpoint `POST /scenarios/from-live-run/:run_id/confirm` accepting operator-edited `FreezeProposal`. Builds a `Scenario` with `source: ScenarioSource::Frozen`, `data_source: AlpacaHistorical`, `replay_mode: Continuous`, `time_window: { start: run.actual_start, end: run.actual_end }`, `bar_cache_policy.cache_key` pointing at the run's cached bars, copies asset/granularity/calendar/timezone/capital/etc. from the run's `LiveConfig`. Inserts via the existing scenario-store path. Returns the new scenario row.
- [ ] E3. `ScenarioSource::Frozen` variant added to the enum + ts-rs export. `validate_v1` accepts `Frozen` rows under the historical rules (past `time_window.end`, single asset, etc.).
- [ ] E4. The bar cache must hold the run's bars persistently — verify the `bars_cache` table holds bars by `(cache_key, asset, granularity, timestamp)` past run completion. (Already does per F30 M1 — confirm the cache key is run-scoped so freeze doesn't accidentally re-use a stale window's bars.)
- [ ] E5. Frontend `SaveAsHistoricalScenarioForm`: on completed Live runs only. Inline (no popup). Fields pre-filled with empirical seeds; operator may edit. Submit → freeze endpoint → on success, navigate to the new scenario's detail page.
- [ ] E6. Integration test `freeze_to_scenario.rs`: build a synthetic Live run record with N fills + N bars in the cache, call freeze, assert (a) new Scenario row exists, (b) it round-trips through `validate_v1`, (c) launching a Backtest against the new scenario completes and produces a run with the same fills replayed via `SimulatedFills` with the seeded cost model.

## Phase F — Pre-launch validation polish

- [ ] F1. Asset whitelist: validate against `xvision_data::asset_whitelist::alpaca_crypto_asset` — same function the existing scenario validator uses.
- [ ] F2. Stop-policy ergonomics: `time_limit_secs` UI accepts human-friendly forms ("7d", "1h", "300s") via a small parser; persists as `u64` seconds.
- [ ] F3. Safety-limits ergonomics: defaults applied if absent; operator may override per the `v2b-broker-wallet-kill-switch` shape (already exists).
- [ ] F4. Broker-creds reachability check: HTTP `GET /v2/account` with the configured creds; on 4xx/5xx surface a clear error. Mirror the `eval-provider-preflight` pattern.
- [ ] F5. Max-horizon cap: time_limit_secs ≤ 30 days. Hard cap. Prevents operator typo runs.
- [ ] F6. Tests for each rule.

## Verification

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
pnpm --dir frontend/web typecheck
pnpm --dir frontend/web test --run
pnpm --dir frontend/web lint
bash scripts/board-lint.sh
# Manual smoke (requires Alpaca paper creds):
XVN_ALPACA_LIVE=1 cargo run -p xvn -- eval run --mode=live --strategy=<known-strategy> --asset=BTC/USD --capital=10000 --stop-time=15m --granularity=1Min
# Watch the run-detail page; freeze it; backtest the frozen scenario; compare.
```

## Acceptance

- A Live run launches via CLI and via dashboard, validates LiveConfig, dispatches the unified Executor with LiveStream + WallClock + RealBrokerFills.
- In-flight UX shows bars + decisions + fills streaming via the existing SSE bus.
- Stop-policy enforcement: a run with `time_limit_secs=900` terminates at ~15 minutes; a run with `decision_limit=10` terminates at the 10th LLM dispatch; a run with `bar_limit=100` terminates at the 100th bar consumed.
- Confused-deputy gate: launching with `venue_label=Paper` and Alpaca paper creds works; launching with `venue_label=Live` is rejected at validation.
- The "Save as historical scenario" form is shown only on completed Live runs; submission produces a `ScenarioSource::Frozen` row that round-trips through `validate_v1`.
- Backtest mode against a frozen scenario completes and produces a run that's structurally comparable to the source Live run (same bars, same decisions modulo LLM nondeterminism, simulated fills via the seeded cost model).
- No popups introduced (CLAUDE.md rule).

## Out of scope

- Real-money Live (`VenueLabel::Live` accepted) — future milestone after kill-switch / per-strategy-verdict hardening.
- Live-vs-replay comparison surface — intake explicitly excludes; a backtest of a frozen scenario *is* the replay.
- Tick-driven / open-bar firing — bar-close only.
- Multi-asset Live launches — `assets.len() == 1` v1 wall; lifted by `docs/superpowers/plans/2026-05-21-multi-asset-alpaca-unlock.md`.
- Re-launching a frozen scenario as a Live run — not supported per intake. If the operator wants the same setup, the Live launch form supports clone-from-prior-run (sourced from the prior Live run record, not from a frozen scenario).

## Source links

- `team/intake/2026-05-21-alpaca-live-eval-and-executor-refactor.md` — design intake.
- `docs/superpowers/plans/2026-05-21-alpaca-live-1-executor-refactor.md` — Phase 1 (depended-on).
- `docs/superpowers/plans/2026-05-21-alpaca-live-2-bar-source.md` — Phase 2 (depended-on).
- `docs/superpowers/plans/2026-05-21-filter-v1.md` — Filter v1 plan; this Live plan does not block on Filter v1 landing, but Live runs with `activation_mode=FilterGated` strategies require it.
- `team/archive/2026-05-21-conductor-sweep/contracts/v2b-broker-wallet-kill-switch.md` — existing safety primitives reused unchanged.
