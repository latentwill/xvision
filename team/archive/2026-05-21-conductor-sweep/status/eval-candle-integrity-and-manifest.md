# Status: eval-candle-integrity-and-manifest

**Track:** eval-candle-integrity-and-manifest  
**Status:** pr_open  
**PR:** https://github.com/latentwill/xvision/pull/415  
**Branch:** task/eval-candle-integrity-and-manifest  
**Updated:** 2026-05-21

## What shipped

All V2E acceptance items 18 and 21 implemented:

- `validate_ohlcv` in `xvision-data/src/validate.rs` — 7 `DataDefect` variants, severity tiers, calendar-aware gap detection, wick-shock outlier detection
- `DataManifest` + `bars_content_hash` in `xvision-data/src/manifest.rs` — canonical JSON hash + SHA-256 over Parquet bytes
- `load_ohlcv_fixture_with_hash` in `fixtures.rs` — recommended entry point for scenario runners
- Migration 026 (`026_run_bars_manifest.sql` + `.down.sql`) — adds `bars_content_hash`, `manifest_canonical`, `bars_manifest` to `eval_runs`
- `ManifestMismatch` typed error + `CompareOptions.allow_manifest_mismatch` guard in `compare.rs`
- `Finding::from_data_defect` constructor in `findings/mod.rs`
- `Scenario::data_manifest` / `calendar_hint` helpers in `eval/scenario.rs`
- `RunStore::set_bars_manifest` writer in `eval/store.rs`
- 20 integration tests in `tests/data_integrity_validator.rs` — all pass

## Test results

- `cargo test -p xvision-data` → 37/37 pass
- `cargo test --test data_integrity_validator` → 20/20 pass
- `cargo fmt --all -- --check` → clean
- `cargo clippy -p xvision-data` → zero warnings in xvision-data files
