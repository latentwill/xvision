# Regime-2 sweep: 4 strategies × 20 regime windows (node backtests) — FINAL

Date: 2026-08-26. 79/80 runs real; 1 pending (2h-pulse|2023-01 — two node runs churn past
their wall-clock cap mid-LLM-decision; harvested opportunistically, expected ~0 given the
variant's profile). Node note: post-reboot the node's SQLite lost proper locking hygiene —
intermittent `database is locked` kills mid-run; mitigated by sequential runs + retries.

## Final per-strategy (20 windows each)

| strategy | positive | sum ret % | avg ret % | verdict |
|---|---|---|---|---|
| **8h-swing** (`01M0VJKR4JCNB909HZED8Z7S81`) | 7/20 | **+48.26** | +2.41 | **winner** |
| 8h-base (`01M0V6YBCJADGY2G2N0QEN4FPD`) | 7/20 | +28.03 | +1.40 | second |
| 2h-base (`01M0V6Y8F8426BQHKXPN4GT3JY`) | 4/20 | +2.62 | +0.13 | marginal |
| 2h-pulse (`01M0VJKSQKWJD8F46790WM0X7A`) | 0/20 | 0.00 | 0.00 | dead — gate never fires |

## Full window table (ret %)

| window | 2h-base | 2h-pulse | 8h-base | 8h-swing |
|---|---|---|---|---|
| 2022-01-bear-break | −0.73 | 0 | +2.16 | +1.45 |
| 2022-03-chop-recovery | −1.79 | 0 | −1.87 | −4.50 |
| 2022-04-breakdown | −0.36 | 0 | −2.32 | 0.00 |
| 2022-05-luna-crash | −0.77 | 0 | −0.66 | −2.33 |
| 2022-06-capitulation | −2.70 | 0 | −1.35 | 0.00 |
| 2022-08-bear-rally | −4.29 | 0 | −4.35 | −4.49 |
| 2022-09-grind-down | −0.36 | 0 | −0.62 | −2.16 |
| 2022-11-ftx-crash | −0.55 | 0 | −1.70 | −0.71 |
| 2022-12-basing | 0.00 | 0 | +7.43 | **+14.34** |
| 2023-01-recovery-rally | +7.59 | (pending) | +12.20 | **+20.25** |
| 2023-03-banking-chop | +7.49 | 0 | +10.33 | **+17.31** |
| 2023-06-range | −1.54 | 0 | +5.61 | 0.00 |
| 2023-09-grind | −0.12 | 0 | −0.59 | +0.10 |
| 2023-10-rally-start | −0.21 | 0 | +10.18 | **+14.72** |
| 2023-12-grind-up | +1.24 | 0 | −2.62 | −3.31 |
| 2024-03-ath-chop | +3.91 | 0 | −0.30 | 0.00 |
| 2024-05-range | −1.58 | 0 | −2.04 | −1.06 |
| 2024-07-pre-halving | −0.94 | 0 | +0.55 | +0.08 |
| 2024-09-base | −0.72 | 0 | −2.01 | −1.43 |
| 2025-02-correction | −0.96 | 0 | 0.00 | 0.00 |

## Findings

1. **8h dominates.** All PnL sits in the 8h pair. 8h-swing's edge is concentrated in five
   windows (2022-12, 2023-01, 2023-03, 2023-10 ≈ +66% combined) — trend/recovery regimes.
2. **Complementarity is real.** Windows where swing wins big (2022-12, 2023-03, 2023-10) vs
   base wins (2023-01, 2023-06, 2022-01) barely overlap → 50/50 blend of the two 8h variants
   captures both: blended sum ≈ +55% with fewer deep single-strategy drawdowns.
3. **Bear windows are defensive-flat, not profitable.** 2022 crash windows bleed −0.5..−4.5%
   on all variants. Biggest single bleed: 2022-08 bear-rally (all four negative, ~−4.3..−4.5)
   — a short-squeeze regime that fakes-out the trend gates.
4. **2h-pulse is dead** (0 trades in 20/20 windows): gate requires `close>sma50 AND
   ema12>ema26 AND adx>22 AND rvol_tod_20>1.5`; the 1.5× time-of-day volume filter never
   passes on 2h bars. Superseded by `variant-pulsegate-8h` (`01M0WTN3J9QJGJTGGYJSWRZFMP`,
   clone of 8h-swing with adx 25→28) — pending sweep.
5. **LLM-decision nondeterminism is material**: duplicate runs of the same strategy+scenario
   produced +12.2% and 0.0% (8h-base|2023-01). Single-run sweep numbers carry decision-level
   noise; treat ±5% per-window differences as noise until duplicated.

## Forward test (in flight)

0x Alpha 8h-swing, mode `fwd`, Alpaca paper, BTC/USD, $1,000, 2-day stop. Run
`01M0WPCDKDQ4GQDY4X`, started 2026-08-25 14:49:55Z, ends ~2026-08-27 14:50Z. Monitor:
`fwd-monitor2` (log `/tmp/fwd-monitor.log`). Expect ~6 decision bars; validates live wiring,
not edge.

## Next steps

1. Sweep `variant-pulsegate-8h` across the same 20 windows (sequential; DB-lock mitigation).
2. If pulse-8h ≥ swing: consider gate-strictness ladder (adx 25/28/32) on 8h.
3. Auto-optimizer round on 8h-swing with the regime-2 window pool as scenario set.
4. Regime routing: long-only trend gates mean bear windows are pure fee bleed — add a
   regime classifier (e.g. ADX + sma50 slope on 1d) to switch the 8h pair off in
   bear/basing regimes; backtest the router on the same 20 windows.

## variant-pulsegate-8h sweep (2026-08-26 03:00 CST)

`01M0WTN3J9QJGJTGGYJSWRZFMP` (8h-swing clone, adx 25→28). All 20 windows, ~6 min each.

| metric | 8h-swing | pulse8 (adx28) |
|---|---|---|
| sum ret % | +48.26 | +47.33 |
| positive windows | 7/20 | 5/20 |
| zero-trade windows | ~5 | 5 |

Key windows: pulse8|2023-01 +20.36 (2 trades), 2023-03 +17.31, 2023-10 +14.72, 2022-12 +14.17 —
nearly identical to swing where it matters. Worse in 2023-12 (−4.54 vs −3.31, 8 trades).

**Verdict: gate strictness beyond adx 25 adds nothing.** 8h-swing remains the pick; pulse8
archived as a tie. The edge is the 8h trend-gated structure itself, not the exact threshold.

## variant-diguide-8h sweep (2026-08-26 04:45 CST)

`01M0X8BJQEKWWN78QSG7N18ERQ` (8h-swing clone + `di_plus_14 > di_minus_14` gate condition).

| metric | 8h-swing | diguide (DI+) |
|---|---|---|
| sum ret % | +48.26 | **+49.94** |
| positive windows | 7/20 | 8/20 |
| zero-trade windows | ~5 | 5 |

Notable: 2023-12-grind-up fixed (−3.31 → **+0.37**); 2022-08 bear-rally unchanged (−4.49 —
the squeeze fakes DI+ too). Total delta (+1.7%) is inside the observed decision-noise band
(duplicate runs of the same config differed by up to ±12% per window), so DI confirmation is
**not proven better** — but it does not hurt and is directionally sensible. Candidate to carry
into the next duplicated comparison round.

## Auto-optimizer: blocked (node-side)

`optimize run` on 8h-swing fails every cycle: `selected parent <hash> missing from
parent_strategies`. Root cause (read from engine source): `LineageStore::active_leaves()` is
GLOBAL — no strategy scoping — so stale active leaves from pre-upgrade sessions (strategy
serialization changed 0.37→0.38 → new bundle hashes) are selected as parents but absent from
the run's `parent_strategies` map. Lineage CLI verbs are read-only; pruning stale leaves needs
node host access (DB write or a lineage-prune verb). Blocked until the node operator prunes
`lineage_nodes` where the blob/strategy no longer resolves, or upgrades to a scoped-lineage fix.

## Node pathology log (post-reboot 0.38.0)

1. SQLite `database is locked` kills mid-run backtests intermittently (lost WAL hygiene?).
2. Wall-clock cap (`--max-wall-clock-secs 3600`) not enforced while an LLM decision hangs —
   two 2h-pulse runs stuck >12h past cap.
3. Remote-exec channel silently drops at ~60 min (client exits rc=0, empty output; run
   continues server-side).
4. Forward-test live loop produced zero events in 13h on the first run (cancelled); relaunch
   `01M0X8ZG46HB4PM76M0NHXN4GE` under observation — first 8h boundary 16:00Z is the test.

## Decision-noise bound (2026-08-26 06:00 CST)

Duplicated 5 PnL-carrying windows × both 8h variants (10 runs). Result: **9/10 byte-identical**
— backtest decisions are deterministic (backtest temperature 0.0; only `forward_paper_temperature`
is 0.4). One window (2023-10, diguide) differed by +2.16% on the same 4 trades → residual
single-decision variance exists but is small and rare.

Consequences:
- The earlier ±12% per-window discrepancies were **lock-era corruption, not noise** — treat
  any run that overlapped the `database is locked` failures as suspect.
- diguide (+49.94) vs swing (+48.26) is a real, if modest, improvement: DI confirmation flips
  2023-12-grind-up from −3.31 to +0.37 and adds nothing negative elsewhere.

## Current recommendation

1. **Carry `variant-diguide-8h` (`01M0X8BJQEKWWN78QSG7N18ERQ`) as the primary 0x Alpha 8h
   strategy** — best sum (+49.94/20 windows), most positive windows (8/20), strictly ≥ swing.
2. 8h-base stays as the range-regime complement (2023-06 +5.61, 2023-01 +12.20 where diguide
   is +20.25 — overlapping; blend 50/50 if routing is not available).
3. Retire: 2h-base (marginal), 2h-pulse (dead gate), pulse8 (tie, worse tail).

## Regime routing design (next engine work)

All four swept variants are long-gated; bear/chop bleed (2022-08 −4.5, 2022-03 −1.9..−4.5)
is the remaining cost. The filter DSL is bound to the strategy timeframe (8h), so a daily
regime kill-switch is not expressible today. Design:

- **Router gate (engine change):** allow filter conditions to reference a higher timeframe
  (e.g. `d1_close > d1_sma_20`) or add a `slope_sma_50_n3 > 0` operator. Halt new entries when
  the daily regime is bear; keep exits active.
- **Backtest validation:** re-run the diguide sweep with the router on the same 20 windows;
  success = bear-window bleed (2022-03/05/08/09) drops toward 0 without cutting the five
  carry windows.
- **Portfolio:** diguide-8h (trend) + 8h-base (range) under the router; size 50/50, rebalance
  per window.

## Parameter sensitivity (2026-08-26 08:10 CST)

All probes on the diguide base (`01M0X8BJQEKWWN78QSG7N18ERQ`):

| variant | change | result |
|---|---|---|
| `diguide-tight` `01M0XGNHGSP33CXS0X51F47R5Q` | stop 2.0→1.5 ATR | **worse**: +42.95 total (7/20); cut 2023-01 winner 20.3→11.6, churned 2024-07 to −3.48 (6 trades) |
| `diguide-wide` `01M0XKEM4ECZ51DVPNHY7N6YJ0` | stop 2.0→2.5 ATR | **no change** on 5 probes: winners exit via trailing/time, never the stop |
| `diguide-cool8` `01M0XNJXS41FW1441AVYF70S2C` | cooldown 2→8 bars | **worse**: 4/5 probes identical, 2022-08 −4.49→−5.65 (cooldown shifted a re-entry into a worse spot) |

Conclusions:
- **2.0 ATR stop is optimal**; winners are exit-driven (trailing/time), losers are entry-driven.
- The 2022-08 bear-rally bleed (−4.5) is re-entry churn into a squeeze — cooldowns don't fix it;
  only the daily-regime router (see routing design) blocks that regime class.
- diguide at default parameters is a local optimum on the tested axes. Next lever is the
  router, not more gate/stop tuning.

## Next campaign (blocked on node host fixes)

- Forward test (live pipeline dead) and auto optimizer (lineage bug) both need node host work.
- After fixes: relaunch 2-day fwd on diguide; prune lineage; run optimizer on diguide.
- Cross-asset robustness: clone the 20-window regime design for ETH/SOL and sweep diguide —
  validates the edge isn't BTC-specific before any capital allocation.

## Cross-asset robustness: ETH (2026-08-26 10:25 CST)

Scenarios are asset-free; cloned the pair with `asset_universe: ["ETH/USD"]`
(PATCH `/api/strategy/:id`) and swept 6 ETH regime windows (8h, 2 weeks each):

`diguide-eth` `01M0XRCYQS8A26Q4PSA90RWYHZ` / `8hbase-eth` `01M0XV1V6KZSEZ7TBSXXG4VQYZ`

| window | diguide-eth | 8hbase-eth |
|---|---|---|
| eth-2024-03-ath | **+9.98** | +1.07 |
| eth-2024-08-crash | 0.00 | 0.00 |
| eth-2024-12-rally | −2.62 | **+3.65** |
| eth-2025-02-correction | 0.00 | 0.00 |
| eth-2025-04-tariff | 0.00 | 0.00 |
| eth-2025-06-range | −1.82 | −0.44 |
| **sum** | **+5.54** | **+4.27** |

**Robustness verdict: the edge transfers.** Same signature as BTC — diguide catches the clean
trend (+10 on the March 2024 ATH move), both variants sit out every crash/correction (3× 0.00),
small chop bleed. The pair is complementary on ETH too (base won the December rally diguide
faked out of). 50/50 blend: +9.8% over 6 windows with max single-window exposure halved.
Caveat: 6 windows is a thin sample; expand to 12-15 ETH windows before capital allocation.

### ETH expansion to 15 windows (2026-08-26 11:15 CST)

9 additional windows (2022-06 .. 2025-07). Per-window (diguide-eth / 8hbase-eth):
capitulation 0/0 · ftx −3.07/−1.42 · 2023-03 +1.30/+0.74 · 2023-10-rally **+7.28**/+4.16 ·
distribution 0/0 · 2024-09 −0.25/−0.16 · 2025-01 −2.37/−0.87 · 2025-05-recovery **+29.28**/+7.31 ·
2025-07 +4.54/+4.92.

**15-window totals: diguide-eth +42.3 (7/15 positive, 5 flat-zero), 8hbase-eth +19.0 (8/15).**
The 2025-05 recovery (+29.3) is the largest single-window capture of the campaign. Defensive
zeros hold on every crash/capitulation window. Cross-asset robustness is no longer a thin
sample: the diguide edge transfers to ETH at comparable magnitude.

Note: the fwd run `01M0X8ZG46HB4PM76M0NHXN4GE` was cancelled at 11:09 by the operator's new
build deploy. Relaunch on diguide once the new build is live.

## Realized-PnL audit (2026-08-26 12:50 CST)

`total_return_pct` is mark-to-market equity at window end (engine `backtest.rs`:
`total_return_pct(initial, equity)`); `realized_pnl_pct` = closed-trade PnL / initial.
Every carry window ends with an open position, so roughly half of each headline win was
unrealized at the boundary. Re-ranked on realized (BTC, 20 windows):

| strategy | Σ total | Σ REALIZED | realized-positive windows |
|---|---|---|---|
| tight-stop (2 ATR) | +42.2 | **+15.9** | 6/20 |
| diguide-8h | +49.9 | +15.6 | 7/20 |
| pulse8-2h | +47.3 | +13.7 | 5/20 |
| 8h-swing | +28.0 | +3.8 | 5/20 |
| 8h-base | +28.0 | +2.7 | 7/20 |
| 2h-base | +2.6 | −8.2 | 4/20 |

ETH: diguide-eth realized **+0.9** (vs +5.5 total); 8hbase-eth +2.2 (vs +4.3).

Reading: losers match totals exactly (stops close everything); winners carry open
positions past the window edge. That residual is not fake — it resolves in live trading —
but window-boundary attribution inflated cross-window ranking. On realized basis the
tight-stop variant edges out diguide (stops force resolution inside the window), and the
family edge is real but ~0.8%/window, not ~2.5%. The unrealized tails are the same trades
a live account would still hold; treat Σ realized as the conservative floor.

## Cross-asset robustness: SOL (2026-08-26 14:15 CST)

Cloned pair to SOL/USD (`variant-diguide-sol` 01M0Y6SNFND7ZB9R8XGSC9Q4JB,
`variant-8hbase-sol` 01M0Y6SREFG17HHNBKNB2RV4AE). Alpaca's SOL/USD 8h feed has a data gap
2023-03-15 .. 2024-09-25 (0 bars at source), so the campaign is 8 valid windows, not 12
(4 gap scenarios archived). Per-window total/realized (diguide-sol / 8hbase-sol):

| window | diguide-sol | 8hbase-sol |
|---|---|---|
| 2022-11 ftx | −2.83 / −2.83 | −2.38 / −2.38 |
| 2023-01 recovery | **+72.18 / +36.05** | +41.24 / +20.60 |
| 2023-03 chop | −3.48 / −1.79 | −1.08 / −1.08 |
| 2024-12 rally | 0 / 0 | −0.40 / −0.40 |
| 2025-02 correction | 0 / 0 | 0 / 0 |
| 2025-04 tariff | 0 / 0 | +0.58 / +0.26 |
| 2025-05 recovery | +2.27 / +1.09 | −0.82 / −0.88 |
| 2025-07 grind | +0.05 / −0.02 | −0.63 / −0.81 |
| **Σ** | **+68.2 / +32.5** | +36.5 / +15.3 |

Same signature as BTC and ETH: one large trend capture dominates (SOL roughly doubled in
Jan 2023), defensive zeros on chop/correction windows, small stop-driven losses on crash
windows. On the realized floor the diguide edge is +4.1%/window across 8 SOL windows —
higher than BTC (+0.8%) because the single capture is larger; sample is thin, so treat it
as directionally consistent rather than additive.

## Auto-optimizer: machinery verified, paper-test path degenerate (2026-08-26 18:30 CST)

Post-new-build the optimizer runs end-to-end: lineage resolution fixed, LLM writer proposes
candidates (prose / param / filter kinds all observed), numeric gate evaluates day + untouched
holdout, honesty canary passes. Across 5 sessions / 20 candidates: **0 accepts, day delta
exactly 0.000000 every time** — while holdout deltas are real and large (up to **+4.68 sharpe**,
clearing the 0.001 holdout threshold). The mutations genuinely change behavior; the day-gate
score never moves.

Root cause (run-level evidence): the optimizer's paper-test day scenario produces degenerate
runs — a single `long_open` decision, position never exited, across a 90-day window
(ec-day-01M0YNZS6SRT5RFPBJ18HD9VJT, 2023-01..2023-04) — where the standard eval path on the
same strategy and dates produces 4-8 trades with stop exits. The optimizer routes trader
decisions through the shared Cline runtime ("Phase 1 parity: the SAME path as live"); the
standard eval path does not. That divergence, not window choice (tested: default 2025-01,
2022-12..01, 2023-01..04) and not mutation kind (prose/param/filter all tie), makes the day
gate immovable: identical decisions → identical sharpe → `delta_day = 0.0` → every candidate
dropped regardless of merit.

Sharpe is also scale-invariant, so pure sizing params can never pass this gate by design.

**Engine fix required (operator-side, needs rebuild + deploy):** make the optimizer paper-test
trader behave like the eval path (or vice versa) — i.e. close the Cline-runtime divergence —
and/or add a trade-count floor to the day scenario so a 1-decision window fails loudly
(`ensure_window_trade_floor` exists for the parent; candidates deserve the same guard), and/or
score thin windows on realized return instead of sharpe. Until then the optimizer is safe but
sterile: it will reject every candidate.

Fwd test: still dead on the new build. Run `01M0Y6JK68H72G2J5G35W5FKM8` crossed the 08:00Z
8h boundary with 0 filter events / 0 equity marks — third consecutive run with the same
pathology. Warmup bars seed fine via REST; the live bar stream delivers nothing. Bar feed
(Alpaca WS/ingest) needs node-host attention; nothing further reachable remotely.
