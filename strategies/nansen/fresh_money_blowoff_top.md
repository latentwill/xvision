# fresh_money_blowoff_top

**Status:** queued
**Cohort × Signal:** FreshMoney × Net-Inflow → ○ fade (short / exit)
**Cross-domain lift:** retail-FOMO contrarian indicator (equities, IPO frenzy)

## Thesis

When wallets less than 30 days old, with low historical transaction
counts, suddenly buy a token in size, the cohort is retail latecomers
chasing a move. Historically — across both equities and crypto — the
late-arrival of unsophisticated capital marks local tops. The Nansen
"Fresh Wallet" / "Smart Money Suspect" labels make this cohort directly
queryable.

This is an explicitly *contrarian* strategy and runs against the
follow-the-cohort logic of most other entries in this folder. The
distinguishing assumption: this cohort's information is *worse than chance*
in aggregate, not just uninformative.

## Inputs

- `OnchainPanel.nansen.fresh_wallets.buy_count_24h` — number of fresh
  wallets that bought the token in the last 24h.
- `OnchainPanel.nansen.fresh_wallets.buy_volume_pct_24h` — fresh-wallet
  buy volume as a fraction of total volume in the window.
- `PriceFrame.close` — for confirmation that this is a rally context.
- `IndicatorPanel.rsi_14` — secondary overbought confirmation.

## Parameters

| Param                     | Default | Range           |
| ------------------------- | ------- | --------------- |
| `min_fresh_buy_volume_pct`| 15%     | 5% – 40%        |
| `min_rally_pct_3d`        | 10%     | 5% – 30%        |
| `min_rsi_14`              | 70      | 60 – 85         |
| `mode`                    | `exit`  | `exit` / `short` |
| `time_stop_hours`         | 48      | 12 – 168        |

A `time_stop_hours` is critical — fade-the-retail trades that don't
resolve within 48h often resolve *the wrong way* (the move goes parabolic
before reversing). Bound the loss.

## Decision rule

```
if fresh_wallets.buy_volume_pct_24h >= min_fresh_buy_volume_pct
   and price_change_3d >= min_rally_pct_3d
   and rsi_14 >= min_rsi_14:
       if in_long: exit long
       if mode == "short" and is_flat: enter short with time_stop_hours
```

## Expected regime

Late-stage rallies in retail-attention names. Fails badly on tokens with
strong reflexive narratives (e.g., meme coin parabolics) where
fresh-wallet inflow accelerates rather than reverts. Avoid on
network-launch days.

## Data dependencies

- Nansen `wallet_age` and `wallet_smart_score` labels — used to define
  "fresh" cohort.
- Mantle-specific wallet-age distribution may be skewed (newer chain) —
  parameters likely need recalibration vs Ethereum mainnet baselines.

## Status

`queued`. Distinct enough from the rest of the population to test as a
diversifier even if it doesn't dominate solo.

## References

- Inverse: smart-money / fund-follow strategies — this strategy fades the
  cohort that those follow.
- Equity analog: high-retail-flow names underperform after the flow peak
  (Barber & Odean, 2000).
