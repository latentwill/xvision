# Filter v1 regression fixtures

Companion fixtures for `tests/golden_determinism.rs` (Stage 5 of the
Filter v1 plan — see `docs/superpowers/plans/2026-05-21-filter-v1.md`).

## Files

| File | Owner | Role |
|---|---|---|
| `spec_example.toml` / `spec_example.json` | Stage 1 | Parse-roundtrip fixtures for `tests/parse_roundtrip.rs`. |
| `filter_trend_pullback.toml` | Stage 5 | Filter DSL used as the input arm of the determinism gate. Mirror of `spec_example.toml`. |
| `scenario_btc_1h_300bars.json` | Stage 5 | 200 warmup + 100 decision bars of deterministic synthetic OHLCV for BTC/USD on 1h. Input arm of the determinism gate. |
| `expected_events.jsonl` | **Stage 3 (deferred)** | Byte-exact `FilterEventV1` stream produced by stepping the runtime over `scenario_btc_1h_300bars.json` with `filter_trend_pullback.toml`. Lands when Stage 3 ships `FilterEventV1`. |
| `expected_summary.json` | **Stage 3 (deferred)** | Byte-exact `FilterSummary` for the same run. Lands with `expected_events.jsonl`. |

## What "deterministic" means here

- `scenario_btc_1h_300bars.json` is reproducible from a fixed seed
  (`0xF11_7E12_5EED`) by the canonical generator embedded in
  `tests/golden_determinism.rs::generate_canonical_scenario`. The
  committed JSON is the gate; the test
  `scenario_fixture_matches_canonical_generator` will fail if the file
  or the generator drifts.
- The generator uses `xorshift64` over a fixed seed plus deterministic
  arithmetic (no platform-conditional floating-point ops; values are
  rounded to 2 decimal places to dodge tie-rounding edge cases). Should
  be byte-identical across the workspace's macOS / Linux targets.

## Regenerating `scenario_btc_1h_300bars.json`

If a deliberate change to the generator lands:

```bash
cargo test -p xvision-filters --test golden_determinism \
  -- --ignored regenerate_scenario
```

The ignored test writes a fresh JSON into this directory. Commit the
new file in the same PR as the generator change and call it out in the
PR description — silent fixture rewrites defeat the gate.

## When Stages 2/3 land

The golden test
`golden_filter_events_match_recorded_jsonl` is currently `#[ignore]`d
with a placeholder body. Once `xvision_filters::runtime::evaluate` and
`xvision_filters::events::FilterEventV1` exist, the test:

1. parses `filter_trend_pullback.toml`,
2. iterates over the 300 bars of `scenario_btc_1h_300bars.json`,
3. calls `runtime::evaluate(...)` per bar against a fresh
   `FilterRuntimeState`,
4. serialises each `FilterEventV1` as JSONL into a string,
5. asserts byte-equality with the committed `expected_events.jsonl`,
6. aggregates the events into `FilterSummary` and asserts equality with
   `expected_summary.json`.

The first run after Stages 2/3 land will need to write those two
expected files (the regenerate-style flow above generalises — a
companion ignored test will produce them).

## Why generated, not historical, OHLCV

A synthetic seed-driven series:
- has zero external dependency (no Binance / Alpaca API call in CI),
- is small (~36 KB) and diffable,
- is reproducible across machines,
- triggers the `trend_pullback` filter often enough to exercise wake /
  cooldown / daily-cap suppression without being so noisy that any
  generator tweak silently invalidates the gate.
