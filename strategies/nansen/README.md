# Nansen-driven strategies

Compendium of trading strategies that use Nansen on-chain analytics as the
primary signal source. Mantle-native execution; signals consumed via
`OnchainPanel` (FOLLOWUPS SLF6 / F14).

Generated via the `ideonomy-rich` skill — operators applied:
**abstraction-lift** + **dimension-identification**, organon: **matrix**,
dimension-prompts: **size · source · modularity**.

## The lift

Surface form: "trade based on what Nansen-labeled wallets are doing on
Mantle DEXes."

Lifted form: **observe a distinguished cohort in a transparent system; take
a position conditional on the cohort's behaviour.**

Cross-domain instances of the lifted form (each gives a Nansen template):

| Domain                       | Template                                              |
| ---------------------------- | ----------------------------------------------------- |
| Equity 13-F filings          | quarterly-rebalance "snapshot delta"                  |
| Sharp-money sportsbooks      | late-entry "last-minute conviction" follow            |
| Cohort-weighted polling      | track-record-weighted ensemble aggregator             |
| Epidemic-network spread      | high-centrality first-mover follow                    |
| Donor-tracking (politics)    | concentration-flip / regime-change detection          |
| Whaling-fleet sightings      | cohort density/dispersion as environmental signal     |

## Dimensions of the strategy space (6 axes)

1. **Cohort** — which Nansen-labeled group? (rows of the matrix)
2. **Signal** — what aspect of cohort behaviour? (cols of the matrix)
3. **Direction** — follow (●), fade (○), filter-only (◇)
4. **Horizon** — flash 1h · intraday 4h · swing 1d · trend 1w
5. **Confirmation** — standalone · OR'd-with-TA · AND'd-with-TA · AND'd-with-funding
6. **Sizing** — equal-weight · cohort-PnL-weighted · Kelly-on-cohort-edge

The matrix below populates axes 1+2. Axes 3–6 are independent multipliers —
every named cell spawns ~16 variants once crossed with horizon ×
confirmation × sizing.

**Pivot dim — `source`:** Nansen's defining value is that every signal
carries a *labeled* source. The label IS the alpha. Strip Nansen of its
labels and you have raw ETL of a public chain. Every strategy below
depends on a label being trustworthy and stably-defined; if Nansen's
labelling pipeline drifts, every strategy in this folder drifts with it.

## Cohort × Signal matrix

```
+---------------+----------------+----------------+----------------+----------------+
| COHORT \ SIG  |  Net-Inflow    |  Net-Outflow   |  Concentr-Δ    |  New-Entrants  |
+---------------+----------------+----------------+----------------+----------------+
| SmartMoney    | ● sm_accum     | ○ sm_dist_fade | ● sm_convict   | ● sm_late_entry|
| Funds         | ● fund_follow  | ○ fund_riskoff | ● fund_13f     |       ◇        |
| Insiders      |       ◇        | ○ insid_riskoff|       ◇        |       ◇        |
| CEX-Hot       | ○ cex_in_off   | ● cex_out_acc  |       ◇        |       ◇        |
| FreshMoney    | ○ fresh_blowoff|       ◇        |       ◇        | ○ fresh_spike  |
| Stables       | ● stbl_riskon  | ○ stbl_riskoff |       ◇        |       ◇        |
+---------------+----------------+----------------+----------------+----------------+

  ●  follow signal             ○  fade signal             ◇  open coinage slot
```

Read the matrix:

- **Row-sums** — `SmartMoney` is the densest row (4/4); `Insiders` is the
  sparsest (1/4) — the only useful insider signal is *exit*.
- **Column-sums** — `Net-Inflow` and `Net-Outflow` are the densest columns;
  `Concentration-Δ` and `New-Entrants` are under-explored.
- **Asymmetry** — the `CEX-Hot` row inverts the usual sign: inflow = bearish
  (tokens moving onto exchange = sell pressure), outflow = bullish
  (tokens leaving exchange = accumulation). Any ensemble that mixes cohorts
  must apply per-cohort sign conventions.

## Cross-token rotation (5th signal column, broken out)

- `sm_rotation_chase` — SmartMoney exiting token A while entering token B → follow rotation.
- `fund_sector_rotation` — Funds rotating across sectors (DeFi → AI etc.) → sector tilt.
- `cex_rotation_signal` — CEX-hot wallets shifting balances between tokens → leading distribution.
- `stbl_token_rotation` — Stables flowing pool-A → pool-B → directional ammo signal.

## Meta-strategies (lifted from cross-domain templates)

- `cohort_weighted_ensemble` — weighted avg of all cohort signals; weights = trailing PnL.
  *(lift from cohort-weighted polling.)*
