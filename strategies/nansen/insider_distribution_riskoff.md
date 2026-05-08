# insider_distribution_riskoff

**Status:** queued
**Cohort × Signal:** Insiders × Net-Outflow → ○ fade (exit / short)
**Cross-domain lift:** equity insider Form 4 sales

## Thesis

Token deployers, team-allocated wallets, and early-investor wallets have
the strongest information set on a token — they hold the project's
private context, vesting schedules, and forthcoming announcements. When
this cohort distributes, especially when it does so off a vesting cliff
or ahead of a perceived narrative peak, the information asymmetry is
maximal.

The asymmetry of this strategy: insider *buying* is rare and weakly
informative (insiders are usually already long); insider *selling* is the
trade. This is the only insider-row cell with a clear edge — hence the
"Insiders is the sparsest row" observation in the matrix.

## Inputs

- `OnchainPanel.nansen.insiders.net_flow_token_24h` — net outflow from
  deployer / team / early-holder labeled wallets.
- `OnchainPanel.nansen.insiders.distribution_destinations` — where the
  outflow is going. Outflow → CEX is more bearish than outflow → DeFi
  (latter may be staking / LPing, not selling).
- `PriceFrame.close`.

## Parameters

| Param                     | Default | Range           |
| ------------------------- | ------- | --------------- |
| `min_outflow_pct_supply`  | 0.1%    | 0.02% – 1.0%    |
| `cex_destination_weight`  | 1.0     | 0.5 – 2.0       |
| `defi_destination_weight` | 0.3     | 0.0 – 1.0       |
| `mode`                    | `exit`  | `exit` / `short` |

The destination weights de-emphasise outflows that may be operational
(staking, LP migration) vs outflows that are clearly distribution.

## Decision rule

```
weighted_outflow = (
    cex_destination_weight * insiders.outflow_to_cex_pct
  + defi_destination_weight * insiders.outflow_to_defi_pct
)

if weighted_outflow >= min_outflow_pct_supply:
       if in_long: exit long
       if mode == "short" and is_flat: enter short
```

## Expected regime

Pre-catalyst distribution windows (token unlocks, narrative peaks,
earnings/airdrop equivalents). Less reliable on long-tail tokens where
insider activity is sparse and individual transactions are noisy.

## Data dependencies

- Nansen insider/deployer labels — accuracy varies by token; check
  coverage per asset before relying on this signal.
- Token vesting / unlock schedule — for context (an unlock-day outflow
  isn't news; an off-schedule outflow is).

## Status

`queued`. Most useful as a *risk-off filter* layered onto other strategies'
long signals, rather than a standalone entry — strong insider distribution
should veto long entries from any cohort.

## References

- Equity analog: Form 4 insider-sale clusters (Lakonishok & Lee, 2001).
- Filter use: layer onto `smart_money_accumulation` etc. — insider
  distribution should override smart-money accumulation longs.
