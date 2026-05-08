# cex_outflow_accumulation

**Status:** queued
**Cohort × Signal:** CEX-Hot × Net-Outflow → ● follow (long bias)
**Cross-domain lift:** float-supply withdrawal (equities — short-availability)

## Thesis

Tokens flowing *out* of centralized-exchange hot wallets reduce the
sellable float. Holders moving tokens to self-custody or DeFi venues are
revealing a holding-period preference inconsistent with imminent selling.
This is one of the oldest on-chain signals and remains robust because it
is structural, not behavioural — fewer tokens on exchange means less
sell-side liquidity, which mechanically biases price up at constant demand.

Note the **sign inversion** for CEX-Hot cohort: outflow = bullish, inflow =
bearish. Any ensemble that mixes CEX-Hot signals with smart-money flows
must respect this.

## Inputs

- `OnchainPanel.nansen.cex.net_flow_token_24h` — net token-units leaving
  (negative) or entering (positive) all CEX-labeled hot wallets.
- `OnchainPanel.nansen.cex.flow_pct_supply` — flow as a fraction of
  circulating supply; normalises across token sizes.
- `PriceFrame.close`.

## Parameters

| Param                    | Default | Range           |
| ------------------------ | ------- | --------------- |
| `flow_window_hours`      | 24      | 4 – 72          |
| `min_outflow_pct_supply` | 0.05%   | 0.01% – 0.3%    |
| `entry_lag_hours`        | 2       | 0 – 12          |
| `stop_atr_multiple`      | 2.0     | 1.0 – 4.0       |
| `min_persistence_days`   | 2       | 1 – 7           |

`min_persistence_days` filters one-off withdrawals from sustained
withdrawal trends — the latter is the signal worth trading.

## Decision rule

```
if cex.net_flow_pct_supply(window=24h) <= -min_outflow_pct_supply
   for at least min_persistence_days consecutive days
   and current_position.is_flat:
       enter long
elif in_long and cex.net_flow_pct_supply(window=24h) >= 0:
       exit long
```

## Expected regime

Works best in mid-cycle accumulation phases. Fails in supply-shock
distribution events (e.g., unlock cliffs) where float reduction is
artifact of token mechanics, not holder intent — pair with an
unlock-schedule filter.

## Data dependencies

- Nansen `cex_flows` endpoint with hot-wallet labels current for major
  exchanges (Binance, OKX, Bybit, Coinbase, Kraken).
- Token unlock-schedule data (CryptoRank / TokenUnlocks) — for filter.

## Status

`queued`. Often the most reliable single Nansen signal for swing horizons;
ship in the first wave with `smart_money_accumulation`.

## References

- `FOLLOWUPS.md` SLF6 — onchain baseline queue.
- Pair: [`cex_inflow_riskoff`](cex_inflow_riskoff.md).