- `first_mover_centrality` — enter when a high-network-centrality wallet enters.
  *(lift from epidemic-network spread.)*
- `mev_avoidance_filter` — filter-only; heavy MEV-bot activity → reduce size or skip.
  *(lift from predator-presence-reduces-forager-activity.)*

## Index

### Queued (full file scoped)

- [`smart_money_accumulation`](smart_money_accumulation.md) — SmartMoney net-inflow → long bias.
- [`smart_money_distribution_fade`](smart_money_distribution_fade.md) — SmartMoney net-outflow → exit / short bias.
- [`cex_outflow_accumulation`](cex_outflow_accumulation.md) — Tokens leaving CEX hot wallets → long bias.
- [`cex_inflow_riskoff`](cex_inflow_riskoff.md) — Tokens flowing onto CEX hot wallets → risk-off.
- [`stablecoin_inflow_riskon`](stablecoin_inflow_riskon.md) — Stables moving into DEX pools → risk-on.
- [`fresh_money_blowoff_top`](fresh_money_blowoff_top.md) — Spike in fresh-wallet buys → fade top.
- [`insider_distribution_riskoff`](insider_distribution_riskoff.md) — Token deployers/insiders distributing → exit.
- [`fund_concentration_signal`](fund_concentration_signal.md) — Funds increasing position concentration → directional confirm.

### Idea pool (matrix-named, not yet scoped)

- `sm_convict` — SmartMoney concentration increasing on a token → conviction breakout follow.
- `sm_late_entry` — Last-hour SmartMoney entry into a token → short-horizon momentum follow.
- `sm_rotation_chase` — SmartMoney rotating from A → B → follow rotation.
- `fund_follow` — Fund-labeled wallet net-inflow → directional follow.
- `fund_riskoff` — Fund-labeled wallet net-outflow → risk-off.
- `fund_13f` — Funds rebalancing concentration (13-F-style snapshot delta) → directional tilt.
- `fund_sector_rotation` — Funds rotating between token sectors → sector tilt.
- `cex_rotation_signal` — CEX hot wallets shifting between tokens → leading distribution.
- `fresh_spike` — Spike in new-wallet count for a token → fade.
- `stbl_riskoff` — Stables leaving DEX pools → risk-off.
- `stbl_token_rotation` — Stables rotating pool-A → pool-B → directional ammo.
- `cohort_weighted_ensemble` — meta; PnL-weighted ensemble of cohort signals.
- `first_mover_centrality` — enter when high-centrality wallet enters.
- `mev_avoidance_filter` — filter; heavy MEV → reduce size or skip.

### Open coinage (◇ slots, no candidate yet)

- Insiders × Net-Inflow — insiders rarely net-buy; under what circumstances would they?
- Insiders × Concentration-Δ — insiders re-concentrating after distribution: re-entry signal?
- Insiders × New-Entrants — new insider-labeled wallets appearing on a token: pre-launch leak?
- Funds × New-Entrants — new fund-labeled wallets in a name; how is that distinct from inflow?
- CEX-Hot × Concentration-Δ — CEX hot-wallet concentration shift across exchanges; cross-venue arb?
- CEX-Hot × New-Entrants — new CEX-labeled wallets receiving a token; pre-listing signal?
- FreshMoney × Net-Outflow — fresh wallets exiting; capitulation by retail latecomers?
- FreshMoney × Concentration-Δ — fresh-wallet HHI shift; mass adoption vs whale-disguise?
- Stables × Concentration-Δ — stablecoin-issuer concentration; depeg / mint-burn signal?
- Stables × New-Entrants — new stablecoin-labeled wallets; new issuance / new market entrant?

## Not surfaced (worth a follow-up ideonomy pass)

These directions came up in the trail but aren't in the matrix:

1. **Adversarial labels** — what if Nansen labels are *gamed*? Whales
   spoofing smart-money labels to induce follows. Dimension `source` with
   value `accident/adversarial` wasn't crossed in this pass.
2. **Time-decay of signal** — does smart-money-signal half-life differ
   across horizons? Axis 4 (`horizon`) wasn't matrixed.
3. **Meta-label strategies** — Nansen's label *changing* as a signal
   (wallet promoted to smart-money, demoted from fund). Second-order signal
   not in current matrix.

Suggested next tuple: `substitution + tree-finding + matrix` over
{cohort × time-horizon} or {cohort × label-stability}.

## See also

- [`../README.md`](../README.md) — parent strategies compendium and per-strategy file format.
- `decisions/0010-hackathon-pivot-strategy-loom.md` — Strategy Loom + ERC-8004 marketplace context.
- `FOLLOWUPS.md` SLF6 / F14 — onchain baseline queue (Nansen smart-money, funding-rate, stables, liquidations).
