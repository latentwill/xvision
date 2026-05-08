# stablecoin_inflow_riskon

**Status:** queued
**Cohort × Signal:** Stables × Net-Inflow → ● follow (long bias)
**Cross-domain lift:** dry-powder-on-sidelines (equities, M2 / money-market flows)

## Thesis

Stablecoin balances flowing *into* DEX pools (or into wallets known to
trade DEXes) are buy-side ammunition. The lifted form is the same as
"investors moving cash from money-market funds into equities" — the cash
is being de-risked into a position-building mode. Unlike inflows of token
positions (which may be neutral rotation), stablecoin inflows to DEX
infrastructure are unambiguously risk-on.

On Mantle specifically, this signal is high-leverage because stablecoin
liquidity on Merchant Moe / Agni / Fluxion is small enough that
significant inflows are detectable as a fraction of TVL.

## Inputs

- `OnchainPanel.nansen.stables.net_flow_to_dex_24h` — net stablecoin
  units (USDC / USDT / DAI / equivalents) flowing into Mantle DEX pools.
- `OnchainPanel.nansen.stables.flow_pct_dex_tvl` — flow as fraction of
  current DEX stablecoin TVL.
- (Optional) per-token stable inflow — flowing into specific token's pools
  is a directional ammo signal (`stbl_token_rotation` in idea pool).

## Parameters

| Param                     | Default | Range          |
| ------------------------- | ------- | -------------- |
| `flow_window_hours`       | 24      | 4 – 72         |
| `min_inflow_pct_tvl`      | 1.0%    | 0.2% – 5.0%    |
| `bias_strength`           | 0.5     | 0.0 – 1.0      |
| `apply_to`                | `BTC`   | per token / index |

`bias_strength` is a portfolio-level tilt — this strategy is most
naturally a *bias modifier* on other strategies' decisions rather than a
standalone entry trigger.

## Decision rule

```
risk_on_score = stables.net_flow_pct_tvl(window=24h) / min_inflow_pct_tvl

if risk_on_score >= 1.0:
    apply long bias to all directional decisions, scaled by bias_strength
    e.g. multiply long-side position size by (1 + bias_strength * risk_on_score)
         multiply short-side position size by (1 - bias_strength * risk_on_score)
```

## Expected regime

A regime-shift signal. Most useful on swing-to-trend horizons (1d–1w).
Underperforms intraday — stablecoin flows don't transmit to price
within hours; they set the *floor* for buy pressure over days.

## Data dependencies

- Nansen `stablecoin_flows` endpoint with Mantle DEX pool addresses
  whitelisted (Merchant Moe / Agni / Fluxion).
- Mantle DEX TVL (DefiLlama) for normalisation.

## Status

`queued` — implements the regime-tilt slot in the strategy ensemble.
Best evaluated as a modifier (axis 6 — sizing) rather than a standalone
entry rule.

## References

- Pair: `stbl_riskoff` (idea pool) — outflow inverse.
- Related: `stbl_token_rotation` (idea pool) — per-token directional version.
