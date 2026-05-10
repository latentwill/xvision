# Fibonacci Strategy

This folder is a working backlog of xvision strategy ideas built around Fibonacci-style structure, generated with the ideonomy-plain lens.

The ideonomy pass pushed the space toward two useful axes:
- naturalness - does price behavior look self-similar and sequence-like, or irregular?
- scope - should the rule act on short pullbacks, full swings, or regime-level cycles?

## Strategy set

- `fibonacci_pullback_reentry.md` - enter pullbacks at common retracement zones when the trend is still intact.
- `fibonacci_extension_breakout.md` - project breakout targets using extension levels after compression resolves.
- `fibonacci_pyramid_scaling.md` - scale into winners in Fibonacci-sized steps instead of all at once.
- `fibonacci_liquidity_retracement_filter.md` - use funding, open interest, and liquidation stress to decide whether a retracement is healthy or dangerous.
- `fibonacci_cycle_alignment.md` - combine Fibonacci spacing with multi-timeframe trend confirmation and volatility regime filters.

## Notes

- These are concept specs, not code.
- Each file uses the xvision strategy template: Thesis, Inputs, Parameters, Decision rule, Expected regime, Data dependencies, Status, References.
- The ideas are written to fit the current `MarketSnapshot` surface in `crates/xvision-core/src/market.rs`.
