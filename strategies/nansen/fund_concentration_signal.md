# fund_concentration_signal

**Status:** queued
**Cohort × Signal:** Funds × Concentration-Δ → ● follow (directional)
**Cross-domain lift:** 13-F holdings-concentration delta (equity quant)

## Thesis

Net flow tells you *if* a cohort is buying or selling. *Concentration
change* tells you whether the cohort is consolidating conviction into
fewer names or diversifying across many. A fund cohort whose holdings'
Herfindahl-Hirschman index (HHI) increases over a token while their
total-portfolio size is flat is making a *relative* conviction bet — they
are choosing this token over alternatives.

The signal is the second-order analogue of fund-following: it's not "are
funds buying" but "are funds *concentrating into* this name." Equity quant
funds use this on 13-F data; the same shape lifts directly to on-chain
fund-labeled wallets.

This entry justifies the `Concentration-Δ` column existing — without
strategies like this, the column collapses into a noisier version of
`Net-Inflow`.

## Inputs

- `OnchainPanel.nansen.funds.holdings_pct_per_token` — fund cohort's
  position size in token T as a fraction of their total portfolio value.
- `OnchainPanel.nansen.funds.holdings_pct_per_token.delta_7d` — 7-day
  change in that fraction.
- `OnchainPanel.nansen.funds.portfolio_value_total` — for normalisation;
  used to confirm portfolio is not just shrinking.

## Parameters

| Param                       | Default | Range           |
| --------------------------- | ------- | --------------- |
| `concentration_window_days` | 7       | 3 – 30          |
| `min_concentration_delta`   | +1.0%   | +0.2% – +5.0%   |
| `portfolio_stability_band`  | ±5%     | ±2% – ±15%      |
| `min_fund_count`            | 3       | 2 – 10          |

`portfolio_stability_band` ensures we're seeing relative re-allocation,
not portfolio-wide growth that mechanically increases per-name exposure.

## Decision rule

```
if funds.holdings_pct_per_token.delta_7d >= min_concentration_delta
   and abs(funds.portfolio_value.delta_7d) <= portfolio_stability_band
   and funds.fund_count_with_position >= min_fund_count:
       enter long
elif in_long and funds.holdings_pct_per_token.delta_7d <= 0:
       exit long
```

## Expected regime

Mid-trend confirmation. Captures the moment a name moves from "in the
universe" to "high conviction" within the cohort's portfolio. Slower
than net-flow signals (7-day window) so misses fast moves.

## Data dependencies

- Nansen `funds` labels with full per-wallet portfolio composition (not
  just per-token flow).
- Token universe coverage on Mantle — concentration math requires
  observable totals; missing tokens distort the HHI.

## Status

`queued`. Differentiates the strategy population — 13-F-style
concentration signals are absent from most onchain baselines and are a
credible "sophisticated quant" angle for the Strategy Marketplace
narrative (ADR 0010).

## References

- Equity 13-F concentration strategies (Cohen, Polk & Silli, 2010 —
  "Best Ideas").
- Pair: `fund_follow` (idea pool) — first-order net-inflow version.
