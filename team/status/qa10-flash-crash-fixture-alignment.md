---
track: qa10-flash-crash-fixture-alignment
worktree: .worktrees/qa10-flash-crash-fixture-alignment
branch: qa10-flash-crash-fixture-alignment
phase: implemented-local-verified
last_updated: 2026-05-15T05:55:57Z
owner: codex
---

# Summary

- Added generic probe lookup support for `$XVN_PROBES_DIR` and
  `$XVN_DATA_DIR/probes`, with the workspace `data/probes` path retained for
  local runs.
- Included `data/probes` in Docker build contexts and copied packaged probe
  assets into both CLI and deploy images.
- Seeded packaged probe assets from `/opt/xvision/data/probes` into
  `$XVN_DATA_DIR/probes` at container startup without overwriting operator
  files.
- Materialized the August 2024 BTC/USD 1h 30-day probe under the computed
  scenario cache key:
  `c658f746e96b671a6bb8935dd9e2a08455bb8f9bbd5cc6796775b5d8aa728a69.parquet`.

# Verification

- `cargo run -p xvision-data --example write_flash_fixture` generated the
  parquet asset, then the temporary generator was removed before commit.
- `cargo run -p xvision-engine --example print_flash_cache_key` was attempted
  only to compute the key, but hit a pre-existing duplicate `delete` compile
  error in `crates/xvision-engine/src/api/strategy.rs`; no engine output was
  used.
- `cargo test -p xvision-data fixtures::tests::ensure_default_test_fixture_creates_file -- --nocapture`
- `git diff --check`

# Notes

- The implementation intentionally avoids scenario-name-specific runtime code.
  Scenario lookup remains driven by `bar_cache_policy.cache_key`; the
  flash-crash item is represented as a packaged data asset at that key.
