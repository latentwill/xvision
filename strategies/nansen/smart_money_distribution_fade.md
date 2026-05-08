# smart_money_distribution_fade

**Status:** queued
**Cohort × Signal:** SmartMoney × Net-Outflow → ○ fade (exit / short)
**Cross-domain lift:** insider-selling exit-rule (equities)

## Thesis

The natural pair of `smart_money_accumulation`. When the same cohort whose
inflow signals conviction long *exits* a token, the information that drove
their accumulation has either been consumed by realized PnL or has changed
sign. Distribution by smart money is the cleanest exit signal available
on-chain. Fading the cohort directly (open shorts) is a stronger claim than
just exiting; the strategy supports both modes.

## Inputs

- `OnchainPanel.nansen.smart_money.net_flow_token_24h` — same series as
  the accumulation strategy, evaluated for net-negative.
- `OnchainPanel.nansen.smart_money.realized_pnl_token` — has the cohort
  taken profit, or are they cutting? PnL-positive distribution is healthy
  rotation; PnL-negative distribution is panic — both bearish, but the
  short-side conviction is higher in the panic case.
- `PriceFrame.close`.

## Parameters

| Param                    | Default | Range           |
| ------------------------ | ------- | --------------- |
| `flow_window_hours`      | 24      | 4 – 72          |
| `max_net_flow_pct`       | -0.5%   | -2.0% – -0.1%   |
| `min_active_wallets`     | 5       | 3 – 20          |
| `mode`                   | `exit`  | `exit` / `short` |
| `short_stop_atr_mult`    | 2.5     | 1.5 – 4.0       |

## Decision rule

```
if in_long_position and smart_money.net_flow_pct(window=24h) <= max_net_flow_pct:
       exit long
       if mode == "short" and smart_money.active_wallets >= min_active_wallets:
           enter short, size = risk_pct / atr_stop_distance
elif is_flat and mode == "short" and smart_money.net_flow_pct(window=24h) <= max_net_flow_pct:
       enter short (cohort distributing into a flat book)
```

## Expected regime

Late-trend or distribution tops. The fade-side underperforms in markets
where smart money rotates token-to-token without a broader risk-off
(distribution from one name into another); pair with `sm_rotation_chase`
to disambiguate.

## Data dependencies

Same as `smart_money_accumulation` plus `realized_pnl_token` field from
Nansen Smart Money endpoint.

## Status

`queued` — implement alongside `smart_money_accumulation` so the long/short
pair is symmetric in the evaluation harness.

## References

- Pair: [`smart_money_accumulation`](smart_money_accumulation.md).
- Disambiguator: `sm_rotation_chase` (idea pool) — distinguishes
  rotation-distribution from regime-change-distribution.
