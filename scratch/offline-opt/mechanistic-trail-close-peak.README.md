# Mechanistic trailing-stop close-peak alignment

Patches (apply against `origin/main` @ `ca77d7d8`, verified `git apply --check` clean):

    git apply scratch/offline-opt/mechanistic-trail-close-peak-sltp.patch
    git apply scratch/offline-opt/mechanistic-trail-close-peak-backtest.patch

## Problem

The mechanistic executor's `ClosePolicy::TrailingStop` diverged from the
validated offline contract (`scratch/offline-opt/run_round2.py`) in two ways:

1. **Intrabar peak source.** Both backtest call sites passed
   `sltp_state.hwm`, which `update_hwm` tracks from bar *highs* (long) /
   *lows* (short). The offline simulator trails on bar *closes* only, so
   node longs trailed too early and shorts too late.
2. **Wrong-side collapse.** `mechanistic_action` computed
   `let peak = peak_price.max(entry_price)` for both directions. For a
   short the anchor must be the trough (`min`); `.max` collapsed it to the
   entry price whenever the trough was below entry, so the short trail
   never tightened below entry+3%.

Observed impact (BTC-only book, camp-m6): long entered 01-15 trailed out
01-16 at a loss where offline rode to time-exit +6.62%; short entered
01-07 held to time-exit where offline trailed out 01-10 +2.19%. This is
the main residual driver of trend-book sign mismatches (19/27 → expected
closer to 22/27 after fix).

## Fix

- `sltp.rs`: add `close_hwm: f64` to `PositionRiskState`, initialised to
  the entry price and updated per closed bar (max close for longs, min
  close for shorts) inside `check_and_update`. Lifecycle (created on
  entry fill, removed on flat) already covers both executor paths.
- `backtest.rs`: both `mechanistic_action` call sites now pass
  `state.close_hwm`; the peak selection picks the correct side
  (`max` long / `min` short).

No behaviour change for agentic strategies: the SLTP stop/TP layer keeps
using intrabar `hwm`; only the mechanistic trailing anchor changes.

## Verification once Rust builds are allowed

    cargo test -p xvision-engine -- mechanistic trailing
    cargo test -p xvision-engine -- eval_filter_hook

Then re-run the three single-asset trend books over the 27 campaign
windows (scripts pattern in `/tmp/asset-sweep-{btc,eth,sol}.sh`,
strategy ids `01M0S6PC9HEFYZKKS6053TBPM9` / `01M0S6PDS6E790D9HWR2C42QYS`
/ `01M0S6PFA9YP56PK8EYX4T0JEQ`) and re-check sign agreement vs
`scratch/offline-opt/results.json` round2 `per_window_pnl`.
