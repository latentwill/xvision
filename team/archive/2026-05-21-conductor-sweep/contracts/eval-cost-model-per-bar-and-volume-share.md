---
track: eval-cost-model-per-bar-and-volume-share
lane: foundation
wave: v2e
worktree: .worktrees/eval-cost-model-per-bar-and-volume-share
branch: task/eval-cost-model-per-bar-and-volume-share
base: origin/main
status: merged
depends_on: []
blocks:
  - eval-intra-bar-fill-ordering
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/scenario.rs              # VenueSettings extension + VenueOverride — disjoint region with candle-integrity
  - crates/xvision-engine/src/eval/executor/backtest.rs     # simulate_fill rewrite — disjoint region with trace/intra-bar/broker tracks
  - crates/xvision-engine/src/eval/cost_arrays.rs           # NEW — per-bar parquet column reader
  - crates/xvision-engine/src/eval/findings.rs              # volume_share_excess kind registration — disjoint region with trace-foundation/candle-integrity
  - crates/xvision-engine/tests/cost_model_*.rs             # NEW
  - frontend/web/src/api/types.gen/**                       # ts-rs regenerated
forbidden_paths:
  - frontend/web/src/**                                     # no UI work this track
  - crates/xvision-data/**                                  # candle-integrity owns this crate
  - crates/xvision-eval/**                                  # lookahead-bias-prober owns baselines
  - crates/xvision-engine/migrations/**                     # no schema change — per-bar arrays live in the bars Parquet alongside OHLCV
interfaces_used:
  - xvision-engine::eval::scenario::VenueSettings
  - xvision-engine::eval::scenario::Scenario
  - xvision-data::fixtures::Ohlcv
parallel_safe: true
parallel_conflicts:
  - eval-candle-integrity-and-manifest (scenario.rs — disjoint regions; this track adds VenueOverride and per-bar array support, candle adds manifest fields)
  - eval-trace-surface-foundation (backtest.rs + findings.rs — disjoint regions; foundation owns the emit schema and findings columns, this track owns the fill math and the volume_share_excess kind)
  - eval-intra-bar-fill-ordering (backtest.rs — disjoint regions; this track owns the fill-price math, intra-bar owns the fill-trigger math; their hunks are adjacent but disjoint)
  - eval-broker-rule-findings (backtest.rs — disjoint regions; broker-rule owns the order-emission hook, this track owns the fill-price math)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine cost_model_
  - cargo test -p xvision-engine eval::scenario
  - pnpm --dir frontend/web typecheck
acceptance:
  - **Per-bar cost arrays.** `Scenario` accepts optional Parquet columns `fee_bps`, `slip_bps`, `spread_bps` aligned to the bars by timestamp. If a column is present at fixture load, the simulator consumes it per-bar; if absent, falls back to the scenario default. Columns live in the same `<bars_content_hash>.parquet` file as OHLCV — no separate file.
  - **Per-asset overrides.** `VenueSettings` extended with `overrides: Vec<VenueOverride { symbol_pattern: String, fees: Option<Fees>, slippage: Option<SlippageModel> }>`. Per-symbol-pattern (glob), not per-scenario. Default falls through when no pattern matches.
  - **Override precedence (highest wins):** per-bar array → per-asset override → scenario default.
  - **Volume-share slippage model.** New `SlippageModel::VolumeShare { price_impact: f64, volume_limit: f64 }` variant. `volume_share = min(order_qty / bar_volume, volume_limit)`; `fill_price = mid * (1 ± price_impact * volume_share²)`. Defaults: `price_impact = 0.1`, `volume_limit = 0.025` (zipline canonical).
  - **`volume_share_excess` finding.** Emitted when the cap binds (`order_qty / bar_volume > volume_limit`). Payload: `{ requested_qty, bar_volume, cap_binding_qty, fill_share }`. `produced_by_check = "sim:volume_cap"`. `evidence_cycle_ids` contains the cycle whose order hit the cap.
  - **Fill provenance written to trace.** Per fill, write `slip_bps_applied`, `spread_bps_applied`, `fee_bps_applied`, `fee_source` (`Default` | `ScenarioOverride` | `PerAssetOverride` | `PerBarArray`), `volume_share`, `volume_cap_bound`. These are the trace fields landed by `eval-trace-surface-foundation`; this track populates them.
  - **Behavior at low volume_share.** For `order_qty / bar_volume < 0.005`, `VolumeShare` should produce fills within 1 bp of `Linear { bps: 5 }` for reasonable parameter choices. (Not a hard test bound; sanity check that the model collapses to near-flat at small sizes.)
  - **Existing `Linear { bps }` and `None` slippage models continue to work.** Old scenarios without per-bar arrays or `VolumeShare` selection produce identical fills to the current behavior — verified via the existing 9 tests at `backtest.rs:830–940` (any updates have an explicit `# Updated because <reason>` comment).
  - **ts-rs exports.** `VenueOverride`, the extended `SlippageModel` enum, `FeeSource` are regenerated under `frontend/web/src/api/types.gen/`.
  - **Tests:**
    * Fee accuracy at varying notionals (1k, 10k, 100k, 1M nominal positions); fee_bps × notional matches expected within 1e-6.
    * Slippage sign per side under realistic positions (buy slips up, sell slips down) under both `Linear` and `VolumeShare`.
    * Per-bar array consumption: provide a 100-bar test fixture with a `slip_bps` column; assert the simulator picks the per-bar value when present and falls back to the scenario default when absent (zero-fill the column for 10 bars).
    * Per-asset override precedence: `BTC/USD` override beats scenario default; `ETH/USD` falls through to default; both round-trip through serde.
    * Volume-share quadratic at boundaries: `volume_share = 0` → zero impact; `volume_share = volume_limit` → max impact at the cap.
    * Cap binding emits `volume_share_excess` finding once per binding cycle.
    * Behavior collapses to near-`Linear` at very low `volume_share`.

---

# Scope

Combines research doc §4.2 (per-bar cost arrays) + §4.3 (volume-share
slippage). The intake's optional re-pairing applied: §4.3 strictly
consumes §4.2's machinery on the same files; splitting them adds
coordination cost without independence value.

This is the single largest architectural unlock in V2E. After it
lands, every downstream cost concern — regime-aware, volatility-aware,
time-of-day, exchange-fee-tier — can be implemented as Parquet column
population offline, then consumed verbatim. No further simulator
changes needed for cost realism.

# Out of scope

- §4.4 partial fills + order rollover. `eval-intra-bar-fill-ordering`
  lands the minimal `OrderState` enum so the schema is ready; the
  carry-loop is deferred to a follow-up wave once `volume_share_excess`
  cap-hits show up in real runs.
- §4.5 maker/taker aggressor-side fees. Promoted into
  `eval-intra-bar-fill-ordering` (depends on its `OrderState`).
- §4.6 Corwin-Schultz spread proxy. Available as an optional
  `spread_bps` column source (this track lands the consumer); full
  default rollout is a follow-up.
- §4.8 latency model. Defer until trace foundation makes the latency
  knob inspectable.
- §4.11 Almgren-Chriss market impact. Skip until trade size justifies.

# Migration coordination

No migration claimed. Per-bar arrays live in the same `<bars_hash>.parquet`
file as OHLCV (additional optional columns); per-asset overrides live
on `Scenario.venue.overrides` (serde JSON, no SQL change).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/eval-cost-model-per-bar-and-volume-share status
git -C .worktrees/eval-cost-model-per-bar-and-volume-share log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-cost-model-per-bar-and-volume-share -b task/eval-cost-model-per-bar-and-volume-share origin/main
```

# Notes

The `symbol_pattern` in `VenueOverride` is a glob (`BTC/USD`, `*USD`,
`NVDA*`). Use the same matching crate the asset_whitelist code already
uses if possible; otherwise a small `glob::Pattern` dependency is
fine.

When `VolumeShare` is the active model but the bar's `volume` field is
missing or zero, fall back to the scenario default `Linear` and emit a
single `tracing::debug` per `(symbol, bar_ts)` pair. Don't fail the run.
