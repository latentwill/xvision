# Liquidation Cascade Reversal

## Thesis

Big liquidation spikes often mark emotional exhaustion. After the flush, price frequently mean-reverts if the move was too fast relative to the recent range.

## Inputs

- `IndicatorPanel.atr_14`
- `IndicatorPanel.donchian_upper`
- `IndicatorPanel.donchian_lower`
- `OnchainPanel.liquidations_24h_usd`
- `MarketSnapshot.recent_bars`
- `Regime`

## Parameters

- `liquidation_spike_mult`: 2x to 4x above 20-day baseline
- `atr_expand_mult`: 1.2x to 1.8x relative to trailing ATR median
- `range_reentry_buffer`: 0.1x to 0.3x ATR
- `lookback_bars`: 20 to 50

## Decision rule

- Detect a liquidation spike relative to the recent baseline.
- Check whether the latest close re-enters the prior range after the flush.
- If price closes back inside the Donchian band and the move was a cascade, buy the bounce after downside flushes or sell the fade after upside squeezes.
- If the close remains outside the band with accelerating volatility, stay out until price stabilizes.

Pseudocode:

```text
if liquidation_spike and close_reentered_range and volatility_spike:
    fade_the_flush
elif liquidation_spike and range_not_recovered:
    flat
```

## Expected regime

- High-volatility selloffs and short squeezes
- Markets with strong liquidation data and thin order books
- Post-news whipsaws that overextend beyond the local range

## Data dependencies

- Liquidation feed
- OHLCV history
- Donchian and ATR indicators computed upstream

## Status

idea

## References

- `crates/xvision-core/src/market.rs`
- `crates/xvision-eval/src/baselines/macd_momentum.rs`
- `FOLLOWUPS.md`
