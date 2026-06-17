# X Strategy

This folder is a working backlog of xvision strategy ideas, generated using the ideonomy-plain lens.

The ideonomy pass emphasized five useful axes:
- homogeneity - do the signals agree, or is the market mixed?
- cyclicity - is the setup one-shot, periodic, or continuous?
- modularity - can the signal be layered, or is it a single gate?
- purpose - is the strategy meant to trend-follow, mean-revert, or de-risk?
- scope - is it intraday, swing, or regime-level?

## Strategy set

- `funding_skew_fade.md` - fade crowded perp positioning when funding and sentiment get stretched.
- `liquidation_cascade_reversal.md` - trade the snapback after capitulation flushes.
- `stablecoin_inflow_risk_off.md` - reduce risk when liquidity looks like it is rotating to the sidelines.
- `volatility_squeeze_breakout.md` - take directional breakouts after compression.
- `multi_timeframe_trend_pyramid.md` - ride aligned trends and add on pullbacks.

## Notes

- These are concept specs, not code.
- Each file uses the xvision strategy template: Thesis, Inputs, Parameters, Decision rule, Expected regime, Data dependencies, Status, References.
- The ideas are written to fit the current `MarketSnapshot` surface in `crates/xvision-core/src/market.rs`.
