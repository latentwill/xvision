# smart_money_accumulation

**Status:** queued
**Cohort × Signal:** SmartMoney × Net-Inflow → ● follow
**Cross-domain lift:** 13-F snapshot delta + sharp-money follow

## Thesis

Nansen's "Smart Money" label tags wallets with high realized PnL across
historical trades. When this cohort, in aggregate, increases its net token
balance over a rolling window, the cohort is taking conviction long. Their
realized edge implies their information advantage is non-trivial; following
their accumulation, with a lag short enough to capture residual move, is
the canonical Nansen-on-DEX play.

## Inputs

- `OnchainPanel.nansen.smart_money.net_flow_token_24h` — net token-units
  accumulated by smart-money wallets over the last 24h.
- `OnchainPanel.nansen.smart_money.wallet_count_active` — number of distinct
  smart-money wallets that traded the token in the window. Used as a
  breadth filter — single-wallet "smart money" inflow is noisier.
- `PriceFrame.close` — for entry/stop placement.
- (Optional confirmation) `IndicatorPanel.rsi_14`, `IndicatorPanel.macd`.

## Parameters

| Param                  | Default | Range            |
| ---------------------- | ------- | ---------------- |
| `flow_window_hours`    | 24      | 4 – 72           |
| `min_net_flow_pct`     | 0.5%    | 0.1% – 2.0%      |
| `min_active_wallets`   | 5       | 3 – 20           |
| `entry_lag_hours`      | 1       | 0 – 6            |
| `stop_atr_multiple`    | 2.0     | 1.0 – 4.0        |
| `take_profit_rr`       | 2.0     | 1.0 – 4.0        |

## Decision rule

```
if smart_money.net_flow_pct(window=24h) >= min_net_flow_pct
   and smart_money.active_wallets >= min_active_wallets
   and current_position.is_flat:
       enter long, size = risk_pct / atr_stop_distance
else if in_long_position
   and smart_money.net_flow_pct(window=12h) <= 0:
       exit long (cohort no longer accumulating)
```

## Expected regime

Trending or early-trend markets where information asymmetry is large and
smart-money wallets have time to act before retail. Underperforms in
mean-reverting chop, where the cohort's accumulation can lead and reverse
within the entry-lag window.

## Data dependencies

- Nansen API key — `smart_money` endpoint, token-level net flow.
- Mantle DEX trade-flow attribution per wallet (Nansen covers Mantle since
  2024).

## Status

`queued` — implementation goes in `crates/xianvec-eval/src/baselines/onchain/`
per FOLLOWUPS SLF6 / F14. This is the canonical Nansen baseline; ship first.

## References

- `decisions/0010-hackathon-pivot-strategy-loom.md` — strategy loom seed-population context.
- `FOLLOWUPS.md` SLF6 — onchain baseline queue.
- 13-F follow strategies (equities) — same lifted shape, quarterly cadence.
