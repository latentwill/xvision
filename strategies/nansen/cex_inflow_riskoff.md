# cex_inflow_riskoff

**Status:** queued
**Cohort × Signal:** CEX-Hot × Net-Inflow → ○ fade (exit / short)
**Cross-domain lift:** insider-deposit-before-sell (equities)

## Thesis

The mirror of `cex_outflow_accumulation`. Tokens flowing *into* CEX hot
wallets are pre-positioned for sale — wallets do not deposit to centralized
exchanges to hold long-term. Sustained inflow above a threshold indicates
distribution risk, especially when paired with a non-trivial price rally
(holders selling into strength).

Sign-inverted vs the smart-money / fund / stables rows: **inflow is bearish**.

## Inputs

- `OnchainPanel.nansen.cex.net_flow_token_24h` (positive = inflow).
- `OnchainPanel.nansen.cex.flow_pct_supply`.
- `PriceFrame.close` — for rally context.
- (Optional) `OnchainPanel.nansen.smart_money.net_flow_token_24h` — fade
  is highest-conviction when smart money is *also* distributing.

## Parameters

| Param                    | Default | Range           |
| ------------------------ | ------- | --------------- |
| `flow_window_hours`      | 24      | 4 – 72          |
| `min_inflow_pct_supply`  | 0.05%   | 0.01% – 0.3%    |
| `min_rally_pct_7d`       | 5%      | 0% – 20%        |
| `mode`                   | `exit`  | `exit` / `short` |

`min_rally_pct_7d` ensures the inflow is selling-into-strength, not
panic-deposit-during-crash (the latter often marks a low and would invert
the signal).

## Decision rule

```
if cex.net_flow_pct_supply(window=24h) >= min_inflow_pct_supply
   and price_change_7d >= min_rally_pct_7d:
       if in_long_position: exit long
       if mode == "short" and is_flat: enter short
```

## Expected regime

Distribution tops in trending markets. Underperforms during capitulation
events — distinguish via the `min_rally_pct_7d` gate.

## Data dependencies

Same as `cex_outflow_accumulation`.

## Status

`queued` — pair with `cex_outflow_accumulation` for symmetric long/short
coverage of CEX flow.

## References

- Pair: [`cex_outflow_accumulation`](cex_outflow_accumulation.md).
- Disambiguator: 7d rally gate — without it, the strategy reverses sign
  on capitulation flushes.
