# Stablecoin Inflow Risk-Off

## Thesis

When stablecoin inflows spike, the market is often preparing to deploy capital or rotate into risk. If that inflow coincides with weak trend structure and rising realized volatility, the safer trade can be to de-risk or short the weakest leg.

## Inputs

- `OnchainPanel.stablecoin_inflows_24h_usd`
- `OnchainPanel.realized_volatility_30d`
- `IndicatorPanel.sma_50`
- `IndicatorPanel.sma_200`
- `IndicatorPanel.macd_hist`
- `Regime`

## Parameters

- `inflow_z`: 1.5 to 3.0 standard deviations above trailing baseline
- `volatility_floor`: realized vol above median
- `trend_confirmation`: SMA50 below SMA200 or MACD histogram negative
- `risk_cut`: 25% to 100% of normal exposure, depending on severity

## Decision rule

- If stablecoin inflows are elevated and the trend structure is weak, cut risk.
- If inflows are elevated but trend confirms upward continuation, do not short blindly - hold or scale only lightly.
- If inflows are normal and trend is clean, do nothing.

Pseudocode:

```text
if inflow_spike and weak_trend and high_volatility:
    de_risk_or_short_weakness
elif inflow_spike and strong_trend:
    hold
else:
    flat_or_normal_size
```

## Expected regime

- Transition markets where liquidity is moving before price does
- Late distribution phases
- Choppy markets where fresh capital does not immediately translate to clean upside

## Data dependencies

- Stablecoin inflow feed
- Historical inflow baseline for normalization
- Volatility and trend indicators already present in `MarketSnapshot`

## Status

idea

## References

- `crates/xvision-core/src/market.rs`
- `decisions/0010-hackathon-pivot-strategy-loom.md`
- `crates/xvision-eval/src/backtest.rs`
