# Fibonacci Liquidity Retracement Filter

## Thesis

Not every retracement is healthy. This strategy uses market liquidity context to decide whether a Fibonacci pullback is just a pause in a trend or the start of a liquidation cascade.

## Inputs

- `OnchainPanel.funding_rate_8h`
- `OnchainPanel.open_interest_usd`
- `OnchainPanel.long_short_ratio`
- `OnchainPanel.liquidations_24h_usd`
- `OnchainPanel.stablecoin_inflows_24h_usd`
- `IndicatorPanel.ema_12`
- `IndicatorPanel.ema_26`
- `IndicatorPanel.rsi_14`
- `Regime`

## Parameters

- `retracement_382`: healthy pullback zone if liquidity is supportive
- `retracement_618`: deeper retracement that needs extra caution
- `liquidation_stress_max`: cap for tolerated liquidation pressure
- `oi_spike_limit`: open interest spike threshold
- `stablecoin_support_min`: minimum inflow support for long entries
- `crowding_limit`: funding and long-short crowding ceiling

## Decision rule

- If price pulls back into a Fibonacci zone but liquidations are accelerating and open interest is fragile, stay flat.
- If price retraces into 0.382 to 0.618 while stablecoin inflows are rising and funding is not extreme, consider the pullback constructive.
- If retracement happens on heavy liquidation stress, wait for the flush to finish or look for a reversal only after support returns.
- Use the liquidity panel as a filter, not as a standalone trigger.

Pseudocode:

```text
if fib_pullback and liquidity_healthy and trend_intact:
    buy_pullback
elif fib_pullback and liquidation_stress_high:
    wait
else:
    flat
```

## Expected regime

- Markets where onchain positioning matters as much as chart structure
- Trend pullbacks with noisy leverage dynamics
- Assets prone to liquidation-driven overshoots

## Data dependencies

- OHLCV bars and indicator panel values
- Onchain liquidity and liquidation feeds
- No extra model inputs required

## Status

idea

## References

- `crates/xvision-core/src/market.rs`
- `decisions/strategy-choices.md`
- `strategies/x strategy/fibonacci_extension_breakout.md`
